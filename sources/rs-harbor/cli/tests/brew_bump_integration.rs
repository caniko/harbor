use std::fs;
use std::path::Path;
use std::process::Command as StdCommand;

use assert_cmd::Command;

const FIXTURE_SHA256: &str = "5891b5b522d5df086d0ff0b110fbd9d21bb4fc7163af34d08286a2e846f6be03";

#[test]
fn brew_bump_writes_formula_to_tap() {
    let temp = tempfile::tempdir().expect("tempdir");
    let tap = temp.path().join("tap");
    let archive = temp.path().join("testapp.tar.gz");
    fs::create_dir(&tap).expect("create tap");
    fs::write(&archive, b"hello\n").expect("write archive");
    init_git_tap(&tap);
    let archive_arg = format!(
        "linux_intel=https://example.com/testapp.tar.gz,{}",
        archive.display()
    );

    Command::cargo_bin("rs-harbor")
        .expect("rs-harbor binary")
        .args([
            "brew",
            "bump",
            "--tap",
            tap.to_str().expect("utf8 tap"),
            "--name",
            "testapp",
            "--version",
            "1.2.3",
            "--description",
            "Test application",
            "--homepage",
            "https://example.com/testapp",
            "--license",
            "MIT",
            "--archive",
            &archive_arg,
        ])
        .assert()
        .success();

    let formula = fs::read_to_string(tap.join("Formula/testapp.rb")).expect("formula");
    assert!(formula.contains("class Testapp < Formula"));
    assert!(formula.contains(&format!("sha256 \"{FIXTURE_SHA256}\"")));
}

#[test]
fn brew_bump_stdout_emits_formula() {
    let temp = tempfile::tempdir().expect("tempdir");
    let archive = temp.path().join("modde.tar.gz");
    fs::write(&archive, b"hello\n").expect("write archive");
    let archive_arg = format!(
        "darwin_arm=https://example.com/foo.tar.gz,{}",
        archive.display()
    );

    let output = Command::cargo_bin("rs-harbor")
        .expect("rs-harbor binary")
        .args([
            "brew",
            "bump",
            "--stdout",
            "--name",
            "modde",
            "--version",
            "0.1.0",
            "--description",
            "Cross-platform game mod manager",
            "--homepage",
            "https://modde.tartanoglu.com",
            "--license",
            "MIT",
            "--archive",
            &archive_arg,
        ])
        .output()
        .expect("run rs-harbor");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("class Modde < Formula"));
    assert!(stdout.contains(&format!("sha256 \"{FIXTURE_SHA256}\"")));
}

#[test]
fn brew_bump_rejects_stdout_with_tap() {
    let temp = tempfile::tempdir().expect("tempdir");
    let archive = temp.path().join("testapp.tar.gz");
    fs::write(&archive, b"hello\n").expect("write archive");
    let archive_arg = format!(
        "linux_intel=https://example.com/testapp.tar.gz,{}",
        archive.display()
    );

    Command::cargo_bin("rs-harbor")
        .expect("rs-harbor binary")
        .args([
            "brew",
            "bump",
            "--stdout",
            "--tap",
            temp.path().to_str().expect("utf8 tap"),
            "--name",
            "testapp",
            "--version",
            "1.2.3",
            "--description",
            "Test application",
            "--homepage",
            "https://example.com/testapp",
            "--license",
            "MIT",
            "--archive",
            &archive_arg,
        ])
        .assert()
        .failure()
        .code(2);
}

#[test]
fn brew_bump_push_aborts_on_unrelated_dirty_file() {
    let temp = tempfile::tempdir().expect("tempdir");
    let tap = temp.path().join("tap");
    let archive = temp.path().join("testapp.tar.gz");
    fs::create_dir(&tap).expect("create tap");
    fs::write(&archive, b"hello\n").expect("write archive");
    init_git_tap(&tap);
    fs::write(tap.join("README.md"), "dirty\n").expect("dirty file");
    let archive_arg = format!(
        "linux_intel=https://example.com/testapp.tar.gz,{}",
        archive.display()
    );

    let output = Command::cargo_bin("rs-harbor")
        .expect("rs-harbor binary")
        .args([
            "brew",
            "bump",
            "--push",
            "--tap",
            tap.to_str().expect("utf8 tap"),
            "--name",
            "testapp",
            "--version",
            "1.2.3",
            "--description",
            "Test application",
            "--homepage",
            "https://example.com/testapp",
            "--license",
            "MIT",
            "--archive",
            &archive_arg,
        ])
        .output()
        .expect("run rs-harbor");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(stderr.contains("tap working tree has unrelated changes"));
    assert!(stderr.contains("README.md"));
}

fn init_git_tap(path: &Path) {
    fs::create_dir(path.join("Formula")).expect("create Formula");
    fs::write(path.join("Formula/.keep"), "").expect("write keep");
    git(path, &["init"]);
    git(path, &["config", "user.name", "rs-harbor test"]);
    git(path, &["config", "user.email", "rs-harbor@example.invalid"]);
    git(path, &["add", "Formula/.keep"]);
    git(path, &["commit", "-m", "init tap"]);
}

fn git(path: &Path, args: &[&str]) {
    let status = StdCommand::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .status()
        .expect("run git");
    assert!(status.success(), "git {args:?} failed");
}
