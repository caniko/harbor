use harbor_cad::{
    atmosphere::AtmosphericReferenceSpec,
    atmosphere_transfer::AtmosphericTransferRequest,
    atmospheric_transport::{AtmosphericSourceBinding, AtmosphericTransportSpec},
    contracts::ExecutionPlan,
};

fn fixture() -> AtmosphericTransportSpec {
    let request: AtmosphericTransferRequest =
        serde_json::from_str(include_str!("../examples/atmosphere-transfer.json")).unwrap();
    let atmosphere: AtmosphericReferenceSpec =
        serde_json::from_str(include_str!("../examples/atmosphere-reference.json")).unwrap();
    AtmosphericTransportSpec {
        schema_version: 1,
        request: request.clone(),
        atmosphere: atmosphere.clone(),
        source: AtmosphericSourceBinding {
            job_id: request.source_job,
            science_id: harbor_cad::contracts::digest(&atmosphere).unwrap(),
            execution_id: "b".repeat(64),
            execution_binding_digest: "c".repeat(64),
            authorization_digest: "d".repeat(64),
            receipt: harbor_cad::qualification::EvidenceRecord {
                path: "stages/atmosphere/atmosphere-receipt.json".into(),
                sha256: "e".repeat(64),
                bytes: 8192,
            },
            original: harbor_cad::qualification::EvidenceRecord {
                path: harbor_cad::atmosphere::ORIGINAL_PATH.into(),
                sha256: "f".repeat(64),
                bytes: 65536,
            },
        },
    }
}

#[test]
fn original_atmospheric_transport_is_a_separate_immutable_bounded_approval() {
    let spec = fixture();
    let plan = ExecutionPlan::atmospheric_transport(spec.clone(), "research".into()).unwrap();
    assert_eq!(plan.schema_version, 15);
    assert!(plan.atmosphere.is_none() && plan.spectral.is_none() && plan.source.is_none());
    assert_eq!(plan.stages[0].id, "atmospheric-transport");
    assert!(plan.peak_ram() >= 512 * 1024 * 1024);
    let original = serde_json::to_value(&plan).unwrap();
    assert_eq!(
        serde_json::from_value::<ExecutionPlan>(original.clone())
            .unwrap()
            .id()
            .unwrap(),
        plan.id().unwrap()
    );
    for field in [
        "atmosphere",
        "spectral",
        "case",
        "cad_source",
        "source",
        "freezing",
        "thermal",
        "contact",
    ] {
        let mut injected = original.clone();
        injected[field] = serde_json::Value::Null;
        assert!(
            serde_json::from_value::<ExecutionPlan>(injected).is_err(),
            "{field}"
        );
    }
    for version in 1..=14 {
        let mut changed = original.clone();
        changed["schema_version"] = serde_json::json!(version);
        assert!(serde_json::from_value::<ExecutionPlan>(changed).is_err());
    }
    let v14 =
        ExecutionPlan::atmospheric_reference(spec.atmosphere.clone(), "research".into()).unwrap();
    let mut injected = serde_json::to_value(&v14).unwrap();
    injected["atmospheric_transport"] = serde_json::Value::Null;
    assert!(serde_json::from_value::<ExecutionPlan>(injected).is_err());
    let mut altered = spec;
    altered.source.original.sha256 = "0".repeat(64);
    assert_ne!(
        ExecutionPlan::atmospheric_transport(altered, "research".into())
            .unwrap()
            .id()
            .unwrap(),
        plan.id().unwrap()
    );
    let mut weak = plan.clone();
    weak.observation.max_artifact_bytes = 1;
    assert!(weak.validate().is_err());
    let mut gpu = plan.clone();
    gpu.stages[0].gpu = harbor_cad::contracts::GpuRequirement::Preferred;
    assert!(gpu.validate().is_err());
}

#[test]
fn malformed_source_binding_and_unexecuted_atmosphere_cannot_authorize_transport() {
    let valid = fixture();
    for pointer in [
        "/source/execution_id",
        "/source/authorization_digest",
        "/source/receipt/sha256",
        "/source/original/sha256",
    ] {
        let mut value = serde_json::to_value(&valid).unwrap();
        *value.pointer_mut(pointer).unwrap() = serde_json::json!("x");
        let spec: AtmosphericTransportSpec = serde_json::from_value(value).unwrap();
        assert!(ExecutionPlan::atmospheric_transport(spec, "research".into()).is_err());
    }
    let mut mismatched = valid.clone();
    mismatched.source.job_id = "00000000-0000-0000-0000-000000000002".into();
    assert!(mismatched.validate().is_err());
    let dir = tempfile::tempdir().unwrap();
    let store = harbor_cad::storage::Store::open(&dir.path().join("state")).unwrap();
    let source = ExecutionPlan::atmospheric_reference(valid.atmosphere, "research".into()).unwrap();
    let job = store.submit(&source, "unexecuted-source").unwrap();
    let mut request = valid.request;
    request.source_job = job.id;
    assert!(harbor_cad::atmospheric_transport::plan(&store, request, "research".into()).is_err());
}

fn packet_receipt(root: &std::path::Path) -> (AtmosphericTransportSpec, String, serde_json::Value) {
    use serde_json::json;
    use sha2::{Digest, Sha256};
    let mut spec = fixture();
    spec.atmosphere.model = "transparent_reference".into();
    spec.atmosphere.mu_bins = 8;
    spec.atmosphere.phi_bins = 8;
    spec.atmosphere.albedo = 0.;
    for value in &mut spec.atmosphere.toa_irradiance {
        value.value = 1.;
    }
    spec.source.science_id = harbor_cad::contracts::digest(&spec.atmosphere).unwrap();
    let source = spec.atmosphere.prepare().unwrap();
    spec.request.receiver.samples = 1024;
    spec.request.receiver.absorptivity = vec![0.5; 3];
    spec.request.receiver.ageing_action = vec![0.25; 3];
    spec.request.receiver.source = harbor_cad::radiation::SpectralSource::Directional {
        propagation_direction: source.propagation_direction,
        irradiance: spec.atmosphere.toa_irradiance.clone(),
    };
    let cosine = -source.propagation_direction[2];
    let original = source
        .wavelengths_nm
        .iter()
        .map(|wl| format!("{wl:.3} {cosine} 0 0 {}", vec!["0"; 128].join(" ")))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    spec.source.original.sha256 = format!("{:x}", Sha256::digest(original.as_bytes()));
    spec.source.original.bytes = original.len() as u64;
    let diffuse=source.umu.iter().enumerate().flat_map(|(i,mu)|source.phi_deg.iter().enumerate().map(move |(j,phi)|json!({"id":format!("angular-{i:03}-{j:03}"),"umu_index":i,"phi_index":j,"umu":mu,"phi_deg":phi,"propagation_direction":harbor_cad::atmosphere_transfer::propagation(*mu,*phi),"irradiance_w_m2_nm":[0.,0.,0.]}))).collect::<Vec<_>>();
    let mapping = json!({"schema_version":1,"direct":{"id":"solar-direct","propagation_direction":source.propagation_direction,"irradiance_w_m2_nm":[1.,1.,1.]},"diffuse":diffuse,"wavelengths_nm":source.wavelengths_nm,"native_emitter_ids":{"direct":["solar-direct"],"diffuse":[]}});
    let bytes = serde_json::to_vec(&mapping).unwrap();
    std::fs::write(root.join("emitter-sources.json"), &bytes).unwrap();
    let mapped = json!({"path":"emitter-sources.json","sha256":format!("{:x}",Sha256::digest(&bytes)),"bytes":bytes.len()});
    let incident = 120. * cosine;
    let channels = json!({"incident":incident,"absorbed":incident*0.5,"ageing":incident*0.25});
    let zero = json!({"incident":0.,"absorbed":0.,"ageing":0.});
    let checks = |channels: &serde_json::Value| {
        let mut rows = serde_json::Map::new();
        for name in ["incident", "absorbed", "ageing"] {
            rows.insert(name.into(),json!({"native_w_m2":channels[name],"reference_w_m2":channels[name],"relative_error":0.,"tolerance":spec.request.receiver.relative_tolerance,"passed":true}));
        }
        json!({"channels":rows,"maximum_normalized_error":0.,"tolerance":spec.request.receiver.relative_tolerance,"passed":true,"physical_validation":"unqualified"})
    };
    let scale =
        |s: f64| json!({"incident":incident*s,"absorbed":incident*0.5*s,"ageing":incident*0.25*s});
    let area = spec.request.receiver.sensor_width.si("length").unwrap()
        * spec.request.receiver.sensor_height.si("length").unwrap();
    let header = "sample,knot_offset,x_m,y_m,z_m,towards_source_x,towards_source_y,towards_source_z,native_cosine,native_emitter_id,native_pdf,native_weight_w_m2_nm_0,native_weight_w_m2_nm_1,native_weight_w_m2_nm_2,native_weight_w_m2_nm_3\n";
    let mut observations = vec![];
    for seed in &spec.request.receiver.seeds {
        let mut originals = vec![];
        for component in ["direct", "diffuse"] {
            let mut text = String::from(header);
            for sample in 0..1024 {
                if component == "direct" {
                    text.push_str(&format!(
                        "{sample},0,0,0,0,{},{},{},{cosine},solar-direct,1,1,1,1,1\n",
                        -source.propagation_direction[0],
                        -source.propagation_direction[1],
                        -source.propagation_direction[2]
                    ));
                } else {
                    text.push_str(&format!("{sample},0,0,0,0,0,0,0,0,none,0,0,0,0,0\n"));
                }
            }
            let path = format!("{component}-{seed}.csv");
            std::fs::write(root.join(&path), &text).unwrap();
            originals.push(json!({"component":component,"path":path,"sha256":format!("{:x}",Sha256::digest(text.as_bytes())),"bytes":text.len()}));
        }
        observations.push(json!({"seed":seed,"samples":1024,"components":{"direct":{"native_channels_w_m2":channels,"numerical_verification":checks(&channels)},"diffuse":{"native_channels_w_m2":zero,"numerical_verification":checks(&zero)}},"native_channels_w_m2":channels,"numerical_verification":checks(&channels),"exposure_j_m2":scale(3600.),"native_power_w":scale(area),"energy_j":scale(3600.*area),"originals":originals,"runtime_s":0.1}));
    }
    let sandbox_checks = [
        "operation_closure_only",
        "no_gpu_nodes",
        "no_sysfs",
        "no_host_home",
        "no_session_bus",
        "no_worker_socket",
        "network_namespace_isolated",
        "descriptor_readonly",
        "original_source_readonly",
    ]
    .into_iter()
    .map(|key| (key.to_owned(), json!(true)))
    .collect::<serde_json::Map<_, _>>();
    let receipt = json!({"schema_version":1,"adapter":"Mitsuba","mitsuba_version":"3.9.1","drjit_version":"1.5.0","backend":"cpu","variant":"scalar_spectral","precision":"Float32","reduction_precision":"Float64","executed":true,"software_fallback":false,"source_runtime_evidence":"requires_independent_registered_originals_qualification","worker_execution":"not_qualified_by_standalone_adapter","source_input":spec.atmosphere,"receiver_input":spec.request.receiver,"original_atmosphere_sha256":spec.source.original.sha256,"request_sha256":harbor_cad::contracts::digest(&spec.native_request().unwrap()).unwrap(),"derived_emitter_sources":mapped,"zero_original_angular_cells":128,"angular_shape":[3,16,8],"transfer_relative_conservation_error":0.,"source_angular_verification":{"maximum_angular_flux_error":0.,"tolerance":spec.atmosphere.relative_tolerance,"passed":true,"physical_validation":"unqualified"},"reference":channels,"component_references":{"direct":channels,"diffuse":zero,"incident":channels},"sensor_area_m2":area,"native_sensor_area_m2":{"direct":area,"diffuse":area},"history_integral_s":3600.,"observations":observations,"convergence":"not_assessed","physical_validation":"unqualified","sandbox":{"policy":harbor_cad::atmospheric_transport::SANDBOX_POLICY,"checks":sandbox_checks}});
    (spec, original, receipt)
}

#[test]
fn transport_receipt_is_reconstructed_from_exact_original_components() {
    let root = tempfile::tempdir().unwrap();
    let (spec, original, receipt) = packet_receipt(root.path());
    let evidence = harbor_cad::atmospheric_transport::verify_original_transport(
        &spec,
        &original,
        root.path(),
        &receipt,
    )
    .unwrap();
    assert!(evidence.error < 1e-14);
    for pointer in [
        "/observations/0/components/direct/native_channels_w_m2/incident",
        "/observations/0/native_channels_w_m2/incident",
        "/observations/0/energy_j/absorbed",
        "/reference/ageing",
        "/component_references/diffuse/incident",
        "/observations/0/originals/0/bytes",
        "/observations/0/seed",
    ] {
        let mut bad = receipt.clone();
        let old = bad.pointer(pointer).unwrap().as_f64().unwrap();
        *bad.pointer_mut(pointer).unwrap() = serde_json::json!(old + 1.);
        assert!(
            harbor_cad::atmospheric_transport::verify_original_transport(
                &spec,
                &original,
                root.path(),
                &bad
            )
            .is_err(),
            "{pointer}"
        );
    }
    for key in [
        "operation_closure_only",
        "original_source_readonly",
        "no_gpu_nodes",
        "network_namespace_isolated",
    ] {
        let mut bad = receipt.clone();
        bad["sandbox"]["checks"][key] = serde_json::json!(false);
        assert!(
            harbor_cad::atmospheric_transport::verify_original_transport(
                &spec,
                &original,
                root.path(),
                &bad
            )
            .is_err()
        );
    }
    let mut missing = receipt.clone();
    missing["observations"].as_array_mut().unwrap().pop();
    assert!(
        harbor_cad::atmospheric_transport::verify_original_transport(
            &spec,
            &original,
            root.path(),
            &missing
        )
        .is_err()
    );
    let path = root
        .path()
        .join(format!("direct-{}.csv", spec.request.receiver.seeds[0]));
    let altered = std::fs::read_to_string(&path).unwrap().replacen(
        ",solar-direct,1,1,1,1,1",
        ",solar-direct,1,2,2,2,2",
        1,
    );
    std::fs::write(&path, &altered).unwrap();
    let mut bad = receipt;
    use sha2::{Digest, Sha256};
    bad["observations"][0]["originals"][0]["sha256"] =
        serde_json::json!(format!("{:x}", Sha256::digest(altered.as_bytes())));
    bad["observations"][0]["originals"][0]["bytes"] = serde_json::json!(altered.len());
    assert!(
        harbor_cad::atmospheric_transport::verify_original_transport(
            &spec,
            &original,
            root.path(),
            &bad
        )
        .is_err()
    );
}
