use harbor_cad::cad_source::CadMeshDescriptor;
use harbor_cad::{
    authority::{ExecutionAuthorization, HostAuthority},
    contracts::*,
    execution::{ExecutionBinding, FileIdentity},
    storage::{Store, commit_artifact},
};

fn descriptor() -> serde_json::Value {
    serde_json::json!({"schema_version":1,"synthetic":true,"backend":"cpu","formulation":"imported_axis_aligned_box",
        "geometry_provenance":"controlled synthetic imported Part solid","brep_file":"solid.brep","brep_sha256":"a".repeat(64),"brep_bytes":1000,
        "region_name":"solid","bounds_m":[0.1,0.12,-0.02,-0.01,0.3,0.31],"volume_m3":2e-6,
        "source_unit":"mm","scale_to_m":0.001,"placement_translation_unit":"mm",
        "source_transform":[1.,0.,0.,100.,0.,1.,0.,-20.,0.,0.,1.,300.,0.,0.,0.,1.],"resolution":4,
        "geometry_tolerance_m":1e-6,"volume_relative_tolerance":1e-10})
}

#[test]
fn explicit_imported_descriptor_retains_world_bounds_source_units_and_bounded_mesh() {
    let geometry: CadMeshDescriptor = serde_json::from_value(descriptor()).unwrap();
    geometry.validate().unwrap();
    assert!((geometry.lengths_m()[0] - 0.02).abs() < 1e-15);
    assert_eq!(geometry.source_transform[3], 100.);
    assert_eq!(geometry.source_unit, "mm");
    assert_eq!(geometry.brep_file, "solid.brep");
}

#[test]
fn imported_geometry_rejects_source_scope_units_nonrigid_placements_and_weak_gates() {
    for (key, value) in [
        ("backend", serde_json::json!("hip")),
        ("brep_file", serde_json::json!("../solid.brep")),
        ("brep_sha256", serde_json::json!("a".repeat(63))),
        ("brep_bytes", serde_json::json!(0)),
        ("source_unit", serde_json::json!("m")),
        ("scale_to_m", serde_json::json!(1.)),
        ("volume_m3", serde_json::json!(4e-6)),
        ("volume_relative_tolerance", serde_json::json!(1e-4)),
        ("geometry_tolerance_m", serde_json::json!(0.01)),
        ("resolution", serde_json::json!(33)),
        ("geometry_provenance", serde_json::json!("")),
        (
            "source_transform",
            serde_json::json!([
                2., 0., 0., 100., 0., 1., 0., -20., 0., 0., 1., 300., 0., 0., 0., 1.
            ]),
        ),
        (
            "source_transform",
            serde_json::json!([
                -1., 0., 0., 100., 0., 1., 0., -20., 0., 0., 1., 300., 0., 0., 0., 1.
            ]),
        ),
    ] {
        let mut value_record = descriptor();
        value_record[key] = value;
        let decoded = serde_json::from_value::<CadMeshDescriptor>(value_record);
        assert!(
            decoded.is_err() || decoded.unwrap().validate().is_err(),
            "{key}"
        );
    }
    let mut foreign = descriptor();
    foreign["contact"] = serde_json::json!(true);
    assert!(serde_json::from_value::<CadMeshDescriptor>(foreign).is_err());
}

fn archived_source(store: &Store) -> String {
    // Persisted-record fixture only; these opaque bytes establish no native
    // CAD execution or hardware qualification.
    let mut case = CaseSpec::reference();
    case.regions = vec!["solid".into()];
    case.geometry.source = "controlled.FCStd".into();
    case.geometry.sha256 = Some("b".repeat(64));
    let plan = ExecutionPlan::cad_inspection(case, "research".into(), 64 * 1024 * 1024).unwrap();
    let profile = HostExecutionProfile {
        schema_version: 1,
        policy: "research".into(),
        allowed_input_root: store.root.to_string_lossy().into(),
        max_ram_bytes: 2 * 1024 * 1024 * 1024,
        max_disk_bytes: 1024 * 1024 * 1024,
        threads: 1,
        timeout_seconds: 60,
        native_runtime: None,
        service_mode: "systemd".into(),
    };
    let job = store
        .submit_with_profile(&plan, "archived-cad", &profile)
        .unwrap();
    let identity = FileIdentity {
        path: "/nix/store/archived-cad-fixture/file".into(),
        sha256: "a".repeat(64),
        bytes: 1,
    };
    let binding = ExecutionBinding {
        schema_version: 1,
        runner_protocol: 1,
        sandbox_policy: harbor_cad::execution::SANDBOX_POLICY.into(),
        plan_digest: plan.id().unwrap(),
        host_profile_digest: digest(&profile).unwrap(),
        runner: identity.clone(),
        native_runtime: Some(identity.clone()),
        native_files: std::collections::BTreeMap::from([
            (
                serde_json::to_string(&StageOperation::CadInspect).unwrap(),
                identity.clone(),
            ),
            ("cad_closure".into(), identity),
        ]),
    };
    let authority = HostAuthority {
        schema_version: 1,
        fleetix_revision: FLEETIX_REV.into(),
        fleetix_contract_digest: fleetix_digest(),
        max_ram_bytes: profile.max_ram_bytes,
        ram_headroom_bytes: 0,
        filesystems: vec![],
        cards: vec![],
        routes: vec![],
        allowed_devices: vec![],
        overrides: vec![],
        native_runtimes: vec![],
        allowed_input_roots: vec![profile.allowed_input_root.clone()],
    };
    let authorization =
        ExecutionAuthorization::capture(&plan, &profile, &binding, &authority).unwrap();
    for (table, record, record_digest) in [
        (
            "job_executions",
            serde_json::to_value(&binding).unwrap(),
            digest(&binding).unwrap(),
        ),
        (
            "job_authorizations",
            serde_json::to_value(&authorization).unwrap(),
            digest(&authorization).unwrap(),
        ),
    ] {
        store
            .connection
            .execute(
                &format!("INSERT INTO {table} VALUES(?1,?2,?3)"),
                rusqlite::params![
                    job.id,
                    record_digest,
                    serde_json::to_string(&record).unwrap()
                ],
            )
            .unwrap();
    }
    let root = store.job_dir(&job.id).unwrap();
    let brep = commit_artifact(
        &root,
        "solid.brep",
        b"opaque archived BREP bytes",
        "brep",
        "persisted-record fixture",
    )
    .unwrap();
    store.add_artifact(&job.id, &brep).unwrap();
    let geometry = descriptor();
    let region = serde_json::json!({"name":"solid","label":"solid","volume_m3":geometry["volume_m3"],"bounds_m":geometry["bounds_m"],"transform":geometry["source_transform"],"triangles":12,"source_unit":"mm","stl_scale_to_m":0.001});
    let manifest_region = serde_json::json!({"region_name":"solid","path":"solid.brep","bytes":brep.bytes,"sha256":brep.sha256,"source_unit":"mm","scale_to_m":0.001,
        "bounds_m":geometry["bounds_m"],"volume_m3":geometry["volume_m3"],"source_transform":geometry["source_transform"],"placement_translation_unit":"mm"});
    for (path, value) in [
        (
            "cad_inspect-receipt.json",
            serde_json::json!({"adapter":"FreeCAD","executed":true,"backend":"cpu","software_fallback":false,"security_minimum":"1.1.4","sandbox_required":true,"import_policy":harbor_cad::sandbox::IMPORT_POLICY}),
        ),
        (
            "regions.json",
            serde_json::json!({"synthetic":true,"regions":[region],"gap_healing":false,"geometry_tolerance":{"value":1e-5,"unit":"m"}}),
        ),
        (
            "brep-manifest.json",
            serde_json::json!({"schema_version":1,"synthetic":true,"regions":[manifest_region],"gap_healing":false}),
        ),
    ] {
        store
            .add_artifact(
                &job.id,
                &commit_artifact(
                    &root,
                    path,
                    &serde_json::to_vec(&value).unwrap(),
                    "json",
                    "persisted-record fixture",
                )
                .unwrap(),
            )
            .unwrap();
    }
    store
        .transition(&job.id, "queued", "starting", None)
        .unwrap();
    store
        .transition(&job.id, "starting", "running", None)
        .unwrap();
    store.finish(&job.id, 0, None).unwrap();
    job.id
}

#[test]
fn retained_cad_source_binds_original_approvals_and_rejects_byte_or_metadata_substitution() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(&temp.path().join("state")).unwrap();
    let id = archived_source(&store);
    let (root, source) = harbor_cad::cad_source::source(&store, &id, "solid", 4, 1e-6).unwrap();
    source.validate().unwrap();
    assert_eq!(
        source.geometry.bounds_m,
        [0.1, 0.12, -0.02, -0.01, 0.3, 0.31]
    );
    assert_eq!(source.geometry.brep_sha256, source.brep.sha256);
    assert_eq!(source.geometry.region_name, "solid");
    assert!(harbor_cad::cad_source::source(&store, &id, "missing", 4, 1e-6).is_err());
    let original = std::fs::read(root.join("solid.brep")).unwrap();
    std::fs::write(root.join("solid.brep"), b"substituted BREP bytes").unwrap();
    assert!(harbor_cad::cad_source::source(&store, &id, "solid", 4, 1e-6).is_err());
    std::fs::write(root.join("solid.brep"), original).unwrap();
    let original = std::fs::read(root.join("brep-manifest.json")).unwrap();
    let mut changed: serde_json::Value = serde_json::from_slice(&original).unwrap();
    changed["regions"][0]["source_transform"][3] = serde_json::json!(200.);
    std::fs::write(
        root.join("brep-manifest.json"),
        serde_json::to_vec(&changed).unwrap(),
    )
    .unwrap();
    assert!(harbor_cad::cad_source::source(&store, &id, "solid", 4, 1e-6).is_err());
    std::fs::write(root.join("brep-manifest.json"), original).unwrap();
    store
        .connection
        .execute("DELETE FROM job_authorizations WHERE job=?1", [&id])
        .unwrap();
    assert!(harbor_cad::cad_source::source(&store, &id, "solid", 4, 1e-6).is_err());
}
