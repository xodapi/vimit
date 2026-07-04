#![cfg_attr(windows, windows_subsystem = "windows")]
#![allow(clippy::collapsible_if)]

use vimit as ng;

slint::include_modules!();

#[path = "vimit-gui/app.rs"]
mod app;
#[path = "vimit-gui/config.rs"]
mod config;
#[path = "vimit-gui/dashboard.rs"]
mod dashboard;
#[path = "vimit-gui/overlay.rs"]
mod overlay;
#[path = "vimit-gui/platform.rs"]
mod platform;
#[path = "vimit-gui/tray.rs"]
mod tray;

fn main() {
    app::run();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{GuiAccount, runtime_config};
    use crate::overlay::{
        CreatureSound, CreatureState, OVERLAY_HISTORY_RETENTION, OverlayHistory,
        build_overlay_state, creature_node_count, creature_node_count_for_skin,
        creature_path_commands, creature_path_commands_for_skin, creature_sound_for_transition,
        creature_state_for, creature_state_from_str, format_overlay_countdown, one_decimal_local,
        organic_creature_node_count, overlay_logical_size, overlay_phase_step, parse_rate_value,
    };
    use crate::platform::read_agent_status_for_gui;
    use crate::tray::tray_status_from_dashboard;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    fn test_window(key: &'static str, level: &str, percent: f64) -> ng::WindowState {
        ng::WindowState {
            key,
            level: level.to_string(),
            reset: "через 1ч".to_string(),
            reset_in_seconds: Some(3600),
            credits: Some(ng::Metric {
                used: percent,
                limit: 100.0,
                remaining: 100.0 - percent,
                percent,
            }),
            requests: None,
            percent,
        }
    }

    #[test]
    fn tray_status_uses_current_dashboard_windows() {
        let status = tray_status_from_dashboard(
            "источник: live VibeMode /v1/me на api",
            &[
                test_window("5h", "warning", 78.0),
                test_window("7d", "ok", 42.0),
            ],
        );

        assert_eq!(status.color, (202, 164, 95));
        assert!(status.tooltip.contains("live VibeMode /v1/me на api"));
        assert!(status.tooltip.contains("5h: warning (78.0%)"));
        assert!(status.tooltip.contains("7d: ok (42.0%)"));
    }

    #[test]
    fn overlay_helpers_format_rates_and_countdown() {
        assert_eq!(parse_rate_value("12,5 токены/мин"), Some(12.5));
        assert_eq!(one_decimal_local(3.25), "3,2");
        assert_eq!(format_overlay_countdown(-1), "unknown");
        assert_eq!(format_overlay_countdown(125), "через 2м");
    }

    #[test]
    fn overlay_size_switches_between_full_and_compact() {
        assert_eq!(overlay_logical_size(false), (340.0, 380.0));
        assert_eq!(overlay_logical_size(true), (260.0, 52.0));
    }

    #[test]
    fn creature_state_uses_sleep_and_thresholds() {
        assert_eq!(
            creature_state_for(10.0, 0.0, false, 2),
            CreatureState::Sleeping
        );
        assert_eq!(
            creature_state_for(40.0, 0.2, false, 2),
            CreatureState::Awake
        );
        assert_eq!(
            creature_state_for(80.0, 0.2, false, 2),
            CreatureState::Alert
        );
        assert_eq!(
            creature_state_for(92.0, 0.2, false, 2),
            CreatureState::Critical
        );
        assert_eq!(
            creature_state_for(10.0, 0.0, true, 2),
            CreatureState::Recovery
        );
    }

    #[test]
    fn creature_phase_step_slows_down_sleeping_state() {
        assert!(
            overlay_phase_step(CreatureState::Sleeping) < overlay_phase_step(CreatureState::Awake)
        );
        assert!(
            overlay_phase_step(CreatureState::Critical) > overlay_phase_step(CreatureState::Alert)
        );
        assert_eq!(creature_state_from_str("sleeping"), CreatureState::Sleeping);
        assert_eq!(creature_state_from_str("unknown"), CreatureState::Awake);
    }

    #[test]
    fn gui_runtime_config_keeps_auto_failover_setting() {
        let dotenv = HashMap::new();
        let account = Arc::new(Mutex::new(None));

        let config = runtime_config(&dotenv, &account, false);

        assert_eq!(config.api_base, ng::DEFAULT_API_BASE);
        assert!(!config.auto_failover);
    }

    #[test]
    fn gui_runtime_config_uses_selected_account_profile() {
        let mut dotenv = HashMap::new();
        dotenv.insert("VIBEMODE_ALT_KEY".to_string(), "test-key".to_string());
        let account = Arc::new(Mutex::new(Some(GuiAccount {
            api_key_env: Some("VIBEMODE_ALT_KEY".to_string()),
            api_base: Some("https://account-api.example".to_string()),
        })));

        let config = runtime_config(&dotenv, &account, true);

        assert_eq!(config.api_base, "https://account-api.example");
        assert_eq!(config.api_key, "test-key");
        assert!(config.auto_failover);
    }

    #[test]
    fn endpoint_label_maps_to_visible_api_url() {
        assert_eq!(
            ng::dashboard_endpoint_for_label("api", "https://custom.example"),
            ng::DEFAULT_API_BASE
        );
        assert_eq!(
            ng::dashboard_endpoint_for_label("r-api", "https://custom.example"),
            ng::FALLBACK_API_BASE
        );
        assert_eq!(
            ng::dashboard_endpoint_for_label("custom", "https://custom.example"),
            "https://custom.example"
        );
    }

    #[test]
    fn creature_points_scale_with_percent() {
        assert_eq!(creature_node_count(0.0), 8);
        assert_eq!(creature_node_count(78.0), 101);
        assert_eq!(creature_node_count(100.0), 128);
        assert_eq!(creature_node_count(250.0), 128);
    }

    #[test]
    fn creature_path_uses_dynamic_cubic_segments() {
        let path = creature_path_commands(78.0, 42.0);

        assert!(path.starts_with("M "));
        assert_eq!(path.matches("C ").count(), 101);
        assert!(path.ends_with('Z'));
    }

    #[test]
    fn organic_creature_skin_keeps_lower_point_cap() {
        assert_eq!(organic_creature_node_count(0.0), 8);
        assert_eq!(organic_creature_node_count(78.0), 51);
        assert_eq!(organic_creature_node_count(100.0), 64);
        assert_eq!(creature_node_count_for_skin(78.0, 0), 101);
        assert_eq!(creature_node_count_for_skin(78.0, 1), 51);
    }

    #[test]
    fn organic_creature_skin_uses_smooth_path() {
        let path = creature_path_commands_for_skin(78.0, 42.0, 1);

        assert!(path.starts_with("M "));
        assert_eq!(path.matches("C ").count(), 51);
        assert!(path.ends_with('Z'));
        assert_ne!(path, creature_path_commands_for_skin(78.0, 42.0, 0));
    }

    #[test]
    fn creature_sound_only_fires_on_relevant_transitions() {
        assert_eq!(creature_sound_for_transition(None, "warning"), None);
        assert_eq!(
            creature_sound_for_transition(Some("ok"), "warning"),
            Some(CreatureSound::Warning)
        );
        assert_eq!(
            creature_sound_for_transition(Some("warning"), "danger"),
            Some(CreatureSound::Danger)
        );
        assert_eq!(
            creature_sound_for_transition(Some("ok"), "danger"),
            Some(CreatureSound::Danger)
        );
        assert_eq!(
            creature_sound_for_transition(Some("danger"), "ok"),
            Some(CreatureSound::Recovery)
        );
        assert_eq!(
            creature_sound_for_transition(Some("warning"), "warning"),
            None
        );
        assert_eq!(creature_sound_for_transition(Some("warning"), "ok"), None);
    }

    #[test]
    fn gui_agent_status_handles_missing_binary() {
        let status = read_agent_status_for_gui("__vimit_missing_abtop_binary__");

        assert_eq!(status.summary, "агенты: abtop не найден; задайте ABTOP_BIN");
        assert_eq!(status.token_rate, "токены/мин: нет данных abtop");
    }

    #[test]
    fn gui_agent_status_is_disabled_without_abtop_bin() {
        let status = read_agent_status_for_gui("");

        assert_eq!(status.summary, "агенты: abtop отключён; задайте ABTOP_BIN");
        assert_eq!(status.token_rate, "токены/мин: нет данных abtop");
    }

    #[test]
    fn overlay_state_uses_rolling_samples() {
        let history = Arc::new(Mutex::new(OverlayHistory::default()));
        let windows = vec![test_window("5h", "warning", 78.0)];

        let state = build_overlay_state(&windows, "10,0 токены/мин", &history);

        assert_eq!(state.credit_rate_text, "0,0 кред/мин");
        assert_eq!(state.token_rate_text, "10,0 токены/мин");
        assert_eq!(state.reset_label, "сброс 5h окна");
        assert_eq!(state.reset_seconds, 3600);
        assert_eq!(state.spark_data.len(), 1);
    }

    #[test]
    fn overlay_rate_is_averaged_over_five_minutes() {
        let mut history = OverlayHistory::default();
        let now = Instant::now();

        assert_eq!(history.record_usage(now, 0.0), (0.0, false));
        assert_eq!(
            history.record_usage(now + Duration::from_secs(10), 1_000.0),
            (200.0, false)
        );
        assert_eq!(
            history.record_usage(now + Duration::from_secs(5 * 60), 1_500.0),
            (300.0, false)
        );
    }

    #[test]
    fn overlay_rate_discards_old_observations() {
        let mut history = OverlayHistory::default();
        let now = Instant::now();
        let later = now + OVERLAY_HISTORY_RETENTION + Duration::from_secs(1);

        history.record_usage(now, 100.0);
        history.record_usage(later, 600.0);

        assert_eq!(history.observations.len(), 1);
        assert_eq!(history.average_credit_rate(later), 0.0);
    }

    #[test]
    fn overlay_history_detects_window_reset() {
        let mut history = OverlayHistory::default();
        let now = Instant::now();

        assert_eq!(history.record_usage(now, 80.0), (0.0, false));
        let (_, reset_detected) = history.record_usage(now + Duration::from_secs(60), 5.0);

        assert!(reset_detected);
    }
}
