use harbor_cad::{materials::*, recipes::*, science::Quantity};
fn q(value: f64, unit: &str) -> Quantity {
    Quantity {
        value,
        unit: unit.into(),
    }
}
fn known<T>(value: T) -> PhysicalInput<T> {
    PhysicalInput::Known {
        value,
        provenance: "synthetic numeric reference".into(),
        synthetic: true,
    }
}
fn curve(unit: &str, value: f64) -> TemperatureCurve {
    TemperatureCurve {
        interpolation: "piecewise_linear".into(),
        knots: vec![
            PropertyKnot {
                temperature: q(200., "K"),
                value: q(value, unit),
            },
            PropertyKnot {
                temperature: q(400., "K"),
                value: q(value, unit),
            },
        ],
    }
}
fn case() -> ColdRestartSpec {
    ColdRestartSpec {
        schema_version: 1,
        synthetic: false,
        geometry_sha256: "a".repeat(64),
        thermal_region: "enclosure".into(),
        material: ThermalMaterial {
            density: known(q(1000., "kg/m3")),
            conductivity: known(curve("W/(m*K)", 1.)),
            specific_heat: known(curve("J/(kg*K)", 1000.)),
        },
        minimum_valid_temperature: q(200., "K"),
        maximum_valid_temperature: q(400., "K"),
        initial_temperature: q(-40., "degC"),
        duration: q(1., "h"),
        history_interpolation: "piecewise_linear".into(),
        ambient_history: known(vec![
            AmbientKnot {
                time: q(0., "s"),
                temperature: q(-40., "degC"),
            },
            AmbientKnot {
                time: q(1., "h"),
                temperature: q(20., "degC"),
            },
        ]),
        heater_history: known(vec![
            PowerKnot {
                time: q(0., "s"),
                power: q(0., "W"),
            },
            PowerKnot {
                time: q(1., "h"),
                power: q(1., "kW"),
            },
        ]),
        convection_coefficient: known(curve("W/(m2*K)", 10.)),
        observation_times: vec![q(0., "s"), q(30., "min"), q(1., "h")],
        numerical_tolerance: 0.01,
        moisture: MoistureRisk::Missing {
            reason: "no humidity or observed surface-temperature history".into(),
        },
    }
}

#[test]
fn cold_restart_preserves_history_materials_heater_energy_and_separate_evidence_states() {
    let case = case();
    let report = case.inspect().unwrap();
    assert_eq!(report["prescribed_heater_energy_j"], 1_800_000.);
    assert_eq!(
        report["observation_times_s"],
        serde_json::json!([0., 1800., 3600.])
    );
    assert_eq!(report["inputs_complete"], false);
    assert_eq!(
        report["missing_inputs"],
        serde_json::json!(["moisture_risk"])
    );
    assert_eq!(report["synthetic"], true);
    assert_eq!(report["physical_validation"], "unqualified");
    assert_eq!(report["execution"], "not_requested");
    let original = serde_json::to_value(&case).unwrap();
    assert_eq!(original["initial_temperature"]["unit"], "degC");
    let round: ColdRestartSpec = serde_json::from_value(original).unwrap();
    assert_eq!(round.inspect().unwrap()["science_id"], report["science_id"]);
    let mut changed = case;
    changed.duration = q(2., "h");
    assert!(changed.inspect().is_err());
}

#[test]
fn histories_units_temperature_coverage_and_unknowns_never_get_synthetic_defaults() {
    let mut missing = case();
    missing.heater_history = PhysicalInput::Missing {
        reason: "operating power history unavailable".into(),
    };
    let report = missing.inspect().unwrap();
    assert!(report["prescribed_heater_energy_j"].is_null());
    assert!(
        report["missing_inputs"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("heater_history"))
    );
    let mut bad = case();
    bad.observation_times.push(q(1., "h"));
    assert!(bad.inspect().is_err());
    let mut bad = case();
    bad.initial_temperature = q(100., "K");
    assert!(bad.inspect().is_err());
    let mut bad = case();
    bad.material.specific_heat = known(curve("W/(m*K)", 1.));
    assert!(bad.inspect().is_err());
    let mut bad = case();
    bad.history_interpolation = "hold_or_guess".into();
    assert!(bad.inspect().is_err());
    let mut bad = case();
    bad.convection_coefficient = known(curve("m/s", 1.));
    assert!(bad.inspect().is_err());
    let mut insulated = case();
    insulated.convection_coefficient = known(curve("W/(m2*K)", 0.));
    insulated.inspect().unwrap();
}

#[test]
fn moisture_is_screening_inapplicable_missing_or_explicitly_unsupported() {
    let screen = MoistureRisk::DewPointScreening {
        air_temperature: q(20., "degC"),
        relative_humidity: 0.5,
        minimum_surface_temperature: q(5., "degC"),
        provenance: "reference air and surface fixture".into(),
    };
    let report = screen.inspect().unwrap();
    assert_eq!(report["status"], "screening");
    assert_eq!(report["condensation_risk"], true);
    if let MoistureRisk::DewPointScreening {
        air_temperature,
        relative_humidity,
        provenance,
        ..
    } = screen
    {
        let frost = MoistureRisk::DewPointScreening {
            air_temperature,
            relative_humidity,
            minimum_surface_temperature: q(-5., "degC"),
            provenance,
        };
        assert_eq!(frost.inspect().unwrap()["status"], "unsupported_screening");
    }
    assert!(
        MoistureRisk::Inapplicable {
            justification: "".into()
        }
        .inspect()
        .is_err()
    );
}
