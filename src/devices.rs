use crate::{
    Error, Result,
    contracts::{Role, invalid},
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fs, path::Path};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Device {
    pub pci: String,
    pub vendor: String,
    pub render_node: Option<String>,
    pub backend_uuid: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct Binding {
    pub physical_key: String,
    pub role: Role,
    pub backend: String,
    pub render_node: Option<String>,
    pub backend_uuid: Option<String>,
    pub qualified: bool,
}
pub fn inventory() -> Result<Vec<Device>> {
    let mut out = Vec::new();
    let path = Path::new("/sys/bus/pci/devices");
    if !path.exists() {
        return Ok(out);
    }
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let class = fs::read_to_string(entry.path().join("class"))?;
        if !class.starts_with("0x03") {
            continue;
        }
        let pci = entry.file_name().to_string_lossy().into_owned();
        let vendor_id = fs::read_to_string(entry.path().join("vendor"))?;
        let vendor = match vendor_id.trim() {
            "0x1002" => "amd",
            "0x10de" => "nvidia",
            "0x8086" => "intel",
            _ => "unknown",
        };
        let render = format!("/dev/dri/by-path/pci-{pci}-render");
        out.push(Device {
            pci,
            vendor: vendor.into(),
            render_node: Path::new(&render).exists().then_some(render),
            backend_uuid: None,
        });
    }
    out.sort_by(|a, b| a.pci.cmp(&b.pci));
    Ok(out)
}
pub fn resolve(
    devices: &[Device],
    role: Role,
    backend: &str,
    pci: Option<&str>,
) -> Result<Binding> {
    let candidates: Vec<_> = devices
        .iter()
        .filter(|d| pci.is_none_or(|id| id == d.pci))
        .filter(|d| match backend {
            "cuda" => d.vendor == "nvidia",
            "egl" | "vaapi" => d.render_node.is_some(),
            _ => false,
        })
        .collect();
    if candidates.len() != 1 {
        return Err(Error::Unqualified(format!(
            "{backend}: expected one exact live device binding, found {}",
            candidates.len()
        )));
    }
    let device = candidates[0];
    if role == Role::Compute && (backend != "cuda" || device.backend_uuid.is_none()) {
        return Err(Error::Unqualified("CUDA UUID/PCI correlation must be established by the adapter; HIP separately unqualified".into()));
    }
    if (role == Role::Render && backend != "egl") || (role == Role::Media && backend != "vaapi") {
        return Err(invalid("operation/backend mismatch"));
    }
    if let Some(node) = &device.render_node
        && (fleetix::gpu::pci_selector(node).is_none() || !node.contains(&device.pci))
    {
        return Err(invalid("stale stable render-node identity"));
    }
    if devices
        .iter()
        .map(|d| &d.pci)
        .collect::<BTreeSet<_>>()
        .len()
        != devices.len()
    {
        return Err(invalid("duplicate physical devices"));
    }
    Ok(Binding {
        physical_key: device.pci.clone(),
        role,
        backend: backend.into(),
        render_node: device.render_node.clone(),
        backend_uuid: device.backend_uuid.clone(),
        qualified: false,
    })
}
