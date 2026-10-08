use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use harbor_sdk::{RealizedSdk, realize_macos_sdk};
use tempfile::TempDir;

#[test]
fn realize_macos_sdk_parses_env_output_and_validates_sdk() {
    if std::env::var_os("HARBOUR_SDK_CHILD").is_some() {
        let expected = RealizedSdk {
            store_path: PathBuf::from("/nix/store/abc123-macos-sdk"),
            sdk_root: PathBuf::from("/nix/store/abc123-macos-sdk/MacOSX26.1.sdk"),
            recursive_hash: String::from("sha256-deadbeef"),
            version: String::from("26.1"),
        };
        let realized = realize_macos_sdk(Path::new("/tmp/unused-archive.tar.xz"), "26.1")
            .expect("realize SDK");
        assert_eq!(realized, expected);
        return;
    }

    let temp = TempDir::new().expect("create temp dir");
    let bin_dir = temp.path().join("bin");
    fs::create_dir_all(&bin_dir).expect("create bin dir");

    write_script(
        &bin_dir.join("realize-macos-sdk"),
        r#"#!/bin/sh
cat "$FIXTURE"
"#,
    );
    write_script(
        &bin_dir.join("validate-macos-sdk"),
        r#"#!/bin/sh
printf '%s\n' "$@" > "$VALIDATE_ARGS"
exit 0
"#,
    );

    let validate_args = temp.path().join("validate-args.txt");

    let mut command = Command::new(std::env::current_exe().expect("current exe"));
    command
        .arg("--exact")
        .arg("realize_macos_sdk_parses_env_output_and_validates_sdk")
        .arg("--nocapture")
        .env("HARBOUR_SDK_CHILD", "1")
        .env("PATH", prepend_path(&bin_dir))
        .env(
            "FIXTURE",
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests")
                .join("fixtures")
                .join("realize-env.txt"),
        )
        .env("VALIDATE_ARGS", &validate_args);

    let status = command.status().expect("run nested test process");
    assert!(status.success(), "nested test process failed: {status}");

    let args = fs::read_to_string(&validate_args).expect("read validate args");
    assert_eq!(args, "/nix/store/abc123-macos-sdk/MacOSX26.1.sdk\n26.1\n");
}

fn write_script(path: &Path, body: &str) {
    fs::write(path, body).expect("write script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut perms = fs::metadata(path).expect("metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).expect("chmod");
    }
}

fn prepend_path(bin_dir: &Path) -> String {
    let current = std::env::var_os("PATH").unwrap_or_default();
    let mut paths = vec![bin_dir.to_path_buf()];
    paths.extend(std::env::split_paths(&current));
    std::env::join_paths(paths)
        .expect("join PATH")
        .to_string_lossy()
        .into_owned()
}
