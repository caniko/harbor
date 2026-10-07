use harbor_cad::{radiation::*, science::Quantity};

fn quantity(value: f64, unit: &str) -> Quantity {
    Quantity {
        value,
        unit: unit.into(),
    }
}
fn fixture() -> SpectralReferenceSpec {
    SpectralReferenceSpec {
        schema_version: 1,
        synthetic: true,
        backend: "cpu".into(),
        variant: "scalar_spectral".into(),
        precision: "Float32".into(),
        wavelengths: vec![quantity(280., "nm"), quantity(400., "nm")],
        source: SpectralSource::Directional {
            propagation_direction: [0., 0., -1.],
            irradiance: vec![quantity(1., "W/(m2*nm)"), quantity(3., "W/(m2*nm)")],
        },
        source_provenance: "synthetic fixed collimated spectrum; no atmospheric derivation".into(),
        sensor_width: quantity(1., "mm"),
        sensor_height: quantity(2., "mm"),
        sensor_normal: [0., 0., 1.],
        occlusion: "none".into(),
        absorptivity: vec![0.2, 0.8],
        optical_provenance:
            "explicit synthetic linear UV absorptivity; no visible RGB substitution".into(),
        ageing_action: vec![1., 0.5],
        ageing_provenance: "synthetic dimensionless action spectrum; no lifetime criterion".into(),
        history: vec![
            RadiantHistoryPoint {
                time: quantity(0., "s"),
                scale: 0.,
            },
            RadiantHistoryPoint {
                time: quantity(1., "h"),
                scale: 2.,
            },
        ],
        history_interpolation: "piecewise_linear_prescribed_scale".into(),
        history_provenance: "explicit synthetic ramp with fixed geometry and angular source".into(),
        samples: 4096,
        seeds: [1, 2, 3],
        relative_tolerance: 0.02,
    }
}

#[test]
fn uv_spectral_units_and_exact_linear_products_preserve_dose_absorption_and_sensor_area_semantics()
{
    let spec = fixture();
    let prepared = spec.prepare().unwrap();
    assert!(!prepared.executed);
    assert!((prepared.incident_irradiance_w_m2 - 240.).abs() < 1e-11);
    // Exact integral of (1+2x)(.2+.6x) over 120 nm is 132 W/m2;
    // endpoint-product trapezoidal quadrature incorrectly gives 156 W/m2.
    assert!((prepared.absorbed_irradiance_w_m2 - 132.).abs() < 1e-11);
    assert!((prepared.ageing_weighted_irradiance_w_m2 - 170.).abs() < 1e-11);
    assert_eq!(prepared.integrated_history_scale_s, 3600.);
    assert!((prepared.absorbed_exposure_j_m2 - 475200.).abs() < 1e-8);
    assert!((prepared.incident_power_w - 0.00048).abs() < 1e-15);
    let mut si = spec.clone();
    si.wavelengths = vec![quantity(280e-9, "m"), quantity(400e-9, "m")];
    if let SpectralSource::Directional { irradiance, .. } = &mut si.source {
        *irradiance = vec![quantity(1e9, "W/(m2*m)"), quantity(3e9, "W/(m2*m)")];
    }
    let si_prepared = si.prepare().unwrap();
    for (nm, metres) in prepared
        .normalized
        .wavelengths_m
        .iter()
        .zip(&si_prepared.normalized.wavelengths_m)
    {
        assert!((nm / metres - 1.).abs() < 1e-15);
    }
    assert_eq!(
        prepared.normalized.source_values_si,
        si_prepared.normalized.source_values_si
    );
    assert!(
        (prepared.absorbed_exposure_j_m2 / si_prepared.absorbed_exposure_j_m2 - 1.).abs() < 1e-15
    );
    assert_ne!(prepared.preparation_id, si_prepared.preparation_id);
    let mut large = spec.clone();
    large.sensor_width.value *= 2.;
    let result = large.prepare().unwrap();
    assert_eq!(
        result.incident_irradiance_w_m2,
        prepared.incident_irradiance_w_m2
    );
    assert_eq!(result.incident_power_w, 2. * prepared.incident_power_w);
    assert_ne!(result.preparation_id, prepared.preparation_id);
}

#[test]
fn declared_angular_source_and_occlusion_are_not_collapsed_to_unqualified_scalar_irradiance() {
    let mut spec = fixture();
    spec.sensor_normal = [0.6, 0., 0.8];
    let inclined = spec.prepare().unwrap();
    assert!((inclined.incident_irradiance_w_m2 - 192.).abs() < 1e-11);
    spec.occlusion = "full_directional_occluder".into();
    assert_eq!(spec.prepare().unwrap().incident_irradiance_w_m2, 0.);
    spec.occlusion = "none".into();
    spec.sensor_normal = [0., 0., -1.];
    assert_eq!(spec.prepare().unwrap().incident_irradiance_w_m2, 0.);
    spec.source = SpectralSource::Isotropic {
        radiance: vec![quantity(1., "W/(m2*sr*nm)"), quantity(3., "W/(m2*sr*nm)")],
    };
    let diffuse = spec.prepare().unwrap();
    assert!((diffuse.incident_irradiance_w_m2 - 240. * std::f64::consts::PI).abs() < 1e-10);
    spec.occlusion = "full_directional_occluder".into();
    assert!(spec.prepare().is_err());
    spec = fixture();
    spec.absorptivity[0] = 1.01;
    assert!(spec.prepare().is_err());
    spec = fixture();
    spec.wavelengths.reverse();
    assert!(spec.prepare().is_err());
    spec = fixture();
    spec.precision = "Float64".into();
    assert!(spec.prepare().is_err());
    spec = fixture();
    spec.variant = "cuda_ad_spectral".into();
    assert!(spec.prepare().is_err());
    spec = fixture();
    spec.relative_tolerance = 0.1;
    assert!(spec.prepare().is_err());
    spec = fixture();
    spec.history[0].time.value = 1.;
    assert!(spec.prepare().is_err());
    spec = fixture();
    spec.history[1].scale = -1.;
    assert!(spec.prepare().is_err());
    spec = fixture();
    spec.optical_provenance.clear();
    assert!(spec.prepare().is_err());
    let mut value = serde_json::to_value(fixture()).unwrap();
    value["temperature_k"] = serde_json::json!(273.15);
    assert!(serde_json::from_value::<SpectralReferenceSpec>(value).is_err());
}
