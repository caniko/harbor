use harbor_cad::{science::Quantity, transfers::*};

fn q(value: f64, unit: &str) -> Quantity {
    Quantity {
        value,
        unit: unit.into(),
    }
}
fn map(quantity: TransferQuantity, unit: &str) -> ConservativeTransfer {
    ConservativeTransfer {
        schema_version: 1,
        source_artifact_sha256: "a".repeat(64),
        quantity,
        source: TransferEndpoint {
            mesh_sha256: "b".repeat(64),
            region: "heater".into(),
            association: Association::Cell,
            orientation: [0., 0., 1.],
            measures: vec![q(2., unit), q(1., unit)],
        },
        destination: TransferEndpoint {
            mesh_sha256: "c".repeat(64),
            region: "shell".into(),
            association: Association::Point,
            orientation: [0., 0., 1.],
            measures: vec![q(1., unit), q(2., unit)],
        },
        normal_mapping: NormalMapping::SameDirection,
        interpolation: "piecewise_constant_overlap".into(),
        overlaps: vec![
            Overlap {
                source: 0,
                destination: 0,
                measure: q(1., unit),
            },
            Overlap {
                source: 0,
                destination: 1,
                measure: q(1., unit),
            },
            Overlap {
                source: 1,
                destination: 1,
                measure: q(1., unit),
            },
        ],
        maximum_relative_conservation_error: 1e-12,
    }
}

#[test]
fn overlap_maps_bind_mesh_regions_and_explicit_physical_measures() {
    for (kind, unit) in [
        (TransferQuantity::Temperature, "J/K"),
        (TransferQuantity::SurfaceHeatFlux, "m2"),
        (TransferQuantity::Irradiance, "m2"),
        (TransferQuantity::MassDensity, "m3"),
    ] {
        let original = map(kind, unit);
        original.validate().unwrap();
        let round: ConservativeTransfer =
            serde_json::from_str(&serde_json::to_string(&original).unwrap()).unwrap();
        assert_eq!(round.id().unwrap(), original.id().unwrap());
        let mut missing = original.clone();
        missing.overlaps.pop();
        assert!(missing.validate().is_err());
        let mut duplicate = original.clone();
        duplicate.overlaps.push(duplicate.overlaps[0].clone());
        assert!(duplicate.validate().is_err());
        let mut shifted = original.clone();
        shifted.destination.mesh_sha256 = "d".repeat(64);
        assert_ne!(shifted.id().unwrap(), original.id().unwrap());
    }
}

#[test]
fn wrong_units_orientations_coverage_and_nonfinite_measures_are_rejected() {
    let original = map(TransferQuantity::Temperature, "J/K");
    let mut wrong = original.clone();
    wrong.overlaps[0].measure = q(1., "m2");
    assert!(wrong.validate().is_err());
    let mut wrong = original.clone();
    wrong.overlaps[0].destination = 8;
    assert!(wrong.validate().is_err());
    let mut wrong = original.clone();
    wrong.source.measures[0].value = f64::INFINITY;
    assert!(wrong.validate().is_err());
    let mut wrong = original.clone();
    wrong.destination.orientation = [1., 0., 0.];
    assert!(wrong.validate().is_err());
    let mut wrong = original.clone();
    wrong.maximum_relative_conservation_error = 1.;
    assert!(wrong.validate().is_err());
    let mut wrong = original;
    wrong.normal_mapping = NormalMapping::OpposingOutwardNormals;
    wrong.destination.orientation = [0., 0., -1.];
    assert!(wrong.validate().is_err());
}

#[test]
fn nonuniform_temperature_transfer_conserves_capacitance_and_constant_fields() {
    let map = map(TransferQuantity::Temperature, "J/K");
    let result = map.apply(&[0., 100.], "degC").unwrap();
    assert!((result.values_si[0] - 273.15).abs() < 1e-12);
    assert!((result.values_si[1] - 323.15).abs() < 1e-12);
    assert!((result.receipt.source_integral - 919.45).abs() < 1e-12);
    assert!((result.receipt.source_integral - result.receipt.destination_integral).abs() < 1e-12);
    let uniform = map.apply(&[233.15, 233.15], "K").unwrap();
    assert!(
        uniform
            .values_si
            .iter()
            .all(|v| (*v - 233.15).abs() < 1e-12)
    );
    assert_eq!(uniform.receipt.physical_validation, "unqualified");
    assert!(map.apply(&[233.15], "K").is_err());
    assert!(map.apply(&[233.15, 233.15], "delta_degC").is_err());
    assert!(map.apply(&[-1., 233.15], "K").is_err());
}

#[test]
fn signed_flux_and_mass_maps_do_not_infer_temperature_or_convection() {
    let mut flux = map(TransferQuantity::SurfaceHeatFlux, "m2");
    flux.normal_mapping = NormalMapping::OpposingOutwardNormals;
    flux.destination.orientation = [0., 0., -1.];
    let result = flux.apply(&[100., -200.], "W/m2").unwrap();
    assert_eq!(result.values_si, vec![-100., 50.]);
    assert_eq!(result.receipt.source_integral, 0.);
    assert_eq!(result.receipt.destination_integral, 0.);
    assert!(flux.apply(&[1., 2.], "m/s").is_err());
    let mass = map(TransferQuantity::MassDensity, "m3")
        .apply(&[1000., 500.], "kg/m3")
        .unwrap();
    assert_eq!(mass.values_si, vec![1000., 750.]);
    assert_eq!(mass.receipt.source_integral, 2500.);
    assert_eq!(mass.receipt.destination_integral, 2500.);
    assert!(
        map(TransferQuantity::Irradiance, "m2")
            .apply(&[-1., 3.], "W/m2")
            .is_err()
    );
    assert!(
        map(TransferQuantity::Temperature, "J/K")
            .apply(&[f64::MAX, f64::MAX], "K")
            .is_err()
    );
}

#[test]
fn cli_transfer_validation_is_bounded_and_does_not_claim_solver_execution() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("transfer.json");
    let transfer = map(TransferQuantity::Temperature, "J/K");
    std::fs::write(&file, serde_json::to_vec(&transfer).unwrap()).unwrap();
    let run = || {
        std::process::Command::new(env!("CARGO_BIN_EXE_harbor-cad"))
            .args(["case", "validate-transfer"])
            .arg(&file)
            .output()
            .unwrap()
    };
    let output = run();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let receipt: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(receipt["transfer_id"], transfer.id().unwrap());
    assert_eq!(receipt["executed"], false);
    assert_eq!(receipt["physical_validation"], "unqualified");
    let mut forged = serde_json::to_value(&transfer).unwrap();
    forged["quantity"] = "velocity".into();
    std::fs::write(&file, serde_json::to_vec(&forged).unwrap()).unwrap();
    assert!(!run().status.success());
}
