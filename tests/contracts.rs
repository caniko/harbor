use harbor_cad::{contracts::*, devices::*, science::*, storage::*};

#[test]
fn required_gpu_never_downgrades_and_roles_share_one_card_budget() {
    let devices = vec![Device {
        pci: "0000:03:00.0".into(),
        vendor: "amd".into(),
        render_node: Some("/dev/dri/by-path/pci-0000:03:00.0-render".into()),
        backend_uuid: None,
    }];
    assert!(resolve(&devices, Role::Compute, "cuda", None).is_err());
    let render = resolve(&devices, Role::Render, "egl", Some("0000:03:00.0")).unwrap();
    let media = resolve(&devices, Role::Media, "vaapi", Some("0000:03:00.0")).unwrap();
    assert_eq!(render.physical_key, media.physical_key);
    assert!(resolve(&devices, Role::Compute, "rocm", Some("0000:03:00.0")).is_err());
}

#[test]
fn ambiguous_device_and_stale_render_alias_are_rejected() {
    let d = Device {
        pci: "0000:03:00.0".into(),
        vendor: "nvidia".into(),
        render_node: None,
        backend_uuid: Some("GPU-a".into()),
    };
    let mut second = d.clone();
    second.pci = "0000:04:00.0".into();
    assert!(resolve(&[d.clone(), second], Role::Compute, "cuda", None).is_err());
    assert!(resolve(&[d], Role::Render, "egl", Some("0000:03:00.0")).is_err());
}

#[test]
fn camera_changes_do_not_change_science_but_mesh_changes_do() {
    let mut case = CaseSpec::reference();
    let first = case.science_id().unwrap();
    case.presentation.camera = [2., 3., 4.];
    assert_eq!(first, case.science_id().unwrap());
    case.resolution += 1;
    assert_ne!(first, case.science_id().unwrap());
}

#[test]
fn applicability_does_not_turn_flow_into_cooling_or_ingress() {
    let mut case = CaseSpec::reference();
    case.claims.push(Claim::Cooling);
    assert!(case.validate().is_err());
    case.claims = vec![Claim::ResolvedIngress];
    assert!(case.validate().is_err());
    case.claims = vec![Claim::MaterialLifetime];
    assert!(case.validate().is_err());
}

#[test]
fn idempotency_is_durable_and_plan_is_immutable() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("state");
    let store = Store::open(&root).unwrap();
    let plan = ExecutionPlan::reference(CaseSpec::reference()).unwrap();
    let a = store.submit(&plan, "same-key").unwrap();
    drop(store);
    let store = Store::open(&root).unwrap();
    assert_eq!(a.id, store.submit(&plan, "same-key").unwrap().id);
    let mut other = plan.clone();
    other.case.resolution += 1;
    assert!(store.submit(&other, "same-key").is_err());
}

#[test]
fn dag_and_budget_rejections_happen_before_submission() {
    let mut plan = ExecutionPlan::reference(CaseSpec::reference()).unwrap();
    plan.stages[0].dependencies.push("missing".into());
    assert!(plan.validate().is_err());
    plan.stages[0].dependencies.clear();
    plan.observation.max_artifact_bytes = 1;
    assert!(plan.validate().is_err());
    plan.observation.max_artifact_bytes = 1048576;
    plan.case.kinematic_viscosity.value = f64::NAN;
    assert!(plan.validate().is_err());
}

#[test]
fn paths_and_artifact_commit_reject_symlinks_and_traversal() {
    let tmp = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink("/etc", tmp.path().join("escape")).unwrap();
    assert!(safe_path(tmp.path(), "../etc/passwd").is_err());
    assert!(safe_path(tmp.path(), "escape/passwd").is_err());
    assert!(safe_path(tmp.path(), "/etc/passwd").is_err());
    let manifest = commit_artifact(
        tmp.path(),
        "field.csv",
        b"x,y\n0,1\n",
        "csv",
        "synthetic reference",
    )
    .unwrap();
    assert_eq!(manifest.bytes, 8);
    assert!(commit_artifact(tmp.path(), "field.csv", b"different", "csv", "test").is_err());
}

#[test]
fn si_normalization_and_dew_point_keep_unknowns_explicit() {
    assert_eq!(
        Quantity {
            value: 20.,
            unit: "degC".into()
        }
        .si("temperature")
        .unwrap(),
        293.15
    );
    assert_eq!(
        Quantity {
            value: 10.,
            unit: "mm".into()
        }
        .si("length")
        .unwrap(),
        0.01
    );
    assert!(
        Quantity {
            value: 1.,
            unit: "furlong".into()
        }
        .si("length")
        .is_err()
    );
    assert!(dew_point(293.15, 0.).is_err());
    assert!((dew_point(293.15, 0.5).unwrap() - 282.405).abs() < 0.02);
}

#[test]
fn reference_round_trip_keeps_fields_and_validation_separate() {
    let result = channel_reference(0.1, 0.01, 1e-5, 33).unwrap();
    assert_eq!(result.process, "succeeded");
    assert_eq!(result.physical_validation, "unqualified");
    assert!(result.numerical_error < 1e-12);
    assert_eq!(result.velocity_m_s[0], 0.);
    assert_eq!(*result.velocity_m_s.last().unwrap(), 0.);
    assert!(channel_reference(0.1, 0.01, 1e-5, 10000001).is_err());
}
