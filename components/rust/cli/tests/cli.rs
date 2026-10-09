//! End-to-end CLI smoke tests covering the harbor-rs binary's public
//! interface. Each test invokes the cargo-built binary via `assert_cmd`.

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use tempfile::tempdir;

#[path = "fixtures/elf.rs"]
mod elf_fixture;
#[path = "fixtures/macho.rs"]
mod macho_fixture;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn rs_harbor() -> Command {
    Command::cargo_bin("harbor-rs").expect("locate harbor-rs binary")
}

#[test]
fn top_level_help_lists_subcommands() {
    let assert = rs_harbor().arg("--help").assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(stdout.contains("audit"), "missing audit: {stdout}");
    assert!(stdout.contains("cache"), "missing cache: {stdout}");
    assert!(stdout.contains("stage"), "missing stage: {stdout}");
    assert!(
        stdout.contains("steam-runtime"),
        "missing steam-runtime: {stdout}"
    );
}

#[test]
fn cache_help_lists_nested_verbs() {
    let assert = rs_harbor().args(["cache", "--help"]).assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(stdout.contains("push"), "missing push: {stdout}");
    assert!(stdout.contains("token"), "missing token: {stdout}");
}

#[test]
fn cache_push_help_lists_required_and_optional_flags() {
    let assert = rs_harbor()
        .args(["cache", "push", "--help"])
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(
        stdout.contains("<store-path>"),
        "missing store-path: {stdout}"
    );
    assert!(stdout.contains("--cache"), "missing cache: {stdout}");
    assert!(stdout.contains("--server"), "missing server: {stdout}");
    assert!(
        stdout.contains("--token-file"),
        "missing token-file: {stdout}"
    );
}

#[test]
fn cache_token_issue_help_lists_flags_and_defaults() {
    let assert = rs_harbor()
        .args(["cache", "token", "issue", "--help"])
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(stdout.contains("--project"), "missing project: {stdout}");
    assert!(stdout.contains("--ssh-host"), "missing ssh-host: {stdout}");
    assert!(stdout.contains("--validity"), "missing validity: {stdout}");
    assert!(
        stdout.contains("--subject-prefix"),
        "missing subject-prefix: {stdout}"
    );
    assert!(
        stdout.contains("harbor-rs-"),
        "missing harbor-rs- default: {stdout}"
    );
}

#[test]
fn audit_elf_rejects_non_binary_input() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("plain.txt");
    fs::write(&path, b"not an ELF").unwrap();

    let assert = rs_harbor()
        .args(["audit", "elf", "--skip-ldd"])
        .arg(&path)
        .assert()
        .failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(
        stderr.contains("no candidate binaries found"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn audit_pe_rejects_non_binary_directory() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("a.txt"), b"abcd").unwrap();
    fs::write(dir.path().join("b.txt"), b"efgh").unwrap();

    let assert = rs_harbor()
        .args(["audit", "pe"])
        .arg(dir.path())
        .assert()
        .failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(
        stderr.contains("no candidate binaries found"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn audit_macho_rejects_non_binary_input() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("plain.bin");
    fs::write(&path, b"\x00\x01\x02\x03").unwrap();

    let assert = rs_harbor()
        .args(["audit", "macho"])
        .arg(&path)
        .assert()
        .failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(
        stderr.contains("no candidate binaries found"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn audit_elf_rejects_missing_input_path() {
    let assert = rs_harbor()
        .args(["audit", "elf", "--skip-ldd", "/no/such/path/exists"])
        .assert()
        .failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(
        stderr.contains("missing path:"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn stage_macos_rejects_empty_target_dir() {
    let target = tempdir().unwrap();
    let dist = tempdir().unwrap();

    let assert = rs_harbor()
        .args(["stage", "macos", "--binary", "myapp"])
        .arg("--target-dir")
        .arg(target.path())
        .arg("--dist-dir")
        .arg(dist.path())
        .assert()
        .failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(stderr.contains("missing"), "unexpected stderr: {stderr}");
}

#[test]
fn cache_push_rejects_half_configured_temp_login() {
    let dir = tempdir().unwrap();
    let store_path = dir.path().join("example-store-path");

    let assert = rs_harbor()
        .args([
            "cache",
            "push",
            "--cache",
            "canix",
            "--server",
            "https://attic.example.invalid",
        ])
        .arg(&store_path)
        .assert()
        .failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(
        stderr.contains("server and token_file must be provided together"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn steam_runtime_exec_fails_when_runner_missing() {
    let assert = rs_harbor()
        .args([
            "steam-runtime",
            "exec",
            "--image",
            "registry.example.com/sniper/sdk",
            "--container-runtime",
            "harbor-rs-no-such-runner",
            "--",
            "true",
        ])
        .assert()
        .failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(
        stderr.contains("container runtime not found"),
        "unexpected stderr: {stderr}"
    );
}

fn elf_path(directory: &Path) -> PathBuf {
    let path = directory.join("fixture.so");
    fs::write(&path, elf_fixture::build_shared_object()).unwrap();
    path
}

#[test]
fn audit_elf_passes_with_permissive_allowlist() {
    let directory = tempdir().unwrap();
    let path = elf_path(directory.path());
    let assert = rs_harbor()
        .args([
            "audit",
            "elf",
            "--skip-ldd",
            "--allow-needed-regex",
            ".*",
            "--forbid-path-regex",
            "harbor-rs-never-matches-this-path",
        ])
        .arg(path)
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(
        stdout.contains("checked 1 file(s)"),
        "expected checked-count line: {stdout}"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn audit_elf_passes_on_the_native_linux_binary() {
    let output = rs_harbor()
        .args([
            "audit",
            "elf",
            "--skip-ldd",
            "--forbid-path-regex",
            "harbor-rs-never-matches-this-path",
            env!("CARGO_BIN_EXE_harbor-rs"),
        ])
        .assert()
        .success();
    assert!(String::from_utf8_lossy(&output.get_output().stdout).contains("checked 1 file(s)"));
}

#[test]
fn audit_elf_fails_when_needed_lib_does_not_match_allowlist() {
    let directory = tempdir().unwrap();
    let path = elf_path(directory.path());
    let assert = rs_harbor()
        .args([
            "audit",
            "elf",
            "--skip-ldd",
            "--allow-needed-regex",
            "^libxyz_definitely_not_a_real_dependency\\.so$",
            "--forbid-path-regex",
            "harbor-rs-never-matches",
        ])
        .arg(path)
        .assert()
        .failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(
        stderr.contains("disallowed DT_NEEDED") && stderr.contains("libharbor-fixture.so"),
        "expected DT_NEEDED rejection: {stderr}"
    );
}

#[cfg(unix)]
#[test]
fn audit_elf_skip_ldd_does_not_invoke_resolver() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempdir().unwrap();
    let path = elf_path(directory.path());
    let ldd = directory.path().join("ldd");
    let marker = directory.path().join("resolver-ran");
    let shell = which::which("sh").expect("resolver fixture requires a shell");
    fs::write(&ldd, format!("#!{}\nprintf ran > \"$HARBOR_ELF_RESOLVER_MARKER\"\nprintf 'libharbor-fixture.so => not found\\n'\n", shell.display())).unwrap();
    fs::set_permissions(&ldd, fs::Permissions::from_mode(0o755)).unwrap();
    rs_harbor()
        .env("PATH", directory.path())
        .env("HARBOR_ELF_RESOLVER_MARKER", &marker)
        .args([
            "audit",
            "elf",
            "--skip-ldd",
            "--allow-needed-regex",
            ".*",
            "--forbid-path-regex",
            "harbor-rs-never-matches",
        ])
        .arg(&path)
        .assert()
        .success();
    assert!(!marker.exists(), "--skip-ldd invoked the resolver");

    let output = rs_harbor()
        .env("PATH", directory.path())
        .env("HARBOR_ELF_RESOLVER_MARKER", &marker)
        .args(["audit", "elf"])
        .arg(path)
        .assert()
        .failure();
    assert!(marker.exists(), "resolver control was not executed");
    assert!(String::from_utf8_lossy(&output.get_output().stderr).contains("missing library"));
}

#[test]
fn audit_pe_passes_on_vendored_hello_exe() {
    // tests/fixtures/hello.exe is a stripped 13KB mingw-built PE32+ that
    // imports KERNEL32.dll, msvcrt.dll, and api-ms-win-* DLLs — exactly
    // the shapes that the Steam runtime allowlist covers.
    let assert = rs_harbor()
        .args([
            "audit",
            "pe",
            "--allow-dll-regex",
            ".*",
            "--forbid-path-regex",
            "harbor-rs-never-matches",
            fixture_path("hello.exe").to_str().unwrap(),
        ])
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(
        stdout.contains("checked 1 file(s)"),
        "expected checked-count line: {stdout}"
    );
}

#[test]
fn audit_pe_fails_when_required_dll_disallowed() {
    let assert = rs_harbor()
        .args([
            "audit",
            "pe",
            "--allow-dll-regex",
            "^(harbor-rs-never-imports-this-dll)\\.dll$",
            "--forbid-path-regex",
            "harbor-rs-never-matches",
            fixture_path("hello.exe").to_str().unwrap(),
        ])
        .assert()
        .failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(
        stderr.contains("disallowed DLL import"),
        "expected DLL rejection: {stderr}"
    );
}

#[test]
fn audit_macho_passes_on_built_thin_dylib() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("libfoo.dylib");
    fs::write(
        &path,
        macho_fixture::build_thin_dylib(
            "@rpath/libfoo.dylib",
            &["/usr/lib/libSystem.B.dylib", "@rpath/libsteam_api.dylib"],
        ),
    )
    .unwrap();

    let assert = rs_harbor()
        .args([
            "audit",
            "macho",
            "--allow-dylib-regex",
            "^(@rpath|/usr/lib/)",
            "--forbid-path-regex",
            "harbor-rs-never-matches",
        ])
        .arg(&path)
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(
        stdout.contains("checked 1 file(s)"),
        "expected checked-count line: {stdout}"
    );
}

#[test]
fn audit_macho_passes_on_built_thin_executable() {
    // Regression: goblin seeds libs[0] with the literal placeholder "self"
    // for any Mach-O without LC_ID_DYLIB (i.e. every executable). audit
    // macho must skip it rather than reporting "disallowed dependency: self".
    let dir = tempdir().unwrap();
    let path = dir.path().join("chessbender");
    fs::write(
        &path,
        macho_fixture::build_thin_executable(&["/usr/lib/libSystem.B.dylib"]),
    )
    .unwrap();

    let assert = rs_harbor()
        .args([
            "audit",
            "macho",
            "--allow-dylib-regex",
            "^(@rpath|/usr/lib/)",
            "--forbid-path-regex",
            "harbor-rs-never-matches",
        ])
        .arg(&path)
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(
        stdout.contains("checked 1 file(s)"),
        "expected checked-count line: {stdout}"
    );
}

#[test]
fn audit_macho_fails_on_disallowed_dependency() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("libbad.dylib");
    fs::write(
        &path,
        macho_fixture::build_thin_dylib(
            "@rpath/libbad.dylib",
            &["/usr/local/Cellar/oops/lib/libforbidden.dylib"],
        ),
    )
    .unwrap();

    let assert = rs_harbor()
        .args([
            "audit",
            "macho",
            "--allow-dylib-regex",
            "^(@rpath|/usr/lib/)",
        ])
        .arg(&path)
        .assert()
        .failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(
        stderr.contains("forbidden dependency path") || stderr.contains("disallowed dependency"),
        "expected dylib rejection: {stderr}"
    );
}

#[test]
fn steam_runtime_exec_requires_command() {
    let assert = rs_harbor()
        .args(["steam-runtime", "exec", "--image", "img"])
        .assert()
        .failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(
        stderr.contains("required") || stderr.contains("<COMMAND>"),
        "unexpected stderr: {stderr}"
    );
}
