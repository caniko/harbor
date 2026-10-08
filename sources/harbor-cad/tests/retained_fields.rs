use harbor_cad::{contracts::*, fields, science::Quantity};
use std::{fs, path::Path};

fn fixture(root: &Path) -> ExecutionPlan {
    let mut case = CaseSpec::reference();
    case.length = Quantity {
        value: 0.02,
        unit: "m".into(),
    };
    case.resolution = 8;
    case.acceleration.value = 0.001;
    case.applicability.formulation = "periodic_forced_channel".into();
    let plan = ExecutionPlan::openlb_reference(case, "research".into()).unwrap();
    fs::create_dir_all(root.join("tmp/vtkData/data")).unwrap();
    let mut pvd = String::from("<VTKFile type=\"Collection\"><Collection>");
    let mut times = Vec::new();
    let case = plan.channel_case().unwrap();
    let dx = case.channel_height.value / f64::from(case.resolution);
    let dt = ((0.8f64 - 0.5) / 3.) * (dx * dx) / case.kinematic_viscosity.value;
    for (index, time) in plan.observation.retained_times_s.iter().enumerate() {
        let step = (*time / dt + 0.5).floor() as u64;
        pvd.push_str(&format!(
            "<DataSet timestep=\"{step}\" file=\"data/t{index}.vtm\"/>"
        ));
        fs::write(root.join(format!("tmp/vtkData/data/t{index}.vtm")),format!("<VTKFile type=\"vtkMultiBlockDataSet\"><vtkMultiBlockDataSet><Block index=\"0\"><DataSet index=\"0\" file=\"t{index}.vti\"/></Block></vtkMultiBlockDataSet></VTKFile>")).unwrap();
        fs::write(root.join(format!("tmp/vtkData/data/t{index}.vti")),
            "<VTKFile type=\"ImageData\"><ImageData WholeExtent=\"0 1 0 1 0 1\" Origin=\"0 0 0\" Spacing=\"1 1 1\"><Piece Extent=\"0 1 0 1 0 1\"><PointData><DataArray type=\"Float64\" Name=\"physVelocity\" NumberOfComponents=\"3\" format=\"binary\" encoding=\"base64\">AAAA</DataArray><DataArray type=\"Float64\" Name=\"physPressure\" NumberOfComponents=\"1\" format=\"binary\" encoding=\"base64\">AAAA</DataArray><DataArray type=\"Float64\" Name=\"geometry\" NumberOfComponents=\"1\" format=\"binary\" encoding=\"base64\">AAAA</DataArray></PointData></Piece></ImageData></VTKFile>").unwrap();
        times.push(serde_json::json!({"requested_s":time,"observed_s":step as f64*dt,"step":step}));
    }
    pvd.push_str("</Collection></VTKFile>");
    fs::write(root.join("tmp/vtkData/channel.pvd"), pvd).unwrap();
    fs::write(root.join("openlb-receipt.json"),serde_json::to_vec(&serde_json::json!({"executed":true,"software_fallback":false,"retained_times":times,"physical_step_s":dt,"field_units":{"geometry":"material ID","physVelocity":"m/s","physPressure":"Pa"}})).unwrap()).unwrap();
    plan
}

#[test]
fn snapshot_preserves_relative_graph_binds_science_and_detects_changed_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("native");
    let plan = fixture(&source);
    let root = temp.path().join("snapshot");
    let snapshot = fields::capture(&source, &root, &plan, "exact-execution-binding").unwrap();
    assert_eq!(snapshot.science_id, plan.science_id().unwrap());
    assert_eq!(snapshot.execution_id, plan.id().unwrap());
    assert_eq!(snapshot.times.len(), 3);
    for file in &snapshot.files {
        let original = source.join(&file.path);
        let retained = root.join(&file.path);
        assert_eq!(fs::read(&original).unwrap(), fs::read(&retained).unwrap());
        use std::os::unix::fs::MetadataExt;
        assert_ne!(
            fs::metadata(original).unwrap().ino(),
            fs::metadata(retained).unwrap().ino()
        );
    }
    assert!(fields::capture(&source, &root, &plan, "replacement").is_err());
    let image = root.join("tmp/vtkData/data/t1.vti");
    let changed = fs::read_to_string(&image).unwrap().replace("AAAA", "BBBB");
    fs::write(&image, changed).unwrap();
    assert!(fields::verify(&root, &snapshot).is_err());
}

#[test]
fn unsafe_references_missing_times_symlinks_and_unsupported_arrays_do_not_publish() {
    for damage in [
        "absolute",
        "traversal",
        "uri",
        "wrong_type",
        "duplicate_time",
        "missing_time",
        "symlink",
        "cell_association",
        "precision",
        "dtd",
        "wrong_observed_time",
        "wrong_requested_time",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("native");
        let plan = fixture(&source);
        let pvd = source.join("tmp/vtkData/channel.pvd");
        let image = source.join("tmp/vtkData/data/t0.vti");
        let receipt_path = source.join("openlb-receipt.json");
        let mut receipt: serde_json::Value =
            serde_json::from_slice(&fs::read(&receipt_path).unwrap()).unwrap();
        let second_step = receipt["retained_times"][1]["step"].as_u64().unwrap();
        match damage {
            "absolute" | "traversal" | "uri" => {
                let unsafe_name = match damage {
                    "absolute" => "/nix/store/payload.vtm",
                    "traversal" => "../../payload.vtm",
                    _ => "https://example.invalid/payload.vtm",
                };
                fs::write(
                    &pvd,
                    fs::read_to_string(&pvd)
                        .unwrap()
                        .replace("data/t0.vtm", unsafe_name),
                )
                .unwrap();
            }
            "wrong_type" => fs::write(
                &pvd,
                fs::read_to_string(&pvd)
                    .unwrap()
                    .replace("type=\"Collection\"", "type=\"UnstructuredGrid\""),
            )
            .unwrap(),
            "duplicate_time" => fs::write(
                &pvd,
                fs::read_to_string(&pvd)
                    .unwrap()
                    .replace(&format!("timestep=\"{second_step}\""), "timestep=\"0\""),
            )
            .unwrap(),
            "missing_time" => fs::write(
                &pvd,
                fs::read_to_string(&pvd).unwrap().replace(
                    &format!("<DataSet timestep=\"{second_step}\" file=\"data/t1.vtm\"/>"),
                    "",
                ),
            )
            .unwrap(),
            "symlink" => {
                fs::rename(&image, source.join("outside.vti")).unwrap();
                std::os::unix::fs::symlink(source.join("outside.vti"), &image).unwrap();
            }
            "cell_association" => fs::write(
                &image,
                fs::read_to_string(&image)
                    .unwrap()
                    .replace("PointData", "CellData"),
            )
            .unwrap(),
            "precision" => fs::write(
                &image,
                fs::read_to_string(&image)
                    .unwrap()
                    .replace("Float64", "Float32"),
            )
            .unwrap(),
            "dtd" => fs::write(
                &pvd,
                format!("<!DOCTYPE VTKFile []>{}", fs::read_to_string(&pvd).unwrap()),
            )
            .unwrap(),
            "wrong_observed_time" | "wrong_requested_time" => {
                let key = if damage == "wrong_observed_time" {
                    "observed_s"
                } else {
                    "requested_s"
                };
                receipt["retained_times"][1][key] = serde_json::json!(1.234);
                fs::write(&receipt_path, serde_json::to_vec(&receipt).unwrap()).unwrap();
            }
            _ => unreachable!(),
        }
        let output = temp.path().join("snapshot");
        assert!(
            fields::capture(&source, &output, &plan, "binding").is_err(),
            "{damage}"
        );
        assert!(!output.exists(), "{damage}");
    }
}
