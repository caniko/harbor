use harbor_cad::science::Quantity;

fn q(value: f64, unit: &str) -> Quantity {
    Quantity {
        value,
        unit: unit.into(),
    }
}

#[test]
fn absolute_temperatures_and_intervals_are_not_interchangeable() {
    assert!((q(-40., "degC").si("temperature").unwrap() - 233.15).abs() < 1e-12);
    assert_eq!(
        q(10., "delta_degC").si("temperature_interval").unwrap(),
        10.
    );
    assert!(q(10., "degC").si("temperature_interval").is_err());
    assert!(q(10., "delta_degC").si("temperature").is_err());
    assert!(q(5., "W").si("energy").is_err());
}

#[test]
fn spectral_density_and_dose_preserve_their_integrals_under_si_conversion() {
    let spectral = q(2., "W/(m2*nm)").si("spectral_irradiance").unwrap();
    let bandwidth = q(50., "nm").si("length").unwrap();
    assert!((spectral * bandwidth - 100.).abs() < 1e-12);
    let dose = q(100., "W/m2").si("irradiance").unwrap() * q(2., "h").si("time").unwrap();
    assert_eq!(dose, q(200., "Wh/m2").si("radiant_exposure").unwrap());
    assert_eq!(
        q(0.5, "kW").si("power").unwrap() * q(1., "h").si("time").unwrap(),
        q(1800., "kJ").si("energy").unwrap()
    );
    assert!((q(100., "mm2").si("area").unwrap() - 1e-4).abs() < 1e-18);
    assert!(q(f64::MAX, "W/(m2*nm)").si("spectral_irradiance").is_err());
}
