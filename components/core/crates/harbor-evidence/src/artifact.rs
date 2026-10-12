use std::{collections::{BTreeMap, BTreeSet}, fs::File, io::{self, Read}, path::{Component, Path, PathBuf}, time::Duration};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileIdentity {
    pub path: String,
    pub sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub root: PathBuf,
    pub files: Vec<FileIdentity>,
}

/// Schema one stays compatible with existing accepted Python-generated outputs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: u32,
    pub artifacts: BTreeMap<String, Artifact>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required_tests: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tests: Option<Vec<Value>>,
}

pub(crate) fn digest(reader: &mut impl Read) -> io::Result<String> {
    let mut hash = Sha256::new();
    let mut buffer = [0; 8192];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 { break; }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn files(root: &Path) -> Result<Vec<FileIdentity>> {
    ensure!(root.is_dir(), "artifact is not a directory: {}", root.display());
    let mut result = Vec::new();
    for entry in WalkDir::new(root).min_depth(1).into_iter().filter_entry(|entry| {
        entry.depth() != 1 || entry.file_name() != ".harbor"
    }) {
        let entry = entry.context("walking artifact tree")?;
        // Like the original inventory, file symlinks bind their target bytes;
        // directory symlinks are not recursively traversed.
        if !entry.path().is_file() { continue; }
        let relative = entry.path().strip_prefix(root)?;
        let path = relative.to_str().context("artifact path must be UTF-8")?.replace(std::path::MAIN_SEPARATOR, "/");
        let sha256 = digest(&mut File::open(entry.path())?).context("hashing artifact file")?;
        result.push(FileIdentity { path, sha256 });
    }
    // Python's Path ordering compares components, not the whole path string.
    // Keep that ordering so nested/sibling assets match old schema-one manifests.
    result.sort_by(|a, b| Path::new(&a.path).cmp(Path::new(&b.path)));
    ensure!(!result.is_empty(), "artifact has no files: {}", root.display());
    Ok(result)
}

pub fn snapshot(roots: &BTreeMap<String, PathBuf>) -> Result<Manifest> {
    ensure!(roots.contains_key("artifact"), "artifact root is required");
    let mut artifacts = BTreeMap::new();
    for (name, root) in roots {
        ensure!(!name.is_empty(), "artifact name is empty");
        artifacts.insert(name.clone(), Artifact { root: root.clone(), files: files(root)? });
    }
    Ok(Manifest { schema_version: 1, artifacts, required_tests: None, tests: None })
}

/// Reports may retain caller metadata, but every result must be a unique PASS.
pub fn validate_report(report: &Value, required: &[String]) -> Result<Vec<Value>> {
    let tests = report.get("tests").and_then(Value::as_array).context("acceptance report contains no tests")?;
    ensure!(!tests.is_empty(), "acceptance report contains no tests");
    let mut seen = BTreeSet::new();
    for test in tests {
        let identifier = test.get("id").and_then(Value::as_str).context("invalid acceptance test ID")?;
        ensure!(!identifier.is_empty() && seen.insert(identifier), "invalid or duplicate acceptance test: {identifier}");
        ensure!(test.get("status").and_then(Value::as_str) == Some("passed"), "acceptance run did not pass: {identifier}");
    }
    let mut required_seen = BTreeSet::new();
    for identifier in required {
        ensure!(!identifier.is_empty() && required_seen.insert(identifier), "invalid or duplicate required test ID");
        ensure!(seen.contains(identifier.as_str()), "missing required acceptance test: {identifier}");
    }
    Ok(tests.clone())
}

fn validate_identity(manifest: &Manifest) -> Result<()> {
    ensure!(manifest.schema_version == 1, "unsupported artifact identity schema");
    ensure!(manifest.artifacts.contains_key("artifact"), "manifest has no artifact identity");
    for artifact in manifest.artifacts.values() {
        ensure!(!artifact.files.is_empty(), "manifest has no files");
        let mut seen = BTreeSet::new();
        for entry in &artifact.files {
            ensure!(!entry.path.is_empty() && !entry.path.contains('\\') && Path::new(&entry.path).components().all(|component| matches!(component, Component::Normal(_))), "unsafe artifact path");
            ensure!(seen.insert(&entry.path), "duplicate artifact path");
            ensure!(entry.sha256.len() == 64 && entry.sha256.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)), "invalid artifact digest");
        }
    }
    Ok(())
}

pub fn accept(mut manifest: Manifest, report: &Value, required: &[String]) -> Result<Manifest> {
    ensure!(!required.is_empty(), "manifest has no required acceptance contract");
    ensure!(manifest.required_tests.is_none() && manifest.tests.is_none(), "manifest is already accepted; create a fresh snapshot for another run");
    validate_identity(&manifest)?;
    let tests = validate_report(report, required)?;
    for artifact in manifest.artifacts.values() {
        ensure!(files(&artifact.root)? == artifact.files, "artifacts changed during acceptance");
    }
    manifest.required_tests = Some(required.to_vec());
    manifest.tests = Some(tests);
    Ok(manifest)
}

/// Library inventory verification is also useful before behavioral acceptance.
/// The CLI additionally requires an accepted report and nonempty required IDs.
pub fn verify_local(manifest: &Manifest, root: &Path, backend: Option<&Path>) -> Result<()> {
    validate_identity(manifest)?;
    ensure!(files(root)? == manifest.artifacts["artifact"].files, "artifact changed after acceptance: {}", root.display());
    if let Some(backend) = backend {
        let expected = manifest.artifacts.get("backend").context("backend does not match acceptance")?;
        ensure!(backend == expected.root, "backend does not match acceptance: {}", backend.display());
        ensure!(files(backend)? == expected.files, "backend changed after acceptance: {}", backend.display());
    }
    Ok(())
}

/// Read-only HTTP checks refuse redirects and hash streamed response bytes.
pub fn verify_http(manifest: &Manifest, base_url: &str, prefix: &str) -> Result<()> {
    validate_identity(manifest)?;
    let base = url::Url::parse(base_url).context("invalid HTTP origin")?;
    ensure!(matches!(base.scheme(), "http" | "https") && base.host_str().is_some(), "HTTP verification requires an http(s) origin");
    ensure!(base.query().is_none() && base.fragment().is_none(), "HTTP origin must not contain a query or fragment");
    let selected = manifest.artifacts["artifact"].files.iter().filter(|entry| entry.path == "index.html" || entry.path.starts_with(prefix)).collect::<Vec<_>>();
    ensure!(selected.iter().any(|entry| entry.path.starts_with(prefix)), "manifest has no deployed assets under {prefix}");
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .max_redirects(0).http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(15))).build().into();
    for entry in selected {
        let mut url = base.clone();
        url.path_segments_mut().map_err(|()| anyhow::anyhow!("origin cannot be a base URL"))?
            .pop_if_empty().extend(entry.path.split('/'));
        let mut response = agent.get(url.as_str()).call().context("requesting deployed artifact")?;
        ensure!(response.status().as_u16() == 200, "asset redirected or unavailable: {}", entry.path);
        let actual = digest(&mut response.body_mut().as_reader()).context("hashing deployed artifact")?;
        ensure!(actual == entry.sha256, "served asset differs from accepted artifact: {}", entry.path);
    }
    Ok(())
}
