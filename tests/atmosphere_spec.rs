use harbor_cad::atmosphere::AtmosphericReferenceSpec;

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
