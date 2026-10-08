use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use semver::Version;
use tar::{Archive, Builder};

use crate::{CoprConfig, ProjectConfig, run_command};

pub fn run_copr_vendor(cfg: &ProjectConfig) -> Result<()> {
    let copr = copr_config(cfg)?;
    let vendor_dir = cfg.workspace_root.join("vendor");
    let vendor_tarball = cfg.resolve(&copr.vendor_tarball);
    let _cleanup = VendorDirGuard {
        path: vendor_dir.clone(),
    };

    let mut vendor = Command::new("cargo");
    vendor
        .current_dir(&cfg.workspace_root)
        .arg("vendor")
        .arg("--locked");
    run_command(&mut vendor)?;

    if vendor_tarball.exists() {
        fs::remove_file(&vendor_tarball)
            .with_context(|| format!("removing {}", vendor_tarball.display()))?;
    }

    let file = fs::File::create(&vendor_tarball)
        .with_context(|| format!("creating {}", vendor_tarball.display()))?;
    let encoder = GzEncoder::new(file, Compression::default());
    let mut builder = Builder::new(encoder);
    builder.follow_symlinks(false);
    builder
        .append_dir_all("vendor", &vendor_dir)
        .with_context(|| format!("archiving {}", vendor_dir.display()))?;
    let encoder = builder.into_inner().context("finishing tar archive")?;
    encoder.finish().context("finishing gzip archive")?;

    run_copr_vendor_check(cfg)
}

pub fn run_copr_vendor_check(cfg: &ProjectConfig) -> Result<()> {
    let copr = copr_config(cfg)?;
    let vendor_tarball = cfg.resolve(&copr.vendor_tarball);
    let file = fs::File::open(&vendor_tarball)
        .with_context(|| format!("opening {}", vendor_tarball.display()))?;
    let decoder = GzDecoder::new(file);
    let mut archive = Archive::new(decoder);
    let mut found_vendor_root = false;

    for entry in archive
        .entries()
        .context("reading vendor tarball entries")?
    {
        let entry = entry.context("reading vendor tarball entry")?;
        let path = entry.path().context("reading vendor tarball entry path")?;
        if path_starts_with_vendor(path.as_ref()) {
            found_vendor_root = true;
            break;
        }
    }

    if found_vendor_root {
        Ok(())
    } else {
        bail!(
            "{} does not contain entries rooted at vendor/",
            vendor_tarball.display()
        )
    }
}

pub fn run_copr_srpm(cfg: &ProjectConfig, version: &Version) -> Result<()> {
    let copr = copr_config(cfg)?;
    let spec_file = cfg
        .spec_file
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("spec_file is required for COPR SRPM builds"))?;
    let spec_path = cfg.resolve(spec_file);
    let source_url = copr
        .source_archive_url_template
        .replace("{version}", &version.to_string());
    let archive_name = source_url
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| anyhow::anyhow!("could not derive archive file name from {source_url}"))?;
    let archive_path = cfg.workspace_root.join(archive_name);

    let response = ureq::get(&source_url)
        .call()
        .with_context(|| format!("downloading {source_url}"))?;
    let mut body = response.into_body();
    let bytes = body.read_to_vec().context("reading source archive body")?;
    fs::write(&archive_path, &bytes)
        .with_context(|| format!("writing {}", archive_path.display()))?;

    run_copr_vendor(cfg)?;
    run_copr_vendor_check(cfg)?;

    let srpm_dir = cfg.resolve(&copr.srpm_dir);
    fs::create_dir_all(&srpm_dir).with_context(|| format!("creating {}", srpm_dir.display()))?;

    let mut rpmbuild = Command::new("rpmbuild");
    rpmbuild
        .current_dir(&cfg.workspace_root)
        .arg("-bs")
        .arg(spec_file)
        .arg("--define")
        .arg(format!("_sourcedir {}", cfg.workspace_root.display()))
        .arg("--define")
        .arg(format!("_srcrpmdir {}", srpm_dir.display()));

    if !spec_path.exists() {
        bail!("spec file does not exist: {}", spec_path.display());
    }

    run_command(&mut rpmbuild)
}

fn copr_config(cfg: &ProjectConfig) -> Result<&CoprConfig> {
    cfg.copr
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("copr configuration is required"))
}

fn path_starts_with_vendor(path: &Path) -> bool {
    path.components()
        .next()
        .is_some_and(|component| component.as_os_str() == "vendor")
}

struct VendorDirGuard {
    path: PathBuf,
}

impl Drop for VendorDirGuard {
    fn drop(&mut self) {
        if self.path.exists() {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
