use harbor_cad::{
    contracts::{CaseSpec, ExecutionPlan},
    storage::Store,
    study::{StudyCase, StudyRequest, prepare},
};

fn request() -> StudyRequest {
    let mut cases = vec![];
    for (name, force) in [("baseline", 0.1), ("less_force", 0.05)] {
        let mut case = CaseSpec::reference();
        case.acceleration.value = force;
        let plan = ExecutionPlan::reference(case).unwrap();
        cases.push(StudyCase {
            name: name.into(),
            approved_digest: plan.id().unwrap(),
            plan,
        });
    }
    StudyRequest {
        schema_version: 1,
        name: "explicit_force_comparison".into(),
        provenance: "synthetic explicit parameters with unchanged accuracy and observations".into(),
        max_total_artifact_bytes: cases
            .iter()
            .map(|c| c.plan.observation.max_artifact_bytes)
            .sum(),
        cases,
    }
}

#[test]
fn study_rejects_changed_approvals_duplicate_cases_and_understated_total_before_state() {
    let spec = request();
    let prepared = prepare(&spec, "ci").unwrap();
    assert_eq!(prepared.cases.len(), 2);
    assert_ne!(prepared.cases[0].science_id, prepared.cases[1].science_id);
    assert_eq!(prepared.total_artifact_bytes, spec.max_total_artifact_bytes);
    let mut changed = spec.clone();
    changed.cases[1]
        .plan
        .case
        .as_mut()
        .unwrap()
        .acceleration
        .value = 0.2;
    assert!(prepare(&changed, "ci").is_err());
    let mut changed = spec.clone();
    changed.max_total_artifact_bytes -= 1;
    assert!(prepare(&changed, "ci").is_err());
    let mut changed = spec.clone();
    changed.cases[1].name = changed.cases[0].name.clone();
    assert!(prepare(&changed, "ci").is_err());
    let mut changed = spec.clone();
    changed.cases[1].plan = changed.cases[0].plan.clone();
    changed.cases[1].approved_digest = changed.cases[0].approved_digest.clone();
    assert!(prepare(&changed, "ci").is_err());
    assert!(prepare(&spec, "foreign_policy").is_err());
    let mut unknown = serde_json::to_value(&spec).unwrap();
    unknown["automatic_parameter_sampling"] = serde_json::json!(true);
    assert!(serde_json::from_value::<StudyRequest>(unknown).is_err());
}

#[test]
fn study_intent_survives_partial_submission_and_cannot_change_approved_collection() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("state");
    let spec = request();
    let profile = harbor_cad::worker::load_profile(std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/profiles/ci.json"
    )))
    .unwrap();
    let binding = "a".repeat(64);
    let store = Store::open(&root).unwrap();
    let intent =
        harbor_cad::study::retain_intent(&store, &spec, "stable_study", &profile, &binding)
            .unwrap();
    let first = store
        .submit_with_profile(&spec.cases[0].plan, &intent.child_keys[0], &profile)
        .unwrap();
    drop(store);
    let store = Store::open(&root).unwrap();
    let retry = harbor_cad::study::retain_intent(&store, &spec, "stable_study", &profile, &binding)
        .unwrap();
    assert_eq!(retry.id, intent.id);
    let report = harbor_cad::study::status(&store, &intent.id).unwrap();
    assert_eq!(report.submission, "incomplete");
    assert_eq!(report.cases[0].job.as_ref().unwrap().id, first.id);
    assert!(report.cases[1].job.is_none());
    let second = store
        .submit_with_profile(&spec.cases[1].plan, &retry.child_keys[1], &profile)
        .unwrap();
    assert_eq!(
        harbor_cad::study::status(&store, &intent.id)
            .unwrap()
            .submission,
        "complete"
    );
    assert_eq!(
        store
            .submit_with_profile(&spec.cases[0].plan, &retry.child_keys[0], &profile)
            .unwrap()
            .id,
        first.id
    );
    store
        .transition(&first.id, "queued", "cancelled", None)
        .unwrap();
    store
        .transition(
            &second.id,
            "queued",
            "interrupted",
            Some("foreground interruption"),
        )
        .unwrap();
    assert_eq!(
        harbor_cad::study::status(&store, &intent.id)
            .unwrap()
            .execution,
        "completed_with_failures"
    );
    assert_eq!(
        store
            .submit_with_profile(&spec.cases[1].plan, &retry.child_keys[1], &profile)
            .unwrap()
            .id,
        second.id
    );
    let mut changed = spec.clone();
    changed.provenance.push_str(" changed");
    assert!(
        harbor_cad::study::retain_intent(&store, &changed, "stable_study", &profile, &binding)
            .is_err()
    );
    assert!(
        harbor_cad::study::retain_intent(&store, &spec, "stable_study", &profile, &"b".repeat(64))
            .is_err()
    );
}
