use harbor_cad::{contracts::*, storage::Store};

fn native_case() -> CaseSpec {
    let mut case = CaseSpec::reference();
    case.applicability.formulation = "periodic_forced_channel".into();
    case.length.value = 0.02;
    case.acceleration.value = 0.001;
    case.resolution = 16;
    case.max_time_s = 20.;
    case
}

fn selections() -> B1Selections {
    B1Selections {
        compute: GpuSelection {
            role: Role::Compute,
            backend: "cuda".into(),
            pci: "0000:03:00.0".into(),
            backend_uuid: Some("GPU-test".into()),
        },
        render: GpuSelection {
            role: Role::Render,
            backend: "egl".into(),
            pci: "0000:04:00.0".into(),
            backend_uuid: None,
        },
        media: GpuSelection {
            role: Role::Media,
            backend: "vaapi".into(),
            pci: "0000:04:00.0".into(),
            backend_uuid: None,
        },
    }
}

#[test]
fn positive_but_understated_ram_and_vram_are_rejected_before_a_job_is_written() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(&temp.path().join("state")).unwrap();
    let plan = ExecutionPlan::b1(native_case(), selections(), "research".into()).unwrap();
    for (index, stage) in plan.stages.iter().enumerate() {
        let mut changed = plan.clone();
        changed.stages[index].ram_bytes = 1;
        assert!(
            store.submit(&changed, &format!("ram-{index}")).is_err(),
            "{:?}",
            stage.operation
        );
        if stage.gpu == GpuRequirement::Required {
            changed = plan.clone();
            changed.stages[index].vram_bytes = 1;
            assert!(
                store.submit(&changed, &format!("vram-{index}")).is_err(),
                "{:?}",
                stage.operation
            );
        }
    }
    let mut reference = ExecutionPlan::reference(CaseSpec::reference()).unwrap();
    reference.stages[0].ram_bytes = 1;
    assert!(store.submit(&reference, "reference").is_err());
    let count: i64 = store
        .connection
        .query_row("SELECT count(*) FROM jobs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn changed_retention_geometry_or_resolution_cannot_keep_the_original_low_estimates() {
    let mut case = native_case();
    case.resolution = 64;
    let plan = ExecutionPlan::openlb_reference(case, "research".into()).unwrap();
    let mut many_snapshots = plan.clone();
    many_snapshots.observation.retained_times_s = (0..=20).map(f64::from).collect();
    assert!(
        many_snapshots.validate().is_err(),
        "full-field output must scale with every retained time"
    );
    let mut refined = plan.clone();
    refined.case.resolution *= 2;
    assert!(
        refined.validate().is_err(),
        "distributions and halo allocation grow under refinement"
    );
    let mut longer = plan.clone();
    longer.case.length.value *= 2.;
    assert!(
        longer.validate().is_err(),
        "whole allocated lattice must be budgeted"
    );
    let mut forged = plan;
    forged.observation.max_artifact_bytes = u64::from(forged.case.resolution) * 128;
    assert!(
        forged.validate().is_err(),
        "old generic point-count gate cannot admit native fields"
    );
}

#[test]
fn generated_estimates_preserve_inputs_and_overprovisioning_remains_explicit() {
    let case = native_case();
    let plan = ExecutionPlan::b1(case.clone(), selections(), "research".into()).unwrap();
    assert_eq!(digest(&plan.case).unwrap(), digest(&case).unwrap());
    let mut more = plan.clone();
    for stage in &mut more.stages {
        stage.ram_bytes *= 2;
        stage.vram_bytes *= 2;
    }
    more.observation.max_artifact_bytes *= 2;
    more.validate().unwrap();
    assert_ne!(plan.id().unwrap(), more.id().unwrap());
    let mut huge = case;
    huge.resolution = 1_000_000;
    huge.length.value = 1e100;
    assert!(ExecutionPlan::openlb_reference(huge, "research".into()).is_err());
}
