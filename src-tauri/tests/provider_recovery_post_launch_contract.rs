#[test]
fn general_recovery_keeps_slow_post_launch_observation_structured() {
    let source = include_str!("../src/provider_recovery.rs");

    assert!(
        source.contains("launch_provider(&path)?;"),
        "general provider recovery must treat a successful launch request separately from runtime re-observation"
    );
    assert!(
        !source.contains("fn launch_provider(provider: CloudProvider"),
        "launch_provider must not convert a slow post-launch runtime observation into a launch failure"
    );
    assert!(
        source.contains("let post_runtime_observed = runtime_observation(provider, observed_at_ms);")
            && source.contains("post_runtime_blockers(post_runtime_observed)"),
        "slow or unavailable post-launch observation must remain structured recovery evidence"
    );
}

#[test]
fn bounded_output_wait_errors_reap_the_spawned_child() {
    let source = include_str!("../src/provider_recovery.rs");
    let bounded_output = source
        .split("fn run_bounded_output")
        .nth(1)
        .and_then(|tail| tail.split("fn launch_provider").next())
        .expect("run_bounded_output source boundary");

    assert!(
        bounded_output.contains(
            "Err(_) => {\n                let _ = child.kill();\n                let _ = child.wait();\n                return Err(\"provider-recovery-command-wait-failed\".into());\n            }"
        ),
        "a wait failure must not leave the OneDrive helper process unreaped"
    );
}
