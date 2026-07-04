use super::*;
use chrono::TimeZone;
use std::collections::HashMap;

use crate::cli::args::Preset;
use crate::cli::theme::Theme;
use crate::cli::trends::TrendDay;

fn test_snapshot() -> StatusSnapshot {
    StatusSnapshot {
        stale: false,
        windows: ng::summarize_me(&ng::demo_payload(), 75.0, 90.0),
        daily: None,
        offline_duration_min: None,
        abtop: Some(serde_json::json!({
            "token_rate": 42.0,
            "sessions_total": 2,
            "sessions_active": 1,
            "agents": [{
                "agent_cli": "codex",
                "sessions": 2,
                "active": 1,
                "waiting": 1,
                "total_tokens": 1000,
                "active_tokens": 500,
                "max_context_pct": 27.0,
                "max_turn_count": 12
            }]
        })),
        fetched_at: chrono::Utc.timestamp_opt(0, 0).single().unwrap(),
        latency_ms: 0,
        api_endpoint: "r-api".to_string(),
    }
}

fn agent_stuck_snapshot() -> StatusSnapshot {
    let mut snapshot = test_snapshot();
    snapshot.abtop = Some(serde_json::json!({
        "token_rate": 128.4,
        "sessions_total": 4,
        "sessions_active": 1,
        "agents": [
            {
                "agent_cli": "codex",
                "sessions": 3,
                "active": 1,
                "waiting": 2,
                "total_tokens": 18500,
                "active_tokens": 9200,
                "max_context_pct": 88.0,
                "max_turn_count": 54
            },
            {
                "agent_cli": "claude",
                "sessions": 1,
                "active": 0,
                "waiting": 1,
                "total_tokens": 2400,
                "active_tokens": 0,
                "max_context_pct": 12.0,
                "max_turn_count": 8
            }
        ]
    }));
    snapshot
}

fn reset_edge_snapshot() -> StatusSnapshot {
    let mut snapshot = test_snapshot();
    snapshot.windows = vec![
        ng::WindowState {
            key: "5h",
            level: "ok".to_string(),
            reset: "sync pending".to_string(),
            reset_in_seconds: None,
            credits: Some(ng::Metric {
                used: 20.0,
                limit: 100.0,
                remaining: 80.0,
                percent: 20.0,
            }),
            requests: None,
            percent: 20.0,
        },
        ng::WindowState {
            key: "24h",
            level: "warning".to_string(),
            reset: "manual review".to_string(),
            reset_in_seconds: Some(600),
            credits: None,
            requests: Some(ng::Metric {
                used: 87.0,
                limit: 100.0,
                remaining: 13.0,
                percent: 87.0,
            }),
            percent: 87.0,
        },
    ];
    snapshot.abtop = None;
    snapshot
}

#[test]
fn monitor_output_has_dashboard_sections() {
    let snapshot = test_snapshot();
    let rendered = render_monitor(Some(&snapshot), None, 100, 30, 5, 4, true, 75.0);

    assert!(rendered.contains("vibemode quota"));
    assert!(rendered.contains("alerts"));
    assert!(rendered.contains("local agents"));
    assert!(rendered.contains("codex"));
}

#[test]
fn hbar_renders_correctly() {
    assert_eq!(hbar(50.0, 10), "[#####-----]");
    assert_eq!(hbar(0.0, 10), "[----------]");
    assert_eq!(hbar(100.0, 10), "[##########]");
}

#[test]
fn fit_text_truncates_with_tilde() {
    assert_eq!(fit_text("hello", 10), "hello");
    assert_eq!(fit_text("hello world", 8), "hello w~");
}

#[test]
fn panel_dimensions_are_correct() {
    let top = panel_top("test", 20);
    assert_eq!(top.len(), 20);
    assert!(top.starts_with('+'));
    assert!(top.ends_with('+'));

    let bottom = panel_bottom(20);
    assert_eq!(bottom.len(), 20);
}

#[test]
fn window_history_sparkline() {
    let mut hist = WindowHistory::new();
    assert!(hist.sparkline_values().is_empty());

    hist.record(50.0);
    hist.record(75.0);
    assert_eq!(hist.sparkline_values(), vec![50, 75]);

    for i in 0..25 {
        hist.record(i as f64);
    }
    assert_eq!(hist.sparkline_values().len(), 20);
}

#[allow(clippy::too_many_arguments)]
fn render_tui_to_string(
    snapshot: Option<&StatusSnapshot>,
    error: Option<&str>,
    interval_secs: u64,
    next_refresh_secs: u64,
    with_abtop: bool,
    warning_threshold: f64,
    window_history: &HashMap<&str, WindowHistory>,
    preset: Preset,
    width: u16,
    height: u16,
) -> String {
    render_tui_to_string_themed(
        snapshot,
        error,
        interval_secs,
        next_refresh_secs,
        with_abtop,
        warning_threshold,
        window_history,
        preset,
        Theme::Btop,
        width,
        height,
    )
}

#[allow(clippy::too_many_arguments)]
fn render_tui_to_string_with_context(
    snapshot: Option<&StatusSnapshot>,
    error: Option<&str>,
    interval_secs: u64,
    next_refresh_secs: u64,
    with_abtop: bool,
    warning_threshold: f64,
    window_history: &HashMap<&str, WindowHistory>,
    preset: Preset,
    width: u16,
    height: u16,
    panels: PanelState,
    trend_days: &[TrendDay],
) -> String {
    use ratatui::backend::TestBackend;
    let backend = TestBackend::new(width, height);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            draw_frame(
                frame,
                snapshot,
                error,
                interval_secs,
                next_refresh_secs,
                with_abtop,
                warning_threshold,
                window_history,
                preset,
                Theme::Btop,
                &panels,
                &[],
                0,
                trend_days,
            );
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let mut out = String::new();
    for y in 0..height {
        for x in 0..width {
            let cell = &buffer[(x, y)];
            out.push_str(cell.symbol());
        }
        out.push('\n');
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn render_tui_to_string_themed(
    snapshot: Option<&StatusSnapshot>,
    error: Option<&str>,
    interval_secs: u64,
    next_refresh_secs: u64,
    with_abtop: bool,
    warning_threshold: f64,
    window_history: &HashMap<&str, WindowHistory>,
    preset: Preset,
    theme: Theme,
    width: u16,
    height: u16,
) -> String {
    use ratatui::backend::TestBackend;
    let backend = TestBackend::new(width, height);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    let panels = PanelState::default();
    terminal
        .draw(|frame| {
            draw_frame(
                frame,
                snapshot,
                error,
                interval_secs,
                next_refresh_secs,
                with_abtop,
                warning_threshold,
                window_history,
                preset,
                theme,
                &panels,
                &[],
                0,
                &[],
            );
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let mut out = String::new();
    for y in 0..height {
        for x in 0..width {
            let cell = &buffer[(x, y)];
            out.push_str(cell.symbol());
        }
        out.push('\n');
    }
    out
}

#[test]
fn tui_snapshot_full_preset() {
    let snapshot = test_snapshot();
    let mut history = HashMap::new();
    history.insert("5h", {
        let mut h = WindowHistory::new();
        h.record(78.0);
        h
    });
    history.insert("24h", {
        let mut h = WindowHistory::new();
        h.record(45.0);
        h
    });
    history.insert("7d", {
        let mut h = WindowHistory::new();
        h.record(30.0);
        h
    });
    history.insert("30d", {
        let mut h = WindowHistory::new();
        h.record(12.0);
        h
    });

    let output = render_tui_to_string(
        Some(&snapshot),
        None,
        5,
        3,
        true,
        75.0,
        &history,
        Preset::Full,
        120,
        40,
    );
    insta::assert_snapshot!(output);
}

#[test]
fn tui_snapshot_compact_preset() {
    let snapshot = test_snapshot();
    let mut history = HashMap::new();
    history.insert("5h", {
        let mut h = WindowHistory::new();
        h.record(78.0);
        h
    });

    let output = render_tui_to_string(
        Some(&snapshot),
        None,
        5,
        3,
        true,
        75.0,
        &history,
        Preset::Compact,
        80,
        25,
    );
    insta::assert_snapshot!(output);
}

#[test]
fn tui_snapshot_mini_preset() {
    let snapshot = test_snapshot();
    let mut history = HashMap::new();
    history.insert("5h", {
        let mut h = WindowHistory::new();
        h.record(78.0);
        h
    });

    let output = render_tui_to_string(
        Some(&snapshot),
        None,
        5,
        3,
        true,
        75.0,
        &history,
        Preset::Mini,
        60,
        15,
    );
    insta::assert_snapshot!(output);
}

#[test]
fn tui_snapshot_with_error() {
    let snapshot = test_snapshot();
    let history = HashMap::new();

    let output = render_tui_to_string(
        Some(&snapshot),
        Some("connection timeout"),
        5,
        3,
        true,
        75.0,
        &history,
        Preset::Full,
        100,
        30,
    );
    insta::assert_snapshot!(output);
}

#[test]
fn tui_snapshot_waiting() {
    let history = HashMap::new();

    let output = render_tui_to_string(
        None,
        None,
        5,
        3,
        true,
        75.0,
        &history,
        Preset::Full,
        100,
        30,
    );
    insta::assert_snapshot!(output);
}

#[test]
fn tui_snapshot_dracula_theme() {
    let snapshot = test_snapshot();
    let mut history = HashMap::new();
    history.insert("5h", {
        let mut h = WindowHistory::new();
        h.record(78.0);
        h
    });

    let output = render_tui_to_string_themed(
        Some(&snapshot),
        None,
        5,
        3,
        true,
        75.0,
        &history,
        Preset::Full,
        Theme::Dracula,
        120,
        40,
    );
    insta::assert_snapshot!(output);
}

#[test]
fn tui_snapshot_high_contrast_theme() {
    let snapshot = test_snapshot();
    let mut history = HashMap::new();
    history.insert("5h", {
        let mut h = WindowHistory::new();
        h.record(78.0);
        h
    });

    let output = render_tui_to_string_themed(
        Some(&snapshot),
        None,
        5,
        3,
        true,
        75.0,
        &history,
        Preset::Full,
        Theme::HighContrast,
        120,
        40,
    );
    insta::assert_snapshot!(output);
}

#[test]
fn tui_snapshot_agent_stuck_state() {
    let snapshot = agent_stuck_snapshot();
    let history = HashMap::new();

    let output = render_tui_to_string(
        Some(&snapshot),
        None,
        5,
        1,
        true,
        75.0,
        &history,
        Preset::Full,
        120,
        40,
    );
    insta::assert_snapshot!(output);
}

#[test]
fn tui_snapshot_reset_edge_with_empty_trends() {
    let snapshot = reset_edge_snapshot();
    let history = HashMap::new();
    let panels = PanelState {
        show_trends: true,
        ..PanelState::default()
    };

    let output = render_tui_to_string_with_context(
        Some(&snapshot),
        None,
        5,
        4,
        true,
        75.0,
        &history,
        Preset::Full,
        120,
        40,
        panels,
        &[],
    );
    insta::assert_snapshot!(output);
}
