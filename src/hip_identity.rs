//! Source-backed ROCm 7.2.3 identity for the single-KFD-GPU policy candidate.
use super::DrmSandbox;
use crate::{Error, Result, contracts::invalid, resources::pci_key};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Read,
    os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt},
    path::Path,
    process::Command,
};

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct HipIdentity {
    pub pci: String,
    pub render_node: String,
    pub backend_uuid: String,
    pub architecture: String,
    pub kfd_node: u32,
    pub kfd_gpu_id: u32,
    pub unique_id: u64,
    pub generation: u64,
    pub qualified: bool,
}

/// Qualification candidate; mounting a shared KFD is limited to a host with
/// exactly one live GPU topology node. No visibility variable grants access.
#[derive(Debug, Serialize)]
pub struct HipSandbox {
    pub identity: HipIdentity,
    drm: DrmSandbox,
}

impl HipSandbox {
    pub fn resolve(pci: &str, uuid: &str) -> Result<Self> {
        let identity = HipIdentity::resolve(pci)?;
        if identity.backend_uuid != uuid {
            return Err(invalid("approved HIP UUID differs from PCI/KFD identity"));
        }
        Ok(Self {
            identity,
            drm: DrmSandbox::resolve(pci)?,
        })
    }

    pub fn apply(&self, command: &mut Command) {
        self.drm.apply_compute(command);
        command.args(["--dev-bind", "/dev/kfd", "/dev/kfd"]);
        for path in [
            "/sys/devices/virtual/kfd/kfd/topology",
            "/sys/devices/system/cpu",
            "/sys/devices/system/node",
        ] {
            command.args(["--ro-bind", path, path]);
        }
    }
}

fn attribute(path: &Path) -> Result<String> {
    let input = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    if !input.metadata()?.is_file() {
        return Err(invalid("KFD identity attributes must be regular files"));
    }
    let mut bytes = Vec::new();
    input.take(16385).read_to_end(&mut bytes)?;
    if bytes.len() > 16384 {
        return Err(invalid("bounded KFD identity attributes required"));
    }
    String::from_utf8(bytes).map_err(|_| invalid("KFD identity encoding"))
}

fn number(path: &Path) -> Result<u64> {
    attribute(path)?
        .trim()
        .parse()
        .map_err(|_| invalid("KFD identity number"))
}

fn properties(path: &Path) -> Result<BTreeMap<String, u64>> {
    let mut properties = BTreeMap::new();
    for line in attribute(path)?.lines() {
        let mut fields = line.split_whitespace();
        let name = fields.next().ok_or_else(|| invalid("KFD property name"))?;
        let value = fields
            .next()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| invalid("KFD property value"))?;
        if fields.next().is_some() || properties.insert(name.into(), value).is_some() {
            return Err(invalid(
                "unique KFD properties without trailing fields required",
            ));
        }
    }
    Ok(properties)
}

impl HipIdentity {
    pub fn verify_filter_receipt(&self, receipt: &serde_json::Value) -> Result<()> {
        let compiled = receipt["compiled_hip_version"]
            .as_u64()
            .filter(|v| *v > 0)
            .ok_or_else(|| invalid("compiled filter HIP version missing"))?;
        if receipt["adapter"] != "Viskores"
            || receipt["backend"] != "hip"
            || receipt["pci"] != self.pci
            || receipt["backend_uuid"] != self.backend_uuid
            || receipt["architecture"] != self.architecture
            || receipt["compiled_architecture"] != self.architecture
            || receipt["hip_runtime_version"].as_u64() != Some(compiled)
            || receipt["hip_driver_version"].as_u64() != Some(compiled)
            || receipt["source_revision"] != "7c0494a68bff379d32d6b1fbaa3d10d27a73af54"
            || receipt["viskores_revision"] != "521f3b72aabe0bf37e9972975700df27adbbae71"
            || receipt["kokkos_revision"] != "6ecdf605e0f7639adec599d25cf0e206d7b8f9f5"
            || receipt["executed"] != true
            || receipt["software_fallback"] != false
            || receipt["precision"] != "float64"
            || receipt["gpu_kernel_completion_verified"] != true
            || receipt["hip_dispatches"].as_u64().is_none_or(|n| n == 0)
        {
            return Err(Error::Unqualified(
                "filter receipt differs from exact HIP device/source/ABI or completion evidence"
                    .into(),
            ));
        }
        Ok(())
    }
    pub fn verify_receipt(&self, receipt: &serde_json::Value) -> Result<()> {
        let compiled = receipt["compiled_hip_version"]
            .as_u64()
            .filter(|v| *v > 0)
            .ok_or_else(|| invalid("compiled HIP version missing"))?;
        if receipt["adapter"] != "OpenLB"
            || receipt["backend"] != "hip"
            || receipt["pci"] != self.pci
            || receipt["backend_uuid"] != self.backend_uuid
            || receipt["architecture"] != self.architecture
            || receipt["compiled_architecture"] != self.architecture
            || receipt["hip_runtime_version"].as_u64() != Some(compiled)
            || receipt["hip_driver_version"].as_u64() != Some(compiled)
            || receipt["source_revision"] != "145cd54810b468f4b6fd3ed86b10644264841578"
            || receipt["executed"] != true
            || receipt["software_fallback"] != false
            || receipt["precision"] != "float64"
            || receipt["gpu_blocks"] != 1
            || receipt["gpu_kernel_completion_verified"] != true
        {
            return Err(Error::Unqualified(
                "HIP receipt differs from exact device/source/compiled-runtime identity; no fallback".into(),
            ));
        }
        Ok(())
    }

    pub fn resolve(pci: &str) -> Result<Self> {
        let drm = DrmSandbox::resolve(pci)?;
        if attribute(&drm.sysfs_device.join("vendor"))?.trim() != "0x1002"
            || fs::canonicalize(drm.sysfs_device.join("driver"))?
                .file_name()
                .and_then(|s| s.to_str())
                != Some("amdgpu")
        {
            return Err(invalid(
                "HIP requires the selected AMD device bound to amdgpu",
            ));
        }
        let kfd = fs::symlink_metadata("/dev/kfd")?;
        if !kfd.file_type().is_char_device() {
            return Err(invalid("KFD must be a direct character-device node"));
        }
        let kfd_sys = Path::new("/sys/dev/char").join(format!(
            "{}:{}",
            libc::major(kfd.rdev()),
            libc::minor(kfd.rdev())
        ));
        if fs::canonicalize(kfd_sys)? != Path::new("/sys/devices/virtual/kfd/kfd") {
            return Err(invalid("KFD character-device/sysfs mismatch"));
        }
        Self::from_topology(
            Path::new("/sys/devices/virtual/kfd/kfd/topology"),
            pci,
            libc::minor(fs::metadata(&drm.node)?.rdev()),
            attribute(&drm.sysfs_device.join("unique_id"))?.trim(),
        )
    }

    /// Parse a complete observed topology. This deliberately rejects another
    /// GPU rather than treating visibility variables as access enforcement.
    pub fn from_topology(root: &Path, pci: &str, minor: u32, pci_unique_id: &str) -> Result<Self> {
        if pci_key(pci)? != pci {
            return Err(invalid("canonical HIP PCI identity required"));
        }
        let unique_id = u64::from_str_radix(pci_unique_id, 16)
            .map_err(|_| invalid("PCI unique ID encoding"))?;
        if unique_id == 0 || pci_unique_id != format!("{unique_id:016x}") {
            return Err(invalid("nonzero canonical PCI unique ID required"));
        }
        let generation = number(&root.join("generation_id"))?;
        let mut gpu = None;
        let mut count = 0;
        for entry in fs::read_dir(root.join("nodes"))? {
            count += 1;
            if count > 256 {
                return Err(invalid("bounded KFD topology nodes required"));
            }
            let entry = entry?;
            let name = entry.file_name();
            let index: u32 = name
                .to_str()
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| invalid("KFD node index"))?;
            if name != index.to_string().as_str() || !entry.file_type()?.is_dir() {
                return Err(invalid("direct canonical KFD node directories required"));
            }
            let id = number(&entry.path().join("gpu_id"))?;
            let p = properties(&entry.path().join("properties"))?;
            let simd = p
                .get("simd_count")
                .ok_or_else(|| invalid("KFD SIMD count missing"))?;
            if id == 0 && *simd == 0 {
                continue;
            }
            if gpu.is_some() {
                return Err(Error::Unqualified("single-KFD-GPU policy rejects another exposed compute device; multi-GPU exclusion unqualified".into()));
            }
            let get = |key| {
                p.get(key)
                    .copied()
                    .ok_or_else(|| invalid(format!("KFD {key} missing")))
            };
            let domain = get("domain")?;
            let location = get("location_id")?;
            let target = get("gfx_target_version")?;
            let architecture = format!(
                "gfx{}{:x}{:x}",
                target / 10000,
                target / 100 % 100,
                target % 100
            );
            if id == 0
                || id > u32::MAX as u64
                || *simd == 0
                || domain > u16::MAX as u64
                || location > u16::MAX as u64
                || format!(
                    "{domain:04x}:{:02x}:{:02x}.{}",
                    location >> 8,
                    (location >> 3) & 31,
                    location & 7
                ) != pci
                || get("vendor_id")? != 0x1002
                || get("drm_render_minor")? != u64::from(minor)
                || get("unique_id")? != unique_id
                || target == 0
                || target / 100 % 100 > 15
                || target % 100 > 15
            {
                return Err(invalid(
                    "HIP PCI/KFD/DRM/UUID/architecture identity mismatch",
                ));
            }
            // ROCr formats UniqueID as 16 ASCII hex characters; CLR copies
            // those characters into hipUUID. Our adapter hex-encodes its bytes.
            let encoded: String = format!("{unique_id:016x}")
                .bytes()
                .map(|b| format!("{b:02x}"))
                .collect();
            gpu = Some(Self {
                pci: pci.into(),
                render_node: format!("/dev/dri/renderD{minor}"),
                backend_uuid: format!("GPU-{encoded}"),
                architecture,
                kfd_node: index,
                kfd_gpu_id: id as u32,
                unique_id,
                generation,
                qualified: false,
            });
        }
        if number(&root.join("generation_id"))? != generation {
            return Err(invalid("KFD topology changed during identity correlation"));
        }
        gpu.ok_or_else(|| {
            Error::Unqualified("selected HIP device has no live KFD topology identity".into())
        })
    }
}
