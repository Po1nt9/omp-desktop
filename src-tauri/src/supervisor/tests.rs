use super::*;

#[tokio::test]
async fn supervisor_returns_unavailable_when_binary_not_found() {
    let supervisor = Supervisor::new(SupervisorConfig {
        binary_path: None, // No binary configured
        ..Default::default()
    });
    let result = supervisor.start().await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("runtime_unavailable"));
}

#[test]
fn supervisor_config_has_sensible_defaults() {
    let config = SupervisorConfig::default();
    assert_eq!(config.max_restarts, 3);
    assert_eq!(config.restart_delay_ms, 1000);
    assert_eq!(config.health_check_interval_ms, 5000);
}

#[test]
fn supervisor_config_defaults_enable_v1_protocol() {
    // Plan 3 Global Constraints: `OMP_DESKTOP_V1_PROTOCOL=1` is the default.
    let config = SupervisorConfig::default();
    assert!(config
        .env_vars
        .iter()
        .any(|(k, v)| k == "OMP_DESKTOP_V1_PROTOCOL" && v == "1"));
}

#[tokio::test]
async fn supervisor_is_not_running_before_start() {
    let supervisor = Supervisor::new(SupervisorConfig::default());
    assert!(!supervisor.is_running().await);
}

#[tokio::test]
async fn supervisor_stop_is_safe_when_never_started() {
    let supervisor = Supervisor::new(SupervisorConfig::default());
    // Must not panic and must succeed even with no child.
    supervisor.stop().await.expect("stop on empty supervisor");
    assert!(!supervisor.is_running().await);
}
