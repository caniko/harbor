use std::fs;
use std::time::Duration;

use harbor_xtask::{CommandSpec, PipelinePlan, run_pipeline, run_step};
use tempfile::tempdir;

#[test]
fn runs_steps_in_order_with_captured_output() {
    let steps = [
        CommandSpec::new("one", "sh")
            .args(["-c", "printf one"])
            .capture(),
        CommandSpec::new("two", "sh")
            .args(["-c", "printf two"])
            .capture(),
    ];
    let results = run_pipeline(&steps).expect("pipeline");
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].stdout, "one");
    assert_eq!(results[1].stdout, "two");
    assert!(results.iter().all(|result| result.success));
}

#[test]
fn fails_fast_on_first_nonzero_exit() {
    let steps = [
        CommandSpec::new("ok", "true"),
        CommandSpec::new("boom", "false"),
        CommandSpec::new("never", "true"),
    ];
    let error = run_pipeline(&steps).expect_err("pipeline must fail");
    let text = format!("{error:#}");
    assert!(
        text.contains("boom"),
        "error mentions failing label: {text}"
    );
}

#[test]
fn failure_reports_captured_stderr() {
    let steps = [CommandSpec::new("boom", "sh")
        .args(["-c", "echo oops >&2; exit 3"])
        .capture()];
    let error = run_pipeline(&steps).expect_err("pipeline must fail");
    let text = format!("{error:#}");
    assert!(
        text.contains("oops"),
        "error surfaces captured stderr: {text}"
    );
    assert!(
        text.contains("stderr"),
        "error labels the captured stream: {text}"
    );
}

#[test]
fn applies_env_add_and_env_remove() {
    let steps = [
        CommandSpec::new("added", "sh")
            .args(["-c", "printf %s \"$HARBOR_TEST_VAR\""])
            .env("HARBOR_TEST_VAR", "present")
            .capture(),
        CommandSpec::new("removed", "sh")
            .args(["-c", "printf %s \"${HARBOR_TEST_VAR-unset}\""])
            .env("HARBOR_TEST_VAR", "present")
            .env_remove("HARBOR_TEST_VAR")
            .capture(),
    ];
    let results = run_pipeline(&steps).expect("pipeline");
    assert_eq!(results[0].stdout, "present");
    assert_eq!(results[1].stdout, "unset");
}

#[test]
fn reports_elapsed_time() {
    let steps = [CommandSpec::new("sleepy", "sh").args(["-c", "sleep 0.05"])];
    let results = run_pipeline(&steps).expect("pipeline");
    assert!(results[0].elapsed >= Duration::from_millis(50));
}

#[test]
fn run_step_returns_success_flag_without_failing() {
    let result = run_step(&CommandSpec::new("boom", "false")).expect("spawn");
    assert!(!result.success);
}

#[test]
fn cleanup_runs_in_reverse_order_after_failure() {
    let temp = tempdir().expect("temporary directory");
    let marker = temp.path().join("cleanup-order");
    let plan = PipelinePlan::new()
        .step(CommandSpec::new("fail", "false"))
        .cleanup(
            CommandSpec::new("first cleanup", "sh")
                .args(["-c", "printf first >> \"$HARBOR_CLEANUP_ORDER\""])
                .env("HARBOR_CLEANUP_ORDER", marker.display().to_string())
                .capture(),
        )
        .cleanup(
            CommandSpec::new("second cleanup", "sh")
                .args(["-c", "printf second >> \"$HARBOR_CLEANUP_ORDER\""])
                .env("HARBOR_CLEANUP_ORDER", marker.display().to_string())
                .capture(),
        );
    let error = plan.run().expect_err("main step must fail");
    assert!(format!("{error:#}").contains("fail"));
    assert_eq!(
        fs::read_to_string(marker).expect("cleanup marker"),
        "secondfirst"
    );
}

#[test]
fn keep_going_executes_following_steps() {
    let report = PipelinePlan::new()
        .step(CommandSpec::new("fail", "false"))
        .step(CommandSpec::new("after", "true"))
        .keep_going(true)
        .run()
        .expect_err("a kept-going failure remains a failure");
    assert!(format!("{report:#}").contains("fail"));
}
