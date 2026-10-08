use harbor_cad::{cad_source::CadMeshDescriptor, cad_triangles::verify_binary_box};

fn geometry() -> CadMeshDescriptor {
    serde_json::from_value(serde_json::json!({"schema_version":1,"synthetic":true,"backend":"cpu","formulation":"imported_axis_aligned_box","geometry_provenance":"synthetic translated native box; no executed importer claim","brep_file":"solid.brep","brep_sha256":"a".repeat(64),"brep_bytes":100,"region_name":"solid","bounds_m":[0.1,0.12,-0.02,-0.01,0.3,0.31],"volume_m3":2e-6,"source_unit":"mm","scale_to_m":0.001,"placement_translation_unit":"mm","source_transform":[1.,0.,0.,100.,0.,1.,0.,-20.,0.,0.,1.,300.,0.,0.,0.,1.],"resolution":2,"geometry_tolerance_m":1e-6,"volume_relative_tolerance":1e-10})).unwrap()
}

fn box_stl() -> Vec<u8> {
    let points = [
        [100f32, -20., 300.],
        [120., -20., 300.],
        [120., -10., 300.],
        [100., -10., 300.],
        [100., -20., 310.],
        [120., -20., 310.],
        [120., -10., 310.],
        [100., -10., 310.],
    ];
    let faces = [
        ([0, 4, 7, 3], [-1f32, 0., 0.]),
        ([1, 2, 6, 5], [1., 0., 0.]),
        ([0, 1, 5, 4], [0., -1., 0.]),
        ([3, 7, 6, 2], [0., 1., 0.]),
        ([0, 3, 2, 1], [0., 0., -1.]),
        ([4, 5, 6, 7], [0., 0., 1.]),
    ];
    let mut data = vec![0u8; 80];
    data[..5].copy_from_slice(b"solid"); // Binary headers can start with the ASCII keyword.
    data.extend(12u32.to_le_bytes());
    for (face, normal) in faces {
        for indices in [[face[0], face[1], face[2]], [face[0], face[2], face[3]]] {
            for value in normal
                .into_iter()
                .chain(indices.into_iter().flat_map(|i| points[i]))
            {
                data.extend(value.to_le_bytes());
            }
            data.extend(0u16.to_le_bytes());
        }
    }
    data
}

#[test]
fn spectral_materials_preserve_missing_inputs_and_refuse_energy_coverage_or_tag_drift() {
    use harbor_cad::cad_spectral::CadSpectralSceneRequest;
    let original: serde_json::Value =
        serde_json::from_str(include_str!("../examples/cad-spectral-scene.json")).unwrap();
    let request: CadSpectralSceneRequest = serde_json::from_value(original.clone()).unwrap();
    assert_eq!(
        request.validate().unwrap(),
        ["synthetic_opaque.ageing_action"]
    );
    for (path, value) in [
        (
            "/materials/0/response/value/absorptivity",
            serde_json::json!([0.8, 0.7]),
        ),
        (
            "/materials/0/response/value/reflectance",
            serde_json::json!([0.2]),
        ),
        (
            "/materials/0/response/value/formulation",
            serde_json::json!("glass"),
        ),
        (
            "/materials/0/response/value/interpolation",
            serde_json::json!("nearest"),
        ),
        ("/materials/0/response/provenance", serde_json::json!("")),
        (
            "/materials/0/ageing_action",
            serde_json::json!({"availability":"known","value":[0.2],"provenance":"explicit reference","synthetic":true}),
        ),
        ("/assignments/0/material_name", serde_json::json!("unknown")),
        ("/assignments/0/region_name", serde_json::json!("../solid")),
        ("/wavelengths/1/value", serde_json::json!(200.)),
        ("/wavelengths/1/unit", serde_json::json!("K")),
        ("/schema_version", serde_json::json!(2)),
    ] {
        let mut changed = original.clone();
        *changed.pointer_mut(path).unwrap() = value;
        let decoded = serde_json::from_value::<CadSpectralSceneRequest>(changed);
        assert!(
            decoded.is_err() || decoded.unwrap().validate().is_err(),
            "{path}"
        );
    }
    let mut changed = original.clone();
    changed["materials"][0]["response"] =
        serde_json::json!({"availability":"missing","reason":"unmeasured UV optics"});
    let missing: CadSpectralSceneRequest = serde_json::from_value(changed).unwrap();
    assert_eq!(
        missing.validate().unwrap(),
        [
            "synthetic_opaque.response",
            "synthetic_opaque.ageing_action"
        ]
    );
    let mut changed = original.clone();
    let value = changed["assignments"][0].clone();
    changed["assignments"].as_array_mut().unwrap().push(value);
    assert!(
        serde_json::from_value::<CadSpectralSceneRequest>(changed)
            .unwrap()
            .validate()
            .is_err()
    );
    let mut changed = original.clone();
    let mut value = changed["materials"][0].clone();
    value["name"] = serde_json::json!("unused");
    changed["materials"].as_array_mut().unwrap().push(value);
    assert!(
        serde_json::from_value::<CadSpectralSceneRequest>(changed)
            .unwrap()
            .validate()
            .is_err()
    );
    let mut changed = original.clone();
    changed["materials"][0]["response"]["value"]["roughness"] = serde_json::json!(0.1);
    assert!(serde_json::from_value::<CadSpectralSceneRequest>(changed).is_err());
}

#[test]
fn complete_translated_triangles_preserve_original_order_float32_units_and_outward_surfaces() {
    let triangles = verify_binary_box(&box_stl(), &geometry()).unwrap();
    assert_eq!(triangles.triangles.len(), 12);
    assert_eq!(
        triangles.triangles[0].vertices_m,
        [[0.1, -0.02, 0.3], [0.1, -0.02, 0.31], [0.1, -0.01, 0.31]]
    );
    assert_eq!(triangles.assessment.face_triangles, [2; 6]);
    assert!((triangles.assessment.volume_m3 / 2e-6 - 1.).abs() < 1e-12);
    assert!((triangles.assessment.area_m2 / 0.001 - 1.).abs() < 1e-12);
    assert!(triangles.assessment.maximum_bounds_error_m < 1e-16);
    assert!(triangles.assessment.maximum_face_area_relative_error < 1e-12);
    assert_eq!(triangles.triangles[0].normal, [-1., 0., 0.]);
}

#[test]
fn binary_box_geometry_rejects_winding_units_coverage_and_nonfinite_or_ambiguous_encoding() {
    let original = box_stl();
    let mut bad = original.clone();
    bad.pop();
    assert!(verify_binary_box(&bad, &geometry()).is_err());
    let mut bad = original.clone();
    bad.push(0);
    assert!(verify_binary_box(&bad, &geometry()).is_err());
    let mut bad = original.clone();
    bad[80..84].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(verify_binary_box(&bad, &geometry()).is_err());
    let mut bad = original.clone();
    bad[84 + 48..84 + 50].copy_from_slice(&1u16.to_le_bytes());
    assert!(verify_binary_box(&bad, &geometry()).is_err());
    let mut bad = original.clone();
    bad[84 + 12..84 + 16].copy_from_slice(&f32::NAN.to_le_bytes());
    assert!(verify_binary_box(&bad, &geometry()).is_err());
    let mut bad = original.clone();
    bad[84..88].copy_from_slice(&1f32.to_le_bytes());
    assert!(verify_binary_box(&bad, &geometry()).is_err());
    let mut bad = original.clone();
    let triangle = bad[84..134].to_vec();
    bad[134..184].copy_from_slice(&triangle);
    assert!(verify_binary_box(&bad, &geometry()).is_err());
    let mut bad = original.clone();
    let vertex = bad[108..120].to_vec();
    let vertex2 = bad[120..132].to_vec();
    bad[108..120].copy_from_slice(&vertex2);
    bad[120..132].copy_from_slice(&vertex);
    assert!(verify_binary_box(&bad, &geometry()).is_err());
    let mut g = geometry();
    g.bounds_m[0] -= 0.001;
    g.bounds_m[1] -= 0.001;
    assert!(verify_binary_box(&original, &g).is_err());
    let mut g = geometry();
    g.source_unit = "m".into();
    assert!(verify_binary_box(&original, &g).is_err());
    assert!(verify_binary_box(b"solid unsupported_ascii\nendsolid\n", &geometry()).is_err());
}
