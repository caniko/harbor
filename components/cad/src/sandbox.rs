//! Operation-specific mount policy. Importers see only their package closure.
use crate::{Result, contracts::invalid, execution::packaged_file, worker::read_bounded};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    process::Command,
};

pub const IMPORT_POLICY: &str = "harbor-cad-importer-v1";

pub fn importer_mounts(manifest: &Path, executable: &Path) -> Result<Vec<PathBuf>> {
    let manifest = packaged_file(manifest)?;
    let data = read_bounded(&manifest, 2 * 1024 * 1024)?;
    parse_importer_mounts(
        std::str::from_utf8(&data)
            .map_err(|_| invalid("UTF-8 importer closure manifest required"))?,
        executable,
    )
}

fn parse_importer_mounts(data: &str, executable: &Path) -> Result<Vec<PathBuf>> {
    let mut mounts = BTreeSet::new();
    for line in data.lines() {
        let path = Path::new(line);
        let object = crate::retention::store_object(line)?;
        if path != object || !mounts.insert(object) {
            return Err(invalid("unique top-level importer store objects required"));
        }
        if mounts.len() > 8192 {
            return Err(invalid("bounded importer closure required"));
        }
    }
    if mounts.is_empty()
        || !mounts.contains(&crate::retention::store_object(
            executable
                .to_str()
                .ok_or_else(|| invalid("importer executable path"))?,
        )?)
    {
        return Err(invalid(
            "importer executable absent from its declared closure",
        ));
    }
    Ok(mounts.into_iter().collect())
}

pub fn mount_importer(command: &mut Command, manifest: &Path, executable: &Path) -> Result<()> {
    mount_closure(command, manifest, executable)?;
    command
        .args(["--ro-bind"])
        .arg(manifest)
        .arg("/import-runtime-closure.txt");
    command.args(["--setenv", "HARBOR_CAD_IMPORT_POLICY", IMPORT_POLICY]);
    command
        .args(["--setenv", "HARBOR_CAD_HOST_NETNS"])
        .arg(std::fs::read_link("/proc/self/ns/net")?);
    // Optional fixed qualification probes are operator environment inputs,
    // never worker-protocol or imported document operations.
    for name in [
        "HARBOR_CAD_IMPORT_PROBE_ROOT",
        "HARBOR_CAD_IMPORT_PROBE_PORT",
    ] {
        if let Some(value) = std::env::var_os(name) {
            command.args(["--setenv", name]).arg(value);
        }
    }
    Ok(())
}

pub fn mount_closure(command: &mut Command, manifest: &Path, executable: &Path) -> Result<()> {
    command.args(["--dir", "/nix", "--dir", "/nix/store"]);
    for path in importer_mounts(manifest, executable)? {
        command.arg("--ro-bind").arg(&path).arg(&path);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn importer_closure_rejects_broad_traversing_unrelated_and_derivation_mounts() {
        let package = "/nix/store/00000000000000000000000000000000-importer";
        let executable = PathBuf::from(format!("{package}/bin/importer"));
        assert_eq!(
            parse_importer_mounts(&format!("{package}\n"), &executable).unwrap(),
            [PathBuf::from(package)]
        );
        for data in [
            "/nix/store",
            "/home/operator",
            "/nix/store/00000000000000000000000000000000-importer/../secret",
            "/nix/store/00000000000000000000000000000000-importer.drv",
            "/nix/store/11111111111111111111111111111111-other",
        ] {
            assert!(parse_importer_mounts(data, &executable).is_err(), "{data}");
        }
        assert!(parse_importer_mounts(&format!("{package}\n{package}\n"), &executable).is_err());
    }
}
