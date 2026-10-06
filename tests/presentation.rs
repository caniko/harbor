use harbor_cad::contracts::*;

fn source_plan() -> ExecutionPlan {
    let mut case = CaseSpec::reference();
    case.length.value = 0.02;
    case.resolution = 8;
    case.acceleration.value = 0.001;
    case.max_time_s = 20.;
    case.applicability.formulation = "periodic_forced_channel".into();
    ExecutionPlan::openlb_reference(case, "research".into()).unwrap()
}

fn source(plan: &ExecutionPlan) -> RetainedSource {
    RetainedSource {
        job_id: "e586dd67-01d5-4a45-9f0e-3df7a917bc89".into(),
        plan_digest: plan.id().unwrap(),
        execution_binding_digest: "a".repeat(64),
        authorization_digest: "b".repeat(64),
        snapshot_sha256: "c".repeat(64),
        artifact_id: "d".repeat(64),
        science_id: plan.science_id().unwrap(),
        bytes: 4096,
    }
}

fn request() -> PresentationRequest {
    PresentationRequest {
        source_job: "e586dd67-01d5-4a45-9f0e-3df7a917bc89".into(),
        times_s: vec![0., 20.],
        presentation: Presentation {
            camera: [0.04, 0.025, 0.03],
            width: 640,
            height: 480,
            field: "velocity".into(),
            range: [0., 0.0015],
        },
        render: GpuSelection {
            role: Role::Render,
            backend: "egl".into(),
            pci: "0000:03:00.0".into(),
            backend_uuid: None,
        },
        media: None,
    }
}

fn filter_request() -> FilterRequest {
    FilterRequest {
        source_job: request().source_job,
        filter: FilterSpec {
            time_s: 20.,
            field: GradientField::Velocity,
        },
        compute: GpuSelection {
            role: Role::Compute,
            backend: "hip".into(),
            pci: "0000:03:00.0".into(),
            backend_uuid: Some("GPU-reference".into()),
        },
    }
}

#[test]
fn numerical_filter_is_source_bound_compute_only_and_has_an_independent_approval() {
    let original = source_plan();
    let plan = ExecutionPlan::numerical_filter(
        &original,
        source(&original),
        filter_request(),
        "research".into(),
    )
    .unwrap();
    assert_eq!(plan.schema_version, 4);
    assert_eq!(plan.science_id().unwrap(), original.science_id().unwrap());
    assert_eq!(plan.observation.retained_times_s, vec![20.]);
    assert_eq!(plan.stages.len(), 2);
    assert!(matches!(
        plan.stages[0].operation,
        StageOperation::NumericalFilter
    ));
    assert_eq!(
        plan.stages[0].selection.as_ref().unwrap().role,
        Role::Compute
    );
    let round: ExecutionPlan =
        serde_json::from_value(serde_json::to_value(&plan).unwrap()).unwrap();
    assert_eq!(round.id().unwrap(), plan.id().unwrap());
    let mut changed = plan.clone();
    changed.filter.as_mut().unwrap().field = GradientField::Pressure;
    assert_ne!(changed.id().unwrap(), plan.id().unwrap());
    let mut wrong = plan.clone();
    wrong.stages[0].gpu = GpuRequirement::Preferred;
    assert!(wrong.validate().is_err());
    let mut wrong = plan.clone();
    wrong.stages[0].operation = StageOperation::Render;
    assert!(wrong.validate().is_err());
    let mut wrong = plan.clone();
    wrong.observation.retained_times_s = vec![0., 20.];
    assert!(wrong.validate().is_err());
    let mut missing = serde_json::to_value(&plan).unwrap();
    missing.as_object_mut().unwrap().remove("filter");
    assert!(serde_json::from_value::<ExecutionPlan>(missing).is_err());
    for version in [1, 2, 3] {
        let mut foreign = serde_json::to_value(&plan).unwrap();
        foreign["schema_version"] = version.into();
        assert!(serde_json::from_value::<ExecutionPlan>(foreign).is_err());
    }
    let mut injected = original;
    injected.stages = plan.stages.clone();
    assert!(injected.validate().is_err());
}

#[test]
fn numerical_filter_rejects_unretained_time_device_substitution_and_unapproved_source() {
    let original = source_plan();
    for field in ["time", "role", "backend", "uuid", "job"] {
        let mut request = filter_request();
        match field {
            "time" => request.filter.time_s = 5.,
            "role" => request.compute.role = Role::Render,
            "backend" => request.compute.backend = "vulkan".into(),
            "uuid" => request.compute.backend_uuid = None,
            _ => request.source_job = "unknown-job".into(),
        }
        assert!(
            ExecutionPlan::numerical_filter(
                &original,
                source(&original),
                request,
                "research".into()
            )
            .is_err()
        );
    }
    let plan = ExecutionPlan::numerical_filter(
        &original,
        source(&original),
        filter_request(),
        "research".into(),
    )
    .unwrap();
    let temp = tempfile::tempdir().unwrap();
    let store = harbor_cad::storage::Store::open(&temp.path().join("state")).unwrap();
    assert!(store.submit(&plan, "unauthorized-filter").is_err());
    let profile = harbor_cad::worker::load_profile(std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/profiles/ci.json"
    )))
    .unwrap();
    let binding = harbor_cad::execution::ExecutionBinding::capture(
        &plan,
        &profile,
        std::path::Path::new(env!("CARGO_BIN_EXE_harbor-cad")),
        Default::default(),
    )
    .unwrap();
    assert_eq!(
        binding.sandbox_policy,
        "harbor-cad-filter-hip-single-kfd-v1"
    );
}

#[test]
fn presentation_changes_camera_without_solver_or_science_changes() {
    let original = source_plan();
    let plan =
        ExecutionPlan::presentation(&original, source(&original), request(), "research".into())
            .unwrap();
    assert_eq!(plan.schema_version, 2);
    assert_eq!(plan.science_id().unwrap(), original.science_id().unwrap());
    assert_eq!(plan.observation.retained_times_s, vec![0., 20.]);
    assert_eq!(plan.stages.len(), 2);
    assert!(matches!(plan.stages[0].operation, StageOperation::Render));
    assert!(matches!(plan.stages[1].operation, StageOperation::Bundle));
    let mut changed = plan.clone();
    changed.source.as_mut().unwrap().snapshot_sha256 = "e".repeat(64);
    assert_ne!(plan.id().unwrap(), changed.id().unwrap());
}

#[test]
fn v1_records_keep_exact_serialization_and_reject_v2_source_injection() {
    let original = source_plan();
    let mut value = serde_json::to_value(&original).unwrap();
    assert!(value.get("source").is_none());
    let restored: ExecutionPlan = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(original.id().unwrap(), restored.id().unwrap());
    value["source"] = serde_json::to_value(source(&original)).unwrap();
    assert!(serde_json::from_value::<ExecutionPlan>(value).is_err());
    let mut null = serde_json::to_value(&original).unwrap();
    null["source"] = serde_json::Value::Null;
    assert!(serde_json::from_value::<ExecutionPlan>(null).is_err());
    let duplicate = serde_json::to_string(&original).unwrap().replacen(
        "\"schema_version\":1",
        "\"schema_version\":1,\"schema_version\":1",
        1,
    );
    assert!(serde_json::from_str::<ExecutionPlan>(&duplicate).is_err());
}

#[test]
fn source_bound_submission_cannot_skip_authorization_or_understate_copies() {
    let original = source_plan();
    let plan =
        ExecutionPlan::presentation(&original, source(&original), request(), "research".into())
            .unwrap();
    let temp = tempfile::tempdir().unwrap();
    let store = harbor_cad::storage::Store::open(&temp.path().join("state")).unwrap();
    assert!(store.submit(&plan, "unauthorized-presentation").is_err());
    assert!(store.active().unwrap().is_empty());
    let mut large_source = source(&original);
    large_source.bytes = 500 * 1024 * 1024;
    let larger =
        ExecutionPlan::presentation(&original, large_source, request(), "research".into()).unwrap();
    assert!(
        larger.disk_reservation().unwrap()
            >= plan.disk_reservation().unwrap() + 2 * (500 * 1024 * 1024 - 4096)
    );
    let mut understated = larger.clone();
    understated.observation.max_artifact_bytes = plan.observation.max_artifact_bytes;
    assert!(understated.validate().is_err());
}

#[test]
fn presentation_media_is_independent_and_all_source_hashes_are_approved() {
    let original = source_plan();
    let mut requested = request();
    requested.media = Some(GpuSelection {
        role: Role::Media,
        backend: "vaapi".into(),
        pci: "0000:04:00.0".into(),
        backend_uuid: None,
    });
    let plan =
        ExecutionPlan::presentation(&original, source(&original), requested, "research".into())
            .unwrap();
    assert_eq!(
        plan.stages[1].selection.as_ref().unwrap().pci,
        "0000:04:00.0"
    );
    let restored: ExecutionPlan =
        serde_json::from_value(serde_json::to_value(&plan).unwrap()).unwrap();
    assert_eq!(restored.id().unwrap(), plan.id().unwrap());
    let mut changed = plan.clone();
    changed.source.as_mut().unwrap().authorization_digest = "f".repeat(64);
    assert_ne!(changed.id().unwrap(), plan.id().unwrap());
    changed.source.as_mut().unwrap().science_id = "f".repeat(64);
    assert!(changed.validate().is_err());
}

#[test]
fn independent_video_has_only_media_stages_and_preserves_source_science() {
    let original = source_plan();
    let render =
        ExecutionPlan::presentation(&original, source(&original), request(), "research".into())
            .unwrap();
    let frames = FrameSource {
        job_id: "84cba551-04aa-4c3a-ac32-b6527625fb29".into(),
        plan_digest: render.id().unwrap(),
        execution_binding_digest: "1".repeat(64),
        authorization_digest: "2".repeat(64),
        sequence_sha256: "3".repeat(64),
        bytes: 8192,
    };
    let request = VideoRequest {
        source_job: frames.job_id.clone(),
        media: GpuSelection {
            role: Role::Media,
            backend: "vaapi".into(),
            pci: "0000:03:00.0".into(),
            backend_uuid: None,
        },
    };
    let plan = ExecutionPlan::video(
        &render,
        source(&original),
        frames,
        request,
        "research".into(),
    )
    .unwrap();
    assert_eq!(plan.schema_version, 3);
    assert_eq!(plan.science_id().unwrap(), original.science_id().unwrap());
    assert_eq!(
        plan.observation.retained_times_s,
        render.observation.retained_times_s
    );
    assert!(matches!(plan.stages[0].operation, StageOperation::Video));
    assert!(matches!(plan.stages[1].operation, StageOperation::Bundle));
    assert_eq!(plan.stages.len(), 2);
    let json = serde_json::to_value(&plan).unwrap();
    assert_eq!(
        serde_json::from_value::<ExecutionPlan>(json.clone())
            .unwrap()
            .id()
            .unwrap(),
        plan.id().unwrap()
    );
    let mut forged = json;
    forged["schema_version"] = 2.into();
    assert!(serde_json::from_value::<ExecutionPlan>(forged).is_err());
    let mut forged = plan;
    forged.stages.insert(0, render.stages[0].clone());
    assert!(forged.validate().is_err());
}

#[test]
fn presentation_rejects_unretained_times_invalid_routes_and_solver_injection() {
    let original = source_plan();
    let mut bad = request();
    bad.times_s = vec![5.];
    assert!(
        ExecutionPlan::presentation(&original, source(&original), bad, "research".into()).is_err()
    );
    let mut bad = request();
    bad.render.role = Role::Compute;
    assert!(
        ExecutionPlan::presentation(&original, source(&original), bad, "research".into()).is_err()
    );
    let mut plan =
        ExecutionPlan::presentation(&original, source(&original), request(), "research".into())
            .unwrap();
    plan.stages.insert(0, original.stages[0].clone());
    assert!(plan.validate().is_err());
    plan.stages.remove(0);
    plan.schema_version = 1;
    assert!(plan.validate().is_err());
}
