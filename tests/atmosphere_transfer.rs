use harbor_cad::{
    atmosphere::AtmosphericReferenceSpec,
    atmosphere_transfer::{AtmosphericTransferRequest, reference},
    contracts::ExecutionPlan,
    radiation::{SpectralReferenceSpec, SpectralSource},
    science::Quantity,
};

fn fixture() -> (AtmosphericReferenceSpec, AtmosphericTransferRequest, String) {
    let mut source: AtmosphericReferenceSpec =
        serde_json::from_str(include_str!("../examples/atmosphere-reference.json")).unwrap();
    source.mu_bins = 8;
    source.phi_bins = 8;
    for value in &mut source.toa_irradiance {
        value.value = 1000.;
    }
    let prepared = source.prepare().unwrap();
    let mut receiver: SpectralReferenceSpec =
        serde_json::from_str(include_str!("../examples/spectral-reference.json")).unwrap();
    receiver.wavelengths = source.wavelengths.clone();
    receiver.absorptivity = vec![0.5; 3];
    receiver.ageing_action = vec![0.25; 3];
    receiver.source = SpectralSource::Directional {
        propagation_direction: prepared.propagation_direction,
        irradiance: source.toa_irradiance.clone(),
    };
    let request = AtmosphericTransferRequest {
        schema_version: 1,
        source_job: "00000000-0000-0000-0000-000000000001".into(),
        receiver,
        angular_mapping: "native_midpoint_solid_angle_quadrature".into(),
        maximum_relative_conservation_error: 1e-10,
    };
    let raw = prepared
        .wavelengths_nm
        .iter()
        .map(|wl| {
            let mut row = vec![
                format!("{wl:.3}"),
                "0".into(),
                std::f64::consts::PI.to_string(),
                "0".into(),
            ];
            row.extend(prepared.umu.iter().flat_map(|mu| {
                std::iter::repeat_n(if *mu < 0. { "1".into() } else { "0".into() }, 8)
            }));
            row.join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n");
    (source, request, raw)
}

#[test]
fn original_atmospheric_midpoints_preserve_surface_units_area_and_separate_optical_dose() {
    let (source, mut request, raw) = fixture();
    let original = reference(&source, &request, &raw).unwrap();
    for value in &original.incident_w_m2_nm {
        assert!((value - std::f64::consts::PI).abs() < 1e-12);
    }
    assert_eq!(original.direct_w_m2_nm, [0.; 3]);
    assert!((original.incident_irradiance_w_m2 - 120. * std::f64::consts::PI).abs() < 1e-10);
    assert_eq!(
        original.absorbed_irradiance_w_m2,
        0.5 * original.incident_irradiance_w_m2
    );
    assert_eq!(
        original.ageing_weighted_irradiance_w_m2,
        0.25 * original.incident_irradiance_w_m2
    );
    assert_eq!(
        original.absorbed_exposure_j_m2,
        original.absorbed_irradiance_w_m2 * 3600.
    );
    request.receiver.sensor_width = Quantity {
        value: 2. * request.receiver.sensor_width.value,
        unit: request.receiver.sensor_width.unit.clone(),
    };
    let enlarged = reference(&source, &request, &raw).unwrap();
    assert_eq!(
        enlarged.incident_irradiance_w_m2,
        original.incident_irradiance_w_m2
    );
    assert_eq!(enlarged.sensor_area_m2, 2. * original.sensor_area_m2);
    request.receiver.sensor_normal = [0., 0., -1.];
    assert_eq!(
        reference(&source, &request, &raw)
            .unwrap()
            .incident_irradiance_w_m2,
        0.
    );
}

#[test]
fn anisotropic_originals_keep_native_phi_sensor_position_and_propagation_sign() {
    let (source, mut request, _) = fixture();
    let prepared = source.prepare().unwrap();
    let irradiance = std::f64::consts::PI / 8.;
    let raw = prepared
        .wavelengths_nm
        .iter()
        .map(|wl| {
            let mut row = vec![
                format!("{wl:.3}"),
                "0".into(),
                irradiance.to_string(),
                "0".into(),
            ];
            row.extend(prepared.umu.iter().flat_map(|mu| {
                (0..8).map(move |phi| {
                    if *mu < 0. && phi == 0 {
                        "1".into()
                    } else {
                        "0".into()
                    }
                })
            }));
            row.join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n");
    request.receiver.sensor_normal = [0., 1., 0.];
    // Native phi=22.5 is a sensor North-East of the source: propagation has
    // positive North/East components, so the South-facing receiver is lit.
    assert_eq!(
        reference(&source, &request, &raw)
            .unwrap()
            .incident_irradiance_w_m2,
        0.
    );
    request.receiver.sensor_normal = [0., -1., 0.];
    assert!(
        reference(&source, &request, &raw)
            .unwrap()
            .incident_irradiance_w_m2
            > 0.
    );
}

#[test]
fn transfer_rejects_source_knots_direction_weak_gates_and_unexecuted_sources() {
    let (source, request, raw) = fixture();
    for kind in 0..4 {
        let mut bad = request.clone();
        match kind {
            0 => bad.receiver.wavelengths[0].value += 0.001,
            1 => {
                bad.receiver.source = SpectralSource::Directional {
                    propagation_direction: [0., 0., -1.],
                    irradiance: source.toa_irradiance.clone(),
                }
            }
            2 => bad.maximum_relative_conservation_error = 0.01,
            _ => bad.receiver.occlusion = "full_directional_occluder".into(),
        }
        assert!(reference(&source, &bad, &raw).is_err());
    }
    let root = tempfile::tempdir().unwrap();
    let store = harbor_cad::storage::Store::open(&root.path().join("state")).unwrap();
    let plan = ExecutionPlan::atmospheric_reference(source, "research".into()).unwrap();
    let job = store.submit(&plan, "unexecuted").unwrap();
    let mut actual = request;
    actual.source_job = job.id;
    assert!(matches!(
        harbor_cad::atmosphere_transfer::prepare(&store, &actual),
        Err(harbor_cad::Error::Unqualified(_))
    ));
}

#[test]
fn direct_originals_use_native_attenuation_and_spectral_unit_conversion() {
    let (mut source, mut request, _) = fixture();
    source.model = "transparent_reference".into();
    let prepared = source.prepare().unwrap();
    let raw = prepared
        .wavelengths_nm
        .iter()
        .zip(&prepared.toa_irradiance_w_m2_nm)
        .map(|(wl, toa)| {
            let mut row = vec![
                format!("{wl:.3}"),
                format!("{:.6e}", -toa * prepared.propagation_direction[2]),
                "0".into(),
                "0".into(),
            ];
            row.extend(std::iter::repeat_n("0".into(), 128));
            row.join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n");
    let original = reference(&source, &request, &raw).unwrap();
    assert_eq!(original.diffuse_w_m2_nm, [0.; 3]);
    for value in &mut request.receiver.wavelengths {
        value.value *= 1e-9;
        value.unit = "m".into();
    }
    let SpectralSource::Directional { irradiance, .. } = &mut request.receiver.source else {
        unreachable!()
    };
    for value in irradiance {
        value.value *= 1e9;
        value.unit = "W/(m2*m)".into();
    }
    let converted = reference(&source, &request, &raw).unwrap();
    assert!(
        (original.incident_irradiance_w_m2 / converted.incident_irradiance_w_m2 - 1.).abs() < 1e-12
    );
    request.receiver.sensor_normal = [0., -1., 0.];
    let tilted = reference(&source, &request, &raw).unwrap();
    assert!(
        (tilted.incident_irradiance_w_m2 / original.incident_irradiance_w_m2
            - 30_f64.to_radians().tan())
        .abs()
            < 1e-12
    );
    request.receiver.sensor_normal = [0., 1., 0.];
    assert_eq!(
        reference(&source, &request, &raw)
            .unwrap()
            .incident_irradiance_w_m2,
        0.
    );
}
