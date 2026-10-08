use harbor_cad::{contracts::*, thermal::ThermalReferenceSpec};

fn input() -> serde_json::Value {
    serde_json::json!({"schema_version":1,"synthetic":true,"backend":"cpu","formulation":"plane_wall_robin",
        "size_m":[0.02,0.01,0.01],"resolution":4,"geometry_tolerance_m":1e-6,
        "initial_temperature_k":293.15,"density_kg_m3":7800.,"specific_heat_j_kg_k":500.,"conductivity_w_m_k":20.,
        "material_temperature_domain_k":[240.,320.],"convection_w_m2_k":200.,"duration_s":120.,"max_step_s":1.,"integration_substeps":64,
        "observation_times_s":[10.,60.,120.],"ambient_history":[[0.,253.15],[60.,253.15],[120.,273.15]],"heater_history":[[0.,0.],[60.,0.],[120.,1.]],
        "numerical_tolerance":0.02,"energy_tolerance":0.02,"geometry_provenance":"synthetic reference box","material_provenance":"synthetic constant-property solid",
        "history_provenance":"prescribed synthetic cold soak and heater ramp","convection_provenance":"prescribed h; no velocity conversion","moisture_risk":{"assessment":"missing","reason":"no humidity"}})
}

#[test]
fn thermal_plan_binds_histories_and_independent_observation_and_solver_schedules() {
    let spec: ThermalReferenceSpec = serde_json::from_value(input()).unwrap();
    assert_eq!(spec.heater_energy(120.).unwrap(), 30.);
    assert_eq!(spec.integration_step(), 1. / 64.);
    let plan = ExecutionPlan::thermal_reference(spec, "research".into()).unwrap();
    let value = serde_json::to_value(&plan).unwrap();
    assert_eq!(value["schema_version"], 6);
    assert_eq!(value["thermal"], input());
    assert!(value.get("case").is_none() && value.get("fem").is_none());
    assert_eq!(plan.science_id().unwrap(), digest(&plan.thermal).unwrap());
    assert_eq!(plan.observation.retained_times_s, [10., 60., 120.]);
    assert!(matches!(
        plan.stages[0].operation,
        StageOperation::ThermalReference
    ));
    assert!(plan.stages.iter().all(|s| s.gpu == GpuRequirement::CpuOnly));
    let decoded: ExecutionPlan = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(decoded.id().unwrap(), plan.id().unwrap());
    for field in ["case", "fem", "source", "frames", "filter"] {
        let mut injected = value.clone();
        injected[field] = serde_json::Value::Null;
        assert!(serde_json::from_value::<ExecutionPlan>(injected).is_err());
    }
    for version in 1..=5 {
        let mut old = value.clone();
        old["schema_version"] = serde_json::json!(version);
        assert!(serde_json::from_value::<ExecutionPlan>(old).is_err());
    }
    let mut changed = plan.clone();
    changed.observation.retained_times_s.pop();
    assert!(changed.validate().is_err());
    changed = plan.clone();
    changed.stages[0].ram_bytes = 1;
    assert!(changed.validate().is_err());
    changed = plan;
    changed.thermal.as_mut().unwrap().integration_substeps = 32;
    assert_ne!(
        changed.science_id().unwrap(),
        digest(&decoded.thermal).unwrap()
    );
}

#[test]
fn thermal_admission_rejects_missing_physics_changed_gates_and_output_overflow() {
    for (field, value) in [
        ("backend", serde_json::json!("hip")),
        ("synthetic", serde_json::json!(false)),
        ("numerical_tolerance", serde_json::json!(0.03)),
        ("energy_tolerance", serde_json::json!(0.03)),
        ("density_kg_m3", serde_json::json!(0)),
        ("convection_provenance", serde_json::json!("")),
        (
            "material_temperature_domain_k",
            serde_json::json!([270., 320.]),
        ),
        ("observation_times_s", serde_json::json!([120., 60.])),
        ("integration_substeps", serde_json::json!(65)),
        ("max_step_s", serde_json::json!(1e-10)),
        ("resolution", serde_json::json!(32)),
        ("implicit_velocity_to_h", serde_json::json!(true)),
        ("moisture_risk", serde_json::Value::Null),
    ] {
        let mut value_input = input();
        value_input[field] = value;
        let result = serde_json::from_value::<ThermalReferenceSpec>(value_input);
        assert!(
            result.is_err() || result.unwrap().validate().is_err(),
            "{field}"
        );
    }
}

#[test]
fn thermal_receipts_cannot_change_precision_scope_time_coverage_or_acceptance() {
    let spec: ThermalReferenceSpec = serde_json::from_value(input()).unwrap();
    let plan = ExecutionPlan::thermal_reference(spec.clone(), "research".into()).unwrap();
    let names = [
        "operation_closure_only",
        "no_gpu_nodes",
        "no_sysfs",
        "no_host_home",
        "no_session_bus",
        "no_worker_socket",
        "network_namespace_isolated",
        "descriptor_readonly",
    ];
    let checks: serde_json::Map<String, serde_json::Value> = names
        .into_iter()
        .map(|name| (name.into(), serde_json::json!(true)))
        .collect();
    let value = serde_json::json!({"schema_version":1,"adapter":"CalculiX","backend":"cpu","factorization":"SPOOLES","executed":true,"software_fallback":false,"synthetic":true,"precision":"float64",
        "request_sha256":digest(&spec).unwrap(),"formulation":"plane_wall_robin","calculix_version":"2.23","gmsh_version":"4.15.2",
        "calculix_source_sha256":"9c88385c10fb04f5dc6c4e98027a51bebdd8aee3920e05190d6c1dd08357d6e7","gmsh_source_sha256":"be3f66f225d27ba9fa014f07e83169285da8a051b0e8ab7103d88066b39bdd3e",
        "temperature_serialization_patch_sha256":harbor_cad::thermal::temperature_patch_sha256(),"temperature_serialization":"E23.15; 16 significant decimal digits from native real*8",
        "nodes":125,"elements":64,"physical_validation":"unqualified","moisture_risk":spec.moisture_risk,"physical_times_s":spec.observation_times_s,
        "energy_output_times_s":spec.output_times(),"maximum_native_step_s":spec.integration_step(),"integration_substeps":64,
        "sandbox":{"policy":harbor_cad::execution::THERMAL_SANDBOX_POLICY,"checks":checks},
        "numerical_verification":{"temperature":{"passed":true,"normalized_max_abs_error":0.001,"tolerance":0.02,"reference":"independent plane wall","unit":"K","samples":375},
        "energy":{"passed":true,"maximum_relative_balance_error":0.001,"tolerance":0.02,"reference":"independent energy integral","unit":"J","samples":120}}});
    assert!(harbor_cad::thermal::verify_receipt(&plan, &value).is_ok());
    for pointer in [
        "/request_sha256",
        "/temperature_serialization_patch_sha256",
        "/sandbox/checks/descriptor_readonly",
        "/physical_times_s",
        "/numerical_verification/energy/tolerance",
        "/numerical_verification/temperature/samples",
        "/moisture_risk",
        "/integration_substeps",
    ] {
        let mut changed = value.clone();
        *changed.pointer_mut(pointer).unwrap() = serde_json::Value::Null;
        assert!(
            harbor_cad::thermal::verify_receipt(&plan, &changed).is_err(),
            "{pointer}"
        );
    }
    let mut changed = value.clone();
    changed["numerical_verification"]["energy"]["maximum_relative_balance_error"] =
        serde_json::json!(0.03);
    assert!(harbor_cad::thermal::verify_receipt(&plan, &changed).is_err());
    changed = value;
    let energy = changed["numerical_verification"]
        .as_object_mut()
        .unwrap()
        .remove("energy")
        .unwrap();
    changed["numerical_verification"]["foreign_field"] = energy;
    assert!(harbor_cad::thermal::verify_receipt(&plan, &changed).is_err());
}
