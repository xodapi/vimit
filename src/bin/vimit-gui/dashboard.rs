use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use slint::{ModelRc, VecModel, Weak};

use crate::AppWindow;
use crate::config::{GuiAccount, endpoint_for_label, gui_load_dotenv, runtime_config};
use crate::ng;
use crate::overlay::{
    CreatureState, OverlayHistory, OverlayState, build_overlay_state, creature_node_count_for_skin,
    creature_path_commands_for_skin, format_overlay_countdown, maybe_play_creature_sound,
};
use crate::platform::read_agent_status_for_gui;
use crate::tray::{TrayStatus, tray_status_from_dashboard, update_tray_status};

pub(crate) struct GuiDashboardResult {
    dashboard: ng::Dashboard,
    active_endpoint_label: String,
    five_trend_data: Vec<f32>,
    day_trend_data: Vec<f32>,
    week_trend_data: Vec<f32>,
    month_trend_data: Vec<f32>,
    overlay: OverlayState,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn start_refresh(
    app: Weak<AppWindow>,
    demo: bool,
    mock_path: Arc<Option<String>>,
    http: Arc<ng::HttpClient>,
    account: Arc<Mutex<Option<GuiAccount>>>,
    router: Arc<Mutex<ng::Router>>,
    warning: f64,
    danger: f64,
    generation: Arc<AtomicU64>,
    is_refreshing: Arc<AtomicBool>,
    overlay_history: Arc<Mutex<OverlayHistory>>,
    auto_failover: Arc<AtomicBool>,
) {
    if is_refreshing.swap(true, Ordering::SeqCst) {
        return;
    }

    let my_gen = generation.fetch_add(1, Ordering::Relaxed) + 1;
    thread::spawn(move || {
        let result = load_dashboard(
            demo,
            mock_path.as_deref(),
            &http,
            &account,
            warning,
            danger,
            &router,
            &overlay_history,
            auto_failover.load(Ordering::Relaxed),
        );
        if generation.load(Ordering::Relaxed) != my_gen {
            is_refreshing.store(false, Ordering::SeqCst);
            return;
        }
        let _ = app.upgrade_in_event_loop(move |app| {
            apply_dashboard(&app, result);
        });
        is_refreshing.store(false, Ordering::SeqCst);
    });
}

pub(crate) fn apply_dashboard(app: &AppWindow, result: Result<GuiDashboardResult, String>) {
    if let Ok(result) = result.as_ref() {
        update_tray_status(tray_status_from_dashboard(
            &result.dashboard.source,
            &result.dashboard.windows,
        ));
    }

    let offline_min = ng::get_offline_duration_min()
        .map(|value| value as i32)
        .unwrap_or(-1);
    app.set_api_offline_min(offline_min);

    match result {
        Ok(result) => {
            let dashboard = result.dashboard;
            let creature_level = dashboard
                .windows
                .iter()
                .find(|window| window.key == "5h")
                .map(|window| window.level.as_str())
                .unwrap_or("ok");
            if result.overlay.creature_state != CreatureState::Sleeping {
                maybe_play_creature_sound(creature_level);
            }
            app.set_status_text(dashboard.status.into());
            app.set_source_text(dashboard.source.into());
            app.set_agent_text(dashboard.agent.into());
            app.set_token_rate_text(dashboard.token_rate.clone().into());
            let raw_rate = dashboard
                .token_rate
                .split_whitespace()
                .next()
                .and_then(|value| value.replace(',', ".").parse::<f32>().ok())
                .unwrap_or(0.0);
            app.set_token_rate_raw(raw_rate);
            app.set_active_endpoint_label(result.active_endpoint_label.into());
            app.set_overlay_credit_rate_text(result.overlay.credit_rate_text.into());
            app.set_overlay_token_rate_text(result.overlay.token_rate_text.into());
            app.set_overlay_percent_hour_text(result.overlay.percent_hour_text.into());
            app.set_overlay_delta_text(result.overlay.delta_text.into());
            app.set_overlay_delta_level(result.overlay.delta_level.into());
            app.set_overlay_reset_label(result.overlay.reset_label.into());
            app.set_overlay_reset_seconds(result.overlay.reset_seconds);
            app.set_overlay_reset_text(
                format_overlay_countdown(result.overlay.reset_seconds).into(),
            );
            app.set_overlay_creature_percent(result.overlay.creature_percent);
            app.set_overlay_creature_state(result.overlay.creature_state.as_str().into());
            let skin = app.get_overlay_creature_skin();
            app.set_overlay_creature_points(creature_node_count_for_skin(
                result.overlay.creature_percent,
                skin,
            ) as i32);
            app.set_overlay_creature_path(
                creature_path_commands_for_skin(
                    result.overlay.creature_percent,
                    app.get_overlay_pulse_phase(),
                    skin,
                )
                .into(),
            );

            if let Some(latest) = ng::cli::update::latest_checked_version() {
                app.set_new_version_label(latest.into());
            } else {
                app.set_new_version_label("".into());
            }

            let create_model =
                |values: Vec<f32>| ModelRc::new(std::rc::Rc::new(VecModel::from(values)));
            app.set_five_trend_data(create_model(result.five_trend_data));
            app.set_day_trend_data(create_model(result.day_trend_data));
            app.set_week_trend_data(create_model(result.week_trend_data));
            app.set_month_trend_data(create_model(result.month_trend_data));
            app.set_overlay_spark_data(create_model(result.overlay.spark_data));

            apply_window(
                app,
                "5h",
                dashboard.windows.iter().find(|window| window.key == "5h"),
            );
            apply_window(
                app,
                "24h",
                dashboard.windows.iter().find(|window| window.key == "24h"),
            );
            apply_window(
                app,
                "7d",
                dashboard.windows.iter().find(|window| window.key == "7d"),
            );
            apply_window(
                app,
                "30d",
                dashboard.windows.iter().find(|window| window.key == "30d"),
            );
        }
        Err(error) => {
            let msg = if error.contains("VibeMode /v1/me returned HTTP 401") {
                "Check your VIBEMODE_API_KEY in .env"
            } else if error.contains("cannot reach VibeMode API") {
                "Check network / VIBEMODE_API_BASE"
            } else if error.contains("VIBEMODE_API_KEY is required")
                || error.contains("NEUROGATE_API_KEY is required")
            {
                "Set VIBEMODE_API_KEY or use Demo"
            } else {
                &error
            };
            app.set_status_text(msg.into());
            app.set_error_text(msg.into());
            update_tray_status(TrayStatus {
                color: (214, 111, 115),
                tooltip: format!("VibeMode Control\nошибка: {msg}"),
            });
        }
    }
}

fn apply_window(app: &AppWindow, key: &str, window: Option<&ng::WindowState>) {
    let fallback = ng::WindowState {
        key: "n/a",
        level: "н/д".to_string(),
        reset: "сброс неизвестен".to_string(),
        reset_in_seconds: None,
        credits: None,
        requests: None,
        percent: 0.0,
    };
    let window = window.unwrap_or(&fallback);
    let level = window.level.clone().into();
    let reset = window.reset.clone().into();
    let credits = ng::metric_text("кредиты", window.credits.as_ref()).into();
    let requests = ng::metric_text("запросы", window.requests.as_ref()).into();
    let percent_text = ng::format_percent(window.percent).into();
    let percent = window.percent as f32;
    let credit_percent =
        ng::peak_percent(window.credits.as_ref(), window.requests.as_ref()).unwrap_or(0.0) as f32;
    let request_percent =
        ng::peak_percent(window.credits.as_ref(), window.requests.as_ref()).unwrap_or(0.0) as f32;

    let donut_remaining = match window.credits.as_ref() {
        Some(metric) => ng::short_number(metric.remaining),
        None => "н/д".to_string(),
    };
    let donut_limit = match window.credits.as_ref() {
        Some(metric) => ng::short_number(metric.limit),
        None => "н/д".to_string(),
    };

    match key {
        "5h" => {
            app.set_five_level(level);
            app.set_five_reset(reset);
            app.set_five_credits(credits);
            app.set_five_requests(requests);
            app.set_five_rate("".into());
            app.set_five_percent_text(percent_text);
            app.set_five_percent(percent);
            app.set_five_credit_percent(credit_percent);
            app.set_five_request_percent(request_percent);
        }
        "24h" => {
            app.set_day_level(level);
            app.set_day_reset(reset);
            app.set_day_credits(credits);
            app.set_day_requests(requests);
            app.set_day_rate("".into());
            app.set_day_percent_text(percent_text);
            app.set_day_percent(percent);
            app.set_day_credit_percent(credit_percent);
            app.set_day_request_percent(request_percent);
        }
        "7d" => {
            app.set_week_level(level);
            app.set_week_reset(reset);
            app.set_week_credits(credits);
            app.set_week_requests(requests);
            app.set_week_rate("".into());
            app.set_week_percent_text(percent_text);
            app.set_week_percent(percent);
            app.set_week_credit_percent(credit_percent);
            app.set_week_request_percent(request_percent);
            app.set_donut_remaining(donut_remaining.into());
            app.set_donut_limit(donut_limit.into());
        }
        "30d" => {
            app.set_month_level(level);
            app.set_month_reset(reset);
            app.set_month_credits(credits);
            app.set_month_requests(requests);
            app.set_month_rate("".into());
            app.set_month_percent_text(percent_text);
            app.set_month_percent(percent);
            app.set_month_credit_percent(credit_percent);
            app.set_month_request_percent(request_percent);
        }
        _ => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn load_dashboard(
    force_demo: bool,
    mock_path: Option<&str>,
    http: &ng::HttpClient,
    account: &Arc<Mutex<Option<GuiAccount>>>,
    warning: f64,
    danger: f64,
    router: &Arc<Mutex<ng::Router>>,
    overlay_history: &Arc<Mutex<OverlayHistory>>,
    auto_failover: bool,
) -> Result<GuiDashboardResult, String> {
    let (payload, source, active_endpoint_label, abtop_bin, live_api_key_present) = if force_demo {
        (
            ng::demo_payload(),
            "источник: встроенные демо-данные".to_string(),
            "demo".to_string(),
            String::new(),
            false,
        )
    } else if let Some(path) = mock_path {
        (
            ng::load_mock(path)?,
            format!("источник: mock {}", path),
            "mock".to_string(),
            String::new(),
            false,
        )
    } else {
        let dotenv = gui_load_dotenv()?;
        let config = runtime_config(&dotenv, account, auto_failover);
        if config.api_key.is_empty() {
            (
                ng::demo_payload(),
                "источник: демо; добавьте VIBEMODE_API_KEY в .env для live-лимитов".to_string(),
                "demo".to_string(),
                config.abtop_bin,
                false,
            )
        } else {
            let (value, label) = if config.auto_failover
                && config.api_base.trim_end_matches('/') == ng::DEFAULT_API_BASE
            {
                let mut router = router.lock().unwrap();
                http.fetch_me_with_retry(&config.api_key, &mut router, &config.api_base)?
            } else {
                let mut router = ng::Router::new(
                    config.api_base.clone(),
                    ng::api_fallbacks_for(&config.api_base, config.auto_failover),
                );
                http.fetch_me_with_retry(&config.api_key, &mut router, &config.api_base)?
            };
            let endpoint = endpoint_for_label(&label, &config.api_base);
            (
                value,
                format!("источник: live VibeMode /v1/me на {endpoint}"),
                label,
                config.abtop_bin,
                true,
            )
        }
    };

    let windows = ng::summarize_me(&payload, warning, danger);
    let status = ng::dashboard_status(&windows);
    let agent = read_agent_status_for_gui(&abtop_bin);
    let overlay = build_overlay_state(&windows, &agent.token_rate, overlay_history);

    let mut five_trend_data = Vec::new();
    let mut day_trend_data = Vec::new();
    let mut week_trend_data = Vec::new();
    let mut month_trend_data = Vec::new();

    if let Ok(Some(store)) = ng::cli::trends::TrendStore::open() {
        if live_api_key_present {
            let _ = store.save_snapshot(&windows, chrono::Utc::now());
        }
        if let Ok(days) = store.query_trends(15) {
            for day in &days {
                if let Some(window) = day.windows.iter().find(|window| window.key == "5h") {
                    five_trend_data.push(window.peak_max as f32);
                }
                if let Some(window) = day.windows.iter().find(|window| window.key == "24h") {
                    day_trend_data.push(window.peak_max as f32);
                }
                if let Some(window) = day.windows.iter().find(|window| window.key == "7d") {
                    week_trend_data.push(window.peak_max as f32);
                }
                if let Some(window) = day.windows.iter().find(|window| window.key == "30d") {
                    month_trend_data.push(window.peak_max as f32);
                }
            }
        }
    }

    if five_trend_data.is_empty() {
        five_trend_data = vec![
            10.0, 15.0, 20.0, 35.0, 40.0, 30.0, 45.0, 50.0, 40.0, 55.0, 60.0, 65.0, 78.0, 70.0,
            78.0,
        ];
        day_trend_data = vec![
            20.0, 25.0, 30.0, 42.0, 35.0, 40.0, 48.0, 55.0, 50.0, 60.0, 58.0, 62.0, 70.0, 65.0,
            75.0,
        ];
        week_trend_data = vec![
            15.0, 18.0, 22.0, 30.0, 28.0, 32.0, 38.0, 45.0, 42.0, 50.0, 48.0, 52.0, 58.0, 55.0,
            62.0,
        ];
        month_trend_data = vec![
            5.0, 8.0, 12.0, 18.0, 15.0, 20.0, 25.0, 32.0, 28.0, 35.0, 32.0, 38.0, 42.0, 40.0, 45.0,
        ];
    }

    Ok(GuiDashboardResult {
        dashboard: ng::Dashboard {
            source,
            status,
            agent: agent.summary,
            token_rate: agent.token_rate,
            windows,
            daily: None,
        },
        active_endpoint_label,
        five_trend_data,
        day_trend_data,
        week_trend_data,
        month_trend_data,
        overlay,
    })
}
