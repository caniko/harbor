use harbor_cad::{cad_transport::CadSpectralTransportRequest, contracts::digest};

fn request() -> serde_json::Value {
    serde_json::json!({
        "schema_version":1,
        "scene":serde_json::from_str::<serde_json::Value>(include_str!("../examples/cad-spectral-scene.json")).unwrap(),
        "formulation":"opaque_lambertian_direct_only",
        "source":{"kind":"directional","propagation_direction":[0.,0.,-1.],"irradiance":[{"value":1.,"unit":"W/(m2*nm)"},{"value":2e9,"unit":"W/(m2*m)"}]},
        "source_provenance":"manufactured explicit collimated spectral source",
        "history":[{"time":{"value":0.,"unit":"s"},"scale":1.},{"time":{"value":1.,"unit":"h"},"scale":3.}],
        "history_interpolation":"piecewise_linear_prescribed_scale","history_provenance":"manufactured exposure history; not measured daylight",
        "samples_per_triangle":64,"seeds":[17,29,43],"relative_tolerance":0.02,"maximum_geometry_rounding_error_m":1e-9
    })
}

#[test]
fn direct_triangle_request_preserves_explicit_units_missing_ageing_and_independent_identity() {
    let value = request();
    let input: CadSpectralTransportRequest = serde_json::from_value(value.clone()).unwrap();
    let prepared = input.illumination().unwrap();
    assert_eq!(prepared.normalized.source_values_si, [1e9, 2e9]);
    assert_eq!(prepared.integrated_history_scale_s, 7200.);
    assert_eq!(
        input.scene.validate().unwrap(),
        ["synthetic_opaque.ageing_action"]
    );
    let id = digest(&input).unwrap();
    for mutate in [
        ("samples_per_triangle", serde_json::json!(128)),
        ("seeds", serde_json::json!([19, 31, 47])),
        (
            "source_provenance",
            serde_json::json!("different documented source"),
        ),
    ] {
        let mut changed = value.clone();
        changed[mutate.0] = mutate.1;
        let changed: CadSpectralTransportRequest = serde_json::from_value(changed).unwrap();
        changed.illumination().unwrap();
        assert_ne!(digest(&changed).unwrap(), id);
    }
}

#[test]
fn triangle_request_rejects_weakened_gate_implicit_physics_unknown_keys_and_spectral_drift() {
    let value = request();
    for (key, change) in [
        ("relative_tolerance", serde_json::json!(0.03)),
        ("maximum_geometry_rounding_error_m", serde_json::json!(1e-6)),
        ("samples_per_triangle", serde_json::json!(32)),
        ("seeds", serde_json::json!([17, 17, 43])),
        ("formulation", serde_json::json!("all_bounce")),
        (
            "source",
            serde_json::json!({"kind":"isotropic","radiance":[{"value":1.,"unit":"W/(m2*sr*nm)"},{"value":2.,"unit":"W/(m2*sr*nm)"}]}),
        ),
    ] {
        let mut changed = value.clone();
        changed[key] = change;
        let input: CadSpectralTransportRequest = serde_json::from_value(changed).unwrap();
        assert!(input.illumination().is_err(), "{key}");
    }
    for (key, change) in [
        ("execute", serde_json::json!(true)),
        ("source_path", serde_json::json!("/etc/passwd")),
    ] {
        let mut changed = value.clone();
        changed[key] = change;
        assert!(serde_json::from_value::<CadSpectralTransportRequest>(changed).is_err());
    }
    let mut changed = value;
    changed["scene"]["materials"][0]["response"]["value"]["reflectance"] =
        serde_json::json!([0.4, 0.4]);
    let input: CadSpectralTransportRequest = serde_json::from_value(changed).unwrap();
    assert!(input.illumination().is_err());
}

#[test]
fn optical_results_require_original_named_region_seed_without_time_or_interpolation_inference() {
    use harbor_cad::cad_transport_results::CadOpticalResultsRequest;
    let value = serde_json::json!({"schema_version":1,"job_id":"00000000-0000-0000-0000-000000000001","seed":17,"region_name":"solid"});
    let request: CadOpticalResultsRequest = serde_json::from_value(value.clone()).unwrap();
    request.validate().unwrap();
    for (key, change) in [
        ("seed", serde_json::json!(0)),
        ("region_name", serde_json::json!("../solid")),
        ("job_id", serde_json::json!("source")),
        ("schema_version", serde_json::json!(2)),
    ] {
        let mut changed = value.clone();
        changed[key] = change;
        let request: CadOpticalResultsRequest = serde_json::from_value(changed).unwrap();
        assert!(request.validate().is_err());
    }
    for key in ["time_s", "interpolation", "execute", "temperature_k"] {
        let mut changed = value.clone();
        changed[key] = serde_json::json!(0);
        assert!(serde_json::from_value::<CadOpticalResultsRequest>(changed).is_err());
    }
}
