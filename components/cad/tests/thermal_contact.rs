use harbor_cad::contracts::{CaseSpec, ExecutionPlan};
use harbor_cad::thermal_contact::{ContactMechanics, ThermalContactSpec};
use serde_json::{Value, json};

fn fixture() -> Value {
    let thermal = json!({"schema_version":1,"synthetic":true,"backend":"cpu","formulation":"plane_wall_robin","size_m":[0.001,0.001,0.001],"resolution":2,"geometry_tolerance_m":1e-8,"initial_temperature_k":293.15,"density_kg_m3":1000.,"specific_heat_j_kg_k":1000.,"conductivity_w_m_k":1.,"material_temperature_domain_k":[243.15,303.15],"convection_w_m2_k":10.,"duration_s":120.,"max_step_s":10.,"integration_substeps":4,"observation_times_s":[10.,60.,120.],"ambient_history":[[0.,253.15],[120.,253.15]],"heater_history":[[0.,0.],[120.,0.]],"numerical_tolerance":0.02,"energy_tolerance":0.02,"geometry_provenance":"synthetic congruent block","material_provenance":"synthetic constant thermal properties","history_provenance":"explicit synthetic cold ambient","convection_provenance":"prescribed synthetic Robin coefficient","moisture_risk":{"assessment":"missing","reason":"separate native-source moisture assessment required"}});
    json!({"schema_version":1,"synthetic":true,"thermal":[thermal.clone(),thermal],"mechanical":{"size_m":[0.001,0.001,0.001],"resolution":2,"geometry_tolerance_m":1e-8,"initial_gap_m":0.25e-6,"preload_compression_m":0.5e-6,"final_compression_m":1e-6,"young_modulus_pa":[1e8,1e8],"expansion_per_k":[1e-5,1e-5],"reference_temperature_k":293.15,"contact_stiffness_pa_m":1e12,"numerical_tolerance":0.002,"material_provenance":"synthetic zero-Poisson constant elastic properties","contact_provenance":"explicit synthetic planar penalty law","boundary_provenance":"prescribed preload/final displacement"},"coupling_time_s":120.,"maximum_projection_error_k":[1.,1.],"maximum_relative_conservation_error":1e-12,"moisture_risk":{"assessment":"missing","reason":"humidity unavailable"},"coupling_provenance":"explicit one-way constant-capacitance temperature projection; no contact-to-thermal feedback"})
}

#[test]
fn coupling_requires_congruent_approved_native_histories_and_preserves_contact_gate() {
    let value = fixture();
    let spec: ThermalContactSpec = serde_json::from_value(value.clone()).unwrap();
    spec.validate().unwrap();
    let contact = spec.mechanical.reference([283.15, 283.15]).unwrap();
    assert_eq!(contact.numerical_tolerance, 0.002);
    assert_eq!(contact.initial_gap_m, 0.25e-6);
    // Two 1-mm blocks cool by 10 K: alpha*dT*h removes 0.2 um
    // of closure. The 0.75-um final mechanical closure becomes 0.55 um.
    let compliance = 2. * 0.001 / 1e8 + 1. / 1e12;
    assert!((contact.reference(1).unwrap().pressure_pa - 0.25e-6 / compliance).abs() < 1e-8);
    assert!((contact.reference(2).unwrap().pressure_pa - 0.55e-6 / compliance).abs() < 1e-8);
    for changed in [
        {
            let mut v = value.clone();
            v["thermal"][1]["size_m"][0] = json!(0.002);
            v
        },
        {
            let mut v = value.clone();
            v["thermal"][0]["initial_temperature_k"] = json!(273.15);
            v
        },
        {
            let mut v = value.clone();
            v["thermal"][0]["geometry_tolerance_m"] = json!(2e-8);
            v
        },
        {
            let mut v = value.clone();
            v["coupling_time_s"] = json!(119.);
            v
        },
        {
            let mut v = value.clone();
            v["maximum_projection_error_k"][0] = json!(2.);
            v
        },
        {
            let mut v = value.clone();
            v["maximum_relative_conservation_error"] = json!(1e-6);
            v
        },
        {
            let mut v = value.clone();
            v["thermal"][0]["material_temperature_domain_k"][1] = json!(500.);
            v
        },
        {
            let mut v = value.clone();
            v["moisture_risk"]["reason"] = json!("");
            v
        },
    ] {
        assert!(
            serde_json::from_value::<ThermalContactSpec>(changed)
                .unwrap()
                .validate()
                .is_err()
        );
    }
}

#[test]
fn mechanical_contract_rejects_supplied_final_temperatures_and_foreign_physics() {
    let value = fixture();
    for key in [
        "final_temperatures_k",
        "poisson_ratio",
        "contact_adjustment",
        "physical_time_s",
    ] {
        let mut changed = value["mechanical"].clone();
        changed[key] = json!([293.15, 293.15]);
        assert!(serde_json::from_value::<ContactMechanics>(changed).is_err());
    }
    let mechanical: ContactMechanics = serde_json::from_value(value["mechanical"].clone()).unwrap();
    assert!(mechanical.reference([f64::NAN, 293.15]).is_err());
    assert!(mechanical.reference([500., 293.15]).is_err());
}

#[test]
fn version_eleven_coupling_binds_stage_sources_transfers_and_retained_times() {
    let spec: ThermalContactSpec = serde_json::from_value(fixture()).unwrap();
    let plan = ExecutionPlan::thermal_contact(spec, "research".into()).unwrap();
    let value = serde_json::to_value(&plan).unwrap();
    assert_eq!(value["schema_version"], 11);
    assert_eq!(
        plan.stages
            .iter()
            .map(|s| s.id.as_str())
            .collect::<Vec<_>>(),
        [
            "thermal-lower",
            "thermal-upper",
            "projection",
            "contact",
            "bundle"
        ]
    );
    assert_eq!(plan.transfers.len(), 2);
    assert_eq!(plan.observation.retained_times_s, [10., 60., 120.]);
    assert_eq!(plan.peak_ram(), 2 * 1024u64.pow(3));
    assert!(plan.observation.max_artifact_bytes >= 640 * 1024u64.pow(2));
    let decoded: ExecutionPlan = serde_json::from_value(value.clone()).unwrap();
    decoded.validate().unwrap();
    assert_eq!(plan.id().unwrap(), decoded.id().unwrap());
    assert!(
        ExecutionPlan::thermal_contact(serde_json::from_value(fixture()).unwrap(), "ci".into())
            .is_err()
    );
    for changed in [
        {
            let mut v = value.clone();
            v["stages"][2]["dependencies"] = json!(["thermal-lower"]);
            v
        },
        {
            let mut v = value.clone();
            v["stages"][3]["dependencies"] = json!(["thermal-upper"]);
            v
        },
        {
            let mut v = value.clone();
            v["stages"][1]["id"] = json!("thermal");
            v
        },
        {
            let mut v = value.clone();
            v["transfers"][0]["destination_region"] = json!("upper");
            v
        },
        {
            let mut v = value.clone();
            v["transfers"][0]["maximum_relative_conservation_error"] = json!(1e-6);
            v
        },
        {
            let mut v = value.clone();
            v["observation"]["retained_times_s"] = json!([120.]);
            v
        },
        {
            let mut v = value.clone();
            v["stages"][0]["ram_bytes"] = json!(1);
            v
        },
    ] {
        assert!(
            serde_json::from_value::<ExecutionPlan>(changed)
                .unwrap()
                .validate()
                .is_err()
        );
    }
    let old =
        serde_json::to_value(ExecutionPlan::reference(CaseSpec::reference()).unwrap()).unwrap();
    for inserted in [value["thermal_contact"].clone(), Value::Null] {
        let mut injected = old.clone();
        injected["thermal_contact"] = inserted;
        assert!(serde_json::from_value::<ExecutionPlan>(injected).is_err());
    }
    let mut changed = value.clone();
    changed["thermal_contact"]["coupling_provenance"] = json!("a different approved mapping");
    assert_ne!(
        plan.science_id().unwrap(),
        serde_json::from_value::<ExecutionPlan>(changed)
            .unwrap()
            .science_id()
            .unwrap()
    );
}
