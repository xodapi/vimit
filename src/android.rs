#[cfg(all(target_os = "android", feature = "android-gui"))]
slint::include_modules!();

#[cfg(all(target_os = "android", feature = "android-gui"))]
use slint::{ComponentHandle, ModelRc, SharedString, VecModel, Weak};

#[cfg(any(test, all(target_os = "android", feature = "android-gui")))]
use crate::AgentPulse;
use crate::{AgentBurnEvent, AgentBurnState, DEFAULT_ANDROID_ALERT_COOLDOWN_SECS, WindowState};
#[cfg(all(target_os = "android", feature = "android-gui"))]
use crate::{
    DEFAULT_ABTOP_BIN, DEFAULT_API_BASE, DEFAULT_DANGER_THRESHOLD, DEFAULT_WARNING_THRESHOLD,
    HttpClient, Router, USER_AGENT_GUI, api_fallbacks_for, dashboard_status, demo_payload,
    format_percent, metric_text, peak_percent, read_abtop_status, short_number, summarize_me,
};
#[cfg(all(target_os = "android", feature = "android-gui"))]
use std::fs;
#[cfg(all(target_os = "android", feature = "android-gui"))]
use std::path::PathBuf;
#[cfg(all(target_os = "android", feature = "android-gui"))]
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
#[cfg(all(target_os = "android", feature = "android-gui"))]
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const ANDROID_NOTIFICATION_RUNTIME_PERMISSION_SDK: i32 = 33;
#[cfg(any(test, all(target_os = "android", feature = "android-gui")))]
const ANDROID_AGENT_HISTORY_SAMPLES: usize = 36;
#[cfg(any(test, all(target_os = "android", feature = "android-gui")))]
const ANDROID_AGENT_IDLE_TIMEOUT_SECS: u64 = 45;
#[cfg(all(target_os = "android", feature = "android-gui"))]
const ANDROID_REFRESH_INTERVAL_SECS: u64 = 10;

#[cfg(any(test, all(target_os = "android", feature = "android-gui")))]
#[derive(Debug, Clone, PartialEq)]
struct AndroidLiveTokenUiState {
    pulse_label: &'static str,
    pulse_level: &'static str,
    rate_text: String,
    detail_text: String,
    history: Vec<f32>,
    history_path: String,
    timed_out: bool,
}

#[cfg(any(test, all(target_os = "android", feature = "android-gui")))]
impl AndroidLiveTokenUiState {
    fn unknown() -> Self {
        Self {
            pulse_label: "Unknown",
            pulse_level: "unknown",
            rate_text: "токены/сек: нет данных".to_string(),
            detail_text: "телеметрия недоступна".to_string(),
            history: Vec::new(),
            history_path: String::new(),
            timed_out: false,
        }
    }
}

#[cfg(any(test, all(target_os = "android", feature = "android-gui")))]
#[derive(Debug, Clone)]
struct AndroidLiveTokenTracker {
    capacity: usize,
    idle_timeout_secs: u64,
    samples: Vec<f64>,
    last_nonzero_secs: Option<u64>,
}

#[cfg(any(test, all(target_os = "android", feature = "android-gui")))]
impl AndroidLiveTokenTracker {
    fn new(capacity: usize, idle_timeout_secs: u64) -> Self {
        Self {
            capacity,
            idle_timeout_secs,
            samples: Vec::new(),
            last_nonzero_secs: None,
        }
    }

    fn observe(&mut self, pulse: Option<&AgentPulse>, now_secs: u64) -> AndroidLiveTokenUiState {
        let Some(pulse) = pulse else {
            let history = self.normalized_history();
            return AndroidLiveTokenUiState {
                history_path: android_history_path(&history),
                history,
                ..AndroidLiveTokenUiState::unknown()
            };
        };

        if !pulse.token_rate_known {
            let history = self.normalized_history();
            return AndroidLiveTokenUiState {
                history_path: android_history_path(&history),
                history,
                ..AndroidLiveTokenUiState::unknown()
            };
        }

        let rate = pulse.token_rate_per_sec.max(0.0);
        self.push_sample(rate);

        if rate > 0.0 {
            self.last_nonzero_secs = Some(now_secs);
            let history = self.normalized_history();
            return AndroidLiveTokenUiState {
                pulse_label: "Active",
                pulse_level: "active",
                rate_text: format_android_token_rate(rate),
                detail_text: format!(
                    "сессии {}/{} · интервал {} ms",
                    pulse.sessions_active, pulse.sessions_total, pulse.interval_ms
                ),
                history_path: android_history_path(&history),
                history,
                timed_out: false,
            };
        }

        let idle_for = self
            .last_nonzero_secs
            .map(|seen| now_secs.saturating_sub(seen))
            .unwrap_or(0);
        let timed_out = self.last_nonzero_secs.is_some() && idle_for >= self.idle_timeout_secs;
        let detail_text = if timed_out {
            format!("пул завершён · {idle_for}с без расхода")
        } else if self.last_nonzero_secs.is_some() {
            format!("расход остановился · {idle_for}с")
        } else {
            "расход 0 ток/сек".to_string()
        };

        let history = self.normalized_history();
        AndroidLiveTokenUiState {
            pulse_label: "Idle",
            pulse_level: if timed_out { "finished" } else { "idle" },
            rate_text: format_android_token_rate(rate),
            detail_text,
            history_path: android_history_path(&history),
            history,
            timed_out,
        }
    }

    fn push_sample(&mut self, value: f64) {
        if self.capacity == 0 {
            return;
        }
        self.samples.push(value);
        if self.samples.len() > self.capacity {
            let overflow = self.samples.len() - self.capacity;
            self.samples.drain(0..overflow);
        }
    }

    fn normalized_history(&self) -> Vec<f32> {
        let max_rate = self.samples.iter().copied().fold(0.0, f64::max);
        if max_rate <= f64::EPSILON {
            return self.samples.iter().map(|_| 0.0).collect();
        }
        self.samples
            .iter()
            .map(|sample| ((sample / max_rate) * 100.0).clamp(0.0, 100.0) as f32)
            .collect()
    }
}

#[cfg(any(test, all(target_os = "android", feature = "android-gui")))]
impl Default for AndroidLiveTokenTracker {
    fn default() -> Self {
        Self::new(
            ANDROID_AGENT_HISTORY_SAMPLES,
            ANDROID_AGENT_IDLE_TIMEOUT_SECS,
        )
    }
}

#[cfg(any(test, all(target_os = "android", feature = "android-gui")))]
fn format_android_token_rate(rate: f64) -> String {
    let value = format!("{:.1}", rate).replace('.', ",");
    format!("{value} ток/сек")
}

#[cfg(any(test, all(target_os = "android", feature = "android-gui")))]
fn android_history_path(samples: &[f32]) -> String {
    if samples.is_empty() {
        return String::new();
    }

    if samples.len() == 1 {
        let y = 38.0 - (samples[0].clamp(0.0, 100.0) / 100.0 * 34.0);
        return format!("M 0.00 {y:.2} L 100.00 {y:.2} ");
    }

    let mut commands = String::with_capacity(samples.len() * 16);
    let last = samples.len().saturating_sub(1).max(1) as f32;
    for (index, sample) in samples.iter().enumerate() {
        let x = index as f32 / last * 100.0;
        let y = 38.0 - (sample.clamp(0.0, 100.0) / 100.0 * 34.0);
        if index == 0 {
            commands.push_str(&format!("M {x:.2} {y:.2} "));
        } else {
            commands.push_str(&format!("L {x:.2} {y:.2} "));
        }
    }

    commands
}
#[cfg(all(target_os = "android", feature = "android-gui"))]
const ANDROID_NOTIFICATION_PERMISSION_GRANTED: i32 = 0;

#[cfg(all(target_os = "android", feature = "android-gui"))]
const ANDROID_POST_NOTIFICATIONS_PERMISSION: &str = "android.permission.POST_NOTIFICATIONS";
#[cfg(all(target_os = "android", feature = "android-gui"))]
const ANDROID_NOTIFICATION_PERMISSION_REQUEST_CODE: i32 = 113;

#[cfg(all(target_os = "android", feature = "android-gui"))]
#[unsafe(no_mangle)]
pub fn android_main(app: slint::android::AndroidApp) {
    let data_dir = app.internal_data_path();
    slint::android::init(app.clone()).expect("cannot initialize Android backend");
    let window = AppWindow::new().expect("cannot initialize Slint window");
    window.set_is_android(true);
    window.set_needs_setup(false);
    if let Err(error) = android_request_notification_permission_if_needed(&app) {
        window.set_source_text(format!("Android notification permission: {error}").into());
    }

    let key_path = android_key_path(data_dir);
    let saved_key = android_load_api_key(&key_path).unwrap_or_default();
    window.set_api_key_configured(!saved_key.is_empty());
    window.set_setup_status_text("Вставьте VIBEMODE_API_KEY и нажмите Сохранить ключ.".into());

    let key_state = Arc::new(Mutex::new(saved_key));
    let live_token_tracker = Arc::new(Mutex::new(AndroidLiveTokenTracker::default()));
    let refresh_in_flight = Arc::new(AtomicBool::new(false));
    let path_for_save = key_path.clone();
    let key_for_save = key_state.clone();
    let weak = window.as_weak();
    window.on_save_api_key(move |value| {
        if let Some(window) = weak.upgrade() {
            let key = value.trim().to_string();
            if key.is_empty() {
                window.set_error_text("API key пустой".into());
                return;
            }
            match android_save_api_key(&path_for_save, &key) {
                Ok(()) => {
                    *key_for_save.lock().unwrap() = key;
                    window.set_api_key_input("".into());
                    window.set_api_key_configured(true);
                    window.set_error_text("".into());
                    window.set_status_text("Ключ сохранён. Нажмите Проверить.".into());
                }
                Err(error) => window.set_error_text(error.into()),
            }
        }
    });

    let key_for_refresh = key_state.clone();
    let tracker_for_refresh = live_token_tracker.clone();
    let in_flight_for_refresh = refresh_in_flight.clone();
    let weak = window.as_weak();
    window.on_refresh_requested(move || {
        android_start_refresh(
            weak.clone(),
            key_for_refresh.clone(),
            tracker_for_refresh.clone(),
            in_flight_for_refresh.clone(),
            false,
        );
    });

    let key_for_demo = key_state.clone();
    let tracker_for_demo = live_token_tracker.clone();
    let in_flight_for_demo = refresh_in_flight.clone();
    let weak = window.as_weak();
    window.on_demo_requested(move || {
        android_start_refresh(
            weak.clone(),
            key_for_demo.clone(),
            tracker_for_demo.clone(),
            in_flight_for_demo.clone(),
            true,
        );
    });

    let auto_refresh_timer = slint::Timer::default();
    let weak = window.as_weak();
    let key_for_timer = key_state.clone();
    let tracker_for_timer = live_token_tracker.clone();
    let in_flight_for_timer = refresh_in_flight.clone();
    auto_refresh_timer.start(
        slint::TimerMode::Repeated,
        Duration::from_secs(ANDROID_REFRESH_INTERVAL_SECS),
        move || {
            android_start_refresh(
                weak.clone(),
                key_for_timer.clone(),
                tracker_for_timer.clone(),
                in_flight_for_timer.clone(),
                false,
            );
        },
    );

    android_start_refresh(
        window.as_weak(),
        key_state,
        live_token_tracker,
        refresh_in_flight,
        false,
    );
    window.run().expect("cannot run Slint window");
}

#[cfg(all(target_os = "android", feature = "android-gui"))]
fn android_key_path(data_dir: Option<PathBuf>) -> PathBuf {
    data_dir
        .unwrap_or_else(|| PathBuf::from("."))
        .join("vimit-api-key")
}

#[cfg(all(target_os = "android", feature = "android-gui"))]
fn android_load_api_key(path: &std::path::Path) -> Result<String, String> {
    match fs::read_to_string(path) {
        Ok(value) => Ok(value.trim().to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(format!("cannot read Android API key: {error}")),
    }
}

#[cfg(all(target_os = "android", feature = "android-gui"))]
fn android_save_api_key(path: &std::path::Path, key: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create Android config dir: {error}"))?;
    }
    fs::write(path, key).map_err(|error| format!("cannot save Android API key: {error}"))
}

#[cfg(all(target_os = "android", feature = "android-gui"))]
fn android_start_refresh(
    app: Weak<AppWindow>,
    key_state: Arc<Mutex<String>>,
    live_token_tracker: Arc<Mutex<AndroidLiveTokenTracker>>,
    refresh_in_flight: Arc<AtomicBool>,
    demo: bool,
) {
    if refresh_in_flight.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || {
        let result = android_load_dashboard(&key_state, &live_token_tracker, demo);
        let _ = app.upgrade_in_event_loop(move |app| android_apply_dashboard(&app, result));
        refresh_in_flight.store(false, Ordering::SeqCst);
    });
}

#[cfg(all(target_os = "android", feature = "android-gui"))]
#[derive(Debug, Clone)]
struct AndroidDashboardSnapshot {
    windows: Vec<WindowState>,
    source: String,
    endpoint: String,
    live_token: AndroidLiveTokenUiState,
}

#[cfg(all(target_os = "android", feature = "android-gui"))]
fn android_load_dashboard(
    key_state: &Arc<Mutex<String>>,
    live_token_tracker: &Arc<Mutex<AndroidLiveTokenTracker>>,
    demo: bool,
) -> Result<AndroidDashboardSnapshot, String> {
    let now_secs = android_now_secs();
    let live_token = android_load_live_token_state(live_token_tracker, demo, now_secs);

    if demo {
        let windows = summarize_me(
            &demo_payload(),
            DEFAULT_WARNING_THRESHOLD,
            DEFAULT_DANGER_THRESHOLD,
        );
        return Ok(AndroidDashboardSnapshot {
            windows,
            source: "источник: встроенные демо-данные".to_string(),
            endpoint: "demo".to_string(),
            live_token,
        });
    }

    let key = key_state.lock().unwrap().clone();
    if key.is_empty() {
        let windows = summarize_me(
            &demo_payload(),
            DEFAULT_WARNING_THRESHOLD,
            DEFAULT_DANGER_THRESHOLD,
        );
        return Ok(AndroidDashboardSnapshot {
            windows,
            source: "источник: демо; сохраните VIBEMODE_API_KEY для live-лимитов".to_string(),
            endpoint: "demo".to_string(),
            live_token,
        });
    }

    let http = HttpClient::new(USER_AGENT_GUI)?;
    let mut router = Router::new(
        DEFAULT_API_BASE.to_string(),
        api_fallbacks_for(DEFAULT_API_BASE, true),
    );
    let (payload, label) = http.fetch_me_with_retry(&key, &mut router, DEFAULT_API_BASE)?;
    let windows = summarize_me(
        &payload,
        DEFAULT_WARNING_THRESHOLD,
        DEFAULT_DANGER_THRESHOLD,
    );
    Ok(AndroidDashboardSnapshot {
        windows,
        source: format!("источник: live VibeMode /v1/me ({label})"),
        endpoint: label,
        live_token,
    })
}

#[cfg(all(target_os = "android", feature = "android-gui"))]
fn android_load_live_token_state(
    tracker: &Arc<Mutex<AndroidLiveTokenTracker>>,
    demo: bool,
    now_secs: u64,
) -> AndroidLiveTokenUiState {
    let pulse = if demo {
        Some(android_demo_agent_pulse(now_secs))
    } else {
        read_abtop_status(DEFAULT_ABTOP_BIN)
            .as_ref()
            .map(AgentPulse::from_abtop_status)
    };
    tracker.lock().unwrap().observe(pulse.as_ref(), now_secs)
}

#[cfg(all(target_os = "android", feature = "android-gui"))]
fn android_demo_agent_pulse(now_secs: u64) -> AgentPulse {
    let token_rate = if now_secs % 40 < 28 { 18.0 } else { 0.0 };
    AgentPulse::from_abtop_status(&serde_json::json!({
        "interval_ms": 1000,
        "token_rate": token_rate,
        "sessions_total": 2,
        "sessions_active": if token_rate > 0.0 { 1 } else { 0 },
        "agents": [
            {"agent_cli": "codex", "token_rate": token_rate, "max_context_pct": 42.0}
        ]
    }))
}

#[cfg(all(target_os = "android", feature = "android-gui"))]
fn android_now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[cfg(all(target_os = "android", feature = "android-gui"))]
fn android_apply_dashboard(app: &AppWindow, result: Result<AndroidDashboardSnapshot, String>) {
    match result {
        Ok(snapshot) => {
            let windows = snapshot.windows;
            app.set_error_text("".into());
            app.set_status_text(dashboard_status(&windows).into());
            app.set_source_text(snapshot.source.into());
            app.set_active_endpoint_label(snapshot.endpoint.into());
            android_apply_window(app, "5h", windows.iter().find(|window| window.key == "5h"));
            android_apply_window(
                app,
                "24h",
                windows.iter().find(|window| window.key == "24h"),
            );
            android_apply_window(app, "7d", windows.iter().find(|window| window.key == "7d"));
            android_apply_window(
                app,
                "30d",
                windows.iter().find(|window| window.key == "30d"),
            );
            android_apply_vimichi_dashboard_state(app, &windows);
            android_apply_live_token_state(app, &snapshot.live_token);
        }
        Err(error) => {
            let msg = if error.contains("HTTP 401") {
                "Проверьте VIBEMODE_API_KEY"
            } else {
                "Не удалось загрузить VibeMode"
            };
            app.set_error_text(msg.into());
            app.set_status_text(msg.into());
            app.set_overlay_creature_state("alert".into());
            app.set_overlay_delta_level("warning".into());
            app.set_overlay_delta_text("нет live-данных".into());
            android_apply_live_token_state(app, &AndroidLiveTokenUiState::unknown());
        }
    }
}

#[cfg(all(target_os = "android", feature = "android-gui"))]
fn android_apply_live_token_state(app: &AppWindow, state: &AndroidLiveTokenUiState) {
    app.set_android_agent_pulse_label(state.pulse_label.into());
    app.set_android_agent_pulse_level(state.pulse_level.into());
    app.set_android_agent_rate_text(state.rate_text.clone().into());
    app.set_android_agent_detail_text(state.detail_text.clone().into());
    app.set_android_agent_history_path(state.history_path.clone().into());
    app.set_android_agent_timeout_visible(state.timed_out);
    app.set_agent_text(state.detail_text.clone().into());
    app.set_token_rate_text(state.rate_text.clone().into());
    let raw_rate = state
        .rate_text
        .split_whitespace()
        .next()
        .and_then(|value| value.replace(',', ".").parse::<f32>().ok())
        .unwrap_or(0.0);
    app.set_token_rate_raw(raw_rate);
    app.set_android_agent_history(ModelRc::new(std::rc::Rc::new(VecModel::from(
        state.history.clone(),
    ))));
}

#[cfg(all(target_os = "android", feature = "android-gui"))]
fn android_apply_vimichi_dashboard_state(app: &AppWindow, windows: &[WindowState]) {
    let state = android_vimichi_dashboard_state(windows);
    app.set_overlay_creature_state(state.creature_state.into());
    app.set_overlay_delta_level(state.delta_level.into());
    app.set_overlay_delta_text(state.delta_text.into());
    app.set_overlay_creature_percent(state.percent as f32);
    app.set_overlay_creature_points(state.points);
}

#[derive(Debug, Clone, PartialEq)]
pub struct AndroidVimichiDashboardState {
    pub creature_state: &'static str,
    pub delta_level: &'static str,
    pub delta_text: &'static str,
    pub percent: f64,
    pub points: i32,
}

pub fn android_vimichi_dashboard_state(windows: &[WindowState]) -> AndroidVimichiDashboardState {
    let percent = windows
        .iter()
        .map(|window| window.percent)
        .fold(0.0, f64::max)
        .clamp(0.0, 100.0);
    let has_usage = windows.iter().any(|window| {
        window.percent > 0.0
            || window
                .credits
                .as_ref()
                .map(|metric| metric.used > 0.0)
                .unwrap_or(false)
            || window
                .requests
                .as_ref()
                .map(|metric| metric.used > 0.0)
                .unwrap_or(false)
    });

    let (creature_state, delta_level, delta_text) =
        if windows.iter().any(|window| window.level == "danger") {
            ("critical", "danger", "лимиты критично")
        } else if windows.iter().any(|window| window.level == "warning") {
            ("alert", "warning", "лимиты на грани")
        } else if has_usage {
            ("awake", "ok", "расход активен")
        } else {
            ("sleeping", "ok", "расхода нет")
        };

    let points = if creature_state == "sleeping" {
        6
    } else {
        ((percent / 12.5).ceil() as i32).clamp(6, 12)
    };

    AndroidVimichiDashboardState {
        creature_state,
        delta_level,
        delta_text,
        percent,
        points,
    }
}

#[cfg(all(target_os = "android", feature = "android-gui"))]
fn android_apply_window(app: &AppWindow, key: &str, window: Option<&WindowState>) {
    let Some(window) = window else { return };
    let level: SharedString = window.level.clone().into();
    let reset: SharedString = window.reset.clone().into();
    let credits: SharedString = metric_text("кредиты", window.credits.as_ref()).into();
    let requests: SharedString = metric_text("запросы", window.requests.as_ref()).into();
    let percent_text: SharedString = format_percent(window.percent).into();
    let percent = window.percent as f32;
    let peak =
        peak_percent(window.credits.as_ref(), window.requests.as_ref()).unwrap_or(0.0) as f32;

    match key {
        "5h" => {
            app.set_five_level(level);
            app.set_five_reset(reset);
            app.set_five_credits(credits);
            app.set_five_requests(requests);
            app.set_five_percent_text(percent_text);
            app.set_five_percent(percent);
            app.set_five_credit_percent(peak);
            app.set_five_request_percent(peak);
        }
        "24h" => {
            app.set_day_level(level);
            app.set_day_reset(reset);
            app.set_day_credits(credits);
            app.set_day_requests(requests);
            app.set_day_percent_text(percent_text);
            app.set_day_percent(percent);
            app.set_day_credit_percent(peak);
            app.set_day_request_percent(peak);
        }
        "7d" => {
            app.set_week_level(level);
            app.set_week_reset(reset.clone());
            app.set_week_credits(credits);
            app.set_week_requests(requests);
            app.set_week_percent_text(percent_text);
            app.set_week_percent(percent);
            app.set_week_credit_percent(peak);
            app.set_week_request_percent(peak);
            if let Some(metric) = window.credits.as_ref() {
                app.set_donut_remaining(short_number(metric.remaining).into());
                app.set_donut_limit(short_number(metric.limit).into());
            }
        }
        "30d" => {
            app.set_month_level(level);
            app.set_month_reset(reset);
            app.set_month_credits(credits);
            app.set_month_requests(requests);
            app.set_month_percent_text(percent_text);
            app.set_month_percent(percent);
            app.set_month_credit_percent(peak);
            app.set_month_request_percent(peak);
        }
        _ => {}
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AndroidAlertText {
    pub title: &'static str,
    pub body: &'static str,
}

pub fn android_runaway_alert_text() -> AndroidAlertText {
    AndroidAlertText {
        title: "Vimichi: агент горит токенами",
        body: "Расход токенов продолжается, но задача не завершается. Проверьте агента.",
    }
}

pub fn android_needs_notification_runtime_permission(sdk: i32) -> bool {
    sdk >= ANDROID_NOTIFICATION_RUNTIME_PERMISSION_SDK
}

pub fn android_should_request_notification_permission(sdk: i32, granted: bool) -> bool {
    android_needs_notification_runtime_permission(sdk) && !granted
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AndroidBurnUiState {
    pub visible: bool,
    pub level: &'static str,
    pub text: &'static str,
    pub creature_state: &'static str,
    pub delta_level: &'static str,
    pub delta_text: &'static str,
}

pub fn android_burn_ui_state(event: &AgentBurnEvent) -> AndroidBurnUiState {
    match event.state {
        AgentBurnState::Runaway => AndroidBurnUiState {
            visible: true,
            level: "danger",
            text: "Vimichi: тревога, токены горят",
            creature_state: "critical",
            delta_level: "danger",
            delta_text: "токены горят",
        },
        AgentBurnState::Active => AndroidBurnUiState {
            visible: false,
            level: "ok",
            text: "",
            creature_state: "awake",
            delta_level: "ok",
            delta_text: "агент работает",
        },
        AgentBurnState::Recovery => AndroidBurnUiState {
            visible: false,
            level: "ok",
            text: "",
            creature_state: "recovery",
            delta_level: "ok",
            delta_text: "расход остановлен",
        },
        AgentBurnState::Idle => AndroidBurnUiState {
            visible: false,
            level: "ok",
            text: "",
            creature_state: "sleeping",
            delta_level: "ok",
            delta_text: "агент простаивает",
        },
        AgentBurnState::Unknown => AndroidBurnUiState {
            visible: false,
            level: "unknown",
            text: "",
            creature_state: "awake",
            delta_level: "ok",
            delta_text: "нет данных",
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AndroidAlertGate {
    cooldown_secs: u64,
    last_alert_secs: Option<u64>,
}

impl AndroidAlertGate {
    pub fn new(cooldown_secs: u64) -> Self {
        Self {
            cooldown_secs,
            last_alert_secs: None,
        }
    }

    pub fn should_alert(&mut self, event: &AgentBurnEvent, now_secs: u64) -> bool {
        if event.state != AgentBurnState::Runaway {
            self.last_alert_secs = None;
            return false;
        }

        let should_alert = self
            .last_alert_secs
            .map(|last| now_secs.saturating_sub(last) >= self.cooldown_secs)
            .unwrap_or(true);

        if should_alert {
            self.last_alert_secs = Some(now_secs);
        }
        should_alert
    }
}

impl Default for AndroidAlertGate {
    fn default() -> Self {
        Self::new(DEFAULT_ANDROID_ALERT_COOLDOWN_SECS)
    }
}

#[cfg(all(target_os = "android", feature = "android-gui"))]
pub struct AndroidAlertBridge {
    app: slint::android::AndroidApp,
    gate: Mutex<AndroidAlertGate>,
}

#[cfg(all(target_os = "android", feature = "android-gui"))]
impl AndroidAlertBridge {
    pub fn new(app: slint::android::AndroidApp) -> Self {
        Self {
            app,
            gate: Mutex::new(AndroidAlertGate::default()),
        }
    }

    pub fn alert_runaway(&self, event: &AgentBurnEvent, now_secs: u64) -> Result<bool, String> {
        if !self.gate.lock().unwrap().should_alert(event, now_secs) {
            return Ok(false);
        }

        let text = android_runaway_alert_text();
        android_vibrate(&self.app, 450)?;
        android_show_notification(&self.app, &text)?;
        Ok(true)
    }
}

#[cfg(all(target_os = "android", feature = "android-gui"))]
pub fn android_handle_burn_event(
    app: &AppWindow,
    bridge: &AndroidAlertBridge,
    event: &AgentBurnEvent,
    now_secs: u64,
) -> Result<bool, String> {
    android_apply_burn_ui_state(app, event);
    bridge.alert_runaway(event, now_secs)
}

#[cfg(all(target_os = "android", feature = "android-gui"))]
fn android_apply_burn_ui_state(app: &AppWindow, event: &AgentBurnEvent) {
    let state = android_burn_ui_state(event);
    app.set_agent_alert_visible(state.visible);
    app.set_agent_alert_level(state.level.into());
    app.set_agent_alert_text(state.text.into());
    app.set_overlay_creature_state(state.creature_state.into());
    app.set_overlay_delta_level(state.delta_level.into());
    app.set_overlay_delta_text(state.delta_text.into());
}

#[cfg(all(target_os = "android", feature = "android-gui"))]
fn android_vibrate(app: &slint::android::AndroidApp, duration_ms: i64) -> Result<(), String> {
    let app = app.clone();
    android_run_on_java_main_thread(&app, move |env, activity| {
        let service_name = env.new_string("vibrator")?;
        let service_name = jni::objects::JObject::from(service_name);
        let vibrator = env
            .call_method(
                activity,
                jni::jni_str!("getSystemService"),
                jni::jni_sig!("(Ljava/lang/String;)Ljava/lang/Object;"),
                &[jni::objects::JValue::Object(&service_name)],
            )?
            .l()?;

        if vibrator.as_raw().is_null() {
            return Ok(());
        }

        env.call_method(
            &vibrator,
            jni::jni_str!("vibrate"),
            jni::jni_sig!("(J)V"),
            &[jni::objects::JValue::Long(duration_ms)],
        )?;
        Ok(())
    })
}

#[cfg(all(target_os = "android", feature = "android-gui"))]
fn android_show_notification(
    app: &slint::android::AndroidApp,
    text: &AndroidAlertText,
) -> Result<(), String> {
    let app = app.clone();
    let title = text.title.to_string();
    let body = text.body.to_string();
    android_run_on_java_main_thread(&app, move |env, activity| {
        let channel_id = "vimit-agent-alerts";
        let service_name = env.new_string("notification")?;
        let service_name = jni::objects::JObject::from(service_name);
        let manager = env
            .call_method(
                activity,
                jni::jni_str!("getSystemService"),
                jni::jni_sig!("(Ljava/lang/String;)Ljava/lang/Object;"),
                &[jni::objects::JValue::Object(&service_name)],
            )?
            .l()?;

        if manager.as_raw().is_null() {
            return Ok(());
        }

        let sdk = android_sdk_int(env)?;
        if android_needs_notification_runtime_permission(sdk)
            && !android_has_notification_permission(env, activity)?
        {
            return Ok(());
        }

        if sdk >= 26 {
            android_create_notification_channel(env, &manager, channel_id)?;
        }

        let channel = env.new_string(channel_id)?;
        let builder = if sdk >= 26 {
            let channel = jni::objects::JObject::from(channel);
            env.new_object(
                jni::jni_str!("android/app/Notification$Builder"),
                jni::jni_sig!("(Landroid/content/Context;Ljava/lang/String;)V"),
                &[
                    jni::objects::JValue::Object(activity),
                    jni::objects::JValue::Object(&channel),
                ],
            )?
        } else {
            env.new_object(
                jni::jni_str!("android/app/Notification$Builder"),
                jni::jni_sig!("(Landroid/content/Context;)V"),
                &[jni::objects::JValue::Object(activity)],
            )?
        };

        let icon_class = env.find_class(jni::jni_str!("android/R$drawable"))?;
        let icon = env
            .get_static_field(
                icon_class,
                jni::jni_str!("stat_sys_warning"),
                jni::jni_sig!("I"),
            )?
            .i()?;
        env.call_method(
            &builder,
            jni::jni_str!("setSmallIcon"),
            jni::jni_sig!("(I)Landroid/app/Notification$Builder;"),
            &[jni::objects::JValue::Int(icon)],
        )?;

        let title = env.new_string(title)?;
        let title = jni::objects::JObject::from(title);
        env.call_method(
            &builder,
            jni::jni_str!("setContentTitle"),
            jni::jni_sig!("(Ljava/lang/CharSequence;)Landroid/app/Notification$Builder;"),
            &[jni::objects::JValue::Object(&title)],
        )?;

        let body = env.new_string(body)?;
        let body = jni::objects::JObject::from(body);
        env.call_method(
            &builder,
            jni::jni_str!("setContentText"),
            jni::jni_sig!("(Ljava/lang/CharSequence;)Landroid/app/Notification$Builder;"),
            &[jni::objects::JValue::Object(&body)],
        )?;
        env.call_method(
            &builder,
            jni::jni_str!("setAutoCancel"),
            jni::jni_sig!("(Z)Landroid/app/Notification$Builder;"),
            &[jni::objects::JValue::Bool(true)],
        )?;

        let notification = env
            .call_method(
                &builder,
                jni::jni_str!("build"),
                jni::jni_sig!("()Landroid/app/Notification;"),
                &[],
            )?
            .l()?;
        env.call_method(
            &manager,
            jni::jni_str!("notify"),
            jni::jni_sig!("(ILandroid/app/Notification;)V"),
            &[
                jni::objects::JValue::Int(111),
                jni::objects::JValue::Object(&notification),
            ],
        )?;
        Ok(())
    })
}

#[cfg(all(target_os = "android", feature = "android-gui"))]
fn android_request_notification_permission_if_needed(
    app: &slint::android::AndroidApp,
) -> Result<(), String> {
    let app = app.clone();
    android_run_on_java_main_thread(&app, move |env, activity| {
        let sdk = android_sdk_int(env)?;
        if !android_should_request_notification_permission(
            sdk,
            android_has_notification_permission(env, activity)?,
        ) {
            return Ok(());
        }

        let permission = env.new_string(ANDROID_POST_NOTIFICATIONS_PERMISSION)?;
        let permissions =
            jni::objects::JObjectArray::<jni::objects::JString>::new(env, 1, &permission)?;
        env.call_method(
            activity,
            jni::jni_str!("requestPermissions"),
            jni::jni_sig!("([Ljava/lang/String;I)V"),
            &[
                jni::objects::JValue::Object(&permissions),
                jni::objects::JValue::Int(ANDROID_NOTIFICATION_PERMISSION_REQUEST_CODE),
            ],
        )?;
        Ok(())
    })
}

#[cfg(all(target_os = "android", feature = "android-gui"))]
fn android_has_notification_permission(
    env: &mut jni::Env,
    activity: &jni::objects::JObject,
) -> jni::errors::Result<bool> {
    let permission = env.new_string(ANDROID_POST_NOTIFICATIONS_PERMISSION)?;
    let permission = jni::objects::JObject::from(permission);
    let grant = env
        .call_method(
            activity,
            jni::jni_str!("checkSelfPermission"),
            jni::jni_sig!("(Ljava/lang/String;)I"),
            &[jni::objects::JValue::Object(&permission)],
        )?
        .i()?;
    Ok(grant == ANDROID_NOTIFICATION_PERMISSION_GRANTED)
}

#[cfg(all(target_os = "android", feature = "android-gui"))]
fn android_create_notification_channel(
    env: &mut jni::Env,
    manager: &jni::objects::JObject,
    channel_id: &str,
) -> jni::errors::Result<()> {
    let id = env.new_string(channel_id)?;
    let id = jni::objects::JObject::from(id);
    let name = env.new_string("Vimichi agent alerts")?;
    let name = jni::objects::JObject::from(name);
    let channel = env.new_object(
        jni::jni_str!("android/app/NotificationChannel"),
        jni::jni_sig!("(Ljava/lang/String;Ljava/lang/CharSequence;I)V"),
        &[
            jni::objects::JValue::Object(&id),
            jni::objects::JValue::Object(&name),
            jni::objects::JValue::Int(4),
        ],
    )?;
    env.call_method(
        manager,
        jni::jni_str!("createNotificationChannel"),
        jni::jni_sig!("(Landroid/app/NotificationChannel;)V"),
        &[jni::objects::JValue::Object(&channel)],
    )?;
    Ok(())
}

#[cfg(all(target_os = "android", feature = "android-gui"))]
fn android_sdk_int(env: &mut jni::Env) -> jni::errors::Result<i32> {
    let version = env.find_class(jni::jni_str!("android/os/Build$VERSION"))?;
    env.get_static_field(version, jni::jni_str!("SDK_INT"), jni::jni_sig!("I"))?
        .i()
}

#[cfg(all(target_os = "android", feature = "android-gui"))]
fn android_run_on_java_main_thread<F>(
    app: &slint::android::AndroidApp,
    callback: F,
) -> Result<(), String>
where
    F: FnOnce(&mut jni::Env, &jni::objects::JObject) -> jni::errors::Result<()> + Send + 'static,
{
    if app.vm_as_ptr().is_null() || app.activity_as_ptr().is_null() {
        return Err("Android activity is not available".to_string());
    }

    let app = app.clone();
    app.clone().run_on_java_main_thread(Box::new(move || {
        let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr() as _) };
        let _ = vm.attach_current_thread(|env| {
            let activity = unsafe {
                jni::objects::JObject::from_raw(env, app.activity_as_ptr() as jni::sys::jobject)
            };
            callback(env, &activity)
        });
    }));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_window(level: &str, percent: f64, used: f64) -> WindowState {
        WindowState {
            key: "24h",
            level: level.to_string(),
            reset: "сброс скоро".to_string(),
            reset_in_seconds: Some(60),
            credits: Some(crate::Metric {
                used,
                limit: 100.0,
                remaining: 100.0 - used,
                percent,
            }),
            requests: None,
            percent,
        }
    }

    fn test_pulse(token_rate: f64, sessions_active: u64) -> AgentPulse {
        AgentPulse::from_abtop_status(&serde_json::json!({
            "interval_ms": 1000,
            "token_rate": token_rate,
            "sessions_total": 1,
            "sessions_active": sessions_active
        }))
    }

    #[test]
    fn android_live_token_tracker_marks_active_with_rate() {
        let mut tracker = AndroidLiveTokenTracker::new(4, 45);
        let pulse = test_pulse(12.5, 1);

        let state = tracker.observe(Some(&pulse), 100);

        assert_eq!(state.pulse_label, "Active");
        assert_eq!(state.pulse_level, "active");
        assert_eq!(state.rate_text, "12,5 ток/сек");
        assert!(!state.timed_out);
        assert_eq!(state.history, vec![100.0]);
        assert!(state.history_path.starts_with("M "));
    }

    #[test]
    fn android_live_token_tracker_marks_finished_after_zero_timeout() {
        let mut tracker = AndroidLiveTokenTracker::new(4, 45);
        let active = test_pulse(8.0, 1);
        let idle = test_pulse(0.0, 0);

        tracker.observe(Some(&active), 100);
        let before_timeout = tracker.observe(Some(&idle), 130);
        let after_timeout = tracker.observe(Some(&idle), 145);

        assert_eq!(before_timeout.pulse_label, "Idle");
        assert_eq!(before_timeout.pulse_level, "idle");
        assert!(!before_timeout.timed_out);
        assert_eq!(after_timeout.pulse_label, "Idle");
        assert_eq!(after_timeout.pulse_level, "finished");
        assert!(after_timeout.timed_out);
        assert!(after_timeout.detail_text.contains("пул заверш"));
    }

    #[test]
    fn android_live_token_tracker_keeps_unknown_privacy_safe() {
        let mut tracker = AndroidLiveTokenTracker::new(4, 45);

        let state = tracker.observe(None, 100);

        assert_eq!(state.pulse_label, "Unknown");
        assert_eq!(state.pulse_level, "unknown");
        assert!(!state.timed_out);
        assert!(state.history.is_empty());
        assert!(!state.detail_text.to_lowercase().contains("api"));
        assert!(!state.detail_text.to_lowercase().contains("key"));
    }

    #[test]
    fn android_history_path_builds_line_commands() {
        let path = android_history_path(&[0.0, 50.0, 100.0]);

        assert!(path.starts_with("M "));
        assert!(path.contains(" L "));
        assert!(path.contains("100.00"));
    }

    #[test]
    fn android_alert_gate_triggers_once_for_runaway() {
        let mut gate = AndroidAlertGate::new(300);
        let event = AgentBurnEvent {
            state: AgentBurnState::Runaway,
            burning_for_secs: Some(90),
            token_rate_per_sec: 7.0,
        };

        assert!(gate.should_alert(&event, 1000));
        assert!(!gate.should_alert(&event, 1100));
    }

    #[test]
    fn android_alert_gate_repeats_after_cooldown() {
        let mut gate = AndroidAlertGate::new(300);
        let event = AgentBurnEvent {
            state: AgentBurnState::Runaway,
            burning_for_secs: Some(90),
            token_rate_per_sec: 7.0,
        };

        assert!(gate.should_alert(&event, 1000));
        assert!(gate.should_alert(&event, 1300));
    }

    #[test]
    fn android_alert_gate_resets_after_recovery() {
        let mut gate = AndroidAlertGate::new(300);
        let runaway = AgentBurnEvent {
            state: AgentBurnState::Runaway,
            burning_for_secs: Some(90),
            token_rate_per_sec: 7.0,
        };
        let recovery = AgentBurnEvent {
            state: AgentBurnState::Recovery,
            burning_for_secs: None,
            token_rate_per_sec: 0.0,
        };

        assert!(gate.should_alert(&runaway, 1000));
        assert!(!gate.should_alert(&recovery, 1010));
        assert!(gate.should_alert(&runaway, 1020));
    }

    #[test]
    fn android_runaway_alert_text_is_privacy_safe() {
        let text = android_runaway_alert_text();
        let combined = format!("{} {}", text.title, text.body).to_lowercase();

        assert!(combined.contains("vimichi"));
        assert!(combined.contains("токен"));
        assert!(!combined.contains("api"));
        assert!(!combined.contains("key"));
        assert!(!combined.contains("prompt"));
        assert!(!combined.contains("session"));
        assert!(!combined.contains("c:\\"));
    }

    #[test]
    fn android_notification_permission_is_runtime_only_on_android_13_plus() {
        assert!(!android_needs_notification_runtime_permission(32));
        assert!(android_needs_notification_runtime_permission(33));
        assert!(android_needs_notification_runtime_permission(34));
    }

    #[test]
    fn android_notification_permission_request_skips_when_already_granted() {
        assert!(!android_should_request_notification_permission(32, false));
        assert!(!android_should_request_notification_permission(33, true));
        assert!(android_should_request_notification_permission(33, false));
    }

    #[test]
    fn android_burn_ui_state_marks_runaway_as_vimichi_alarm() {
        let event = AgentBurnEvent {
            state: AgentBurnState::Runaway,
            burning_for_secs: Some(90),
            token_rate_per_sec: 12.0,
        };

        let state = android_burn_ui_state(&event);

        assert!(state.visible);
        assert_eq!(state.level, "danger");
        assert_eq!(state.text, "Vimichi: тревога, токены горят");
        assert_eq!(state.creature_state, "critical");
        assert_eq!(state.delta_level, "danger");
    }

    #[test]
    fn android_burn_ui_state_resets_after_recovery() {
        let event = AgentBurnEvent {
            state: AgentBurnState::Recovery,
            burning_for_secs: None,
            token_rate_per_sec: 0.0,
        };

        let state = android_burn_ui_state(&event);

        assert!(!state.visible);
        assert_eq!(state.creature_state, "recovery");
        assert_eq!(state.delta_text, "расход остановлен");
    }

    #[test]
    fn android_alert_gate_default_uses_one_minute_cooldown() {
        let mut gate = AndroidAlertGate::default();
        let event = AgentBurnEvent {
            state: AgentBurnState::Runaway,
            burning_for_secs: Some(90),
            token_rate_per_sec: 7.0,
        };

        assert!(gate.should_alert(&event, 1000));
        assert!(!gate.should_alert(&event, 1059));
        assert!(gate.should_alert(&event, 1060));
    }

    #[test]
    fn android_vimichi_dashboard_state_marks_danger_as_critical() {
        let state = android_vimichi_dashboard_state(&[test_window("danger", 94.0, 94.0)]);

        assert_eq!(state.creature_state, "critical");
        assert_eq!(state.delta_level, "danger");
        assert_eq!(state.delta_text, "лимиты критично");
        assert_eq!(state.percent, 94.0);
        assert_eq!(state.points, 8);
    }

    #[test]
    fn android_vimichi_dashboard_state_marks_warning_as_alert() {
        let state = android_vimichi_dashboard_state(&[test_window("warning", 78.0, 78.0)]);

        assert_eq!(state.creature_state, "alert");
        assert_eq!(state.delta_level, "warning");
        assert_eq!(state.delta_text, "лимиты на грани");
    }

    #[test]
    fn android_vimichi_dashboard_state_marks_usage_as_awake() {
        let state = android_vimichi_dashboard_state(&[test_window("ok", 12.0, 12.0)]);

        assert_eq!(state.creature_state, "awake");
        assert_eq!(state.delta_level, "ok");
        assert_eq!(state.delta_text, "расход активен");
        assert_eq!(state.points, 6);
    }

    #[test]
    fn android_vimichi_dashboard_state_sleeps_without_usage() {
        let state = android_vimichi_dashboard_state(&[test_window("ok", 0.0, 0.0)]);

        assert_eq!(state.creature_state, "sleeping");
        assert_eq!(state.delta_level, "ok");
        assert_eq!(state.delta_text, "расхода нет");
        assert_eq!(state.points, 6);
    }
}
