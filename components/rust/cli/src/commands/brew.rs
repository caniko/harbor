//! `harbor-rs brew` - Homebrew tap helpers.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::fs;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};
use sha2::{Digest, Sha256};

#[derive(Subcommand, Debug)]
pub enum BrewCommand {
    /// Render or update a Homebrew formula in a tap repo.
    Bump(BumpArgs),
}

#[derive(Args, Debug)]
#[command(disable_version_flag = true)]
pub struct BumpArgs {
    /// Formula name (for example, "modde"). `UpperCamelCased` for the Ruby class.
    #[arg(long)]
    pub name: String,

    /// Version string without a leading "v".
    #[arg(long)]
    pub version: String,

    /// Short description, 80 characters or fewer.
    #[arg(long)]
    pub description: String,

    /// Homepage URL. Must start with https://.
    #[arg(long)]
    pub homepage: String,

    /// SPDX license identifier.
    #[arg(long)]
    pub license: String,

    /// Per-platform archive as PLATFORM=URL,PATH. URL is embedded in the formula;
    /// PATH is the local archive used to compute sha256.
    #[arg(long, value_parser = parse_archive_spec)]
    pub archive: Vec<ArchiveSpec>,

    /// Binaries to install. Defaults to the formula name when omitted.
    #[arg(long)]
    pub binary: Vec<String>,

    /// Homebrew dependencies, for example --depends openssl@3.
    #[arg(long = "depends")]
    pub dependencies: Vec<String>,

    /// Target tap repository. Writes Formula/<name>.rb unless --stdout is set.
    #[arg(long, required_unless_present = "stdout")]
    pub tap: Option<PathBuf>,

    /// Print the formula to stdout instead of writing to the tap.
    #[arg(long, conflicts_with = "tap")]
    pub stdout: bool,

    /// After writing, run git add, git commit, and git push in the tap repo.
    /// Requires git credentials to be configured outside harbor-rs.
    #[arg(long, requires = "tap")]
    pub push: bool,

    /// Commit message for --push. Defaults to "<name> <version>".
    #[arg(long, requires = "push")]
    pub commit_message: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Platform {
    DarwinArm,
    DarwinIntel,
    LinuxArm,
    LinuxIntel,
}

#[derive(Clone, Debug)]
pub struct ArchiveSpec {
    pub platform: Platform,
    pub url: String,
    pub local_path: PathBuf,
}

#[derive(Debug)]
struct PlatformArchive {
    url: String,
    sha256: String,
}

#[derive(Debug)]
struct Formula {
    name: String,
    version: String,
    description: String,
    homepage: String,
    license: String,
    platforms: HashMap<Platform, PlatformArchive>,
    dependencies: Vec<String>,
    binaries: Vec<String>,
}

pub fn run(cmd: BrewCommand) -> Result<()> {
    match cmd {
        BrewCommand::Bump(args) => bump(&args),
    }
}

fn bump(args: &BumpArgs) -> Result<()> {
    validate_args(args)?;
    let formula = formula_from_args(args)?;
    let text = render_formula(&formula);

    if args.stdout {
        println!("{text}");
        return Ok(());
    }

    let tap = args
        .tap
        .as_deref()
        .context("--tap is required unless --stdout is set")?;
    let formula_path = formula_path(tap, &args.name);
    if let Some(parent) = formula_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating formula dir {}", parent.display()))?;
    }
    fs::write(&formula_path, text)
        .with_context(|| format!("writing formula {}", formula_path.display()))?;

    if args.push {
        let commit_message = args
            .commit_message
            .as_deref()
            .map_or_else(|| format!("{} {}", args.name, args.version), String::from);
        push_formula(tap, &args.name, &formula_path, &commit_message)?;
    }

    Ok(())
}

fn validate_args(args: &BumpArgs) -> Result<()> {
    if !valid_name(&args.name) {
        bail!("name must match [a-z][a-z0-9-]*, got: {}", args.name);
    }
    if args.version.starts_with('v') || !valid_version(&args.version) {
        bail!(
            "version must match [0-9A-Za-z.+~_-]+ without a leading v, got: {}",
            args.version
        );
    }
    if args.description.is_empty() || args.description.chars().count() > 80 {
        bail!("description must be a non-empty string of 80 characters or fewer");
    }
    if !args.homepage.starts_with("https://") {
        bail!("homepage must start with https://");
    }
    if args.license.is_empty() {
        bail!("license must be a non-empty string");
    }
    if args.archive.is_empty() {
        bail!("at least one --archive is required");
    }
    if args.binary.iter().any(String::is_empty) {
        bail!("binaries must be non-empty strings");
    }
    if args.dependencies.iter().any(String::is_empty) {
        bail!("dependencies must be non-empty strings");
    }

    let mut platforms = HashSet::new();
    for archive in &args.archive {
        if !platforms.insert(archive.platform) {
            bail!("duplicate archive for platform {}", archive.platform.key());
        }
        if !archive.url.starts_with("https://") {
            bail!(
                "archive URL for {} must start with https://",
                archive.platform.key()
            );
        }
        if !archive.local_path.is_file() {
            bail!(
                "archive path does not exist: {}",
                archive.local_path.display()
            );
        }
    }

    Ok(())
}

fn formula_from_args(args: &BumpArgs) -> Result<Formula> {
    let mut platforms = HashMap::new();
    for archive in &args.archive {
        let sha256 = sha256_file(&archive.local_path)
            .with_context(|| format!("hashing {}", archive.local_path.display()))?;
        platforms.insert(
            archive.platform,
            PlatformArchive {
                url: archive.url.clone(),
                sha256,
            },
        );
    }

    let binaries = if args.binary.is_empty() {
        vec![args.name.clone()]
    } else {
        args.binary.clone()
    };

    Ok(Formula {
        name: args.name.clone(),
        version: args.version.clone(),
        description: args.description.clone(),
        homepage: args.homepage.clone(),
        license: args.license.clone(),
        platforms,
        dependencies: args.dependencies.clone(),
        binaries,
    })
}

pub fn parse_archive_spec(raw: &str) -> Result<ArchiveSpec, String> {
    let (platform, rest) = raw
        .split_once('=')
        .ok_or_else(|| "archive must be PLATFORM=URL,PATH".to_string())?;
    let platform = Platform::parse(platform)?;
    let (url, path) = rest
        .split_once(',')
        .ok_or_else(|| "archive must be PLATFORM=URL,PATH".to_string())?;
    if url.is_empty() {
        return Err("archive URL must not be empty".to_string());
    }
    if path.is_empty() {
        return Err("archive path must not be empty".to_string());
    }

    Ok(ArchiveSpec {
        platform,
        url: url.to_string(),
        local_path: PathBuf::from(path),
    })
}

fn sha256_file(path: &Path) -> Result<String> {
    let file = fs::File::open(path)?;
    let mut reader = BufReader::with_capacity(64 * 1024, file);
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];

    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

fn render_formula(formula: &Formula) -> String {
    let mut lines = vec![
        format!("class {} < Formula", class_name(&formula.name)),
        format!("  desc {}", ruby_string(&formula.description)),
        format!("  homepage {}", ruby_string(&formula.homepage)),
        format!("  version {}", ruby_string(&formula.version)),
        format!("  license {}", ruby_string(&formula.license)),
        String::new(),
    ];

    let platform_lines = platform_lines(&formula.platforms);
    lines.extend(indent_lines("  ", &platform_lines));
    lines.push(String::new());

    let body_blocks = vec![
        dependency_lines(&formula.dependencies),
        install_lines(&formula.binaries),
    ];

    lines.extend(indent_lines("  ", &join_blocks(body_blocks)));
    lines.push("end".to_string());

    lines.join("\n")
}

fn platform_lines(platforms: &HashMap<Platform, PlatformArchive>) -> Vec<String> {
    join_blocks(vec![
        os_block_lines(
            platforms,
            "macos",
            Platform::DarwinArm,
            Platform::DarwinIntel,
        ),
        os_block_lines(platforms, "linux", Platform::LinuxArm, Platform::LinuxIntel),
    ])
}

fn os_block_lines(
    platforms: &HashMap<Platform, PlatformArchive>,
    os: &str,
    arm: Platform,
    intel: Platform,
) -> Vec<String> {
    let arch_lines = join_blocks(vec![
        arch_block_lines(platforms, arm, "arm"),
        arch_block_lines(platforms, intel, "intel"),
    ]);
    if arch_lines.is_empty() {
        Vec::new()
    } else {
        let mut lines = vec![format!("on_{os} do")];
        lines.extend(indent_lines("  ", &arch_lines));
        lines.push("end".to_string());
        lines
    }
}

fn arch_block_lines(
    platforms: &HashMap<Platform, PlatformArchive>,
    platform: Platform,
    arch: &str,
) -> Vec<String> {
    platforms.get(&platform).map_or_else(Vec::new, |archive| {
        vec![
            format!("on_{arch} do"),
            format!("  url {}", ruby_string(&archive.url)),
            format!("  sha256 {}", ruby_string(&archive.sha256)),
            "end".to_string(),
        ]
    })
}

fn dependency_lines(dependencies: &[String]) -> Vec<String> {
    dependencies
        .iter()
        .map(|dependency| format!("depends_on {}", ruby_string(dependency)))
        .collect()
}

fn install_lines(binaries: &[String]) -> Vec<String> {
    let mut lines = vec!["def install".to_string()];
    lines.extend(
        binaries
            .iter()
            .map(|binary| format!("  bin.install {}", ruby_string(binary))),
    );
    lines.push("end".to_string());
    lines
}

fn indent_lines(prefix: &str, lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .map(|line| {
            if line.is_empty() {
                String::new()
            } else {
                format!("{prefix}{line}")
            }
        })
        .collect()
}

fn join_blocks(blocks: Vec<Vec<String>>) -> Vec<String> {
    let mut joined = Vec::new();
    for block in blocks.into_iter().filter(|block| !block.is_empty()) {
        if !joined.is_empty() {
            joined.push(String::new());
        }
        joined.extend(block);
    }
    joined
}

fn class_name(name: &str) -> String {
    name.split('-').map(upper_first).collect()
}

fn upper_first(part: &str) -> String {
    let mut chars = part.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    first.to_uppercase().chain(chars).collect()
}

fn ruby_string(value: &str) -> String {
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if ch.is_control() => {
                write!(out, "\\u{:04x}", u32::from(ch)).expect("write to String cannot fail");
            }
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_lowercase()
        && chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
}

fn valid_version(version: &str) -> bool {
    !version.is_empty()
        && version
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '+' | '~' | '_' | '-'))
}

fn formula_path(tap: &Path, name: &str) -> PathBuf {
    tap.join("Formula").join(format!("{name}.rb"))
}

fn push_formula(tap: &Path, name: &str, formula_path: &Path, commit_message: &str) -> Result<()> {
    guard_clean_for_push(tap, name)?;
    git_add(tap, formula_path)?;

    if git_status_for_path(tap, &format!("Formula/{name}.rb"))?
        .trim()
        .is_empty()
    {
        bail!("no formula changes to commit");
    }

    git(tap, &["commit", "-m", commit_message])?;
    let default_branch = origin_default_branch(tap)?;
    git(tap, &["push", "origin", &format!("HEAD:{default_branch}")])?;
    Ok(())
}

fn guard_clean_for_push(tap: &Path, name: &str) -> Result<()> {
    let allowed = format!("Formula/{name}.rb");
    let dirty = git(tap, &["status", "--porcelain"])?;
    let unrelated: Vec<&str> = dirty
        .lines()
        .filter(|line| !status_line_is_for_path(line, &allowed))
        .collect();

    if !unrelated.is_empty() {
        bail!(
            "tap working tree has unrelated changes:\n{}",
            unrelated.join("\n")
        );
    }

    Ok(())
}

fn status_line_is_for_path(line: &str, allowed: &str) -> bool {
    let Some(path) = line.get(3..) else {
        return false;
    };
    path == allowed || path.ends_with(&format!(" -> {allowed}"))
}

fn git_add(tap: &Path, formula_path: &Path) -> Result<()> {
    let status = Command::new("git")
        .arg("-C")
        .arg(tap)
        .arg("add")
        .arg(formula_path)
        .status()
        .with_context(|| format!("running git add in {}", tap.display()))?;
    if !status.success() {
        bail!("git add failed with status {status}");
    }
    Ok(())
}

fn git_status_for_path(tap: &Path, path: &str) -> Result<String> {
    git(tap, &["status", "--porcelain", "--", path])
}

fn origin_default_branch(tap: &Path) -> Result<String> {
    let symbolic = git(
        tap,
        &[
            "symbolic-ref",
            "--quiet",
            "--short",
            "refs/remotes/origin/HEAD",
        ],
    )
    .context("detecting origin default branch from refs/remotes/origin/HEAD")?;
    symbolic
        .trim()
        .strip_prefix("origin/")
        .map(str::to_string)
        .filter(|branch| !branch.is_empty())
        .context("origin/HEAD did not resolve to origin/<branch>")
}

fn git(tap: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(tap)
        .args(args)
        .output()
        .with_context(|| format!("running git {} in {}", args.join(" "), tap.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git {} failed: {}", args.join(" "), stderr.trim());
    }

    String::from_utf8(output.stdout).context("git output was not valid UTF-8")
}

impl Platform {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "darwin_arm" => Ok(Self::DarwinArm),
            "darwin_intel" => Ok(Self::DarwinIntel),
            "linux_arm" => Ok(Self::LinuxArm),
            "linux_intel" => Ok(Self::LinuxIntel),
            other => Err(format!(
                "unknown platform {other}; expected darwin_arm, darwin_intel, linux_arm, or linux_intel"
            )),
        }
    }

    fn key(self) -> &'static str {
        match self {
            Self::DarwinArm => "darwin_arm",
            Self::DarwinIntel => "darwin_intel",
            Self::LinuxArm => "linux_arm",
            Self::LinuxIntel => "linux_intel",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_archive() -> tempfile::NamedTempFile {
        let file = tempfile::NamedTempFile::new().expect("fixture archive");
        fs::write(file.path(), b"hello\n").expect("write fixture");
        file
    }

    #[test]
    fn computes_sha256_from_file_stream() {
        let file = fixture_archive();
        assert_eq!(
            sha256_file(file.path()).expect("sha256"),
            "5891b5b522d5df086d0ff0b110fbd9d21bb4fc7163af34d08286a2e846f6be03"
        );
    }

    #[test]
    fn class_name_matches_nix_formula_helper() {
        assert_eq!(class_name("modde"), "Modde");
        assert_eq!(class_name("my-app"), "MyApp");
        assert_eq!(class_name("harbor-rs"), "HarborRs");
    }

    #[test]
    fn archive_parser_accepts_valid_specs() {
        let spec = parse_archive_spec("darwin_arm=https://example.com/app.tar.gz,/tmp/app.tar.gz")
            .expect("valid archive");
        assert_eq!(spec.platform, Platform::DarwinArm);
        assert_eq!(spec.url, "https://example.com/app.tar.gz");
        assert_eq!(spec.local_path, PathBuf::from("/tmp/app.tar.gz"));
    }

    #[test]
    fn archive_parser_rejects_unknown_platform() {
        let err = parse_archive_spec("freebsd_intel=https://example.com/app.tar.gz,/tmp/app")
            .expect_err("invalid platform");
        assert!(err.contains("unknown platform"));
    }

    #[test]
    fn validation_rejects_duplicate_platforms() {
        let file = fixture_archive();
        let args = BumpArgs {
            name: "modde".to_string(),
            version: "1.0.0".to_string(),
            description: "Cross-platform game mod manager".to_string(),
            homepage: "https://example.com".to_string(),
            license: "MIT".to_string(),
            archive: vec![
                ArchiveSpec {
                    platform: Platform::LinuxIntel,
                    url: "https://example.com/one.tar.gz".to_string(),
                    local_path: file.path().to_path_buf(),
                },
                ArchiveSpec {
                    platform: Platform::LinuxIntel,
                    url: "https://example.com/two.tar.gz".to_string(),
                    local_path: file.path().to_path_buf(),
                },
            ],
            binary: Vec::new(),
            dependencies: Vec::new(),
            tap: None,
            stdout: true,
            push: false,
            commit_message: None,
        };
        let err = validate_args(&args).expect_err("duplicate platform");
        assert!(err.to_string().contains("duplicate archive"));
    }

    #[test]
    fn formula_contains_platform_blocks() {
        let formula = Formula {
            name: "multi-app".to_string(),
            version: "1.0.0".to_string(),
            description: "Multi platform app".to_string(),
            homepage: "https://example.com".to_string(),
            license: "Apache-2.0".to_string(),
            platforms: HashMap::from([
                (
                    Platform::DarwinArm,
                    PlatformArchive {
                        url: "https://example.com/darwin.tar.gz".to_string(),
                        sha256: "a".repeat(64),
                    },
                ),
                (
                    Platform::LinuxIntel,
                    PlatformArchive {
                        url: "https://example.com/linux.tar.gz".to_string(),
                        sha256: "b".repeat(64),
                    },
                ),
            ]),
            dependencies: vec!["openssl@3".to_string()],
            binaries: vec!["multi-app".to_string()],
        };

        let rendered = render_formula(&formula);
        assert!(rendered.contains("class MultiApp < Formula"));
        assert!(rendered.contains("on_macos do"));
        assert!(rendered.contains("on_linux do"));
        assert!(rendered.contains("on_arm do"));
        assert!(rendered.contains("on_intel do"));
        assert!(rendered.contains("depends_on \"openssl@3\""));
    }

    #[test]
    fn validation_rejects_long_description() {
        let file = fixture_archive();
        let args = BumpArgs {
            name: "modde".to_string(),
            version: "1.0.0".to_string(),
            description: "This description is deliberately longer than eighty characters so validation rejects it."
                .to_string(),
            homepage: "https://example.com".to_string(),
            license: "MIT".to_string(),
            archive: vec![ArchiveSpec {
                platform: Platform::LinuxIntel,
                url: "https://example.com/app.tar.gz".to_string(),
                local_path: file.path().to_path_buf(),
            }],
            binary: Vec::new(),
            dependencies: Vec::new(),
            tap: None,
            stdout: true,
            push: false,
            commit_message: None,
        };
        let err = validate_args(&args).expect_err("long desc");
        assert!(err.to_string().contains("80 characters"));
    }
}
