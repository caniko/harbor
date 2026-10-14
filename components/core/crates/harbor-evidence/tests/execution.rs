use std::{fs, io::Write};

use harbor_evidence::{
    execution::{
        BoundFile, EffectPolicy, Observation, Status, StepIdentity, StepReceipt, record_step,
    },
    read_json,
};

fn fixture_identity(directory: &std::path::Path, policy: EffectPolicy) -> StepIdentity {
    let source = directory.join("source");
    fs::write(&source, b"original").unwrap();
    let executable = directory.join("tool");
    fs::write(&executable, b"tool-bytes").unwrap();
    StepIdentity {
        label: "fixture-step".into(),
        executable: BoundFile::capture(&executable).unwrap(),
        inputs: vec![BoundFile::capture(&source).unwrap()],
        effect_policy: policy,
    }
}

#[test]
fn terminal_receipts_preserve_partial_raw_logs_without_retry_or_cleanup() {
    for status in [
        Status::Passed,
        Status::Failed,
        Status::TimedOut,
        Status::Cancelled,
        Status::UnknownEffect,
    ] {
        let directory = tempfile::tempdir().unwrap();
        let identity = fixture_identity(directory.path(), EffectPolicy::MayMutate);
        let mut calls = 0;
        let record = record_step(
            directory.path(),
            identity.clone(),
            |mut stdout, mut stderr| {
                calls += 1;
                let running: StepReceipt =
                    read_json(&directory.path().join("receipt.json")).unwrap();
                assert_eq!(running.status, Status::Running);
                assert!(running.requires_reconciliation);
                stdout.write_all(&[255, 0, b'x']).unwrap();
                stderr.write_all(b"partial-error").unwrap();
                fs::write(directory.path().join("external-effect"), "retain").unwrap();
                Observation {
                    status,
                    exit_code: Some(if status == Status::Passed { 0 } else { 7 }),
                    elapsed_ms: 150,
                }
            },
        )
        .unwrap();
        assert_eq!(calls, 1);
        assert_eq!(record.status, status);
        assert!(record.requires_reconciliation);
        assert_eq!(
            fs::read(directory.path().join("stdout")).unwrap(),
            [255, 0, b'x']
        );
        assert!(directory.path().join("external-effect").exists());
        record.stdout.as_ref().unwrap().verify().unwrap();
        record.stderr.as_ref().unwrap().verify().unwrap();
        assert!(record_step(directory.path(), identity, |_, _| panic!("must not replay")).is_err());
        let saved: StepReceipt = read_json(&directory.path().join("receipt.json")).unwrap();
        assert_eq!(saved.status, status);
    }
}

#[test]
fn source_drift_refuses_before_execute_and_after_execute_cannot_pass() {
    let directory = tempfile::tempdir().unwrap();
    let identity = fixture_identity(directory.path(), EffectPolicy::MayMutate);
    fs::write(&identity.inputs[0].path, b"drift").unwrap();
    assert!(
        record_step(directory.path(), identity, |_, _| panic!(
            "must refuse drift"
        ))
        .is_err()
    );
    let saved: StepReceipt = read_json(&directory.path().join("receipt.json")).unwrap();
    assert_eq!(saved.status, Status::NotExecuted);
    assert!(!saved.requires_reconciliation);

    let directory = tempfile::tempdir().unwrap();
    let identity = fixture_identity(directory.path(), EffectPolicy::MayMutate);
    let source = identity.inputs[0].path.clone();
    let record = record_step(directory.path(), identity, |_, _| {
        fs::write(source, b"drift").unwrap();
        Observation {
            status: Status::Passed,
            exit_code: Some(0),
            elapsed_ms: 1,
        }
    })
    .unwrap();
    assert_eq!(record.status, Status::SourceDrift);
    assert!(record.requires_reconciliation);
    assert_eq!(record.observation.unwrap().status, Status::Passed);
}

#[test]
fn explicit_pre_spawn_refusal_does_not_claim_partial_mutation() {
    let directory = tempfile::tempdir().unwrap();
    let identity = fixture_identity(directory.path(), EffectPolicy::MayMutate);
    let record = record_step(directory.path(), identity, |_, _| Observation {
        status: Status::NotExecuted,
        exit_code: None,
        elapsed_ms: 0,
    })
    .unwrap();
    assert_eq!(record.status, Status::NotExecuted);
    assert!(!record.requires_reconciliation);
}
