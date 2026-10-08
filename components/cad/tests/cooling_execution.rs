use harbor_cad::{
    contracts::digest,
    cooling_execution::{CoolingExecutionRequest, resolve},
    retained_cooling::RetainedCoolingRequest,
    storage::Store,
};

fn request() -> CoolingExecutionRequest {
    let initialization: RetainedCoolingRequest =
        serde_json::from_str(include_str!("../examples/retained-cooling.json")).unwrap();
    CoolingExecutionRequest {
        schema_version: 1,
        initialization,
        spatial_refinement: 2,
        integration_substeps: 1,
        base_steps: 8192,
        observation_base_steps: vec![0, 256, 1024, 8192],
    }
}

#[test]
fn independent_refinement_keeps_original_science_and_equal_physical_observation_intents() {
    let base = request();
    base.validate().unwrap();
    assert_eq!(base.native_steps(), 32768);
    assert_eq!(base.observation_steps(), [0, 1024, 4096, 32768]);
    let id = digest(&base).unwrap();
    for (q, sub) in [(1, 1), (2, 2), (2, 4), (3, 1), (4, 1)] {
        let mut changed = base.clone();
        changed.spatial_refinement = q;
        changed.integration_substeps = sub;
        changed.validate().unwrap();
        assert_eq!(
            changed.native_steps(),
            base.base_steps * u64::from(q * q * sub)
        );
        assert_eq!(
            digest(&changed.initialization).unwrap(),
            digest(&base.initialization).unwrap()
        );
        assert_ne!(digest(&changed).unwrap(), id);
    }
    for (key, value) in [
        ("execute", serde_json::json!(true)),
        ("integration_substeps", serde_json::json!(null)),
        ("spatial_refinement", serde_json::json!(true)),
    ] {
        let mut changed = serde_json::to_value(&base).unwrap();
        changed[key] = value;
        assert!(serde_json::from_value::<CoolingExecutionRequest>(changed).is_err());
    }
}

#[test]
fn unresolved_missing_source_and_overflowing_native_work_cannot_approve_or_create_artifacts() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(&temp.path().join("state")).unwrap();
    let base = request();
    assert!(resolve(&store, base.clone()).is_err());
    assert!(!store.root.join("artifacts").exists());
    for field in ["spatial_refinement", "integration_substeps", "base_steps"] {
        let mut changed = serde_json::to_value(&base).unwrap();
        changed[field] = serde_json::json!(u32::MAX);
        let changed: CoolingExecutionRequest = serde_json::from_value(changed).unwrap();
        assert!(changed.validate().is_err());
        assert!(resolve(&store, changed).is_err());
    }
    let mut changed = base.clone();
    changed.base_steps = 16384;
    changed.spatial_refinement = 4;
    changed.integration_substeps = 4;
    changed.observation_base_steps = vec![0, 16384];
    assert!(changed.validate().is_err());
    changed = base.clone();
    changed.observation_base_steps = vec![0, 0, 8192];
    assert!(changed.validate().is_err());
    changed = base.clone();
    changed
        .initialization
        .thermal
        .maximum_relative_conservation_error = 0.02;
    assert!(changed.validate().is_err());
    assert!(!store.root.join("artifacts").exists());
}
