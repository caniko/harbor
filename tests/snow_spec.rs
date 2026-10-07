use harbor_cad::{
    contracts::ExecutionPlan, snow::SnowReferenceSpec, thermal::ThermalReferenceSpec,
};

fn fixture() -> SnowReferenceSpec {
    serde_json::from_str(include_str!("../examples/snow-reference.json")).unwrap()
}

#[test]
fn prescribed_snow_conserves_series_heat_flux_and_binds_original_si_inputs_to_the_thermal_approval()
{
    let spec = fixture();
    let prepared = spec.prepare().unwrap();
    // Independent series resistance: R_air=1/50, R_snow=.002/.05.
    assert!((prepared.effective_convection_w_m2_k - 50. / 3.).abs() < 1e-13);
    assert!((prepared.initial_heat_loss_w - 1. / 30.).abs() < 1e-14);
    assert!((prepared.snow_diffusion_time_s - 16.).abs() < 1e-13);
    assert!((prepared.omitted_capacity_ratio - 2. / 243.).abs() < 1e-15);
    assert!((prepared.diffusion_timescale_ratio - 0.008).abs() < 1e-15);
    assert!(!prepared.executed);
    assert_eq!(prepared.physical_validation, "unqualified");
    assert_eq!(
        prepared.native.ambient_history,
        spec.thermal.ambient_history
    );
    assert_eq!(prepared.native.heater_history, spec.thermal.heater_history);
    assert_eq!(prepared.native.size_m, spec.thermal.size_m);
    let original =
        ExecutionPlan::thermal_reference(spec.thermal.clone(), "research".into()).unwrap();
    let insulated =
        ExecutionPlan::thermal_reference(prepared.native.clone(), "research".into()).unwrap();
    assert_ne!(original.id().unwrap(), insulated.id().unwrap());
    assert_ne!(
        original.science_id().unwrap(),
        insulated.science_id().unwrap()
    );
    assert_eq!(insulated.schema_version, 6);
    let mut changed = spec.clone();
    changed
        .snow
        .provenance
        .push_str("; independent prescription");
    let annotated =
        ExecutionPlan::thermal_reference(changed.prepare().unwrap().native, "research".into())
            .unwrap();
    assert_ne!(insulated.id().unwrap(), annotated.id().unwrap());
    let mut corrupted = prepared.native.clone();
    corrupted.convection_w_m2_k *= 1.01;
    assert!(corrupted.validate().is_err());
    let serialized = serde_json::to_value(&prepared.native).unwrap();
    let roundtrip: ThermalReferenceSpec = serde_json::from_value(serialized).unwrap();
    roundtrip.validate().unwrap();
}

#[test]
fn snow_prescription_rejects_unresolved_storage_melting_partial_coverage_and_invented_opening_claims()
 {
    let original = serde_json::to_value(fixture()).unwrap();
    for (field, value) in [
        ("coverage", serde_json::json!("partial")),
        ("model", serde_json::json!("deposition")),
        ("opening_model", serde_json::json!("blocked_from_image")),
        ("synthetic", serde_json::json!(false)),
        ("thickness", serde_json::json!({"value":0.,"unit":"m"})),
        (
            "conductivity",
            serde_json::json!({"value":0.05,"unit":"W/m2"}),
        ),
        ("maximum_omitted_capacity_ratio", serde_json::json!(0.03)),
        ("maximum_diffusion_timescale_ratio", serde_json::json!(0.03)),
        (
            "density",
            serde_json::json!({"value":10000.,"unit":"kg/m3"}),
        ),
        ("provenance", serde_json::json!("")),
    ] {
        let mut changed = original.clone();
        changed["snow"][field] = value;
        let spec: SnowReferenceSpec = serde_json::from_value(changed).unwrap();
        assert!(spec.prepare().is_err(), "{field}");
    }
    let mut changed = fixture();
    changed.thermal.initial_temperature_k = 273.15;
    changed.thermal.material_temperature_domain_k[1] = 283.15;
    assert!(changed.prepare().is_err());
    let mut changed = fixture();
    changed.thermal.observation_times_s[0] = 10.;
    assert!(changed.prepare().is_err());
    let mut changed = fixture();
    changed.thermal.heater_history[0][1] = 10.;
    changed.thermal.heater_history[1][1] = 10.;
    assert!(changed.prepare().is_err());
    let mut changed = original.clone();
    changed["snow"]["blocked_openings"] = serde_json::json!(["vent"]);
    assert!(serde_json::from_value::<SnowReferenceSpec>(changed).is_err());
}
