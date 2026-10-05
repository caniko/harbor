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

#[test]
fn unsupported_formulation_cannot_be_run_as_an_analytical_reference() {
    let mut case = CaseSpec::reference();
    case.applicability.formulation = "periodic_forced_channel".into();
    assert!(ExecutionPlan::reference(case).is_err());
}

#[test]
fn native_cpu_plan_is_explicit_and_keeps_scientific_parameters() {
    let mut case = CaseSpec::reference();
    case.applicability.formulation = "periodic_forced_channel".into();
    case.applicability.numerical_tolerance = 0.02;
    case.length.value = 0.02;
    case.acceleration.value = 0.001;
    case.resolution = 16;
    case.max_time_s = 20.;
    let plan = ExecutionPlan::openlb_reference(case.clone(), "research".into()).unwrap();
    plan.validate().unwrap();
    assert_eq!(plan.case.science_id().unwrap(), case.science_id().unwrap());
    assert!(matches!(
        plan.stages[0].operation,
        StageOperation::CadFixture
    ));
    assert!(matches!(plan.stages[1].operation, StageOperation::Openlb));
    assert_eq!(plan.stages[1].gpu, GpuRequirement::CpuOnly);
    assert!(plan.stages[1].selection.is_none());
    assert_eq!(plan.observation.retained_times_s, vec![0., 10., 20.]);
    assert!(ExecutionPlan::openlb_reference(case, "ci".into()).is_err());
}

#[test]
fn cad_inspection_binds_a_source_digest_and_has_no_implicit_solver() {
    let mut case = CaseSpec::reference();
    case.geometry = Provenance {
        source: "channel.FCStd".into(),
        sha256: Some("a".repeat(64)),
        synthetic: false,
    };
    let plan =
        ExecutionPlan::cad_inspection(case.clone(), "research".into(), 64 * 1024 * 1024).unwrap();
    assert_eq!(plan.case.science_id().unwrap(), case.science_id().unwrap());
    assert_eq!(plan.stages.len(), 2);
    assert!(matches!(
        plan.stages[0].operation,
        StageOperation::CadInspect
    ));
    assert!(matches!(plan.stages[1].operation, StageOperation::Bundle));
    assert!(plan.observation.retained_times_s.is_empty());
    assert!(ExecutionPlan::cad_inspection(case, "ci".into(), 64 * 1024 * 1024).is_err());
    for sha in [None, Some("b".repeat(63)), Some("g".repeat(64))] {
        let mut corrupted = plan.clone();
        corrupted.case.geometry.sha256 = sha;
        assert!(corrupted.validate().is_err());
    }
    for source in ["", "../outside.FCStd", "/absolute.FCStd"] {
        let mut corrupted = plan.clone();
        corrupted.case.geometry.source = source.into();
        assert!(corrupted.validate().is_err());
    }
    let mut changed = plan.clone();
    changed.case.geometry.sha256 = Some("b".repeat(64));
    assert_ne!(plan.id().unwrap(), changed.id().unwrap());
}

#[test]
fn native_tree_ingest_copies_closed_files_and_rejects_symlinks_and_partial_outputs() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("native");
    let committed = tmp.path().join("committed");
    std::fs::create_dir(&source).unwrap();
    std::fs::create_dir(&committed).unwrap();
    std::fs::create_dir(source.join("fields")).unwrap();
    std::fs::write(source.join("fields/data.vti"), b"scientific payload").unwrap();
    let artifacts = ingest_native_tree(&source, &committed, 1024).unwrap();
    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0].path, "fields/data.vti");
    std::fs::write(source.join("fields/data.vti"), b"changed").unwrap();
    assert_eq!(
        std::fs::read(committed.join("fields/data.vti")).unwrap(),
        b"scientific payload"
    );
    std::os::unix::fs::symlink("/etc/passwd", source.join("escape")).unwrap();
    assert!(ingest_native_tree(&source, &committed, 1024).is_err());
    std::fs::remove_file(source.join("escape")).unwrap();
    std::fs::write(source.join("unfinished.partial"), b"incomplete").unwrap();
    assert!(ingest_native_tree(&source, &committed, 1024).is_err());
    std::fs::remove_file(source.join("unfinished.partial")).unwrap();
    assert!(ingest_native_tree(&source, &committed, 1).is_err());
}

#[test]
fn b1_planning_keeps_compute_render_and_media_independent_and_required() {
    let mut case = CaseSpec::reference();
    case.applicability.formulation = "periodic_forced_channel".into();
    case.acceleration.value = 0.001;
    let selections = B1Selections {
        compute: GpuSelection {
            role: Role::Compute,
            backend: "cuda".into(),
            pci: "0000:03:00.0".into(),
            backend_uuid: Some("GPU-synthetic".into()),
        },
        render: GpuSelection {
            role: Role::Render,
            backend: "egl".into(),
            pci: "0000:04:00.0".into(),
            backend_uuid: None,
        },
        media: GpuSelection {
            role: Role::Media,
            backend: "vaapi".into(),
            pci: "0000:04:00.0".into(),
            backend_uuid: None,
        },
    };
    let plan = ExecutionPlan::b1(case.clone(), selections.clone(), "research".into()).unwrap();
    plan.validate().unwrap();
    assert_eq!(plan.stages.len(), 5);
    assert_eq!(
        plan.stages[1].selection.as_ref().unwrap().pci,
        "0000:03:00.0"
    );
    assert_eq!(
        plan.stages[2].selection.as_ref().unwrap().pci,
        "0000:04:00.0"
    );
    for stage in &plan.stages[1..4] {
        assert_eq!(stage.gpu, GpuRequirement::Required);
        assert!(stage.vram_bytes > 0);
    }
    let mut invalid = selections;
    invalid.compute.backend_uuid = None;
    assert!(ExecutionPlan::b1(case, invalid, "research".into()).is_err());
}

#[test]
fn persisted_service_identity_and_job_paths_cannot_redirect_operations() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("state");
    let store = Store::open(&root).unwrap();
    let job = store
        .submit(
            &ExecutionPlan::reference(CaseSpec::reference()).unwrap(),
            "owned-unit",
        )
        .unwrap();
    store
        .connection
        .execute(
            "UPDATE jobs SET unit='unrelated.service' WHERE id=?1",
            [&job.id],
        )
        .unwrap();
    assert!(store.job(&job.id).is_err());
    store
        .connection
        .execute(
            "UPDATE jobs SET unit=?1 WHERE id=?2",
            [&format!("harbor-cad-job-{}.service", job.id), &job.id],
        )
        .unwrap();
    std::os::unix::fs::symlink(temp.path(), root.join("artifacts")).unwrap();
    assert!(store.job_dir(&job.id).is_err());
}

#[test]
fn high_mach_native_reference_is_rejected_before_approval() {
    let mut case = CaseSpec::reference();
    case.applicability.formulation = "periodic_forced_channel".into();
    assert!(ExecutionPlan::openlb_reference(case, "research".into()).is_err());
}

#[test]
fn native_approval_rejects_formulation_and_lattice_time_drift() {
    let mut case = CaseSpec::reference();
    case.applicability.formulation = "periodic_forced_channel".into();
    case.length.value = 0.02;
    case.acceleration.value = 0.001;
    case.resolution = 8;
    case.max_time_s = 20.;
    let plan = ExecutionPlan::openlb_reference(case, "research".into()).unwrap();
    let mut high_mach = plan.clone();
    high_mach.case.acceleration.value = 0.1;
    assert!(
        high_mach.validate().is_err(),
        "hand-built plans must enforce the same formulation gate"
    );
    let mut extent = plan.clone();
    extent.case.length.value = 0.0201;
    assert!(
        extent.validate().is_err(),
        "nonintegral periodic extent cannot be silently rounded"
    );
    let mut collapsed = plan.clone();
    collapsed.observation.retained_times_s = vec![0., 0.001, 20.];
    assert!(
        collapsed.validate().is_err(),
        "distinct SI times collapse on this lattice"
    );
    let mut substep = plan.clone();
    substep.case.max_time_s = 0.001;
    substep.observation.retained_times_s = vec![0., 0.001];
    assert!(
        substep.validate().is_err(),
        "duration must reach a lattice step"
    );
    let mut missing_final = plan.clone();
    missing_final.observation.retained_times_s = vec![0., 10.];
    assert!(
        missing_final.validate().is_err(),
        "numerical final state must remain available for verification"
    );
    assert_eq!(plan.observation.retained_times_s, vec![0., 10., 20.]);
}
