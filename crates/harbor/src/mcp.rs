//! MCP surface over one operator-selected Harbor checkout.
//!
//! The root comes only from `--root` at startup. Tool arguments can never
//! select another directory.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use fleetix::mcp_server::rmcp;
use fleetix::mcp_server::{Effect, Server};
use rmcp::handler::server::common::schema_for_input;
use rmcp::handler::server::router::tool::ToolRoute;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, JsonObject, Tool, ToolAnnotations};
use rmcp::{ErrorData, schemars};
use serde::{Deserialize, Serialize};

use crate::align::align_locks;
use crate::bump::{BumpOptions, bump, validate_rev};

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct AlignArgs {
    /// Only align these root inputs (default: every shared input).
    inputs: Option<Vec<String>>,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct BumpPlanArgs {
    /// Root input name, for example `nixpkgs`.
    input: String,
    /// Target 40-character lowercase hex revision.
    rev: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct BumpArgs {
    /// Root input name, for example `nixpkgs`.
    input: String,
    /// Target 40-character lowercase hex revision.
    rev: String,
    /// Component and superproject branch name (default: build/<input>-<rev7>).
    branch: Option<String>,
    /// Push each branch to origin.
    #[serde(default)]
    push: bool,
    /// Open a pull request for each branch.
    #[serde(default)]
    pr: bool,
}

fn tool<T: schemars::JsonSchema + 'static>(
    name: &'static str,
    description: &'static str,
    read_only: bool,
) -> Result<Tool, ErrorData> {
    let schema = schema_for_input::<T>().map_err(|error| ErrorData::internal_error(error, None))?;
    Ok(Tool::new(name, description, schema).with_annotations(
        ToolAnnotations::new()
            .read_only(read_only)
            .destructive(false)
            .idempotent(read_only)
            .open_world(false),
    ))
}

fn encoded<T: Serialize>(value: &T) -> Result<CallToolResult, ErrorData> {
    serde_json::to_value(value)
        .map(CallToolResult::structured)
        .map_err(|_| ErrorData::internal_error("could not serialize harbor result", None))
}

fn tool_error(code: &str, error: &anyhow::Error) -> CallToolResult {
    CallToolResult::structured_error(serde_json::json!({
        "code": code,
        "message": format!("{error:#}"),
    }))
}

/// Deny publishing bump calls unless the server is explicitly allowed to.
///
/// Arguments are untrusted: anything that does not parse as the declared tool
/// arguments is denied.
fn write_allowed(name: &str, arguments: Option<&JsonObject>, allow_publish: bool) -> bool {
    if name != "harbor_bump" {
        return false;
    }
    if allow_publish {
        return true;
    }
    let Some(arguments) = arguments else {
        return false;
    };
    let Ok(args) = serde_json::from_value::<BumpArgs>(serde_json::Value::Object(arguments.clone()))
    else {
        return false;
    };
    !args.push && !args.pr
}

pub fn build_server(root: PathBuf, allow_publish: bool) -> Result<Server, ErrorData> {
    let root = Arc::new(root);
    let mut server =
        Server::new("harbor", env!("CARGO_PKG_VERSION")).with_authorizer(move |call| {
            match call.effect {
                Effect::Read => true,
                Effect::Write => write_allowed(call.name, call.arguments, allow_publish),
                Effect::Destructive => false,
            }
        });
    register_align(&mut server, &root)?;
    register_plan(&mut server, &root)?;
    register_bump(&mut server, &root)?;
    Ok(server)
}

fn register_align(server: &mut Server, root: &Arc<PathBuf>) -> Result<(), ErrorData> {
    let align_root = Arc::clone(root);
    server.register(
        Effect::Read,
        ToolRoute::new(
            tool::<AlignArgs>(
                "harbor_align_locks_check",
                "Report component locks that drifted from the root flake.lock",
                true,
            )?,
            move |Parameters(args): Parameters<AlignArgs>| {
                let root = Arc::clone(&align_root);
                async move {
                    let inputs = args.inputs.unwrap_or_default();
                    match tokio::task::spawn_blocking(move || align_locks(&root, &inputs, false))
                        .await
                    {
                        Ok(Ok(report)) => encoded(&serde_json::json!({
                            "ok": report.is_clean(),
                            "changed": report.changed,
                        })),
                        Ok(Err(error)) => Ok(tool_error("align_failed", &error)),
                        Err(_) => Err(ErrorData::internal_error("align task failed", None)),
                    }
                }
            },
        ),
    )
}

fn register_plan(server: &mut Server, root: &Arc<PathBuf>) -> Result<(), ErrorData> {
    let plan_root = Arc::clone(root);
    server.register(
        Effect::Read,
        ToolRoute::new(
            tool::<BumpPlanArgs>(
                "harbor_bump_plan",
                "Show the root pin change and components a bump would update",
                true,
            )?,
            move |Parameters(args): Parameters<BumpPlanArgs>| {
                let root = Arc::clone(&plan_root);
                async move {
                    if let Err(error) = validate_rev(&args.rev) {
                        return Ok(tool_error("invalid_rev", &error));
                    }
                    let options = BumpOptions {
                        root: (*root).clone(),
                        input: args.input,
                        rev: args.rev,
                        branch: None,
                        push: false,
                        pr: false,
                        plan: true,
                    };
                    match tokio::task::spawn_blocking(move || bump(&options)).await {
                        Ok(Ok(report)) => encoded(&report),
                        Ok(Err(error)) => Ok(tool_error("bump_plan_failed", &error)),
                        Err(_) => Err(ErrorData::internal_error("bump plan task failed", None)),
                    }
                }
            },
        ),
    )
}

fn register_bump(server: &mut Server, root: &Arc<PathBuf>) -> Result<(), ErrorData> {
    let bump_root = Arc::clone(root);
    server.register(
        Effect::Write,
        ToolRoute::new(
            tool::<BumpArgs>(
                "harbor_bump",
                "Move a root input, align component locks, and commit the result",
                false,
            )?,
            move |Parameters(args): Parameters<BumpArgs>| {
                let root = Arc::clone(&bump_root);
                async move {
                    if let Err(error) = validate_rev(&args.rev) {
                        return Ok(tool_error("invalid_rev", &error));
                    }
                    let options = BumpOptions {
                        root: (*root).clone(),
                        input: args.input,
                        rev: args.rev,
                        branch: args.branch,
                        push: args.push,
                        pr: args.pr,
                        plan: false,
                    };
                    match tokio::task::spawn_blocking(move || bump(&options)).await {
                        Ok(Ok(report)) => encoded(&report),
                        Ok(Err(error)) => Ok(tool_error("bump_failed", &error)),
                        Err(_) => Err(ErrorData::internal_error("bump task failed", None)),
                    }
                }
            },
        ),
    )
}

pub fn serve(root: PathBuf, allow_publish: bool) -> anyhow::Result<()> {
    let server = build_server(root, allow_publish)
        .map_err(|error| anyhow::anyhow!("register harbor MCP tools: {error}"))?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("build tokio runtime")?;
    runtime
        .block_on(server.serve_stdio())
        .map_err(|error| anyhow::anyhow!("MCP server failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arguments(value: &serde_json::Value) -> JsonObject {
        value.as_object().unwrap().clone()
    }

    #[test]
    fn publishing_calls_are_denied_without_the_flag() {
        let readonly = arguments(&serde_json::json!({
            "input": "nixpkgs",
            "rev": "2222222222222222222222222222222222222222"
        }));
        assert!(write_allowed("harbor_bump", Some(&readonly), false));
        assert!(write_allowed("harbor_bump", Some(&readonly), true));

        let push = arguments(&serde_json::json!({
            "input": "nixpkgs",
            "rev": "2222222222222222222222222222222222222222",
            "push": true
        }));
        assert!(!write_allowed("harbor_bump", Some(&push), false));
        assert!(write_allowed("harbor_bump", Some(&push), true));

        let garbage = arguments(&serde_json::json!({"input": "nixpkgs", "push": "yes"}));
        assert!(!write_allowed("harbor_bump", Some(&garbage), false));
        assert!(!write_allowed(
            "harbor_align_locks_check",
            Some(&readonly),
            false
        ));
        assert!(!write_allowed("harbor_bump", None, false));
    }
}
