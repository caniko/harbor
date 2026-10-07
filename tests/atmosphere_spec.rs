use harbor_cad::atmosphere::AtmosphericReferenceSpec;
use harbor_cad::contracts::{ExecutionPlan, StageOperation};

#[test]
fn atmospheric_worker_recipe_has_a_strict_independent_approval_and_bounded_full_sphere_budget() {
    let spec: AtmosphericReferenceSpec =
        serde_json::from_str(include_str!("../examples/atmosphere-reference.json")).unwrap();
    let plan = ExecutionPlan::atmospheric_reference(spec.clone(), "research".into()).unwrap();
    assert_eq!(plan.schema_version, 14);
    assert_eq!(
        plan.stages[0].operation,
        StageOperation::AtmosphericReference
    );
    assert_eq!(plan.stages[1].dependencies, ["atmosphere"]);
    assert!(plan.case.is_none() && plan.observation.retained_times_s.is_empty());
    assert!(plan.stages[0].ram_bytes >= 512 * 1024 * 1024 && plan.stages[0].vram_bytes == 0);
    let encoded = serde_json::to_value(&plan).unwrap();
    assert_eq!(
        serde_json::from_value::<ExecutionPlan>(encoded.clone())
            .unwrap()
            .id()
            .unwrap(),
        plan.id().unwrap()
    );
    for version in 1..14 {
        let mut injected = encoded.clone();
        injected["schema_version"] = serde_json::json!(version);
        assert!(serde_json::from_value::<ExecutionPlan>(injected).is_err());
    }
    for field in ["case", "spectral", "freezing", "thermal_contact"] {
        let mut injected = encoded.clone();
        injected[field] = serde_json::Value::Null;
        assert!(serde_json::from_value::<ExecutionPlan>(injected).is_err());
    }
    let mut understated = plan.clone();
    understated.stages[0].ram_bytes = 1;
    assert!(understated.validate().is_err());
    let mut bad = plan.clone();
    bad.observation.retained_times_s = vec![0.];
    assert!(bad.validate().is_err());
    assert!(ExecutionPlan::atmospheric_reference(spec.clone(), "ci".into()).is_err());
    let mut changed = spec;
    changed.phi_bins = 64;
    let next = ExecutionPlan::atmospheric_reference(changed, "research".into()).unwrap();
    assert_ne!(next.id().unwrap(), plan.id().unwrap());
    assert!(next.observation.max_artifact_bytes > plan.observation.max_artifact_bytes);
}

#[test]
fn atmospheric_original_reconstruction_preserves_rows_and_rejects_changed_angular_energy() {
    let mut spec: AtmosphericReferenceSpec =
        serde_json::from_str(include_str!("../examples/atmosphere-reference.json")).unwrap();
    spec.model = "transparent_reference".into();
    let prepared = spec.prepare().unwrap();
    let text = prepared
        .wavelengths_nm
        .iter()
        .zip(&prepared.toa_irradiance_w_m2_nm)
        .map(|(wl, toa)| {
            let mut columns = vec![
                format!("{wl:.3}"),
                format!("{:.6e}", toa * 30f64.to_radians().cos()),
                "0".into(),
                "0".into(),
            ];
            columns.extend(std::iter::repeat_n("0".into(), 2048));
            columns.join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n");
    let result = harbor_cad::atmosphere::observations(&spec, &text).unwrap();
    assert!(result.maximum_angular_flux_error < 1e-6);
    assert_eq!(result.diffuse_downward_w_m2_nm, [0.; 3]);
    for bad in [
        text.replace("280.000", "280.001"),
        text.rsplit_once(' ').unwrap().0.into(),
        text.replacen(" 0 0 ", " 1 0 ", 1),
        text.replacen(" 0 ", " nan ", 1),
    ] {
        assert!(harbor_cad::atmosphere::observations(&spec, &bad).is_err());
    }
    spec.model = "clear_sky_molecular_crs".into();
    let excess = prepared
        .wavelengths_nm
        .iter()
        .map(|wl| {
            let mut row = vec![
                format!("{wl:.3}"),
                "0".into(),
                format!("{}", 10. * std::f64::consts::PI),
                "0".into(),
            ];
            row.extend(prepared.umu.iter().flat_map(|mu| {
                std::iter::repeat_n(if *mu < 0. { "10".into() } else { "0".into() }, 32)
            }));
            row.join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(harbor_cad::atmosphere::observations(&spec, &excess).is_err());
}

#[test]
fn atmospheric_preparation_preserves_solar_direction_full_angular_grid_units_and_missing_execution()
{
    let mut spec: AtmosphericReferenceSpec =
        serde_json::from_str(include_str!("../examples/atmosphere-reference.json")).unwrap();
    let prepared = spec.prepare().unwrap();
    assert!(!prepared.executed);
    assert!(
        prepared
            .wavelengths_nm
            .iter()
            .zip([280., 320., 400.])
            .all(|(a, b)| (a - b).abs() < 1e-12)
    );
    assert!(
        prepared
            .toa_irradiance_w_m2_nm
            .iter()
            .zip([1., 2., 3.])
            .all(|(a, b)| (a - b).abs() < 1e-14)
    );
    assert!(prepared.propagation_direction[0].abs() < 1e-14);
    assert!((prepared.propagation_direction[1] - 0.5).abs() < 1e-14);
    assert!((prepared.propagation_direction[2] + 30f64.to_radians().cos()).abs() < 1e-14);
    assert_eq!(prepared.umu.len(), 64);
    assert_eq!(prepared.phi_deg.len(), 32);
    assert!(prepared.umu.windows(2).all(|p| p[0] < p[1]));
    assert!(prepared.umu.iter().all(|v| *v != 0. && v.abs() < 1.));
    assert!(
        (prepared.angular_cell_solid_angle_sr * 64. * 32. - 4. * std::f64::consts::PI).abs()
            < 1e-13
    );
    assert!(prepared.transparent_horizontal_reference_w_m2_nm.is_none());
    spec.model = "transparent_reference".into();
    assert!(
        spec.prepare()
            .unwrap()
            .transparent_horizontal_reference_w_m2_nm
            .is_some()
    );
    let first = spec.prepare().unwrap().request_sha256;
    spec.solar_azimuth_deg = 90.;
    let west = spec.prepare().unwrap();
    assert!(
        (west.propagation_direction[0] - 0.5).abs() < 1e-14
            && west.propagation_direction[1].abs() < 1e-14
    );
    assert_ne!(first, west.request_sha256);
    spec.wavelengths.iter_mut().for_each(|q| {
        q.value *= 1e-9;
        q.unit = "m".into();
    });
    spec.toa_irradiance.iter_mut().for_each(|q| {
        q.value *= 1e9;
        q.unit = "W/(m2*m)".into();
    });
    assert!(
        spec.prepare()
            .unwrap()
            .toa_irradiance_w_m2_nm
            .iter()
            .zip([1., 2., 3.])
            .all(|(a, b)| (a - b).abs() < 1e-14)
    );
    let raw = serde_json::to_value(&spec).unwrap();
    for (field, value) in [
        ("backend", serde_json::json!("hip")),
        ("profile", serde_json::json!("../../credentials")),
        ("relative_tolerance", serde_json::json!(0.1)),
        ("solar_zenith_deg", serde_json::json!(90.)),
        ("phi_bins", serde_json::json!(100000)),
        ("albedo", serde_json::json!(0.4)),
    ] {
        let mut bad = raw.clone();
        bad[field] = value;
        let bad: AtmosphericReferenceSpec = serde_json::from_value(bad).unwrap();
        assert!(bad.prepare().is_err());
    }
    let mut bad = raw;
    bad["diffuse_isotropic"] = serde_json::json!(true);
    assert!(serde_json::from_value::<AtmosphericReferenceSpec>(bad).is_err());
    spec.wavelengths[0] = harbor_cad::science::Quantity {
        value: 280.0001,
        unit: "nm".into(),
    };
    assert!(
        spec.prepare().is_err(),
        "native %.3f output cannot retain sub-millinm input identity"
    );
}
