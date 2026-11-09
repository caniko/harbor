//! Move one root flake input to a new revision: relock the root, align every
//! component lock, commit each component branch, and bump the gitlinks.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::align;

#[derive(Debug, Clone)]
pub struct BumpOptions {
    pub root: PathBuf,
    pub input: String,
    pub rev: String,
    pub branch: Option<String>,
    pub push: bool,
    pub pr: bool,
    pub plan: bool,
}

#[derive(Debug, Serialize)]
pub struct BumpReport {
    pub input: String,
    pub rev: String,
    pub plan: bool,
    pub root: RootReport,
    pub components: Vec<ComponentReport>,
    pub superproject: SuperprojectReport,
}

#[derive(Debug, Serialize)]
pub struct RootReport {
    pub old: String,
    pub new: String,
}

#[derive(Debug, Serialize)]
pub struct ComponentReport {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pr_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SuperprojectReport {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pr_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<String>>,
}

pub fn validate_rev(rev: &str) -> Result<()> {
    if rev.len() == 40
        && rev
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        bail!("revision {rev:?} is not a 40-character lowercase hex commit id")
    }
}

/// Replace the locked revision in the `INPUT.url` declaration only.
///
/// Other inputs in the same flake are left alone even when they share the old
/// revision.
fn replace_input_rev(text: &str, input: &str, old: &str, new: &str) -> Result<String> {
    let input_url = format!("{input}.url");
    let nested_url = format!("inputs.{input}.url");
    let mut replaced = false;
    let mut output = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with(&input_url) || trimmed.starts_with(&nested_url) {
            if !line.contains(old) {
                bail!(
                    "root flake.nix declares {input}.url without the locked revision {old}; pin the URL to a revision before bumping"
                );
            }
            output.push_str(&line.replace(old, new));
            replaced = true;
        } else {
            output.push_str(line);
        }
    }
    if replaced {
        return Ok(output);
    }
    bail!("root flake.nix does not declare {input}.url; expected a URL pinned to {old}")
}

fn root_lock_old_rev(root_lock: &serde_json::Value, input: &str) -> Result<String> {
    let reference = root_lock
        .get("nodes")
        .and_then(serde_json::Value::as_object)
        .and_then(|nodes| nodes.get("root"))
        .and_then(|node| node.get("inputs"))
        .and_then(serde_json::Value::as_object)
        .and_then(|inputs| inputs.get(input))
        .and_then(serde_json::Value::as_str)
        .with_context(|| format!("root flake.lock does not declare input {input:?}"))?;
    root_lock
        .get("nodes")
        .and_then(serde_json::Value::as_object)
        .and_then(|nodes| nodes.get(reference))
        .and_then(|node| node.get("locked"))
        .and_then(|locked| locked.get("rev"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .with_context(|| {
            format!("root flake.lock input {input:?} does not pin a revision (node {reference})")
        })
}

fn default_branch(repo: &Path) -> Result<String> {
    let reference = git_ok(
        repo,
        &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
    )
    .with_context(|| {
        format!(
            "{} has no origin/HEAD; run `git remote set-head origin -a`",
            repo.display()
        )
    })?;
    reference
        .strip_prefix("origin/")
        .map(str::to_owned)
        .with_context(|| {
            format!(
                "{} origin/HEAD is not a remote branch: {reference}",
                repo.display()
            )
        })
}

fn default_branch_name(input: &str, rev: &str) -> String {
    format!("build/{input}-{}", &rev[..7])
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).trim().to_owned()
}

fn git(dir: &Path, args: &[&str]) -> Result<Output> {
    Command::new("git")
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("run git {} in {}", args.join(" "), dir.display()))
}

fn git_ok(dir: &Path, args: &[&str]) -> Result<String> {
    let output = git(dir, args)?;
    if !output.status.success() {
        bail!(
            "git {} failed in {}: {}",
            args.join(" "),
            dir.display(),
            stderr(&output)
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn git_optional(dir: &Path, args: &[&str]) -> Result<Option<String>> {
    let output = git(dir, args)?;
    if output.status.success() {
        return Ok(Some(
            String::from_utf8_lossy(&output.stdout).trim().to_owned(),
        ));
    }
    if output.status.code() == Some(1) {
        return Ok(None);
    }
    bail!(
        "git {} failed in {}: {}",
        args.join(" "),
        dir.display(),
        stderr(&output)
    );
}

fn git_exit(dir: &Path, args: &[&str]) -> Result<i32> {
    let output = git(dir, args)?;
    if let Some(code) = output.status.code() {
        return Ok(code);
    }
    bail!(
        "git {} was terminated in {}: {}",
        args.join(" "),
        dir.display(),
        stderr(&output)
    )
}

fn gh(dir: &Path, args: &[&str]) -> Result<Output> {
    Command::new("gh")
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("run gh {} in {}", args.join(" "), dir.display()))
}

fn commit_paths(repo: &Path, subject: &str, body: &str, paths: &[String]) -> Result<String> {
    let mut args = vec!["commit", "-m", subject, "-m", body, "--"];
    args.extend(paths.iter().map(String::as_str));
    git_ok(repo, &args)?;
    git_ok(repo, &["rev-parse", "HEAD"])
}

fn existing_pr(repo: &Path, branch: &str) -> Result<Option<String>> {
    let output = gh(repo, &["pr", "view", branch, "--json", "url"])?;
    if !output.status.success() {
        return Ok(None);
    }
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).context("parse gh pr view output")?;
    Ok(value
        .get("url")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned))
}

fn open_pr(repo: &Path, branch: &str, title: &str, body: &str) -> Result<String> {
    if let Some(url) = existing_pr(repo, branch)? {
        return Ok(url);
    }
    let base = default_branch(repo)?;
    let output = gh(
        repo,
        &[
            "pr", "create", "--base", &base, "--head", branch, "--title", title, "--body", body,
        ],
    )?;
    if !output.status.success() {
        bail!(
            "gh pr create failed in {}: {}",
            repo.display(),
            stderr(&output)
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn ensure_submodules(root: &Path) -> Result<()> {
    let status = git_ok(root, &["submodule", "status"])?;
    if !status.lines().any(|line| line.starts_with('-')) {
        return Ok(());
    }
    git_ok(root, &["submodule", "update", "--init"])
        .context("initialize harbor submodules before bumping")?;
    Ok(())
}

fn run_nix_flake_lock(root: &Path) -> Result<()> {
    let output = Command::new("nix")
        .args(["flake", "lock"])
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .output()
        .context("run nix flake lock")?;
    if !output.status.success() {
        bail!(
            "nix flake lock failed in {}: {}",
            root.display(),
            stderr(&output)
        );
    }
    Ok(())
}

fn component_body(input: &str, old: &str, new: &str) -> String {
    format!("{input}: {old} -> {new}")
}

struct CommitContext<'a> {
    root: &'a Path,
    input: &'a str,
    old: &'a str,
    new: &'a str,
    branch: &'a str,
    publish: bool,
    pr: bool,
}

fn commit_component(context: &CommitContext<'_>, name: &str) -> Result<ComponentReport> {
    let CommitContext {
        root,
        input,
        old,
        new,
        branch,
        publish,
        pr,
    } = *context;
    let dir = root.join("components").join(name);
    let branch_ref = format!("refs/heads/{branch}");
    let subject = format!("build(nix): align {input} to harbor root {}", &new[..7]);
    let body = component_body(input, old, new);

    let commit = if git_optional(&dir, &["rev-parse", "--verify", "--quiet", &branch_ref])?
        .is_some()
    {
        for path in ["flake.lock", "flake.nix"] {
            let working = git_ok(&dir, &["hash-object", "--", path])?;
            let committed = git_optional(
                &dir,
                &[
                    "rev-parse",
                    "--verify",
                    "--quiet",
                    &format!("{branch_ref}:{path}"),
                ],
            )?;
            if committed.as_deref() != Some(working.as_str()) {
                bail!(
                    "component {name}: branch {branch} already exists with a different tree; refusing to reuse it"
                );
            }
        }
        let head = git_optional(&dir, &["symbolic-ref", "--quiet", "HEAD"])?;
        if head.as_deref() != Some(branch_ref.as_str()) {
            git_ok(&dir, &["checkout", branch])
                .with_context(|| format!("component {name}: switch to branch {branch}"))?;
        }
        git_ok(&dir, &["rev-parse", &branch_ref])?
    } else {
        git_ok(&dir, &["checkout", "-b", branch])
            .with_context(|| format!("component {name}: create branch {branch}"))?;
        git_ok(&dir, &["add", "--", "flake.lock", "flake.nix"])
            .with_context(|| format!("component {name}: stage aligned locks"))?;
        commit_paths(
            &dir,
            &subject,
            &body,
            &["flake.lock".to_owned(), "flake.nix".to_owned()],
        )
        .with_context(|| format!("component {name}: commit aligned locks"))?
    };

    if publish {
        git_ok(&dir, &["push", "-u", "origin", branch])
            .with_context(|| format!("component {name}: push branch {branch}"))?;
    }
    let pr_url = if pr {
        Some(
            open_pr(&dir, branch, &subject, &body)
                .with_context(|| format!("component {name}: open pull request for {branch}"))?,
        )
    } else {
        None
    };

    Ok(ComponentReport {
        name: name.to_owned(),
        change: Some(format!("{input}: {old} -> {new}")),
        commit: Some(commit),
        branch: Some(branch.to_owned()),
        pr_url,
    })
}

fn existing_branch_components(
    context: &CommitContext<'_>,
    known: &[ComponentReport],
) -> Result<Vec<ComponentReport>> {
    let CommitContext {
        root,
        input,
        old,
        new,
        branch,
        publish,
        pr,
    } = *context;
    let branch_ref = format!("refs/heads/{branch}");
    let mut found = Vec::new();
    for (name, dir) in align::component_dirs(root)? {
        if known.iter().any(|component| component.name == name) {
            continue;
        }
        let Some(tip) = git_optional(&dir, &["rev-parse", "--verify", "--quiet", &branch_ref])?
        else {
            continue;
        };
        let head = git_ok(&dir, &["rev-parse", "HEAD"])?;
        if tip != head {
            continue;
        }
        if publish {
            git_ok(&dir, &["push", "-u", "origin", branch])
                .with_context(|| format!("component {name}: push branch {branch}"))?;
        }
        let subject = format!("build(nix): align {input} to harbor root {}", &new[..7]);
        let body = component_body(input, old, new);
        let pr_url = if pr {
            Some(
                open_pr(&dir, branch, &subject, &body)
                    .with_context(|| format!("component {name}: open pull request for {branch}"))?,
            )
        } else {
            None
        };
        found.push(ComponentReport {
            name,
            change: Some(format!("{input}: {old} -> {new}")),
            commit: Some(head),
            branch: Some(branch.to_owned()),
            pr_url,
        });
    }
    Ok(found)
}

fn superproject_body(input: &str, old: &str, new: &str, components: &[ComponentReport]) -> String {
    use std::fmt::Write as _;

    let mut body = format!("{input}: {old} -> {new}\n\ncomponents:\n");
    for component in components {
        if let Some(commit) = &component.commit {
            let _ = writeln!(body, "- {}: {commit}", component.name);
        }
    }
    body
}

fn update_superproject(
    context: &CommitContext<'_>,
    components: &[ComponentReport],
) -> Result<SuperprojectReport> {
    let CommitContext {
        root,
        input,
        old,
        new,
        branch,
        publish,
        pr,
    } = *context;
    let branch_ref = format!("refs/heads/{branch}");
    let current = git_optional(root, &["symbolic-ref", "--short", "HEAD"])?;
    let default = default_branch(root)?;
    if current.as_deref() == Some(default.as_str()) {
        if git_optional(root, &["rev-parse", "--verify", "--quiet", &branch_ref])?.is_some() {
            git_ok(root, &["checkout", branch])?;
        } else {
            git_ok(root, &["checkout", "-b", branch])?;
        }
    }

    let mut paths = vec!["flake.nix".to_owned(), "flake.lock".to_owned()];
    paths.extend(
        components
            .iter()
            .map(|component| format!("components/{}", component.name)),
    );
    let mut add = vec!["add", "--"];
    add.extend(paths.iter().map(String::as_str));
    git_ok(root, &add).context("stage root pin and component gitlinks")?;

    let subject = format!("build(nix): bump {input} to {}", &new[..7]);
    let body = superproject_body(input, old, new, components);
    let commit = if git_exit(root, &["diff", "--cached", "--quiet"])? == 0 {
        git_ok(root, &["rev-parse", "HEAD"])?
    } else {
        commit_paths(root, &subject, &body, &paths).context("commit root pin and gitlinks")?
    };

    if publish {
        git_ok(root, &["push", "-u", "origin", branch]).context("push harbor branch")?;
    }
    let pr_url = if pr {
        Some(open_pr(root, branch, &subject, &body)?)
    } else {
        None
    };

    Ok(SuperprojectReport {
        commit: Some(commit),
        branch: Some(branch.to_owned()),
        pr_url,
        files: None,
    })
}

pub fn bump(options: &BumpOptions) -> Result<BumpReport> {
    validate_rev(&options.rev)?;
    let root = options.root.as_path();
    let flake_path = root.join("flake.nix");
    let root_lock = align::read_json(&root.join("flake.lock"))?;
    let old = root_lock_old_rev(&root_lock, &options.input)?;
    let already_at_rev = old == options.rev;
    let flake = fs::read_to_string(&flake_path)
        .with_context(|| format!("read {}", flake_path.display()))?;
    let updated = replace_input_rev(&flake, &options.input, &old, &options.rev)?;
    let branch = options
        .branch
        .clone()
        .unwrap_or_else(|| default_branch_name(&options.input, &options.rev));

    if options.plan {
        // Surface lock-alignment errors that a real run would hit, without
        // writing anything.
        align::align_locks(root, std::slice::from_ref(&options.input), false)?;
        let mut components = Vec::new();
        let mut files = vec!["flake.nix".to_owned(), "flake.lock".to_owned()];
        for pin in align::direct_pins(root, &options.input)? {
            files.push(format!("components/{}", pin.component));
            components.push(ComponentReport {
                name: pin.component,
                change: Some(format!(
                    "{}: {} -> {}",
                    options.input, pin.current, options.rev
                )),
                commit: None,
                branch: None,
                pr_url: None,
            });
        }
        return Ok(BumpReport {
            input: options.input.clone(),
            rev: options.rev.clone(),
            plan: true,
            root: RootReport {
                old,
                new: options.rev.clone(),
            },
            components,
            superproject: SuperprojectReport {
                commit: None,
                branch: None,
                pr_url: None,
                files: Some(files),
            },
        });
    }

    ensure_submodules(root)?;
    if !already_at_rev {
        fs::write(&flake_path, updated)
            .with_context(|| format!("write {}", flake_path.display()))?;
        run_nix_flake_lock(root)?;
    }
    let aligned = align::align_locks(root, std::slice::from_ref(&options.input), true)?;
    let context = CommitContext {
        root,
        input: &options.input,
        old: &old,
        new: &options.rev,
        branch: &branch,
        publish: options.push || options.pr,
        pr: options.pr,
    };
    let mut components = Vec::with_capacity(aligned.components.len());
    for name in &aligned.components {
        components.push(commit_component(&context, name)?);
    }
    // A previous run may have committed every component and stopped before the
    // gitlink bump. Reuse those branches instead of failing or re-committing.
    components.extend(existing_branch_components(&context, &components)?);
    let superproject = update_superproject(&context, &components)?;

    Ok(BumpReport {
        input: options.input.clone(),
        rev: options.rev.clone(),
        plan: false,
        root: RootReport {
            old,
            new: options.rev.clone(),
        },
        components,
        superproject,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const OLD: &str = "1111111111111111111111111111111111111111";
    const NEW: &str = "2222222222222222222222222222222222222222";

    #[test]
    fn rev_must_be_forty_lowercase_hex_characters() {
        assert!(validate_rev(NEW).is_ok());
        assert!(validate_rev("short").is_err());
        assert!(validate_rev(&"A".repeat(40)).is_err());
        assert!(validate_rev(&format!("{}g", &NEW[..39])).is_err());
    }

    #[test]
    fn only_the_named_input_url_is_rewritten() {
        let flake = format!(
            "{{\n  inputs = {{\n    nixpkgs.url = \"github:NixOS/nixpkgs/{OLD}\";\n    other.url = \"github:foo/bar/{OLD}\";\n  }};\n}}\n"
        );
        let updated = replace_input_rev(&flake, "nixpkgs", OLD, NEW).unwrap();
        assert!(updated.contains(&format!("nixpkgs/{NEW}")));
        assert!(updated.contains(&format!("foo/bar/{OLD}")));
    }

    #[test]
    fn missing_old_revision_is_an_error() {
        let error = replace_input_rev("{}", "nixpkgs", OLD, NEW).unwrap_err();
        assert!(error.to_string().contains("does not declare"), "{error:#}");
    }
}
