use harbor_cad::contact::ContactReferenceSpec;

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
