use harbor_cad::{
    contracts::digest,
    snow_openings::{SnowOpeningRequest, prepare},
};
use serde_json::json;

fn input() -> serde_json::Value {
    let q = |x| json!({"value": x, "unit":"m"});
    json!({"schema_version":1,"synthetic":true,"model":"prescribed_closed_prisms",
        "geometry_tolerance":{"value":1e-8,"unit":"m"},
        "provenance":"explicit synthetic geometry; no deposition model",
        "openings":[{"name":"vent","normal_axis":"z","plane":q(0.),
            "rectangle":[[q(0.),q(4.)],[q(0.),q(4.)]],"provenance":"prescribed planar aperture"}],
        "snow":[{"name":"left","bounds":[[q(0.),q(3.)],[q(0.),q(2.)],[q(-1.),q(1.)]],"provenance":"prescribed snow prism"},
            {"name":"right","bounds":[[q(2.),q(4.)],[q(1.),q(4.)],[q(-1.),q(1.)]],"provenance":"second prescribed prism"}]})
}

#[test]
fn overlapping_snow_counts_union_once_and_keeps_original_units_and_provenance() {
    let spec: SnowOpeningRequest = serde_json::from_value(input()).unwrap();
    let report = prepare(&spec).unwrap();
    let vent = &report.openings[0];
    assert_eq!(vent.original_area_m2, 16.);
    assert_eq!(vent.covered_area_m2, 11.);
    assert_eq!(vent.remaining_area_m2, 5.);
    assert_eq!(vent.covered_fraction, 11. / 16.);
    assert!(!report.executed);
    assert_eq!(report.physical_validation, "unqualified");
    assert_eq!(digest(&report.input).unwrap(), digest(&spec).unwrap());
    let mut equivalent = input();
    for opening in equivalent["openings"].as_array_mut().unwrap() {
        for dimension in opening["rectangle"].as_array_mut().unwrap() {
            for point in dimension.as_array_mut().unwrap() {
                point["value"] = json!(point["value"].as_f64().unwrap() * 1000.);
                point["unit"] = json!("mm");
            }
        }
    }
    let other: SnowOpeningRequest = serde_json::from_value(equivalent).unwrap();
    let normalized = prepare(&other).unwrap();
    assert_eq!(normalized.openings[0].covered_area_m2, vent.covered_area_m2);
    assert_ne!(normalized.preparation_id, report.preparation_id);
}

#[test]
fn all_normal_axes_and_closed_plane_tangency_preserve_small_remaining_gaps() {
    for axis in ["x", "y", "z"] {
        let normal = match axis {
            "x" => 0,
            "y" => 1,
            _ => 2,
        };
        let tangential: Vec<_> = (0..3).filter(|i| *i != normal).collect();
        let mut value = input();
        value["openings"][0]["normal_axis"] = json!(axis);
        value["snow"].as_array_mut().unwrap().truncate(1);
        let q = |x| json!({"value":x,"unit":"m"});
        let mut bounds = vec![json!([q(0.), q(1.)]); 3];
        bounds[normal] = json!([q(-1.), q(0.)]); // finite prism tangent to plane
        bounds[tangential[0]] = json!([q(0.), q(3.9999)]);
        bounds[tangential[1]] = json!([q(0.), q(4.)]);
        value["snow"][0]["bounds"] = json!(bounds);
        let spec: SnowOpeningRequest = serde_json::from_value(value).unwrap();
        let vent = &prepare(&spec).unwrap().openings[0];
        assert!((vent.remaining_area_m2 - 0.0004).abs() < 1e-14);
        assert!(vent.covered_fraction < 1.);
        let mut outside = spec;
        outside.openings[0].plane.value = 1.;
        assert_eq!(prepare(&outside).unwrap().openings[0].covered_area_m2, 0.);
    }
}

#[test]
fn integer_rectangles_match_independent_unit_square_coverage() {
    // Independent exhaustive area accounting, including overlapping/duplicate
    // projected prisms and rectangles clipped by each aperture edge.
    for seed in 0..64 {
        let mut value = input();
        let q = |x| json!({"value":x,"unit":"m"});
        let rectangles: Vec<_> = (0..5)
            .map(|i| {
                let x = (seed * 13 + i * 7) % 7 - 2;
                let y = (seed * 11 + i * 3) % 7 - 2;
                (x, x + 1 + (seed + i) % 4, y, y + 1 + (seed + 2 * i) % 4)
            })
            .collect();
        value["snow"] = json!(rectangles.iter().enumerate().map(|(i,(x0,x1,y0,y1))| json!({"name":format!("snow{i}"),"bounds":[[q(*x0),q(*x1)],[q(*y0),q(*y1)],[q(-1),q(1)]],"provenance":"synthetic exhaustive geometry"})).collect::<Vec<_>>());
        let expected = (0..4)
            .flat_map(|x| (0..4).map(move |y| (x, y)))
            .filter(|(x, y)| {
                rectangles
                    .iter()
                    .any(|(x0, x1, y0, y1)| x0 <= x && x < x1 && y0 <= y && y < y1)
            })
            .count();
        let spec: SnowOpeningRequest = serde_json::from_value(value).unwrap();
        assert_eq!(
            prepare(&spec).unwrap().openings[0].covered_area_m2,
            expected as f64
        );
    }
}

#[test]
fn snow_openings_reject_missing_units_unresolved_geometry_and_foreign_claims() {
    for value in [
        {
            let mut v = input();
            v["synthetic"] = json!(false);
            v
        },
        {
            let mut v = input();
            v["snow"][0]["bounds"][0][0]["unit"] = json!("kg");
            v
        },
        {
            let mut v = input();
            v["openings"][0]["rectangle"][0][1]["value"] = json!(1e-9);
            v
        },
        {
            let mut v = input();
            v["snow"][0]["bounds"][0][1]["value"] = json!(0.);
            v
        },
        {
            let mut v = input();
            v["snow"][1]["name"] = json!("left");
            v
        },
        {
            let mut v = input();
            v["provenance"] = json!("");
            v
        },
        {
            let mut v = input();
            v["schema_version"] = json!(2);
            v
        },
    ] {
        let spec: SnowOpeningRequest = serde_json::from_value(value).unwrap();
        assert!(prepare(&spec).is_err());
    }
    let mut unknown = input();
    unknown["convection_from_open_area"] = json!(true);
    assert!(serde_json::from_value::<SnowOpeningRequest>(unknown).is_err());
    let mut oversized = input();
    oversized["snow"] = json!(
        (0..32)
            .map(|i| {
                let mut prism = input()["snow"][0].clone();
                prism["name"] = json!(format!("snow{i}"));
                prism["provenance"] = json!("p".repeat(4096));
                prism
            })
            .collect::<Vec<_>>()
    );
    let oversized: SnowOpeningRequest = serde_json::from_value(oversized).unwrap();
    assert!(prepare(&oversized).is_err());
}
