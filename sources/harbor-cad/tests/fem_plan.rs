use harbor_cad::{contracts::*, fem::FemReferenceSpec};

fn input() -> serde_json::Value {
    serde_json::json!({"schema_version":1,"synthetic":true,"backend":"cpu","mode":"thermal_boundary", "size_m":[0.02,0.01,0.01],
        "resolution":4,"geometry_tolerance_m":1e-6,"temperatures_k":[293.15,303.15],"numerical_tolerance":1e-6,"conductivity_w_m_k":20.})
}

#[test]
fn fem_plan_has_its_own_science_and_no_invented_fluid_or_physical_time_inputs() {
    let spec: FemReferenceSpec = serde_json::from_value(input()).unwrap();
    let plan = ExecutionPlan::fem_reference(spec.clone(), "research".into()).unwrap();
    let value = serde_json::to_value(&plan).unwrap();
    assert_eq!(value["schema_version"], 5);
    assert!(value.get("case").is_none());
    assert_eq!(value["fem"], input());
    assert_eq!(plan.science_id().unwrap(), digest(&spec).unwrap());
    assert_eq!(value["stages"][0]["operation"], "fem_reference");
    assert!(plan.observation.retained_times_s.is_empty());
    let decoded: ExecutionPlan = serde_json::from_value(value).unwrap();
    assert_eq!(decoded.id().unwrap(), plan.id().unwrap());
    let mut changed = input();
    changed["conductivity_w_m_k"] = serde_json::json!(21.);
    let changed: FemReferenceSpec = serde_json::from_value(changed).unwrap();
    assert_ne!(plan.science_id().unwrap(), digest(&changed).unwrap());
    assert!(plan.channel_case().is_err());
}

#[test]
fn fem_inputs_and_plan_cannot_smuggle_unsupported_backends_or_weaken_acceptance() {
    for (field, value) in [
        ("backend", serde_json::json!("hip")),
        ("synthetic", serde_json::json!(false)),
        ("resolution", serde_json::json!(64)),
        ("temperatures_k", serde_json::json!([0., 303.15])),
        ("numerical_tolerance", serde_json::json!(0.1)),
        ("conductivity_w_m_k", serde_json::json!(0.)),
        ("geometry_tolerance_m", serde_json::json!(1.)),
        ("size_m", serde_json::json!([1., 0.00001, 1.])),
    ] {
        let mut raw = input();
        raw[field] = value;
        let rejected = match serde_json::from_value::<FemReferenceSpec>(raw) {
            Ok(s) => s.validate().is_err(),
            Err(_) => true,
        };
        assert!(rejected, "{field}");
    }
    let spec: FemReferenceSpec = serde_json::from_value(input()).unwrap();
    let plan = ExecutionPlan::fem_reference(spec, "research".into()).unwrap();
    for version in 1..=4 {
        let mut old =
            serde_json::to_value(ExecutionPlan::reference(CaseSpec::reference()).unwrap()).unwrap();
        old["schema_version"] = serde_json::json!(version);
        old["fem"] = input();
        assert!(serde_json::from_value::<ExecutionPlan>(old).is_err());
    }
    let mut wrong = serde_json::to_value(&plan).unwrap();
    wrong["case"] = serde_json::to_value(CaseSpec::reference()).unwrap();
    assert!(serde_json::from_value::<ExecutionPlan>(wrong).is_err());
    let mut temporal = plan.clone();
    temporal.observation.retained_times_s = vec![0.];
    assert!(temporal.validate().is_err());
    let mut resource = plan.clone();
    resource.stages[0].ram_bytes = 1024;
    assert!(resource.validate().is_err());
    let mut gpu = plan.clone();
    gpu.stages[0].gpu = GpuRequirement::Required;
    assert!(gpu.validate().is_err());
}

#[test]
fn complete_native_fem_checks_bind_recipe_units_coverage_and_exact_acceptance() {
    let plan =
        ExecutionPlan::fem_reference(serde_json::from_value(input()).unwrap(), "research".into())
            .unwrap();
    let receipt = serde_json::json!({"schema_version":1,"adapter":"CalculiX","backend":"cpu","factorization":"SPOOLES","executed":true,
        "software_fallback":false,"synthetic":true,"precision":"float64","request_sha256":plan.science_id().unwrap(),"formulation":"thermal_boundary",
        "calculix_version":"2.23","gmsh_version":"4.15.2","calculix_source_sha256":"9c88385c10fb04f5dc6c4e98027a51bebdd8aee3920e05190d6c1dd08357d6e7",
        "gmsh_source_sha256":"be3f66f225d27ba9fa014f07e83169285da8a051b0e8ab7103d88066b39bdd3e",
        "sandbox":{"policy":"harbor-cad-fem-cpu-v1","checks":{"operation_closure_only":true,"no_gpu_nodes":true,"no_sysfs":true,"no_host_home":true,"no_session_bus":true,"no_worker_socket":true,"network_namespace_isolated":true,"descriptor_readonly":true}},
        "nodes":125,"elements":64,"physical_validation":"unqualified","numerical_verification":{
            "temperature":{"passed":true,"normalized_max_abs_error":0.,"tolerance":1e-6,"reference":"linear temperature","unit":"K","samples":125},
            "heat_flux":{"passed":true,"normalized_max_abs_error":1e-12,"tolerance":1e-6,"reference":"Fourier flux","unit":"W/m2","samples":512}}});
    assert_eq!(
        harbor_cad::fem::verify_receipt(&plan, &receipt)
            .unwrap()
            .error,
        1e-12
    );
    for (field, value) in [
        ("request_sha256", serde_json::json!("a".repeat(64))),
        ("nodes", serde_json::json!(124)),
        ("software_fallback", serde_json::json!(true)),
        ("formulation", serde_json::json!("contact")),
        ("calculix_version", serde_json::json!("2.22")),
        ("sandbox", serde_json::Value::Null),
    ] {
        let mut changed = receipt.clone();
        changed[field] = value;
        assert!(harbor_cad::fem::verify_receipt(&plan, &changed).is_err());
    }
    for (field, value) in [
        ("passed", serde_json::json!(false)),
        ("normalized_max_abs_error", serde_json::json!(0.1)),
        ("unit", serde_json::json!("m/s")),
        ("samples", serde_json::json!(124)),
        ("tolerance", serde_json::json!(0.1)),
    ] {
        let mut changed = receipt.clone();
        changed["numerical_verification"]["temperature"][field] = value;
        assert!(harbor_cad::fem::verify_receipt(&plan, &changed).is_err());
    }
}
