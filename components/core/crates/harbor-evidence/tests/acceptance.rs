use std::{collections::BTreeMap, fs, path::Path};

use harbor_evidence::{accept, snapshot, validate_report, verify_http, verify_local};
use serde_json::json;

fn identity(root: &Path) -> harbor_evidence::Manifest {
    snapshot(&BTreeMap::from([("artifact".into(), root.to_path_buf())])).unwrap()
}

#[test]
fn report_requires_every_test_and_rejects_skips_and_duplicates() {
    for tests in [
        json!([]),
        json!([{"id":"other","status":"passed"}]),
        json!([{"id":"review","status":"skipped"}]),
        json!([{"id":"review","status":"passed"},{"id":"review","status":"passed"}]),
        json!([{"id":"review","status":"passed"},{"id":"other","status":"failed"}]),
    ] {
        assert!(validate_report(&json!({"tests":tests}), &["review".into()]).is_err());
    }
    validate_report(
        &json!({"tests":[{"id":"review","status":"passed"}]}),
        &["review".into()],
    )
    .unwrap();
}

#[test]
fn substituted_missing_and_extra_assets_are_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    fs::write(root.join("index.html"), "correct").unwrap();
    let manifest = identity(root);
    verify_local(&manifest, root, None).unwrap();
    fs::write(root.join("index.html"), "stale").unwrap();
    assert!(verify_local(&manifest, root, None).is_err());
    fs::write(root.join("index.html"), "correct").unwrap();
    fs::write(root.join("stale.wasm"), "stale").unwrap();
    assert!(verify_local(&manifest, root, None).is_err());
    fs::remove_file(root.join("stale.wasm")).unwrap();
    fs::remove_file(root.join("index.html")).unwrap();
    assert!(verify_local(&manifest, root, None).is_err());
}

#[test]
fn empty_artifact_is_rejected_and_harbor_metadata_is_excluded() {
    let directory = tempfile::tempdir().unwrap();
    assert!(
        snapshot(&BTreeMap::from([(
            "artifact".into(),
            directory.path().into()
        )]))
        .is_err()
    );
    fs::create_dir(directory.path().join(".harbor")).unwrap();
    fs::write(directory.path().join(".harbor/receipt.json"), "metadata").unwrap();
    assert!(
        snapshot(&BTreeMap::from([(
            "artifact".into(),
            directory.path().into()
        )]))
        .is_err()
    );
    fs::write(directory.path().join("index.html"), "landing").unwrap();
    assert_eq!(
        identity(directory.path()).artifacts["artifact"].files.len(),
        1
    );
}

#[test]
fn mismatched_or_changed_backend_is_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    fs::write(root.join("index.html"), "correct").unwrap();
    let manifest = snapshot(&BTreeMap::from([
        ("artifact".into(), root.into()),
        ("backend".into(), root.into()),
    ]))
    .unwrap();
    assert!(verify_local(&manifest, root, Some(&root.join("old-backend"))).is_err());
    verify_local(&manifest, root, Some(root)).unwrap();
}

#[test]
fn acceptance_refuses_source_drift_and_preserves_report_metadata() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("index.html"), "correct").unwrap();
    let manifest = identity(directory.path());
    let report = json!({"tests":[{"id":"review","status":"passed","receipt":"source-bound"}]});
    let accepted = accept(manifest.clone(), &report, &["review".into()]).unwrap();
    assert_eq!(
        accepted.tests.as_ref().unwrap()[0]["receipt"],
        "source-bound"
    );
    assert!(accept(accepted, &report, &["review".into()]).is_err());
    fs::write(directory.path().join("index.html"), "stale").unwrap();
    assert!(accept(manifest, &report, &["review".into()]).is_err());
}

#[test]
fn legacy_manifest_ordering_compares_path_components() {
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir(directory.path().join("app")).unwrap();
    fs::write(directory.path().join("app/bundle.wasm"), b"correct").unwrap();
    fs::write(directory.path().join("app.html"), b"correct").unwrap();
    let manifest = identity(directory.path());
    let paths: Vec<_> = manifest.artifacts["artifact"]
        .files
        .iter()
        .map(|entry| entry.path.as_str())
        .collect();
    assert_eq!(paths, ["app/bundle.wasm", "app.html"]);
    let raw = json!({
        "schema_version": 1,
        "artifacts": {"artifact": {"root": directory.path(), "files": [
            {"path":"app/bundle.wasm","sha256":"15a596e3c98c407e043751ff3b21ff0358a1bdfdf3fe948b1523893a8e5de2e8"},
            {"path":"app.html","sha256":"15a596e3c98c407e043751ff3b21ff0358a1bdfdf3fe948b1523893a8e5de2e8"}
        ]}}
    });
    verify_local(
        &serde_json::from_value(raw).unwrap(),
        directory.path(),
        None,
    )
    .unwrap();
}

#[test]
fn old_schema_one_manifest_keeps_byte_identity() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("index.html"), b"correct").unwrap();
    let manifest = identity(directory.path());
    // sha256sum of the exact original Python fixture bytes (no newline).
    assert_eq!(
        manifest.artifacts["artifact"].files[0].sha256,
        "15a596e3c98c407e043751ff3b21ff0358a1bdfdf3fe948b1523893a8e5de2e8"
    );
    let mut raw = serde_json::to_value(manifest).unwrap();
    raw["schema_version"] = json!(2);
    let unsupported = serde_json::from_value(raw).unwrap();
    assert!(verify_local(&unsupported, directory.path(), None).is_err());
}

// Compiled fixture server replaces the Python ThreadingHTTPServer contract.
fn server(responses: Vec<(&'static str, &'static [u8])>) -> (String, std::thread::JoinHandle<()>) {
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let thread = std::thread::spawn(move || {
        for (status, body) in responses {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                .unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
                assert!(
                    request.len() <= 4096,
                    "fixture request headers exceed bound"
                );
            }
            write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            stream.write_all(body).unwrap();
        }
    });
    (url, thread)
}

#[test]
fn http_verification_checks_every_asset_and_refuses_stale_content_or_redirects() {
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir(directory.path().join("app")).unwrap();
    fs::write(directory.path().join("index.html"), b"landing").unwrap();
    fs::write(directory.path().join("app/bundle.wasm"), b"correct").unwrap();
    let manifest = identity(directory.path());
    let (url, thread) = server(vec![("200 OK", b"correct"), ("200 OK", b"landing")]);
    verify_http(&manifest, &url, "app/").unwrap();
    thread.join().unwrap();
    let (url, thread) = server(vec![("200 OK", b"stale")]);
    assert!(verify_http(&manifest, &url, "app/").is_err());
    thread.join().unwrap();
    let (url, thread) = server(vec![("302 Found\r\nLocation: /app/bundle.wasm", b"")]);
    assert!(verify_http(&manifest, &url, "app/").is_err());
    thread.join().unwrap();
}
