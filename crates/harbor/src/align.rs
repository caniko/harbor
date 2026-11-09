//! Align every component lock with the root input graph.
//!
//! This is the Rust port of the former `scripts/align-component-locks`: for
//! each `components/<name>/flake.lock`, every root input the component also
//! locks directly (a string reference, not a `follows` edge) receives the root
//! node's `locked` attributes. A component input pinned by revision in its
//! `flake.nix` URL also gets that revision replaced there and in `original`.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde_json::{Map, Value};

#[derive(Debug, Default)]
pub struct AlignReport {
    /// Human-readable drift entries, `component/input: old -> new`.
    pub changed: Vec<String>,
    /// Components whose lock or flake changed.
    pub components: Vec<String>,
}

impl AlignReport {
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.changed.is_empty()
    }
}

/// A directly pinned component input, used by `bump --plan`.
#[derive(Debug)]
pub struct DirectPin {
    pub component: String,
    pub current: String,
}

pub fn read_json(path: &Path) -> Result<Value> {
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parse {}", path.display()))
}

fn write_json(path: &Path, value: &Value) -> Result<()> {
    // serde_json's default map sorts keys; Nix writes locks the same way.
    let mut text = serde_json::to_string_pretty(value)
        .with_context(|| format!("serialize {}", path.display()))?;
    text.push('\n');
    fs::write(path, text).with_context(|| format!("write {}", path.display()))
}

fn node<'a>(lock: &'a Value, reference: &str, path: &Path) -> Result<&'a Map<String, Value>> {
    lock.get("nodes")
        .and_then(Value::as_object)
        .and_then(|nodes| nodes.get(reference))
        .and_then(Value::as_object)
        .with_context(|| format!("{} has no node {reference}", path.display()))
}

fn node_mut<'a>(
    lock: &'a mut Value,
    reference: &str,
    path: &Path,
) -> Result<&'a mut Map<String, Value>> {
    lock.get_mut("nodes")
        .and_then(Value::as_object_mut)
        .and_then(|nodes| nodes.get_mut(reference))
        .and_then(Value::as_object_mut)
        .with_context(|| format!("{} has no node {reference}", path.display()))
}

fn pin(node: &Map<String, Value>) -> Option<&str> {
    node.get("rev")
        .and_then(Value::as_str)
        .or_else(|| node.get("narHash").and_then(Value::as_str))
}

pub(crate) fn component_dirs(root: &Path) -> Result<Vec<(String, PathBuf)>> {
    let components = root.join("components");
    let mut found = Vec::new();
    if !components.is_dir() {
        return Ok(found);
    }
    for entry in
        fs::read_dir(&components).with_context(|| format!("read {}", components.display()))?
    {
        let entry = entry.with_context(|| format!("read {}", components.display()))?;
        let path = entry.path();
        if !path.is_dir() || !path.join("flake.lock").is_file() || !path.join("flake.nix").is_file()
        {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        found.push((name, path));
    }
    found.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(found)
}

fn root_inputs(root_lock: &Value) -> Result<&Map<String, Value>> {
    root_lock
        .get("nodes")
        .and_then(Value::as_object)
        .and_then(|nodes| nodes.get("root"))
        .and_then(|root| root.get("inputs"))
        .and_then(Value::as_object)
        .context("root flake.lock has no nodes.root.inputs")
}

/// Align component locks with the root lock.
///
/// With `apply = false` nothing is written; the same drift is reported.
pub fn align_locks(root: &Path, selected: &[String], apply: bool) -> Result<AlignReport> {
    let root_lock_path = root.join("flake.lock");
    let root_lock = read_json(&root_lock_path)?;
    let root_inputs = root_inputs(&root_lock)?;
    let mut report = AlignReport::default();

    for (name, dir) in component_dirs(root)? {
        if let Some(changed) = align_component(
            &name,
            &dir,
            &root_lock,
            &root_lock_path,
            root_inputs,
            selected,
            apply,
        )? {
            report.changed.extend(changed);
            report.components.push(name);
        }
    }

    Ok(report)
}

fn align_component(
    name: &str,
    dir: &Path,
    root_lock: &Value,
    root_lock_path: &Path,
    root_inputs: &Map<String, Value>,
    selected: &[String],
    apply: bool,
) -> Result<Option<Vec<String>>> {
    let lock_path = dir.join("flake.lock");
    let flake_path = dir.join("flake.nix");
    let mut lock = read_json(&lock_path)?;
    let mut flake_text = fs::read_to_string(&flake_path)
        .with_context(|| format!("read {}", flake_path.display()))?;
    let original_flake = flake_text.clone();
    let inputs = lock
        .get("nodes")
        .and_then(Value::as_object)
        .and_then(|nodes| nodes.get("root"))
        .and_then(|root| root.get("inputs"))
        .and_then(Value::as_object)
        .cloned()
        .with_context(|| format!("{} has no nodes.root.inputs", lock_path.display()))?;
    let mut input_names: Vec<&String> = inputs.keys().collect();
    input_names.sort();
    let mut changed = Vec::new();

    for input in input_names {
        if !selected.is_empty() && !selected.iter().any(|wanted| wanted == input) {
            continue;
        }
        // A non-string reference is a follows edge and inherits its target.
        let Some(reference) = inputs[input].as_str() else {
            continue;
        };
        let Some(root_reference) = root_inputs.get(input).and_then(Value::as_str) else {
            continue;
        };
        let want = node(root_lock, root_reference, root_lock_path)?
            .get("locked")
            .and_then(Value::as_object)
            .cloned()
            .with_context(|| format!("root flake.lock node {root_reference} has no locked data"))?;
        let have_node = node(&lock, reference, &lock_path)?;
        let have = have_node
            .get("locked")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        if pin(&have) == pin(&want) {
            continue;
        }

        let old_rev = have.get("rev").and_then(Value::as_str);
        let new_rev = want.get("rev").and_then(Value::as_str);
        let pinned_by_url = have_node
            .get("original")
            .and_then(Value::as_object)
            .is_some_and(|original| original.contains_key("rev"));
        if pinned_by_url {
            let (Some(old_rev), Some(new_rev)) = (old_rev, new_rev) else {
                bail!(
                    "{name}/{input}: pinned by URL but {} is not in flake.nix",
                    old_rev.unwrap_or("no old revision")
                );
            };
            if !flake_text.contains(old_rev) {
                bail!("{name}/{input}: pinned by URL but {old_rev} is not in flake.nix");
            }
            flake_text = flake_text.replace(old_rev, new_rev);
        }

        let node = node_mut(&mut lock, reference, &lock_path)?;
        node.insert("locked".to_owned(), Value::Object(want.clone()));
        if pinned_by_url {
            let original = node
                .get_mut("original")
                .and_then(Value::as_object_mut)
                .with_context(|| {
                    format!("{} node {reference} lost original", lock_path.display())
                })?;
            original.insert(
                "rev".to_owned(),
                Value::String(new_rev.unwrap_or_default().to_owned()),
            );
        }
        let before = old_rev
            .or_else(|| have.get("narHash").and_then(Value::as_str))
            .unwrap_or("unknown");
        let after = new_rev
            .or_else(|| want.get("narHash").and_then(Value::as_str))
            .unwrap_or("unknown");
        changed.push(format!("{name}/{input}: {before} -> {after}"));
    }

    if changed.is_empty() {
        return Ok(None);
    }
    if apply {
        write_json(&lock_path, &lock)?;
        if flake_text != original_flake {
            fs::write(&flake_path, flake_text)
                .with_context(|| format!("write {}", flake_path.display()))?;
        }
    }
    Ok(Some(changed))
}

/// List components that pin `input` directly, with their current pin.
pub fn direct_pins(root: &Path, input: &str) -> Result<Vec<DirectPin>> {
    let mut pins = Vec::new();
    for (name, dir) in component_dirs(root)? {
        let lock_path = dir.join("flake.lock");
        let lock = read_json(&lock_path)?;
        let Some(reference) = lock
            .get("nodes")
            .and_then(Value::as_object)
            .and_then(|nodes| nodes.get("root"))
            .and_then(|root| root.get("inputs"))
            .and_then(Value::as_object)
            .and_then(|inputs| inputs.get(input))
            .and_then(Value::as_str)
        else {
            continue;
        };
        let Some(current) = node(&lock, reference, &lock_path)
            .ok()
            .and_then(|node| node.get("locked"))
            .and_then(Value::as_object)
            .and_then(pin)
        else {
            continue;
        };
        pins.push(DirectPin {
            component: name,
            current: current.to_owned(),
        });
    }
    Ok(pins)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const OLD: &str = "1111111111111111111111111111111111111111";
    const NEW: &str = "2222222222222222222222222222222222222222";

    fn write(path: PathBuf, value: impl AsRef<[u8]>) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, value).unwrap();
    }

    fn root_lock(rev: &str) -> Value {
        json!({
            "nodes": {
                "root": {"inputs": {"nixpkgs": "nixpkgs"}},
                "nixpkgs": {
                    "locked": {
                        "lastModified": 1,
                        "narHash": "sha256-root",
                        "owner": "NixOS",
                        "repo": "nixpkgs",
                        "rev": rev,
                        "type": "github"
                    },
                    "original": {
                        "owner": "NixOS",
                        "repo": "nixpkgs",
                        "rev": rev,
                        "type": "github"
                    }
                }
            },
            "root": "root",
            "version": 7
        })
    }

    fn component_lock(rev: &str, pinned_by_url: bool, follows: bool) -> Value {
        let mut node = json!({
            "locked": {
                "lastModified": 1,
                "narHash": "sha256-component",
                "owner": "NixOS",
                "repo": "nixpkgs",
                "rev": rev,
                "type": "github"
            }
        });
        if pinned_by_url {
            node["original"] = json!({
                "owner": "NixOS",
                "repo": "nixpkgs",
                "rev": rev,
                "type": "github"
            });
        }
        let inputs = if follows {
            json!({"nixpkgs": ["rust-overlay", "nixpkgs"]})
        } else {
            json!({"nixpkgs": "nixpkgs"})
        };
        json!({
            "nodes": {"root": {"inputs": inputs}, "nixpkgs": node},
            "root": "root",
            "version": 7
        })
    }

    fn fixture(rev: &str, pinned_by_url: bool, follows: bool, flake: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path().join("flake.lock"),
            serde_json::to_vec(&root_lock(NEW)).unwrap(),
        );
        write(
            dir.path().join("components/demo/flake.lock"),
            serde_json::to_vec(&component_lock(rev, pinned_by_url, follows)).unwrap(),
        );
        write(dir.path().join("components/demo/flake.nix"), flake);
        dir
    }

    #[test]
    fn aligned_lock_is_a_no_op() {
        let dir = fixture(NEW, true, false, "{}");
        let before = fs::read(dir.path().join("components/demo/flake.lock")).unwrap();
        let report = align_locks(dir.path(), &[], true).unwrap();
        assert!(report.is_clean(), "{:?}", report.changed);
        assert!(report.components.is_empty());
        let after = fs::read(dir.path().join("components/demo/flake.lock")).unwrap();
        assert_eq!(before, after);
    }

    #[test]
    fn drifted_direct_input_gets_the_root_pin_and_check_writes_nothing() {
        let dir = fixture(OLD, false, false, "{}");
        let lock_path = dir.path().join("components/demo/flake.lock");
        let before = fs::read(&lock_path).unwrap();

        let report = align_locks(dir.path(), &[], false).unwrap();
        assert_eq!(report.changed, [format!("demo/nixpkgs: {OLD} -> {NEW}")]);
        assert_eq!(report.components, ["demo"]);
        assert_eq!(before, fs::read(&lock_path).unwrap());

        let report = align_locks(dir.path(), &[], true).unwrap();
        assert_eq!(report.changed, [format!("demo/nixpkgs: {OLD} -> {NEW}")]);
        let text = fs::read_to_string(&lock_path).unwrap();
        assert!(text.starts_with("{\n  \"nodes\""), "{text}");
        assert!(text.ends_with("}\n"), "{text}");
        let lock: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(lock["nodes"]["nixpkgs"]["locked"]["rev"], NEW);
        assert_eq!(lock["nodes"]["nixpkgs"]["locked"]["narHash"], "sha256-root");
    }

    #[test]
    fn follows_edge_is_skipped() {
        let dir = fixture(OLD, false, true, "{}");
        let lock_path = dir.path().join("components/demo/flake.lock");
        let before = fs::read(&lock_path).unwrap();
        let report = align_locks(dir.path(), &[], true).unwrap();
        assert!(report.is_clean(), "{:?}", report.changed);
        assert_eq!(before, fs::read(&lock_path).unwrap());
    }

    #[test]
    fn url_pinned_revision_is_replaced_in_flake_and_original() {
        let flake = format!("{{\n  inputs.nixpkgs.url = \"github:NixOS/nixpkgs/{OLD}\";\n}}\n");
        let dir = fixture(OLD, true, false, &flake);
        let report = align_locks(dir.path(), &[], true).unwrap();
        assert_eq!(report.changed, [format!("demo/nixpkgs: {OLD} -> {NEW}")]);
        let updated = fs::read_to_string(dir.path().join("components/demo/flake.nix")).unwrap();
        assert!(updated.contains(NEW));
        assert!(!updated.contains(OLD));
        let lock: Value = read_json(&dir.path().join("components/demo/flake.lock")).unwrap();
        assert_eq!(lock["nodes"]["nixpkgs"]["original"]["rev"], NEW);
        assert_eq!(lock["nodes"]["nixpkgs"]["locked"]["rev"], NEW);
    }

    #[test]
    fn url_pinned_revision_missing_from_flake_is_an_error() {
        let dir = fixture(OLD, true, false, "{}");
        let error = align_locks(dir.path(), &[], true).unwrap_err();
        assert!(error.to_string().contains("pinned by URL"), "{error:#}");
    }
}
