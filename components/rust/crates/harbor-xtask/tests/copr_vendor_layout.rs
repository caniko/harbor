use std::fs::File;
use std::path::Path;

use flate2::Compression;
use flate2::write::GzEncoder;
use harbor_xtask::{CoprConfig, ProjectConfig, run_copr_vendor_check};
use tar::Builder;
use tempfile::tempdir;

#[test]
fn accepts_vendor_rooted_archive() {
    let temp_dir = tempdir().expect("temp dir");
    let tarball = temp_dir.path().join("vendor.tar.gz");
    write_archive(&tarball, "vendor/config.toml", b"[source.crates-io]\n");

    let cfg = copr_cfg(temp_dir.path());
    run_copr_vendor_check(&cfg).expect("vendor layout should pass");
}

#[test]
fn rejects_archive_without_vendor_root() {
    let temp_dir = tempdir().expect("temp dir");
    let tarball = temp_dir.path().join("vendor.tar.gz");
    write_archive(&tarball, "wrong-root/config.toml", b"[source.crates-io]\n");

    let cfg = copr_cfg(temp_dir.path());
    let error = run_copr_vendor_check(&cfg).expect_err("vendor layout should fail");
    assert!(
        error
            .to_string()
            .contains("does not contain entries rooted at vendor/")
    );
}

fn copr_cfg(workspace_root: &Path) -> ProjectConfig {
    let mut cfg = ProjectConfig::from_workspace_root(workspace_root);
    cfg.copr = Some(CoprConfig {
        source_archive_url_template: String::from("https://example.invalid/v{version}.tar.gz"),
        srpm_dir: workspace_root.join("srpms"),
        vendor_tarball: workspace_root.join("vendor.tar.gz"),
    });
    cfg
}

fn write_archive(path: &Path, entry_path: &str, body: &[u8]) {
    let file = File::create(path).expect("create archive");
    let encoder = GzEncoder::new(file, Compression::default());
    let mut builder = Builder::new(encoder);

    let mut header = tar::Header::new_gnu();
    header.set_path(entry_path).expect("set path");
    header.set_size(body.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    builder.append(&header, body).expect("append archive entry");

    let encoder = builder.into_inner().expect("finish tar");
    encoder.finish().expect("finish gzip");
}
