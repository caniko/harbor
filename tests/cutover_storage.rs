use harbor_db::storage::{custody, cutover, process};
use serde_json::json;
use std::{fs, os::unix::fs::symlink};

// Fixture subprocesses share this test executable's descriptor table with
// concurrent authority tests. Coordinate only the fork/exec handshake, just as
// production workers do; waiting and interacting with the child stay parallel.
fn output(command: &mut std::process::Command) -> std::process::Output {
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    process::spawn(command).unwrap().wait_with_output().unwrap()
}

struct Fixture {
    temp: tempfile::TempDir,
    source: std::path::PathBuf,
    restore: std::path::PathBuf,
    state: std::path::PathBuf,
    config: serde_json::Value,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let restore = temp.path().join("restore");
        let state = temp.path().join("authority");
        for p in [&source, &restore, &state] {
            fs::create_dir(p).unwrap();
        }
        for p in [&source, &restore] {
            fs::create_dir(p.join("historical.git")).unwrap();
            fs::write(p.join("historical.git/HEAD"), b"ref: refs/heads/trunk\n").unwrap();
            fs::write(p.join("history"), b"historical objects").unwrap();
        }
        let config = json!({"kind":"filesystem","user":harbor_db::storage::login_shell::current_user().unwrap(),"runtime_units":[],"authority":{"resource":"archive","state_dir":state,"directories":[source],"binding":{"backend":"files"}},"custody_file":state.join("custody.json"),"max_age_seconds":60});
        Self {
            temp,
            source,
            restore,
            state,
            config,
        }
    }
    fn certify(&self) -> harbor_db::storage::Result<()> {
        custody::certify_filesystem(
            &self.config,
            std::slice::from_ref(&self.restore),
            "verified-corpus",
            Some(100),
            None,
            &json!([]),
        )
    }
    fn check(&self, phase: &str, now: i64) -> harbor_db::storage::Result<serde_json::Value> {
        cutover::check_resource(&self.config, phase, Some(now), None)
    }
    fn manifest(&self) -> serde_json::Value {
        json!({"version":1,"enforced":true,"host":"fixture","timeout_seconds":5,"resources":{"archive":self.config}})
    }
    fn cli(&self, args: &[&str]) -> std::process::Output {
        let path = self.temp.path().join("contract.json");
        harbor_db::storage::durable::write_json(&path, &self.manifest()).unwrap();
        output(
            std::process::Command::new(env!("CARGO_BIN_EXE_harbor-db-cutover"))
                .args(args)
                .args(["--contract", path.to_str().unwrap(), "--host", "fixture"]),
        )
    }
}

#[test]
fn coordinator_rejects_restore_roots_in_every_enrolled_namespace_in_both_engines() {
    use harbor_db::storage::durable;
    use std::os::unix::fs::MetadataExt;
    for python in [false, true] {
        let f = Fixture::new();
        let other_state = f.temp.path().join("other-authority");
        fs::create_dir(&other_state).unwrap();
        fs::write(other_state.join("control"), b"other authority").unwrap();
        let postgres = f.temp.path().join("postgres.json");
        let postgres_roots: Vec<_> = ["primary", "pg-state", "pg-startup"]
            .map(|name| f.temp.path().join(name))
            .into();
        for path in &postgres_roots {
            fs::create_dir(path).unwrap();
            fs::write(path.join("control"), b"PostgreSQL control").unwrap();
        }
        durable::write_json(&postgres, &json!({"data_dir":postgres_roots[0],"state_dir":postgres_roots[1],"startup_inhibition":{"state_dir":postgres_roots[2]}})).unwrap();
        let mut manifest = f.manifest();
        let mut peer = f.config.clone();
        peer["authority"]["resource"] = json!("other");
        peer["authority"]["state_dir"] = json!(other_state);
        peer["authority"]["directories"] = json!([f.restore]);
        peer["custody_file"] = json!(other_state.join("custody.json"));
        manifest["resources"]["other"] = peer;
        manifest["resources"]["postgresql"] =
            json!({"kind":"postgres","user":"postgres","config":postgres});
        let contract = f.temp.path().join("global.json");
        durable::write_json(&contract, &manifest).unwrap();
        let invoke = |root: &std::path::Path| {
            let mut command = if python {
                let mut command = std::process::Command::new("python3");
                command.args(["-B", "-m", "harbor_db.cutover"]).env(
                    "PYTHONPATH",
                    format!("{}/python", env!("CARGO_MANIFEST_DIR")),
                );
                command
            } else {
                std::process::Command::new(env!("CARGO_BIN_EXE_harbor-db-cutover"))
            };
            output(
                command
                    .args(["certify", "--contract"])
                    .arg(&contract)
                    .args([
                        "--host",
                        "fixture",
                        "--resource",
                        "archive",
                        "--identity",
                        "retained",
                        "--restore-root",
                    ])
                    .arg(root),
            )
        };
        let before = fs::metadata(other_state.join("control")).unwrap().ino();
        for root in std::iter::once(&f.restore)
            .chain(std::iter::once(&other_state))
            .chain(postgres_roots.iter())
        {
            for path in [root.clone(), root.join("nested")] {
                let result = invoke(&path);
                assert!(
                    !result.status.success()
                        && String::from_utf8_lossy(&result.stderr).contains("restore roots"),
                    "{result:?}"
                );
                assert!(!f.state.join("identity.json").exists());
                assert!(!f.state.join("custody.json").exists());
                assert!(!f.source.join(".harbor-db-archive-identity.json").exists());
            }
        }
        assert_eq!(
            fs::metadata(other_state.join("control")).unwrap().ino(),
            before
        );
        let independent = f.temp.path().join("independent");
        fs::create_dir(&independent).unwrap();
        fs::create_dir(independent.join("historical.git")).unwrap();
        fs::write(
            independent.join("historical.git/HEAD"),
            fs::read(f.source.join("historical.git/HEAD")).unwrap(),
        )
        .unwrap();
        fs::write(
            independent.join("history"),
            fs::read(f.source.join("history")).unwrap(),
        )
        .unwrap();
        let result = invoke(&independent);
        assert!(result.status.success(), "{result:?}");
        assert!(f.state.join("custody.json").exists());
    }
}

#[test]
fn hardlinked_custody_corpus_rejects_metadata_and_content_admission_in_both_engines() {
    let f = Fixture::new();
    let external = f.temp.path().join("external-history");
    fs::hard_link(f.source.join("history"), &external).unwrap();
    for contents in [false, true] {
        let error = custody::inventory(&f.config, contents).unwrap_err();
        assert!(error.to_string().contains("hard link"), "{error}");
    }
    let script = r#"import json,sys
from harbor_db import cutover
c=json.loads(sys.argv[1])
for contents in (False,True):
    try: cutover.inventory(c,contents=contents)
    except ValueError as error: assert 'hard link' in str(error)
    else: raise AssertionError('shared inode accepted')
"#;
    let mut spec = process::CommandSpec::new(vec![
        "python3".into(),
        "-B".into(),
        "-c".into(),
        script.into(),
        f.config.to_string(),
    ]);
    spec.environment = Some(std::collections::BTreeMap::from([
        (
            "PYTHONPATH".into(),
            format!("{}/python", env!("CARGO_MANIFEST_DIR")),
        ),
        ("PATH".into(), std::env::var("PATH").unwrap()),
    ]));
    process::execute(&spec).unwrap();
    fs::remove_file(external).unwrap();
    custody::inventory(&f.config, true).unwrap();
    f.certify().unwrap();
}

#[test]
fn enrolled_postgres_primary_and_state_exclude_filesystem_namespaces_in_both_engines() {
    let f = Fixture::new();
    let config = f.temp.path().join("postgres.json");
    let data = f.temp.path().join("postgres-data");
    let state = f.temp.path().join("postgres-state");
    let startup = f.temp.path().join("postgres-startup");
    fs::write(
        &config,
        json!({"data_dir":data,"state_dir":state,"startup_inhibition":{"state_dir":startup}})
            .to_string(),
    )
    .unwrap();
    let mut base = f.manifest();
    base["resources"]["postgresql"] = json!({"kind":"postgres","user":"postgres","config":config});
    let check = |manifest: &serde_json::Value, accepted: bool| {
        assert_eq!(
            cutover::validate_manifest(manifest, "fixture").is_ok(),
            accepted,
            "{manifest}"
        );
        let result = output(std::process::Command::new("python3").args([
            "-B", "-c",
            "import json,sys;from harbor_db import cutover\ntry:cutover.validate_manifest(json.loads(sys.argv[1]),'fixture')\nexcept ValueError as e: assert sys.argv[2]=='false' and 'custody namespaces' in str(e),str(e)\nelse: assert sys.argv[2]=='true','overlapping PostgreSQL storage admitted'",
            &manifest.to_string(), &accepted.to_string(),
        ]).env("PYTHONPATH", format!("{}/python",env!("CARGO_MANIFEST_DIR"))));
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    };
    check(&base, true);
    for root in [&data, &state, &startup] {
        for path in [
            root.clone(),
            root.join("nested"),
            root.parent().unwrap().to_path_buf(),
        ] {
            for kind in ["state_dir", "directories"] {
                let mut manifest = base.clone();
                if kind == "state_dir" {
                    manifest["resources"]["archive"]["authority"][kind] = json!(path);
                    manifest["resources"]["archive"]["custody_file"] =
                        json!(path.join("custody.json"));
                } else {
                    manifest["resources"]["archive"]["authority"][kind] = json!([path]);
                }
                check(&manifest, false);
            }
        }
    }
    for directory in [&data, &state, &startup] {
        fs::create_dir(directory).unwrap();
    }
    let alias = startup.join("foreign-corpus");
    fs::hard_link(f.source.join("history"), &alias).unwrap();
    assert!(cutover::validate_storage_namespaces(&base).is_err());
    let result = output(std::process::Command::new("python3").args([
        "-B", "-c",
        "import json,sys;from harbor_db import cutover\ntry:cutover.validate_storage_namespaces(json.loads(sys.argv[1]))\nexcept ValueError as e: assert 'alias shared storage' in str(e),str(e)\nelse: raise AssertionError('startup storage inode alias admitted')",
        &base.to_string(),
    ]).env("PYTHONPATH",format!("{}/python",env!("CARGO_MANIFEST_DIR"))));
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    fs::remove_file(alias).unwrap();
    cutover::validate_storage_namespaces(&base).unwrap();
    fs::write(
        &config,
        json!({"data_dir":data,"state_dir":state}).to_string(),
    )
    .unwrap();
    check(&base, true);
    assert!(!f.state.join("lock").exists());
}

#[test]
fn cross_resource_inode_aliases_reject_before_workers_in_both_engines() {
    for mount_kind in ["hardlink", "nested-empty", "root"] {
        let mounted = mount_kind != "hardlink";
        for (source_state, target_state) in
            [(false, false), (false, true), (true, false), (true, true)]
        {
            let f = Fixture::new();
            let mut peer = Fixture::new();
            peer.config["authority"]["resource"] = json!("peer");
            let mut manifest = f.manifest();
            manifest["resources"]["peer"] = peer.config.clone();
            let contract = f.temp.path().join("contract.json");
            fs::write(&contract, manifest.to_string()).unwrap();
            cutover::validate_storage_namespaces(&manifest).unwrap();
            let source = if source_state { &f.state } else { &f.source };
            let target = if target_state {
                &peer.state
            } else {
                &peer.source
            };
            let original = if mount_kind == "root" {
                source.clone()
            } else {
                source.join("shared-entry")
            };
            let alias = if mount_kind == "root" {
                target.clone()
            } else {
                target.join("shared-entry")
            };
            if mount_kind == "nested-empty" {
                fs::create_dir(&original).unwrap();
                fs::create_dir(&alias).unwrap();
            } else if !mounted {
                fs::write(&original, b"protected source bytes").unwrap();
                fs::hard_link(&original, &alias).unwrap();
            }
            for python in [false, true] {
                let mut argv = if python {
                    vec![
                        "python3".to_owned(),
                        "-B".into(),
                        "-m".into(),
                        "harbor_db.cutover".into(),
                    ]
                } else {
                    vec![env!("CARGO_BIN_EXE_harbor-db-cutover").into()]
                };
                argv.extend([
                    "check".into(),
                    "--contract".into(),
                    contract.display().to_string(),
                    "--host".into(),
                    "fixture".into(),
                    "--phase".into(),
                    "startup".into(),
                ]);
                if mounted {
                    let mut namespace = vec![
                        "unshare".into(),
                        "--user".into(),
                        "--map-root-user".into(),
                        "--mount".into(),
                        "--".into(),
                        "sh".into(),
                        "-c".into(),
                        "mount --bind -- \"$1\" \"$2\" || exit;shift 2;exec \"$@\"".into(),
                        "namespace-fixture".into(),
                        original.display().to_string(),
                        alias.display().to_string(),
                    ];
                    namespace.append(&mut argv);
                    argv = namespace;
                }
                let result = output(std::process::Command::new(&argv[0]).args(&argv[1..]).env(
                    "PYTHONPATH",
                    format!("{}/python", env!("CARGO_MANIFEST_DIR")),
                ));
                assert!(
                    !result.status.success()
                        && String::from_utf8_lossy(&result.stderr)
                            .contains("custody namespaces alias"),
                    "python={python},mount_kind={mount_kind},source_state={source_state},target_state={target_state}: {}\n{}",
                    String::from_utf8_lossy(&result.stdout),
                    String::from_utf8_lossy(&result.stderr)
                );
                assert!(!f.state.join("lock").exists());
                assert!(!peer.state.join("lock").exists());
            }
            if !mounted {
                fs::remove_file(alias).unwrap();
            }
            cutover::validate_storage_namespaces(&manifest).unwrap();
            let accepted = output(std::process::Command::new("python3").args(["-B","-c","import json,sys;from harbor_db import cutover;cutover.validate_storage_namespaces(json.loads(sys.argv[1]))",&manifest.to_string()]).env("PYTHONPATH",format!("{}/python",env!("CARGO_MANIFEST_DIR"))));
            assert!(
                accepted.status.success(),
                "{}",
                String::from_utf8_lossy(&accepted.stderr)
            );
        }
    }
}

#[test]
fn custody_authorities_and_corpora_have_disjoint_global_namespaces_in_both_engines() {
    let f = Fixture::new();
    let mut base = f.manifest();
    let mut peer = f.config.clone();
    peer["authority"]["resource"] = json!("peer");
    peer["authority"]["state_dir"] = json!(f.temp.path().join("peer-authority"));
    peer["custody_file"] = json!(f.temp.path().join("peer-authority/custody.json"));
    peer["authority"]["directories"] = json!([f.temp.path().join("peer-history")]);
    base["resources"]["peer"] = peer;
    let check = |manifest: &serde_json::Value, accepted: bool| {
        let native = cutover::validate_manifest(manifest, "fixture");
        if accepted {
            native.unwrap();
        } else {
            assert!(
                native
                    .unwrap_err()
                    .to_string()
                    .contains("custody namespaces")
            );
        }
        let script = r#"import json,sys
from harbor_db import cutover
try: cutover.validate_manifest(json.loads(sys.argv[1]),'fixture')
except ValueError as error:
    assert sys.argv[2]=='false' and 'custody namespaces' in str(error), str(error)
else: assert sys.argv[2]=='true', 'shared custody namespace accepted'
"#;
        let mut command = process::CommandSpec::new(vec![
            "python3".into(),
            "-B".into(),
            "-c".into(),
            script.into(),
            manifest.to_string(),
            accepted.to_string(),
        ]);
        command.environment = Some(std::collections::BTreeMap::from([
            (
                "PYTHONPATH".into(),
                format!("{}/python", env!("CARGO_MANIFEST_DIR")),
            ),
            ("PATH".into(), std::env::var("PATH").unwrap()),
        ]));
        process::execute(&command).unwrap();
        assert!(!f.state.join("lock").exists());
        assert!(!f.state.join("identity.json").exists());
    };
    check(&base, true);
    for resource in ["archive", "peer"] {
        let other = if resource == "archive" {
            "peer"
        } else {
            "archive"
        };
        for kind in ["state_dir", "directories"] {
            let path = if kind == "state_dir" {
                std::path::PathBuf::from(
                    base["resources"][other]["authority"][kind]
                        .as_str()
                        .unwrap(),
                )
            } else {
                std::path::PathBuf::from(
                    base["resources"][other]["authority"][kind][0]
                        .as_str()
                        .unwrap(),
                )
            };
            for state in [
                path.clone(),
                path.join("state"),
                path.parent().unwrap().to_path_buf(),
            ] {
                let mut changed = base.clone();
                changed["resources"][resource]["authority"]["state_dir"] = json!(state);
                changed["resources"][resource]["custody_file"] = json!(state.join("custody.json"));
                check(&changed, false);
            }
        }
    }
    let mut sibling = base;
    sibling["resources"]["peer"]["authority"]["state_dir"] =
        json!(f.temp.path().join("authority-sibling"));
    sibling["resources"]["peer"]["custody_file"] =
        json!(f.temp.path().join("authority-sibling/custody.json"));
    check(&sibling, true);
}

#[test]
fn database_corpus_query_cannot_replace_the_policy_root_in_both_engines() {
    let item = json!({"path":"history","directory":false});
    assert_eq!(
        custody::database_entry(&json!(0), &item).unwrap(),
        json!({"root":0,"path":"history","directory":false})
    );
    for root in [json!(1), json!(0), json!(null)] {
        let mut malicious = item.clone();
        malicious["root"] = root;
        assert!(custody::database_entry(&json!(0), &malicious).is_err());
    }
    let script = r#"from harbor_db import cutover
assert cutover.database_entry(0,{'path':'history','directory':False})=={'root':0,'path':'history','directory':False}
for root in (0,1,None):
    try: cutover.database_entry(0,{'path':'history','root':root})
    except ValueError: pass
    else: raise AssertionError('query root accepted')
"#;
    let mut spec = process::CommandSpec::new(vec![
        "python3".into(),
        "-B".into(),
        "-c".into(),
        script.into(),
    ]);
    spec.environment = Some(std::collections::BTreeMap::from([
        (
            "PYTHONPATH".into(),
            format!("{}/python", env!("CARGO_MANIFEST_DIR")),
        ),
        ("PATH".into(), std::env::var("PATH").unwrap()),
    ]));
    process::execute(&spec).unwrap();
}

#[test]
fn deployed_cutover_policy_alias_is_read_before_login_authority_selection() {
    use harbor_db::storage::{durable, login_shell, process};
    let root = tempfile::tempdir().unwrap();
    let policy = root.path().join("cutover.json");
    durable::write_json(
        &policy,
        &json!({"version":1,"enforced":true,"host":"fixture","resources":{}}),
    )
    .unwrap();
    let stored = if let Some(fixture) = option_env!("HARBOR_DB_TEST_CONFIG_FIXTURE") {
        fs::canonicalize(std::path::Path::new(fixture).join("cutover.json")).unwrap()
    } else {
        let mut command = std::process::Command::new("nix-store");
        command
            .arg("--add")
            .arg(&policy)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        let output = process::spawn(&mut command)
            .unwrap()
            .wait_with_output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        std::path::PathBuf::from(String::from_utf8(output.stdout).unwrap().trim())
    };
    let etc = root.path().join("etc");
    fs::create_dir(&etc).unwrap();
    let alias = etc.join("cutover.json");
    symlink(&stored, &alias).unwrap();
    assert!(
        durable::read_json(&alias).is_err(),
        "receipts must reject aliases"
    );
    assert!(
        durable::lock(&alias, true, false).is_err(),
        "leases must reject aliases"
    );
    let error = login_shell::serve_login_shell(&alias, &["-c".into(), "exit 0".into()], "fixture")
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("login user must select exactly one declared filesystem authority"),
        "deployed policy should load, then reject missing authority: {error}"
    );
    let mutable = root.path().join("mutable-cutover.json");
    symlink(&policy, &mutable).unwrap();
    assert!(login_shell::serve_login_shell(&mutable, &[], "fixture").is_err());
    let check = |path: &std::path::Path| {
        output(
            std::process::Command::new(env!("CARGO_BIN_EXE_harbor-db-cutover"))
                .args(["check", "--contract"])
                .arg(path)
                .args(["--host", "fixture", "--phase", "preflight"]),
        )
    };
    let deployed = check(&alias);
    assert!(
        deployed.status.success(),
        "immutable deployed contract should load: {}",
        String::from_utf8_lossy(&deployed.stderr)
    );
    let receipt: serde_json::Value = serde_json::from_slice(&deployed.stdout).unwrap();
    assert_eq!(receipt["status"], "ready");
    let redirected = check(&mutable);
    assert!(
        !redirected.status.success(),
        "cutover dispatcher must reject a mutable configuration alias"
    );
    assert!(redirected.stdout.is_empty());
    assert_eq!(fs::read_dir(&etc).unwrap().count(), 1);
}

#[test]
fn missing_corpus_is_rejected_without_initialization() {
    let mut f = Fixture::new();
    let missing = f.temp.path().join("missing");
    f.config["authority"]["directories"] = json!([missing]);
    assert!(
        f.check("preflight", 100)
            .unwrap_err()
            .to_string()
            .contains("missing")
    );
    assert!(!missing.exists());
    assert!(!f.state.join("identity.json").exists());
}
#[test]
fn empty_corpus_cannot_be_certified() {
    let f = Fixture::new();
    fs::remove_dir_all(f.source.join("historical.git")).unwrap();
    fs::remove_file(f.source.join("history")).unwrap();
    assert!(f.certify().unwrap_err().to_string().contains("empty"));
    assert!(!f.state.join("identity.json").exists());
}
#[test]
fn matching_size_and_count_cannot_hide_changed_history() {
    let f = Fixture::new();
    fs::write(f.restore.join("history"), b"HISTORICAL OBJECTS").unwrap();
    assert!(f.certify().unwrap_err().to_string().contains("differ"));
    assert!(!f.state.join("identity.json").exists());
}
#[test]
fn certification_checks_are_read_only() {
    let f = Fixture::new();
    f.certify().unwrap();
    let before: Vec<_> = fs::read_dir(&f.state)
        .unwrap()
        .map(|e| {
            let p = e.unwrap().path();
            let b = fs::read(&p).unwrap();
            (p, b)
        })
        .collect();
    for phase in ["preflight", "activate", "certify", "startup"] {
        f.check(phase, 101).unwrap();
    }
    for (p, b) in before {
        assert_eq!(fs::read(p).unwrap(), b);
    }
}
#[test]
fn early_pass_does_not_accept_later_deleted_storage() {
    let f = Fixture::new();
    f.certify().unwrap();
    f.check("preflight", 101).unwrap();
    fs::remove_file(f.source.join("historical.git/HEAD")).unwrap();
    assert!(f.check("activate", 102).is_err());
}
#[test]
fn freshness_rejects_future_and_expired_receipts() {
    let f = Fixture::new();
    f.certify().unwrap();
    for now in [99, 161] {
        assert!(
            f.check("preflight", now)
                .unwrap_err()
                .to_string()
                .contains("stale")
        );
    }
    f.check("preflight", 160).unwrap();
}
#[test]
fn startup_admits_normal_turnover_without_recertification() {
    let f = Fixture::new();
    f.certify().unwrap();
    fs::remove_dir_all(f.source.join("historical.git")).unwrap();
    fs::remove_file(f.source.join("history")).unwrap();
    f.check("startup", 1000).unwrap();
    assert!(
        f.check("preflight", 100)
            .unwrap_err()
            .to_string()
            .contains("empty")
    );
}
#[test]
fn receipt_cannot_rebind_backend_or_replaced_root() {
    let mut f = Fixture::new();
    f.certify().unwrap();
    f.config["authority"]["binding"]["backend"] = json!("sqlite");
    assert!(f.check("startup", 101).is_err());
    f.config["authority"]["binding"]["backend"] = json!("files");
    fs::rename(&f.source, f.temp.path().join("old")).unwrap();
    fs::rename(&f.restore, &f.source).unwrap();
    fs::copy(
        f.temp.path().join("old/.harbor-db-archive-identity.json"),
        f.source.join(".harbor-db-archive-identity.json"),
    )
    .unwrap();
    assert!(
        f.check("startup", 101)
            .unwrap_err()
            .to_string()
            .contains("binding")
    );
}
#[test]
fn independent_restore_rejects_aliases_and_overlapping_roots() {
    let f = Fixture::new();
    fs::remove_file(f.restore.join("history")).unwrap();
    fs::hard_link(f.source.join("history"), f.restore.join("history")).unwrap();
    assert!(f.certify().unwrap_err().to_string().contains("aliases"));
    for root in [&f.source, &f.state] {
        assert!(
            custody::certify_filesystem(
                &f.config,
                std::slice::from_ref(root),
                "verified",
                Some(100),
                None,
                &json!([])
            )
            .is_err()
        );
    }
    assert!(!f.state.join("identity.json").exists());
}
#[test]
fn restored_corpus_roots_must_be_pairwise_independent_in_both_engines() {
    for nested in [false, true] {
        let mut f = Fixture::new();
        let other = f.temp.path().join("other-source");
        fs::create_dir(&other).unwrap();
        fs::create_dir(other.join("historical.git")).unwrap();
        fs::copy(
            f.source.join("historical.git/HEAD"),
            other.join("historical.git/HEAD"),
        )
        .unwrap();
        fs::copy(f.source.join("history"), other.join("history")).unwrap();
        f.config["authority"]["directories"] = json!([f.source, other]);
        let second = if nested {
            f.restore.join("historical.git")
        } else {
            f.restore.clone()
        };
        let restores = vec![f.restore.clone(), second];
        let native = custody::certify_filesystem(
            &f.config,
            &restores,
            "independent",
            None,
            None,
            &json!([]),
        );
        let script = r#"import json,sys
from harbor_db import cutover
c,roots=json.loads(sys.argv[1]),json.loads(sys.argv[2])
try: cutover.certify_filesystem(c,roots,'independent')
except ValueError as error: print(str(error))
else: print('ACCEPTED')
"#;
        let mut spec = process::CommandSpec::new(vec![
            "python3".into(),
            "-B".into(),
            "-c".into(),
            script.into(),
            f.config.to_string(),
            json!(restores).to_string(),
        ]);
        spec.environment = Some(std::collections::BTreeMap::from([
            (
                "PYTHONPATH".into(),
                format!("{}/python", env!("CARGO_MANIFEST_DIR")),
            ),
            ("PATH".into(), std::env::var("PATH").unwrap()),
        ]));
        let peer = String::from_utf8(process::execute(&spec).unwrap()).unwrap();
        assert!(
            native.is_err(),
            "one restore was accepted as two independent corpora"
        );
        assert!(
            native
                .unwrap_err()
                .to_string()
                .contains("restore roots must be independent")
        );
        assert!(peer.contains("restore roots must be independent"), "{peer}");
        assert!(!f.state.join("identity.json").exists());
        assert!(!f.state.join("custody.json").exists());
    }
}
#[test]
fn restore_root_permissions_are_bound() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    fs::set_permissions(&f.source, fs::Permissions::from_mode(0o750)).unwrap();
    fs::set_permissions(&f.restore, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(f.certify().unwrap_err().to_string().contains("root modes"));
}
#[test]
fn database_requirements_cannot_omit_or_substitute_history() {
    let f = Fixture::new();
    for requirement in [
        json!({"root":0,"path":"other.git","directory":true}),
        json!({"root":0,"path":"history","directory":false,"size":999}),
        json!({"root":0,"path":"history","directory":false,"sha256":"0".repeat(64)}),
        json!({"root":false,"path":"history","directory":false}),
        json!({"root":0,"path":"history","directory":false,"size":false}),
        json!({"root":0,"path":"history","directory":false,"git_repository":false}),
    ] {
        assert!(
            custody::certify_filesystem(
                &f.config,
                std::slice::from_ref(&f.restore),
                "verified",
                Some(100),
                None,
                &json!([requirement])
            )
            .is_err()
        );
        assert!(!f.state.join("identity.json").exists());
    }
}
#[test]
fn exclusive_writer_lease_blocks_certification() {
    let f = Fixture::new();
    f.certify().unwrap();
    let before = fs::read(f.state.join("custody.json")).unwrap();
    let _lease = harbor_db::storage::durable::lock(&f.state.join("lock"), true, false).unwrap();
    assert!(matches!(
        f.certify(),
        Err(harbor_db::storage::StorageError::Io(error))
            if error.kind() == std::io::ErrorKind::WouldBlock
    ));
    assert_eq!(before, fs::read(f.state.join("custody.json")).unwrap());
}
#[test]
fn enrollment_rejects_unsafe_paths_units_dependencies_and_empty_roots() {
    let f = Fixture::new();
    let base = f.manifest();
    for (key, value) in [
        ("runtime_units", json!(["postgres;reboot.service"])),
        ("database_resource", json!("unenrolled")),
        ("custody_file", json!(f.temp.path().join("outside.json"))),
        ("max_age_seconds", json!(false)),
    ] {
        let mut changed = base.clone();
        changed["resources"]["archive"][key] = value;
        assert!(cutover::validate_manifest(&changed, "fixture").is_err());
    }
    let mut changed = base.clone();
    changed["resources"]["archive"]["authority"]["directories"] = json!([]);
    assert!(cutover::validate_manifest(&changed, "fixture").is_err());
    for path in [
        "relative",
        "/path/../corpus",
        "/path//corpus",
        "/path/./corpus",
        "/path/",
    ] {
        assert!(custody::validate_path(path).is_err());
    }
}

#[test]
fn custody_receipts_cannot_replace_authority_controls_in_both_engines() {
    use std::os::unix::fs::MetadataExt;
    for adopted in [false, true] {
        let f = Fixture::new();
        if adopted {
            f.certify().unwrap();
        }
        fs::write(
            f.state.join("transition.json"),
            b"retained transition journal",
        )
        .unwrap();
        let _lease = harbor_db::storage::durable::lock(&f.state.join("lock"), false, true).unwrap();
        let before: Vec<_> = fs::read_dir(&f.state)
            .unwrap()
            .map(|entry| {
                let path = entry.unwrap().path();
                let info = path.metadata().unwrap();
                (
                    path.file_name().unwrap().to_owned(),
                    info.dev(),
                    info.ino(),
                    fs::read(path).unwrap(),
                )
            })
            .collect();
        for name in ["identity.json", "transition.json", "lock"] {
            let mut config = f.config.clone();
            config["custody_file"] = json!(f.state.join(name));
            let mut manifest = f.manifest();
            manifest["resources"]["archive"] = config.clone();
            assert!(
                cutover::validate_manifest(&manifest, "fixture")
                    .unwrap_err()
                    .to_string()
                    .contains("custody receipt")
            );
            assert!(
                custody::certify_filesystem(
                    &config,
                    std::slice::from_ref(&f.restore),
                    "verified-storage",
                    Some(1000),
                    None,
                    &json!([])
                )
                .unwrap_err()
                .to_string()
                .contains("custody receipt")
            );
            let script = r#"import json,sys
from harbor_db import cutover
manifest=json.loads(sys.argv[1]);config=manifest['resources']['archive']
for operation in (lambda: cutover.validate_manifest(manifest,'fixture'),lambda: cutover.certify_filesystem(config,[sys.argv[2]],'verified-storage',now=1000)):
 try: operation()
 except ValueError as error: assert 'custody receipt' in str(error),str(error)
 else: raise AssertionError('authority control admitted as custody receipt')
"#;
            let result = output(
                std::process::Command::new("python3")
                    .args([
                        "-B",
                        "-c",
                        script,
                        &manifest.to_string(),
                        f.restore.to_str().unwrap(),
                    ])
                    .env(
                        "PYTHONPATH",
                        format!("{}/python", env!("CARGO_MANIFEST_DIR")),
                    ),
            );
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            let after: Vec<_> = fs::read_dir(&f.state)
                .unwrap()
                .map(|entry| {
                    let path = entry.unwrap().path();
                    let info = path.metadata().unwrap();
                    (
                        path.file_name().unwrap().to_owned(),
                        info.dev(),
                        info.ino(),
                        fs::read(path).unwrap(),
                    )
                })
                .collect();
            assert_eq!(before, after);
            assert_eq!(
                f.source.join(".harbor-db-archive-identity.json").exists(),
                adopted
            );
        }
    }
    let f = Fixture::new();
    f.certify().unwrap();
}
#[test]
fn real_dispatcher_reports_failure_without_creating_authority() {
    let f = Fixture::new();
    let output = f.cli(&["check"]);
    assert_eq!(output.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["status"], "blocked");
    assert_eq!(report["failures"][0]["resource"], "archive");
    assert!(!f.state.join("identity.json").exists());
}
#[test]
fn real_dispatcher_certifies_and_admits_all_phases_through_symlink() {
    let f = Fixture::new();
    let path = f.temp.path().join("contract.json");
    harbor_db::storage::durable::write_json(&path, &f.manifest()).unwrap();
    // This contract contains the disposable fixture's runtime corpus paths.
    // Native host tests import those exact bytes through the daemon. Cargo's
    // sandbox cannot add store inputs, so it uses the supported direct regular
    // policy; deployed aliases are covered by the declared build-time fixture
    // in deployed_cutover_policy_alias_is_read_before_login_authority_selection.
    let selected = if option_env!("HARBOR_DB_TEST_CONFIG_FIXTURE").is_some() {
        path.clone()
    } else {
        let mut command = std::process::Command::new("nix-store");
        command
            .args(["--add"])
            .arg(&path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        let output = harbor_db::storage::process::spawn(&mut command)
            .unwrap()
            .wait_with_output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stored = std::path::PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
        assert_eq!(fs::read(&stored).unwrap(), fs::read(&path).unwrap());
        let link = f.temp.path().join("deployed.json");
        symlink(stored, &link).unwrap();
        link
    };
    let invoke = |args: &[&str]| {
        output(
            std::process::Command::new(env!("CARGO_BIN_EXE_harbor-db-cutover"))
                .args(args)
                .args([
                    "--contract",
                    selected.to_str().unwrap(),
                    "--host",
                    "fixture",
                ]),
        )
    };
    let output = invoke(&[
        "certify",
        "--resource",
        "archive",
        "--identity",
        "historical-corpus",
        "--restore-root",
        f.restore.to_str().unwrap(),
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let before = fs::read(f.state.join("custody.json")).unwrap();
    for phase in ["preflight", "activate", "startup", "certify"] {
        let output = invoke(&["check", "--phase", phase]);
        assert!(
            output.status.success(),
            "{} {}",
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(&output.stdout)
        );
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()["status"],
            "ready"
        );
    }
    assert_eq!(before, fs::read(f.state.join("custody.json")).unwrap());
}
#[test]
fn cli_rejects_unknown_flags_and_wrong_host() {
    let f = Fixture::new();
    assert!(!f.cli(&["check", "--unknown"]).status.success());
    assert!(cutover::validate_manifest(&f.manifest(), "elsewhere").is_err());
    assert!(
        f.cli(&[
            "certify",
            "--resource",
            "missing",
            "--identity",
            "x",
            "--restore-root",
            f.restore.to_str().unwrap()
        ])
        .status
        .code()
            == Some(1)
    );
}

fn git() -> std::path::PathBuf {
    std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|p| p.join("git"))
        .find(|p| p.is_file())
        .expect("git is required by the approved development shell")
}
#[test]
fn git_integrity_rejects_empty_commit_claims_partial_shallow_and_external_history() {
    let f = Fixture::new();
    let repository = f.source.join("complete.git");
    let executable = git();
    assert!(
        output(
            std::process::Command::new(&executable)
                .args(["init", "--bare"])
                .arg(&repository)
        )
        .status
        .success()
    );
    let requirement = json!({"git_has_commits":false});
    custody::validate_git_repository(&repository, &requirement, executable.to_str()).unwrap();
    assert!(
        custody::validate_git_repository(
            &repository,
            &json!({"git_has_commits":true}),
            executable.to_str()
        )
        .is_err()
    );
    for marker in [
        "objects/pack/incomplete.promisor",
        "objects/info/alternates",
        "objects/info/http-alternates",
        "shallow",
    ] {
        let p = repository.join(marker);
        fs::write(&p, b"external").unwrap();
        assert!(
            custody::validate_git_repository(&repository, &requirement, executable.to_str())
                .is_err()
        );
        fs::remove_file(p).unwrap();
    }
    assert!(
        process::spawn(
            std::process::Command::new(&executable)
                .arg(format!("--git-dir={}", repository.display()))
                .args(["config", "remote.origin.promisor", "true"])
        )
        .unwrap()
        .wait()
        .unwrap()
        .success()
    );
    assert!(
        custody::validate_git_repository(&repository, &requirement, executable.to_str())
            .unwrap_err()
            .to_string()
            .contains("partial-clone")
    );
}
#[test]
fn git_strict_fsck_detects_missing_referenced_objects() {
    let f = Fixture::new();
    let repository = f.source.join("complete.git");
    let executable = git();
    assert!(
        output(
            std::process::Command::new(&executable)
                .args(["init", "--bare"])
                .arg(&repository)
        )
        .status
        .success()
    );
    use std::io::Write;
    let mut command = process::spawn(
        std::process::Command::new(&executable)
            .arg(format!("--git-dir={}", repository.display()))
            .args(["hash-object", "-w", "--stdin"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped()),
    )
    .unwrap();
    command
        .stdin
        .take()
        .unwrap()
        .write_all(b"historical content")
        .unwrap();
    let output = command.wait_with_output().unwrap();
    let blob = String::from_utf8(output.stdout).unwrap().trim().to_owned();
    assert!(
        process::spawn(
            std::process::Command::new(&executable)
                .arg(format!("--git-dir={}", repository.display()))
                .args(["update-ref", "refs/tags/historical-blob", &blob])
        )
        .unwrap()
        .wait()
        .unwrap()
        .success()
    );
    custody::validate_git_repository(&repository, &json!({}), executable.to_str()).unwrap();
    fs::remove_file(repository.join(format!("objects/{}/{}", &blob[..2], &blob[2..]))).unwrap();
    assert!(
        custody::validate_git_repository(&repository, &json!({}), executable.to_str())
            .unwrap_err()
            .to_string()
            .contains("integrity")
    );
}

#[test]
fn git_integrity_rejects_mutable_checkers_before_execution_in_both_engines() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let f = Fixture::new();
    let repository = f.source.join("incomplete.git");
    fs::create_dir(&repository).unwrap();
    let called = f.source.join("checker-called");
    let fake = f.source.join("fake-git");
    fs::write(
        &fake,
        format!(
            "#!/bin/sh\ntouch '{}'\ncase \"$3\" in config) exit 1;; rev-parse) echo true;; esac\n",
            called.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&fake, fs::Permissions::from_mode(0o755)).unwrap();
    let alias = f.source.join("git-alias");
    symlink(git(), &alias).unwrap();
    for executable in [&fake, &alias] {
        let result = custody::validate_git_repository(&repository, &json!({}), executable.to_str());
        assert!(result.is_err(), "mutable Git checker admitted");
        assert!(!called.exists(), "mutable checker ran before admission");
        let mut manifest = f.manifest();
        manifest["resources"]["archive"]["git_executable"] = json!(executable);
        assert!(cutover::validate_manifest(&manifest, "fixture").is_err());
    }
    let script = r#"import json,os,subprocess,sys
from pathlib import Path
from unittest.mock import patch
from harbor_db import cutover
repo,called,fake,alias=map(Path,sys.argv[1:5]);manifest=json.loads(sys.argv[5]);git=sys.argv[6];uid=int(sys.argv[7])
if uid:
    original=Path.lstat
    def mapped(path):
        info=original(path)
        if info.st_uid!=uid: return info
        fields=list(info);fields[4]=0
        return os.stat_result(fields)
    patch.object(Path,'lstat',mapped).start()
for exe in (fake,alias):
    try: cutover.validate_git_repository(repo,{},str(exe))
    except ValueError: pass
    else: raise AssertionError('mutable Git checker admitted')
    assert not called.exists(),'mutable checker ran before admission'
    manifest['resources']['archive']['git_executable']=str(exe)
    try: cutover.validate_manifest(manifest,'fixture')
    except ValueError: pass
    else: raise AssertionError('mutable checker admitted by external manifest')
manifest['resources']['archive']['git_executable']=git
cutover.validate_manifest(manifest,'fixture')
complete=repo.parent/'verified.git'
subprocess.run([git,'init','--bare',str(complete)],check=True,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
cutover.validate_git_repository(complete,{},git)
"#;
    let mut spec = process::CommandSpec::new(vec![
        "python3".into(),
        "-B".into(),
        "-c".into(),
        script.into(),
        repository.to_string_lossy().into_owned(),
        called.to_string_lossy().into_owned(),
        fake.to_string_lossy().into_owned(),
        alias.to_string_lossy().into_owned(),
        f.manifest().to_string(),
        git().to_string_lossy().into_owned(),
        option_env!("HARBOR_DB_TEST_ROOT_UID").unwrap_or("0").into(),
    ]);
    spec.environment = Some(std::collections::BTreeMap::from([
        (
            "PYTHONPATH".into(),
            format!("{}/python", env!("CARGO_MANIFEST_DIR")),
        ),
        ("PATH".into(), std::env::var("PATH").unwrap()),
    ]));
    process::execute(&spec).unwrap();
}

#[test]
fn content_replacement_with_same_mtime_and_size_is_rejected() {
    use std::os::unix::{ffi::OsStrExt, fs::MetadataExt};
    let f = Fixture::new();
    f.certify().unwrap();
    let path = f.source.join("history");
    let old = fs::metadata(&path).unwrap();
    fs::write(&path, b"HISTORICAL OBJECTS").unwrap();
    let times = [
        libc::timespec {
            tv_sec: old.atime(),
            tv_nsec: old.atime_nsec(),
        },
        libc::timespec {
            tv_sec: old.mtime(),
            tv_nsec: old.mtime_nsec(),
        },
    ];
    let name = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
    assert_eq!(
        unsafe { libc::utimensat(libc::AT_FDCWD, name.as_ptr(), times.as_ptr(), 0) },
        0
    );
    assert_eq!(fs::metadata(&path).unwrap().mtime_nsec(), old.mtime_nsec());
    assert!(f.check("activate", 101).is_err());
}
#[test]
fn traversal_permission_errors_cannot_hide_corpus() {
    use std::os::unix::fs::PermissionsExt;
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let f = Fixture::new();
    let hidden = f.source.join("inaccessible");
    fs::create_dir(&hidden).unwrap();
    fs::write(hidden.join("retained-history"), b"history").unwrap();
    fs::set_permissions(&hidden, fs::Permissions::from_mode(0o0)).unwrap();
    let result = custody::inventory(&f.config, true);
    fs::set_permissions(&hidden, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(
        matches!(result,Err(harbor_db::storage::StorageError::Io(ref e)) if e.kind()==std::io::ErrorKind::PermissionDenied)
    );
    assert!(!f.state.join("identity.json").exists());
}
#[test]
fn fifo_is_rejected_without_blocking() {
    use std::os::unix::ffi::OsStrExt;
    let f = Fixture::new();
    let path = f.source.join("fifo");
    let name = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    assert!(
        custody::inventory(&f.config, true)
            .unwrap_err()
            .to_string()
            .contains("special")
    );
}
fn shell() -> std::path::PathBuf {
    std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|p| p.join("sh"))
        .find(|p| p.is_file())
        .expect("approved shell contains sh")
}
fn wait_ready(path: &std::path::Path, child: &mut std::process::Child) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !path.exists() {
        assert!(
            child.try_wait().unwrap().is_none(),
            "writer exited before ready"
        );
        assert!(std::time::Instant::now() < deadline, "writer did not start");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
#[test]
fn actual_writer_exec_keeps_original_authority_lease() {
    let f = Fixture::new();
    assert!(
        f.cli(&[
            "certify",
            "--resource",
            "archive",
            "--identity",
            "verified-corpus",
            "--restore-root",
            f.restore.to_str().unwrap()
        ])
        .status
        .success()
    );
    let contract = f.temp.path().join("contract.json");
    let ready = f.temp.path().join("ready");
    let executable = shell();
    let mut child = process::spawn(
        std::process::Command::new(env!("CARGO_BIN_EXE_harbor-db-cutover"))
            .args([
                "serve",
                "--contract",
                contract.to_str().unwrap(),
                "--host",
                "fixture",
                "--resource",
                "archive",
                "--",
            ])
            .arg(executable)
            .args(["-c", "printf '%s' \"$$\" > \"$1\"; exec sleep 30", "writer"])
            .arg(&ready),
    )
    .unwrap();
    wait_ready(&ready, &mut child);
    let blocked = harbor_db::storage::durable::lock(&f.state.join("lock"), false, false).is_err();
    let certification = f.cli(&[
        "certify",
        "--resource",
        "archive",
        "--identity",
        "verified-corpus",
        "--restore-root",
        f.restore.to_str().unwrap(),
    ]);
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(blocked);
    assert!(!certification.status.success());
    harbor_db::storage::durable::lock(&f.state.join("lock"), false, false).unwrap();
}
#[test]
fn login_fixture_child() {
    let Ok(contract) = std::env::var("HARBOR_TEST_LOGIN_CONTRACT") else {
        return;
    };
    let ready = std::env::var("HARBOR_TEST_LOGIN_READY").unwrap();
    harbor_db::storage::login_shell::serve_login_shell(
        std::path::Path::new(&contract),
        &[
            "-c".into(),
            "printf '%s' \"$$\" > \"$1\"; exec sleep 30".into(),
            "writer".into(),
            ready,
        ],
        "fixture",
    )
    .unwrap();
}
#[test]
fn ssh_login_exec_retains_lease_and_missing_custody_never_executes() {
    let mut f = Fixture::new();
    f.config["login_shell"] = json!(shell());
    let contract = f.temp.path().join("login.json");
    let ready = f.temp.path().join("login-ready");
    harbor_db::storage::durable::write_json(&contract, &f.manifest()).unwrap();
    let invoke = || {
        let mut c = std::process::Command::new(std::env::current_exe().unwrap());
        c.args(["--exact", "login_fixture_child", "--nocapture"])
            .env("HARBOR_TEST_LOGIN_CONTRACT", &contract)
            .env("HARBOR_TEST_LOGIN_READY", &ready)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        c
    };
    let output = output(&mut invoke());
    assert!(!output.status.success());
    assert!(!ready.exists());
    assert!(!f.state.join("lock").exists());
    f.certify().unwrap();
    let mut child = process::spawn(&mut invoke()).unwrap();
    wait_ready(&ready, &mut child);
    let blocked = harbor_db::storage::durable::lock(&f.state.join("lock"), false, false).is_err();
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(blocked);
    harbor_db::storage::durable::lock(&f.state.join("lock"), false, false).unwrap();
}

#[test]
fn inventories_reject_redirects_and_bind_database_content() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("corpus");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("history"), b"retained history").unwrap();
    let config = json!({"authority":{"resource":"demo","directories":[root]}});
    let contents = custody::inventory(&config, true).unwrap();
    custody::require_database_paths(
        &contents,
        &json!([{"root":0,"path":"history","directory":false,"size":16}]),
        &[],
        None,
    )
    .unwrap();
    for requirement in [
        json!({"root":0,"path":"../history","directory":false}),
        json!({"root":1,"path":"history","directory":false}),
        json!({"root":0,"path":"history","directory":false,"sha256":"0".repeat(64)}),
    ] {
        assert!(
            custody::require_database_paths(&contents, &json!([requirement]), &[], None).is_err()
        );
    }
    symlink(root.join("history"), root.join("redirect")).unwrap();
    assert!(custody::inventory(&config, true).is_err());
}

#[test]
fn enrollment_retains_unknown_keys_but_rejects_invalid_budgets() {
    let manifest =
        json!({"version":1,"enforced":true,"host":"atlas","resources":{},"future_extension":true});
    assert_eq!(
        cutover::validate_manifest(&manifest, "atlas").unwrap(),
        manifest
    );
    for value in [json!(false), json!(0), json!(301)] {
        let mut changed = manifest.clone();
        changed["timeout_seconds"] = value;
        assert!(cutover::validate_manifest(&changed, "atlas").is_err());
    }
    assert!(cutover::validate_manifest(&manifest, "another-host").is_err());
}
