use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use clap::Parser;
use miette::{Context, IntoDiagnostic, Result, miette};
use reqwest::blocking::{Client, Response};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const PRODUCTION_API_BASE: &str = "https://zenodo.org/api";
const PRODUCTION_AUTH: &str = "https://zenodo.org/oauth/authorize";
const PRODUCTION_TOKEN: &str = "https://zenodo.org/oauth/token";
const SANDBOX_API_BASE: &str = "https://sandbox.zenodo.org/api";
const SANDBOX_AUTH: &str = "https://sandbox.zenodo.org/oauth/authorize";
const SANDBOX_TOKEN: &str = "https://sandbox.zenodo.org/oauth/token";
const REDIRECT_PORT: u16 = 53682;
const REDIRECT_PATH: &str = "/oauth/zenodo/callback";
const LOCAL_REDIRECT_URI: &str = "http://127.0.0.1:53682/oauth/zenodo/callback";
const REQUIRED_SCOPES: &str = "deposit:write deposit:actions";

#[derive(Parser)]
#[command(name = "anx-plugin-zenodo", version)]
struct Cli {
    #[arg(long = "plugin-context", required = true)]
    plugin_context: String,
}

#[derive(Deserialize)]
struct PluginContext {
    action: String,
    #[serde(default)]
    paths: BTreeMap<String, String>,
    #[serde(default)]
    metadata: Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct ZenodoOAuthStore {
    client_id: String,
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_at: Option<DateTime<Utc>>,
    token_url: String,
    authorization_url: String,
    scopes: Vec<String>,
    #[serde(default)]
    sandbox: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct DepositionStore {
    id: u64,
    #[serde(default)]
    conceptrecid: Option<String>,
    state: String,
    submitted: bool,
    html_url: String,
    api_url: String,
    bucket_url: String,
    publish_url: String,
    #[serde(default)]
    doi: Option<String>,
    #[serde(default)]
    doi_url: Option<String>,
    #[serde(default)]
    uploaded_files: Vec<String>,
}

struct ZenodoConfig {
    api_base: &'static str,
    auth_url: &'static str,
    token_url: &'static str,
    sandbox: bool,
}

impl ZenodoConfig {
    fn new(sandbox: bool) -> Self {
        if sandbox {
            Self {
                api_base: SANDBOX_API_BASE,
                auth_url: SANDBOX_AUTH,
                token_url: SANDBOX_TOKEN,
                sandbox: true,
            }
        } else {
            Self {
                api_base: PRODUCTION_API_BASE,
                auth_url: PRODUCTION_AUTH,
                token_url: PRODUCTION_TOKEN,
                sandbox: false,
            }
        }
    }
}

fn main() {
    let cli = Cli::parse();
    let result = run(&cli.plugin_context);
    match result {
        Ok(output) => {
            println!("{}", serde_json::to_string(&output).unwrap());
        }
        Err(err) => {
            let msg = format!("{err:#}");
            eprintln!("{msg}");
            let output = json!({
                "success": false,
                "error": msg,
            });
            println!("{}", serde_json::to_string(&output).unwrap());
            std::process::exit(1);
        }
    }
}

fn run(context_json: &str) -> Result<Value> {
    let ctx: PluginContext =
        serde_json::from_str(context_json).into_diagnostic()?;

    let sandbox = ctx
        .metadata
        .get("sandbox")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let cfg = ZenodoConfig::new(sandbox);

    match ctx.action.as_str() {
        "oauth-login" => oauth_login(&cfg, &ctx),
        "deposit-create" => deposit_create(&cfg, &ctx),
        "reserve-doi" => reserve_doi(&cfg, &ctx),
        "upload-file" => upload_file(&cfg, &ctx),
        "publish" => publish_deposition(&cfg, &ctx),
        other => Err(miette!("Unknown action: {other}")),
    }
}

// ── OAuth Login ──────────────────────────────────────────────────────────

fn oauth_login(cfg: &ZenodoConfig, ctx: &PluginContext) -> Result<Value> {
    let client_id = ctx
        .metadata
        .get("client_id")
        .and_then(Value::as_str)
        .ok_or_else(|| miette!("metadata.client_id is required for oauth-login"))?;
    let client_secret = ctx
        .metadata
        .get("client_secret")
        .and_then(Value::as_str)
        .ok_or_else(|| miette!("metadata.client_secret is required for oauth-login"))?;

    let state = random_token();
    let auth_url = build_authorization_url(cfg.auth_url, client_id, &state)?;
    let listener = TcpListener::bind(("127.0.0.1", REDIRECT_PORT))
        .into_diagnostic()
        .wrap_err("Cannot bind OAuth callback port 53682; is another login in progress?")?;

    println!("Opening Zenodo OAuth login in your browser...");
    println!("If the browser does not open, visit this URL manually:");
    println!("  {auth_url}");
    let _ = webbrowser::open(auth_url.as_str());

    let callback = wait_for_callback(listener)?;
    if callback.path != REDIRECT_PATH {
        return Err(miette!(
            "Unexpected OAuth callback path '{}'; expected '{}'",
            callback.path,
            REDIRECT_PATH
        ));
    }
    let returned_state = callback
        .query
        .get("state")
        .ok_or_else(|| miette!("Zenodo OAuth callback missing state"))?;
    if returned_state != &state {
        return Err(miette!("Zenodo OAuth state mismatch; refusing token exchange"));
    }
    let code = callback
        .query
        .get("code")
        .ok_or_else(|| miette!("Zenodo OAuth callback missing authorization code"))?;

    let token = exchange_code(cfg.token_url, client_id, client_secret, code)?;
    let store = token_store_from_response(cfg, client_id.to_owned(), token)?;
    save_oauth_store(&store)?;

    Ok(json!({
        "success": true,
        "token_path": oauth_store_path()?.display().to_string(),
    }))
}

// ── Deposit Create ───────────────────────────────────────────────────────

fn deposit_create(cfg: &ZenodoConfig, ctx: &PluginContext) -> Result<Value> {
    let token = access_token(cfg)?;
    let client = Client::new();

    let metadata = build_deposition_metadata(ctx);
    let body = json!({ "metadata": metadata });

    let response = client
        .post(format!("{}/deposit/depositions", cfg.api_base))
        .bearer_auth(&token)
        .json(&body)
        .send()
        .into_diagnostic()?;

    if response.status() != StatusCode::CREATED {
        return zenodo_error("create draft deposition", response);
    }

    let value: Value = response.json().into_diagnostic()?;
    let store = deposition_store_from_value(cfg, &value)?;
    save_deposition_store(&store)?;

    Ok(json!({
        "success": true,
        "deposition_id": store.id,
        "html_url": store.html_url,
        "doi": store.doi,
        "doi_url": store.doi_url,
    }))
}

// ── Reserve DOI ──────────────────────────────────────────────────────────

fn reserve_doi(cfg: &ZenodoConfig, ctx: &PluginContext) -> Result<Value> {
    let token = access_token(cfg)?;
    let client = Client::new();
    let store = load_deposition_store()?;

    let mut metadata = build_deposition_metadata(ctx);
    metadata["prereserve_doi"] = json!(true);

    let body = json!({ "metadata": metadata });
    let response = client
        .put(format!("{}/deposit/depositions/{}", cfg.api_base, store.id))
        .bearer_auth(&token)
        .json(&body)
        .send()
        .into_diagnostic()?;

    if !response.status().is_success() {
        return zenodo_error("reserve draft DOI", response);
    }

    let value: Value = response.json().into_diagnostic()?;
    let store = deposition_store_from_value(cfg, &value)?;
    save_deposition_store(&store)?;

    let doi = store
        .doi
        .clone()
        .ok_or_else(|| miette!("Zenodo response did not include a reserved DOI"))?;
    let doi_url = store
        .doi_url
        .clone()
        .unwrap_or_else(|| format!("https://doi.org/{doi}"));

    Ok(json!({
        "success": true,
        "doi": doi,
        "doi_url": doi_url,
        "deposition_id": store.id,
        "html_url": store.html_url,
    }))
}

// ── Upload File ──────────────────────────────────────────────────────────

fn upload_file(cfg: &ZenodoConfig, ctx: &PluginContext) -> Result<Value> {
    let file_path = ctx
        .paths
        .get("file")
        .map(PathBuf::from)
        .ok_or_else(|| miette!("paths.file is required for upload-file"))?;
    if !file_path.is_file() {
        return Err(miette!("File not found: {}", file_path.display()));
    }

    let mut store = load_deposition_store()?;
    let token = access_token(cfg)?;

    let filename = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| miette!("File name is not valid UTF-8"))?;
    let bytes = fs::read(&file_path)
        .into_diagnostic()
        .wrap_err_with(|| format!("read {}", file_path.display()))?;
    let upload_url = format!(
        "{}/{}",
        store.bucket_url.trim_end_matches('/'),
        filename
    );

    let response = Client::new()
        .put(&upload_url)
        .bearer_auth(&token)
        .body(bytes)
        .send()
        .into_diagnostic()?;

    if !response.status().is_success() {
        return zenodo_error("upload file", response);
    }

    store.uploaded_files.push(filename.to_owned());
    store.uploaded_files.sort();
    store.uploaded_files.dedup();
    save_deposition_store(&store)?;

    Ok(json!({
        "success": true,
        "filename": filename,
        "deposition_id": store.id,
    }))
}

// ── Publish ──────────────────────────────────────────────────────────────

fn publish_deposition(cfg: &ZenodoConfig, _ctx: &PluginContext) -> Result<Value> {
    let store = load_deposition_store()?;
    let token = access_token(cfg)?;

    let response = Client::new()
        .post(&store.publish_url)
        .bearer_auth(&token)
        .send()
        .into_diagnostic()?;

    if response.status() != StatusCode::ACCEPTED {
        return zenodo_error("publish deposition", response);
    }

    let value: Value = response.json().into_diagnostic()?;
    let store = deposition_store_from_value(cfg, &value)?;
    save_deposition_store(&store)?;

    Ok(json!({
        "success": true,
        "deposition_id": store.id,
        "doi": store.doi,
        "doi_url": store.doi_url,
        "html_url": store.html_url,
    }))
}

// ── Token Management ─────────────────────────────────────────────────────

fn access_token(cfg: &ZenodoConfig) -> Result<String> {
    let mut store = load_oauth_store()?;
    if !store.sandbox == cfg.sandbox {
        return Err(miette!(
            "Stored token is for {} but requested {}",
            if store.sandbox { "sandbox" } else { "production" },
            if cfg.sandbox { "sandbox" } else { "production" },
        ));
    }

    let refresh_needed = store
        .expires_at
        .is_some_and(|expires_at| Utc::now() + ChronoDuration::seconds(60) >= expires_at);
    if !refresh_needed {
        return Ok(store.access_token);
    }

    let refresh_token = store.refresh_token.clone().ok_or_else(|| {
        miette!(
            "Zenodo OAuth access token is expired and no refresh token is stored.\n\
             Run `oauth-login` again."
        )
    })?;

    let response = Client::new()
        .post(&store.token_url)
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body(form_body(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token.as_str()),
            ("client_id", store.client_id.as_str()),
        ]))
        .send()
        .into_diagnostic()?;

    if !response.status().is_success() {
        return zenodo_error("refresh Zenodo OAuth token", response);
    }

    let token: Value = response.json().into_diagnostic()?;
    store.access_token = token_string(&token, "access_token")?;
    store.refresh_token = token
        .get("refresh_token")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or(store.refresh_token);
    store.expires_at = token
        .get("expires_in")
        .and_then(Value::as_i64)
        .map(|seconds| Utc::now() + ChronoDuration::seconds(seconds));
    let access_token = store.access_token.clone();
    save_oauth_store(&store)?;
    Ok(access_token)
}

fn build_authorization_url(base: &str, client_id: &str, state: &str) -> Result<String> {
    let mut url = reqwest::Url::parse(base).into_diagnostic()?;
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", LOCAL_REDIRECT_URI)
        .append_pair("scope", REQUIRED_SCOPES)
        .append_pair("state", state);
    Ok(url.to_string())
}

fn exchange_code(
    token_url: &str,
    client_id: &str,
    client_secret: &str,
    code: &str,
) -> Result<Value> {
    let response = Client::new()
        .post(token_url)
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body(form_body(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", LOCAL_REDIRECT_URI),
            ("client_id", client_id),
            ("client_secret", client_secret),
        ]))
        .send()
        .into_diagnostic()?;

    if !response.status().is_success() {
        return zenodo_error("exchange authorization code", response);
    }
    response.json().into_diagnostic()
}

fn token_store_from_response(
    cfg: &ZenodoConfig,
    client_id: String,
    token: Value,
) -> Result<ZenodoOAuthStore> {
    let access_token = token_string(&token, "access_token")?;
    let refresh_token = token
        .get("refresh_token")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let expires_at = token
        .get("expires_in")
        .and_then(Value::as_i64)
        .map(|seconds| Utc::now() + ChronoDuration::seconds(seconds));

    Ok(ZenodoOAuthStore {
        client_id,
        access_token,
        refresh_token,
        expires_at,
        token_url: cfg.token_url.to_owned(),
        authorization_url: cfg.auth_url.to_owned(),
        scopes: REQUIRED_SCOPES
            .split_whitespace()
            .map(str::to_owned)
            .collect(),
        sandbox: cfg.sandbox,
    })
}

fn token_string(token: &Value, field: &str) -> Result<String> {
    token
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| miette!("Zenodo token response missing `{field}`"))
}

// ── Deposition Metadata ──────────────────────────────────────────────────

fn build_deposition_metadata(ctx: &PluginContext) -> Value {
    let mut metadata = json!({
        "upload_type": "dataset",
        "access_right": "open",
        "license": "cc-by-4.0",
    });

    if let Some(title) = ctx.metadata.get("title").and_then(Value::as_str) {
        metadata["title"] = json!(title);
    }
    if let Some(desc) = ctx.metadata.get("description").and_then(Value::as_str) {
        metadata["description"] = json!(desc);
    }
    if let Some(creators) = ctx.metadata.get("creators").and_then(Value::as_array) {
        metadata["creators"] = json!(creators);
    }
    if let Some(keywords) = ctx.metadata.get("keywords").and_then(Value::as_array) {
        metadata["keywords"] = json!(keywords);
    }

    metadata
}

// ── Deposition Store Helpers ────────────────────────────────────────────

fn deposition_store_from_value(cfg: &ZenodoConfig, value: &Value) -> Result<DepositionStore> {
    let id = value
        .get("id")
        .and_then(Value::as_u64)
        .ok_or_else(|| miette!("Zenodo deposition response missing numeric id"))?;
    let links = value
        .get("links")
        .ok_or_else(|| miette!("Zenodo deposition response missing links"))?;

    let html_url = json_string(links, "html")
        .or_else(|_| json_string(links, "latest_draft_html"))
        .unwrap_or_else(|_| format!("https://zenodo.org/deposit/{id}"));

    let api_url = json_string(links, "self")
        .or_else(|_| json_string(links, "latest_draft"))
        .unwrap_or_else(|_| format!("{}/deposit/depositions/{id}", cfg.api_base));

    let bucket_url = json_string(links, "bucket")?;
    let publish_url =
        json_string(links, "publish").unwrap_or_else(|_| format!("{api_url}/actions/publish"));

    let doi = value
        .get("doi")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| {
            value
                .pointer("/metadata/prereserve_doi/doi")
                .and_then(Value::as_str)
                .map(str::to_owned)
        });
    let doi_url = value
        .get("doi_url")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| doi.as_ref().map(|doi| format!("https://doi.org/{doi}")));

    Ok(DepositionStore {
        id,
        conceptrecid: value.get("conceptrecid").and_then(|v| {
            v.as_str()
                .map(str::to_owned)
                .or_else(|| v.as_u64().map(|n| n.to_string()))
        }),
        state: value
            .get("state")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_owned(),
        submitted: value
            .get("submitted")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        html_url,
        api_url,
        bucket_url,
        publish_url,
        doi,
        doi_url,
        uploaded_files: load_deposition_store()
            .map(|s| s.uploaded_files)
            .unwrap_or_default(),
    })
}

fn json_string(value: &Value, field: &str) -> Result<String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| miette!("Zenodo response missing `{field}`"))
}

// ── OAuth Callback ──────────────────────────────────────────────────────

struct Callback {
    path: String,
    query: BTreeMap<String, String>,
}

fn wait_for_callback(listener: TcpListener) -> Result<Callback> {
    let (mut stream, _) = listener.accept().into_diagnostic()?;
    let mut buffer = [0_u8; 4096];
    let bytes_read = stream.read(&mut buffer).into_diagnostic()?;
    let request = String::from_utf8_lossy(&buffer[..bytes_read]);
    let request_line = request
        .lines()
        .next()
        .ok_or_else(|| miette!("OAuth callback request was empty"))?;
    let target = request_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| miette!("OAuth callback request line missing target"))?;

    let url = reqwest::Url::parse(&format!("http://127.0.0.1{target}")).into_diagnostic()?;
    let query = url
        .query_pairs()
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();

    let body = b"Zenodo OAuth login complete. You may close this tab.";
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_diagnostic()?;
    stream.write_all(body).into_diagnostic()?;

    Ok(Callback {
        path: url.path().to_owned(),
        query,
    })
}

// ── Helpers ──────────────────────────────────────────────────────────────

fn random_token() -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(rand::random::<[u8; 32]>())
}

fn form_body(pairs: &[(&str, &str)]) -> String {
    let mut url = reqwest::Url::parse("http://localhost/").expect("static URL parses");
    url.query_pairs_mut().extend_pairs(pairs.iter().copied());
    url.query().unwrap_or_default().to_owned()
}

fn zenodo_error<T>(action: &str, response: Response) -> Result<T> {
    let status = response.status();
    let text = response.text().unwrap_or_default();
    Err(miette!("Zenodo {action} failed ({status}): {text}"))
}

// ── File I/O ────────────────────────────────────────────────────────────

fn oauth_store_path() -> Result<PathBuf> {
    let dir = dirs::config_dir()
        .ok_or_else(|| miette!("Cannot determine config directory"))?
        .join("anx");
    Ok(dir.join("zenodo-token.json"))
}

fn deposition_store_path() -> Result<PathBuf> {
    let dir = dirs::config_dir()
        .ok_or_else(|| miette!("Cannot determine config directory"))?
        .join("anx");
    Ok(dir.join("zenodo-deposition.json"))
}

fn load_oauth_store() -> Result<ZenodoOAuthStore> {
    let path = oauth_store_path()?;
    let data = fs::read_to_string(&path).map_err(|_| {
        miette!(
            "Zenodo OAuth token not found at {}.\nRun `oauth-login` first.",
            path.display()
        )
    })?;
    serde_json::from_str(&data).into_diagnostic()
}

fn save_oauth_store(store: &ZenodoOAuthStore) -> Result<()> {
    write_secret_json(&oauth_store_path()?, store)
}

fn load_deposition_store() -> Result<DepositionStore> {
    let path = deposition_store_path()?;
    let data = fs::read_to_string(&path).map_err(|_| {
        miette!(
            "Zenodo deposition state not found at {}.\nRun `deposit-create` first.",
            path.display()
        )
    })?;
    serde_json::from_str(&data).into_diagnostic()
}

fn save_deposition_store(store: &DepositionStore) -> Result<()> {
    write_secret_json(&deposition_store_path()?, store)
}

fn write_secret_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).into_diagnostic()?;
    }
    let data = serde_json::to_string_pretty(value).into_diagnostic()?;
    fs::write(path, data.as_bytes()).into_diagnostic()?;
    #[cfg(unix)]
    {
        use std::fs::Permissions;
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, Permissions::from_mode(0o600));
    }
    Ok(())
}
