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

fn diffuse_packets(
    source: &AtmosphericReferenceSpec,
    request: &AtmosphericTransferRequest,
) -> String {
    let prepared = source.prepare().unwrap();
    let mut text = String::from(
        "sample,knot_offset,x_m,y_m,z_m,towards_source_x,towards_source_y,towards_source_z,native_cosine,native_emitter_id,native_pdf,native_weight_w_m2_nm_0,native_weight_w_m2_nm_1,native_weight_w_m2_nm_2,native_weight_w_m2_nm_3\n",
    );
    for sample in 0..request.receiver.samples {
        let cell = sample as usize % 64;
        let i = cell / 8;
        let j = cell % 8;
        let direction =
            harbor_cad::atmosphere_transfer::propagation(prepared.umu[i], prepared.phi_deg[j]);
        let cosine = -prepared.umu[i];
        let weight = prepared.angular_cell_solid_angle_sr * 64.;
        for offset in (0..source.wavelengths.len()).step_by(4) {
            text.push_str(&format!("{sample},{offset},0,0,0,{},{},{},{cosine},angular-{i:03}-{j:03},{},{weight},{weight},{weight},{weight}\n", -direction[0], -direction[1], -direction[2], 1. / 64.));
        }
    }
    text
}

#[test]
fn independent_native_packet_reduction_preserves_every_knot_and_optical_channel() {
    use harbor_cad::atmosphere_transfer::{AtmosphericComponent, reconstruct_native_packets};
    let (source, mut request, raw) = fixture();
    request.receiver.samples = 1024;
    let text = diffuse_packets(&source, &request);
    let actual = reconstruct_native_packets(
        &source,
        &request,
        &raw,
        AtmosphericComponent::Diffuse,
        &text,
    )
    .unwrap();
    let expected = reference(&source, &request, &raw).unwrap();
    assert!((actual.incident_w_m2 / expected.incident_irradiance_w_m2 - 1.).abs() < 1e-14);
    assert!((actual.absorbed_w_m2 / expected.absorbed_irradiance_w_m2 - 1.).abs() < 1e-14);
    assert!((actual.ageing_w_m2 / expected.ageing_weighted_irradiance_w_m2 - 1.).abs() < 1e-14);
    for mutation in 0..6 {
        let mut lines = text.lines().map(str::to_owned).collect::<Vec<_>>();
        match mutation {
            0 => {
                lines.pop();
            }
            1 => {
                lines.push(lines[1].clone());
            }
            2 => {
                let mut row = lines[1].split(',').map(str::to_owned).collect::<Vec<_>>();
                row[2] = "0.00075".into();
                lines[1] = row.join(",");
            }
            3 => {
                for (index, pdf) in [(1, 0.5), (65, 1. / 126.)] {
                    let mut row = lines[index]
                        .split(',')
                        .map(str::to_owned)
                        .collect::<Vec<_>>();
                    row[10] = pdf.to_string();
                    for lane in row.iter_mut().skip(11) {
                        *lane = (std::f64::consts::PI / 32. / pdf).to_string();
                    }
                    lines[index] = row.join(",");
                }
            }
            4 => {
                let mut row = lines[1].split(',').map(str::to_owned).collect::<Vec<_>>();
                row[7] = "0".into();
                lines[1] = row.join(",");
            }
            _ => {
                lines[1] = lines[1].replace("angular-000-000", "angular-008-000");
            }
        }
        assert!(
            reconstruct_native_packets(
                &source,
                &request,
                &raw,
                AtmosphericComponent::Diffuse,
                &lines.join("\n")
            )
            .is_err(),
            "mutation {mutation}"
        );
    }
}

#[test]
fn original_multiknot_packets_reject_missing_offsets_changed_draws_padding_and_nonfinite_values() {
    use harbor_cad::atmosphere_transfer::{AtmosphericComponent, reconstruct_native_packets};
    let (mut source, mut request, raw) = fixture();
    source.wavelengths = (0..6)
        .map(|k| Quantity {
            value: 280. + f64::from(k) * 24.,
            unit: "nm".into(),
        })
        .collect();
    source.toa_irradiance = vec![source.toa_irradiance[0].clone(); 6];
    request.receiver.wavelengths = source.wavelengths.clone();
    request.receiver.source = SpectralSource::Directional {
        propagation_direction: source.prepare().unwrap().propagation_direction,
        irradiance: source.toa_irradiance.clone(),
    };
    request.receiver.absorptivity = vec![0., 1., 0.2, 0.7, 0.3, 0.8];
    request.receiver.ageing_action = vec![1., 0., 0.6, 0.9, 0.1, 0.4];
    request.receiver.samples = 1024;
    let angular = raw
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .skip(1)
        .collect::<Vec<_>>()
        .join(" ");
    let original = source
        .wavelengths
        .iter()
        .map(|wl| format!("{:.3} {angular}", wl.value))
        .collect::<Vec<_>>()
        .join("\n");
    let text = diffuse_packets(&source, &request);
    let actual = reconstruct_native_packets(
        &source,
        &request,
        &original,
        AtmosphericComponent::Diffuse,
        &text,
    )
    .unwrap();
    let expected = reference(&source, &request, &original).unwrap();
    assert!((actual.absorbed_w_m2 / expected.absorbed_irradiance_w_m2 - 1.).abs() < 1e-14);
    assert!((actual.ageing_w_m2 / expected.ageing_weighted_irradiance_w_m2 - 1.).abs() < 1e-14);
    for (index, column, value) in [(2, 1, "0"), (2, 2, "0.0001"), (2, 14, "0"), (2, 13, "NaN")] {
        let mut rows = text.lines().map(str::to_owned).collect::<Vec<_>>();
        let mut row = rows[index]
            .split(',')
            .map(str::to_owned)
            .collect::<Vec<_>>();
        row[column] = value.into();
        rows[index] = row.join(",");
        assert!(
            reconstruct_native_packets(
                &source,
                &request,
                &original,
                AtmosphericComponent::Diffuse,
                &rows.join("\n")
            )
            .is_err()
        );
    }
    let empty = format!(
        "{}\n{}",
        text.lines().next().unwrap(),
        (0..1024)
            .flat_map(|sample| [0, 4]
                .map(move |offset| format!("{sample},{offset},0,0,0,0,0,0,0,none,0,0,0,0,0")))
            .collect::<Vec<_>>()
            .join("\n")
    );
    let direct = reconstruct_native_packets(
        &source,
        &request,
        &original,
        AtmosphericComponent::Direct,
        &empty,
    )
    .unwrap();
    assert_eq!(
        [
            direct.incident_w_m2,
            direct.absorbed_w_m2,
            direct.ageing_w_m2
        ],
        [0.; 3]
    );
    assert!(
        reconstruct_native_packets(
            &source,
            &request,
            &original,
            AtmosphericComponent::Diffuse,
            &empty
        )
        .is_err()
    );
}
