use std::time::Duration;

use harbor_xtask::{CommandSpec, run_pipeline, run_step};

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
