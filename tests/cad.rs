use harbor_cad::{cad::RegionSnapshot, contracts::*};

fn fixture() -> serde_json::Value {
    serde_json::json!({"synthetic":true,"gap_healing":false,"geometry_tolerance":{"value":0.00001,"unit":"m"},
        "regions":[{"name":"fluid","label":"synthetic fluid channel","volume_m3":0.000002,
        "bounds_m":[0.,0.02,0.,0.01,0.,0.01],"transform":[1.,0.,0.,0.,0.,1.,0.,0.,0.,0.,1.,0.,0.,0.,0.,1.],
        "triangles":12,"source_unit":"mm","stl_scale_to_m":0.001}]})
}

#[test]
fn named_cad_regions_preserve_si_bounds_and_original_transform_units() {
    let mut case = CaseSpec::reference();
    case.geometry_tolerance = harbor_cad::science::Quantity {
        value: 0.01,
        unit: "mm".into(),
    };
    let snapshot: RegionSnapshot = serde_json::from_value(fixture()).unwrap();
    snapshot.verify(&case, StageOperation::CadFixture).unwrap();
    assert_eq!(snapshot.regions[0].bounds_m[1], 0.02);
    assert_eq!(snapshot.regions[0].stl_scale_to_m, 0.001);
    assert!(snapshot.verify(&case, StageOperation::CadInspect).is_err());
}

#[test]
fn ambiguous_changed_or_unsupported_region_metadata_cannot_be_presented_as_verified() {
    let case = CaseSpec::reference();
    for (field, value) in [
        ("name", serde_json::json!("wall")),
        ("volume_m3", serde_json::json!(-1)),
        ("bounds_m", serde_json::json!([0., 0., 0., 0.01, 0., 0.01])),
        ("source_unit", serde_json::json!("m")),
        ("stl_scale_to_m", serde_json::json!(1.)),
        ("triangles", serde_json::json!(0)),
        (
            "transform",
            serde_json::json!([
                1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 0.
            ]),
        ),
    ] {
        let mut changed = fixture();
        changed["regions"][0][field] = value;
        let snapshot: RegionSnapshot = serde_json::from_value(changed).unwrap();
        assert!(snapshot.verify(&case, StageOperation::CadFixture).is_err());
    }
    for (field, value) in [
        ("synthetic", serde_json::json!(false)),
        ("gap_healing", serde_json::json!(true)),
        (
            "geometry_tolerance",
            serde_json::json!({"value":1.,"unit":"mm"}),
        ),
    ] {
        let mut changed = fixture();
        changed[field] = value;
        let snapshot: RegionSnapshot = serde_json::from_value(changed).unwrap();
        assert!(snapshot.verify(&case, StageOperation::CadFixture).is_err());
    }
    let mut duplicate = fixture();
    let first = duplicate["regions"][0].clone();
    duplicate["regions"].as_array_mut().unwrap().push(first);
    let snapshot: RegionSnapshot = serde_json::from_value(duplicate).unwrap();
    assert!(snapshot.verify(&case, StageOperation::CadFixture).is_err());
}
