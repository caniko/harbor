use harbor_cad::{contracts::*, storage::*};
use std::{path::Path, process::Command};

fn completed_job(root: &Path) -> (Store, String) {
    let store = Store::open(root).unwrap();
    let plan = ExecutionPlan::reference(CaseSpec::reference()).unwrap();
    let job = store.submit(&plan, "export-fixture").unwrap();
    let dir = store.job_dir(&job.id).unwrap();
    for (name, data) in [("a.json", b"{}".as_slice()), ("fields/b.csv", b"x\n1\n")] {
        let artifact = commit_artifact(&dir, name, data, "fixture", "synthetic").unwrap();
        store.add_artifact(&job.id, &artifact).unwrap();
    }
    store
        .transition(&job.id, "queued", "starting", None)
        .unwrap();
    store.finish(&job.id, 0, None).unwrap();
    (store, job.id)
}

fn export(root: &Path, id: &str, destination: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_harbor-cad"))
        .args(["artifact", "export", "--state"])
        .arg(root)
        .arg(id)
        .arg(destination)
        .output()
        .unwrap()
}

#[test]
fn failed_export_never_exposes_partial_final_bundle() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("state");
    let (store, id) = completed_job(&root);
    std::fs::write(store.job_dir(&id).unwrap().join("fields/b.csv"), b"bad").unwrap();
    let destination = temp.path().join("portable");
    let result = export(&root, &id, &destination);
    assert!(!result.status.success());
    assert!(
        !destination.exists(),
        "partial bundle visible at final path"
    );
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 1);
}

#[test]
fn successful_export_preserves_nested_paths_and_never_replaces_destination() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("state");
    let (_, id) = completed_job(&root);
    let destination = temp.path().join("portable");
    let result = export(&root, &id, &destination);
    assert!(result.status.success(), "{:?}", result);
    assert_eq!(
        std::fs::read(destination.join("fields/b.csv")).unwrap(),
        b"x\n1\n"
    );
    assert!(destination.join("manifest.json").is_file());
    std::fs::write(destination.join("operator-note"), b"keep").unwrap();
    assert!(!export(&root, &id, &destination).status.success());
    assert_eq!(
        std::fs::read(destination.join("operator-note")).unwrap(),
        b"keep"
    );
}

#[test]
fn dangling_destination_symlink_is_not_replaced() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("state");
    let (_, id) = completed_job(&root);
    let destination = temp.path().join("portable");
    std::os::unix::fs::symlink("absent", &destination).unwrap();
    assert!(!export(&root, &id, &destination).status.success());
    assert_eq!(
        std::fs::read_link(destination).unwrap(),
        Path::new("absent")
    );
}

#[test]
fn export_includes_every_registered_shard_beyond_response_page_size() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("state");
    let (store, id) = completed_job(&root);
    let directory = store.job_dir(&id).unwrap();
    for n in 0..300 {
        let artifact = commit_artifact(
            &directory,
            &format!("shards/{n:04}.csv"),
            b"x\n1\n",
            "csv",
            "synthetic",
        )
        .unwrap();
        store.add_artifact(&id, &artifact).unwrap();
    }
    let destination = temp.path().join("portable");
    assert!(export(&root, &id, &destination).status.success());
    assert_eq!(
        std::fs::read_dir(destination.join("shards"))
            .unwrap()
            .count(),
        300
    );
    let manifests: Vec<ArtifactManifest> =
        serde_json::from_slice(&std::fs::read(destination.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifests.len(), 302);
}
