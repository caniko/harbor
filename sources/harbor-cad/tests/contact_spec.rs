use harbor_cad::contact::ContactReferenceSpec;
use harbor_cad::contracts::{ExecutionPlan, StageOperation};

fn fixture() -> serde_json::Value {
    serde_json::json!({
        "schema_version":1,"synthetic":true,"backend":"cpu",
        "formulation":"planar_linear_penalty_contact","size_m":[0.001,0.001,0.001],
        "resolution":2,"geometry_tolerance_m":1e-8,"initial_gap_m":0.,
        "preload_compression_m":0.5e-6,"final_compression_m":1e-6,
        "young_modulus_pa":[1e8,1e8],"expansion_per_k":[1e-5,1e-5],
        "reference_temperature_k":293.15,"final_temperatures_k":[293.15,293.15],
        "contact_stiffness_pa_m":1e12,"numerical_tolerance":0.002,
        "material_provenance":"synthetic zero-Poisson constant elastic properties",
        "contact_provenance":"prescribed synthetic linear pressure/overclosure law",
        "boundary_provenance":"fixed transverse motion and displacement controlled top"
    })
}

#[test]
fn contact_retains_explicit_preload_thermal_strain_gap_and_static_state_identity() {
    let mut spec: ContactReferenceSpec = serde_json::from_value(fixture()).unwrap();
    spec.validate().unwrap();
    let preload = spec.reference(1).unwrap();
    assert!((preload.pressure_pa - 23809.52380952381).abs() < 1e-9);
    assert!(preload.physical_time_s.is_none());
    spec.final_temperatures_k = [233.15; 2];
    let final_state = spec.reference(2).unwrap();
    assert_eq!(final_state.pressure_pa, 0.);
    assert!((final_state.gap_m - 2e-7).abs() < 1e-18);
    assert!(spec.reference(0).is_err());
}

#[test]
fn contact_contract_rejects_backend_physics_or_acceptance_substitution() {
    for (key, value) in [
        ("synthetic", serde_json::json!(false)),
        ("backend", serde_json::json!("hip")),
        ("poisson_ratio", serde_json::json!(0.3)),
        ("initial_gap_m", serde_json::json!(-1e-6)),
        ("resolution", serde_json::json!(true)),
        ("numerical_tolerance", serde_json::json!(0.01)),
        ("contact_stiffness_pa_m", serde_json::json!(0)),
        ("contact_stiffness_pa_m", serde_json::json!(5e-324)),
        ("final_temperatures_k", serde_json::json!([0, 293])),
        ("expansion_per_k", serde_json::json!([0.01, 1e-5])),
        ("final_compression_m", serde_json::json!(1e-4)),
        ("material_provenance", serde_json::json!(" ")),
    ] {
        let mut value_spec = fixture();
        value_spec[key] = value;
        assert!(
            serde_json::from_value::<ContactReferenceSpec>(value_spec)
                .map_or(true, |spec| spec.validate().is_err()),
            "{key}"
        );
    }
}

#[test]
fn version_ten_contact_plan_binds_two_static_states_and_exact_cpu_dag() {
    let spec: ContactReferenceSpec = serde_json::from_value(fixture()).unwrap();
    let plan = ExecutionPlan::contact_reference(spec, "research".into()).unwrap();
    assert_eq!(plan.schema_version, 10);
    assert_eq!(plan.stages[0].operation, StageOperation::ContactReference);
    assert_eq!(plan.stages[1].dependencies, ["contact"]);
    assert!(plan.observation.retained_times_s.is_empty());
    assert_eq!(plan.stages[0].ram_bytes, 2 * 1024 * 1024 * 1024);
    let value = serde_json::to_value(&plan).unwrap();
    assert!(!value.as_object().unwrap().contains_key("wetting"));
    let mut changed = plan.clone();
    changed.contact.as_mut().unwrap().preload_compression_m *= 0.5;
    assert_ne!(plan.id().unwrap(), changed.id().unwrap());
    assert_ne!(plan.science_id().unwrap(), changed.science_id().unwrap());
    for version in 1..10 {
        let mut older = value.clone();
        older["schema_version"] = serde_json::json!(version);
        assert!(serde_json::from_value::<ExecutionPlan>(older).is_err());
    }
    for (key, value) in [("contact", serde_json::Value::Null), ("wetting", fixture())] {
        let mut bad = serde_json::to_value(&plan).unwrap();
        bad[key] = value;
        assert!(serde_json::from_value::<ExecutionPlan>(bad).is_err());
    }
    let mut bad = plan;
    bad.observation.retained_times_s.push(1.0);
    assert!(bad.validate().is_err());
}
