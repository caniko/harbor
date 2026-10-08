use harbor_cad::{materials::*, science::Quantity};
fn q(value: f64, unit: &str) -> Quantity {
    Quantity {
        value,
        unit: unit.into(),
    }
}
fn curve(unit: &str) -> TemperatureCurve {
    TemperatureCurve {
        interpolation: "piecewise_linear".into(),
        knots: vec![
            PropertyKnot {
                temperature: q(-40., "degC"),
                value: q(100., unit),
            },
            PropertyKnot {
                temperature: q(60., "degC"),
                value: q(200., unit),
            },
        ],
    }
}
fn known<T>(value: T) -> PhysicalInput<T> {
    PhysicalInput::Known {
        value,
        provenance: "explicit synthetic contract fixture".into(),
        synthetic: true,
    }
}

#[test]
fn material_curves_normalize_absolute_temperature_and_refuse_extrapolation() {
    let property = curve("J/(kg*K)");
    assert!((property.value_si(283.15, "specific_heat").unwrap() - 150.).abs() < 1e-12);
    assert!((property.value_si(233.15, "specific_heat").unwrap() - 100.).abs() < 1e-12);
    assert!(property.value_si(200., "specific_heat").is_err());
    assert!(property.value_si(400., "specific_heat").is_err());
    assert!(property.value_si(283.15, "thermal_conductivity").is_err());
    let mut unordered = property.clone();
    unordered.knots.reverse();
    assert!(unordered.validate("specific_heat").is_err());
    let mut interval = property;
    interval.knots[0].temperature.unit = "delta_degC".into();
    assert!(interval.validate("specific_heat").is_err());
}

#[test]
fn unknown_material_inputs_remain_missing_and_do_not_become_defaults() {
    let material = ThermalMaterial {
        density: PhysicalInput::Missing {
            reason: "no cold-material density measurement".into(),
        },
        conductivity: known(curve("W/(m*K)")),
        specific_heat: known(curve("J/(kg*K)")),
    };
    assert_eq!(material.validate(233.15, 333.15).unwrap(), vec!["density"]);
    assert!(material.validate(200., 333.15).is_err());
    let mut incomplete = material;
    incomplete.density = PhysicalInput::Missing { reason: "".into() };
    assert!(incomplete.validate(233.15, 333.15).is_err());
    incomplete.density = known(q(0., "kg/m3"));
    assert!(incomplete.validate(233.15, 333.15).is_err());
}
