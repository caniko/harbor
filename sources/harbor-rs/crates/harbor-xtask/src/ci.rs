use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use toml::Value;

use crate::{CargoCiOptions, TestRunner};

/// Configuration read from `[workspace.metadata.harbor-ci]` or
/// `[package.metadata.harbor-ci]` in the root Cargo manifest.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarborCiConfig {
    pub packages: Vec<String>,
    pub excludes: Vec<String>,
    pub nextest_args: Vec<String>,
    pub locked: bool,
    pub all_features: bool,
    pub test_runner: TestRunner,
    pub docs: bool,
    pub audit: bool,
    pub deny: bool,
    pub package: bool,
}

impl Default for HarborCiConfig {
    fn default() -> Self {
        Self {
            packages: Vec::new(),
            excludes: Vec::new(),
            nextest_args: Vec::new(),
            locked: true,
            all_features: true,
            test_runner: TestRunner::Cargo,
            docs: false,
            audit: false,
            deny: false,
            package: false,
        }
    }
}

impl HarborCiConfig {
    #[must_use]
    pub fn cargo_options(&self) -> CargoCiOptions {
        CargoCiOptions {
            locked: self.locked,
            all_features: self.all_features,
            test_runner: self.test_runner,
            docs: self.docs,
            audit: self.audit,
            deny: self.deny,
            package: self.package,
        }
    }
}

pub fn load_harbor_ci_config(workspace_root: &Path) -> Result<HarborCiConfig> {
    let manifest_path = workspace_root.join("Cargo.toml");
    let text = fs::read_to_string(&manifest_path)
        .with_context(|| format!("reading {}", manifest_path.display()))?;
    let manifest = text
        .parse::<Value>()
        .with_context(|| format!("parsing {}", manifest_path.display()))?;
    let table = manifest
        .get("workspace")
        .and_then(Value::as_table)
        .and_then(|workspace| workspace.get("metadata"))
        .and_then(Value::as_table)
        .and_then(|metadata| metadata.get("harbor-ci"))
        .and_then(Value::as_table)
        .or_else(|| {
            manifest
                .get("package")
                .and_then(Value::as_table)
                .and_then(|package| package.get("metadata"))
                .and_then(Value::as_table)
                .and_then(|metadata| metadata.get("harbor-ci"))
                .and_then(Value::as_table)
        });

    let Some(table) = table else {
        return Ok(HarborCiConfig::default());
    };

    let mut config = HarborCiConfig {
        packages: string_array(table, "packages")?,
        excludes: string_array(table, "exclude")?,
        nextest_args: string_array(table, "nextest-args")?,
        ..Default::default()
    };
    if !config.packages.is_empty() && !config.excludes.is_empty() {
        bail!("harbor-ci.packages and harbor-ci.exclude cannot both be set");
    }
    config.locked = bool_value(table, "locked", config.locked)?;
    config.all_features = bool_value(table, "all-features", config.all_features)?;
    config.docs = bool_value(table, "docs", config.docs)?;
    config.audit = bool_value(table, "audit", config.audit)?;
    config.deny = bool_value(table, "deny", config.deny)?;
    config.package = bool_value(table, "package", config.package)?;
    if let Some(value) = table.get("test-runner") {
        config.test_runner = match value.as_str() {
            Some("cargo") => TestRunner::Cargo,
            Some("nextest") => TestRunner::Nextest,
            Some(other) => {
                bail!("harbor-ci.test-runner must be `cargo` or `nextest`, got `{other}`")
            }
            None => bail!("harbor-ci.test-runner must be a string"),
        };
    }
    if !config.nextest_args.is_empty() && config.test_runner != TestRunner::Nextest {
        bail!("harbor-ci.nextest-args requires test-runner = `nextest`");
    }
    Ok(config)
}

fn bool_value(table: &toml::map::Map<String, Value>, key: &str, default: bool) -> Result<bool> {
    match table.get(key) {
        None => Ok(default),
        Some(value) => value
            .as_bool()
            .with_context(|| format!("harbor-ci.{key} must be a boolean")),
    }
}

fn string_array(table: &toml::map::Map<String, Value>, key: &str) -> Result<Vec<String>> {
    let Some(value) = table.get(key) else {
        return Ok(Vec::new());
    };
    value
        .as_array()
        .with_context(|| format!("harbor-ci.{key} must be an array of package names"))?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .with_context(|| format!("harbor-ci.{key} must contain only strings"))
        })
        .collect()
}
