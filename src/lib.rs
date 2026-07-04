#![allow(clippy::collapsible_if)]

pub mod api;
pub mod cli;
pub mod parse;

pub use api::*;
pub use parse::*;

#[cfg(all(target_os = "android", feature = "android-gui"))]
slint::include_modules!();

#[cfg(all(target_os = "android", feature = "android-gui"))]
use slint::{ComponentHandle, SharedString, Weak};

use chrono::{SecondsFormat, Utc};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

pub static OFFLINE_SINCE: Mutex<Option<Instant>> = Mutex::new(None);
static DEPRECATED_ENV_WARNINGS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

pub fn update_offline_state(is_error: bool) -> Option<u64> {
    let mut state = OFFLINE_SINCE.lock().unwrap();
    if is_error {
        if state.is_none() {
            *state = Some(Instant::now());
        }
        state.map(|since| since.elapsed().as_secs() / 60)
    } else {
        *state = None;
        None
    }
}

pub fn get_offline_duration_min() -> Option<u64> {
    OFFLINE_SINCE
        .lock()
        .unwrap()
        .map(|since| since.elapsed().as_secs() / 60)
}

pub const DEFAULT_API_BASE: &str = "https://api.vibemod.pro";
pub const FALLBACK_API_BASE: &str = "https://r-api.vibemod.pro";
pub const VPN_API_BASE: &str = FALLBACK_API_BASE;
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const USER_AGENT: &str = concat!("vimit/", env!("CARGO_PKG_VERSION"));
pub const USER_AGENT_GUI: &str = concat!("vimit-gui/", env!("CARGO_PKG_VERSION"));
pub const DEFAULT_WARNING_THRESHOLD: f64 = 75.0;
pub const DEFAULT_DANGER_THRESHOLD: f64 = 90.0;
pub const DEFAULT_ABTOP_BIN: &str = "abtop";

#[cfg(all(target_os = "android", feature = "android-gui"))]
#[unsafe(no_mangle)]
pub fn android_main(app: slint::android::AndroidApp) {
    let data_dir = app.internal_data_path();
    slint::android::init(app).expect("cannot initialize Android backend");
    let window = AppWindow::new().expect("cannot initialize Slint window");
    window.set_is_android(true);
    window.set_needs_setup(false);

    let key_path = android_key_path(data_dir);
    let saved_key = android_load_api_key(&key_path).unwrap_or_default();
    window.set_api_key_configured(!saved_key.is_empty());
    window.set_setup_status_text("Вставьте VIBEMODE_API_KEY и нажмите Сохранить ключ.".into());

    let key_state = std::sync::Arc::new(std::sync::Mutex::new(saved_key));
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
    let weak = window.as_weak();
    window.on_refresh_requested(move || {
        android_start_refresh(weak.clone(), key_for_refresh.clone(), false);
    });

    let key_for_demo = key_state.clone();
    let weak = window.as_weak();
    window.on_demo_requested(move || {
        android_start_refresh(weak.clone(), key_for_demo.clone(), true);
    });

    android_start_refresh(window.as_weak(), key_state, false);
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
    key_state: std::sync::Arc<std::sync::Mutex<String>>,
    demo: bool,
) {
    std::thread::spawn(move || {
        let result = android_load_dashboard(&key_state, demo);
        let _ = app.upgrade_in_event_loop(move |app| android_apply_dashboard(&app, result));
    });
}

#[cfg(all(target_os = "android", feature = "android-gui"))]
fn android_load_dashboard(
    key_state: &std::sync::Arc<std::sync::Mutex<String>>,
    demo: bool,
) -> Result<(Vec<WindowState>, String, String), String> {
    if demo {
        let windows = summarize_me(
            &demo_payload(),
            DEFAULT_WARNING_THRESHOLD,
            DEFAULT_DANGER_THRESHOLD,
        );
        return Ok((
            windows,
            "источник: встроенные демо-данные".to_string(),
            "demo".to_string(),
        ));
    }

    let key = key_state.lock().unwrap().clone();
    if key.is_empty() {
        let windows = summarize_me(
            &demo_payload(),
            DEFAULT_WARNING_THRESHOLD,
            DEFAULT_DANGER_THRESHOLD,
        );
        return Ok((
            windows,
            "источник: демо; сохраните VIBEMODE_API_KEY для live-лимитов".to_string(),
            "demo".to_string(),
        ));
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
    Ok((
        windows,
        format!("источник: live VibeMode /v1/me ({label})"),
        label,
    ))
}

#[cfg(all(target_os = "android", feature = "android-gui"))]
fn android_apply_dashboard(
    app: &AppWindow,
    result: Result<(Vec<WindowState>, String, String), String>,
) {
    match result {
        Ok((windows, source, endpoint)) => {
            app.set_error_text("".into());
            app.set_status_text(dashboard_status(&windows).into());
            app.set_source_text(source.into());
            app.set_active_endpoint_label(endpoint.into());
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
        }
        Err(error) => {
            let msg = if error.contains("HTTP 401") {
                "Проверьте VIBEMODE_API_KEY"
            } else {
                "Не удалось загрузить VibeMode"
            };
            app.set_error_text(msg.into());
            app.set_status_text(msg.into());
        }
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

pub const WINDOWS: [(&str, &str, &str, &str); 4] = [
    (
        "5h",
        "5Hours",
        "window5HoursStartedAt",
        "window5HoursEndsAt",
    ),
    (
        "24h",
        "24Hours",
        "window24HoursStartedAt",
        "window24HoursEndsAt",
    ),
    ("7d", "7Days", "window7DaysStartedAt", "window7DaysEndsAt"),
    (
        "30d",
        "30Days",
        "window30DaysStartedAt",
        "window30DaysEndsAt",
    ),
];

#[derive(Debug, Clone)]
pub struct Metric {
    pub used: f64,
    pub limit: f64,
    pub remaining: f64,
    pub percent: f64,
}

#[derive(Debug, Clone)]
pub struct WindowState {
    pub key: &'static str,
    pub level: String,
    pub reset: String,
    pub reset_in_seconds: Option<i64>,
    pub credits: Option<Metric>,
    pub requests: Option<Metric>,
    pub percent: f64,
}

#[derive(Debug)]
pub struct RuntimeConfig {
    pub api_base: String,
    pub api_key: String,
    pub abtop_bin: String,
    pub auto_failover: bool,
}

impl RuntimeConfig {
    pub fn from_dotenv(
        api_base_override: Option<String>,
        api_key_env: &str,
        env_file: Option<&PathBuf>,
    ) -> Result<Self, String> {
        if let Some(path) = env_file.filter(|p| !p.is_file()) {
            return Err(format!("env file not found: {}", path.display()));
        }
        let dotenv = load_dotenv_custom(env_file)?;
        Ok(Self {
            api_base: api_base_override
                .or_else(|| config_value("VIBEMODE_API_BASE", &dotenv))
                .unwrap_or_else(|| DEFAULT_API_BASE.to_string()),
            api_key: config_value(api_key_env, &dotenv).unwrap_or_default(),
            abtop_bin: config_value("ABTOP_BIN", &dotenv)
                .unwrap_or_else(|| DEFAULT_ABTOP_BIN.to_string()),
            auto_failover: true,
        })
    }
}

#[derive(Debug, Clone)]
pub struct Dashboard {
    pub source: String,
    pub status: String,
    pub agent: String,
    pub token_rate: String,
    pub windows: Vec<WindowState>,
    pub daily: Option<DailyState>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DailyState {
    pub spent_today: f64,
    pub daily_limit: f64,
    pub percent: f64,
    pub level: String,
}

#[derive(Debug, Clone)]
pub struct AgentStatus {
    pub summary: String,
    pub token_rate: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentActivity {
    Active,
    Idle,
    Unknown,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentPulse {
    pub activity: AgentActivity,
    pub token_rate_known: bool,
    pub token_rate_per_tick: f64,
    pub token_rate_per_sec: f64,
    pub interval_ms: u64,
    pub sessions_total: u64,
    pub sessions_active: u64,
    pub max_context_pct: Option<f64>,
}

impl AgentPulse {
    pub fn from_abtop_status(status: &Value) -> Self {
        let interval_ms = status
            .get("interval_ms")
            .and_then(Value::as_u64)
            .filter(|value| *value > 0)
            .unwrap_or(1000);
        let token_rate = status
            .get("token_rate")
            .and_then(to_number)
            .or_else(|| summed_agent_token_rate(status));
        let token_rate_known = token_rate.is_some();
        let token_rate_per_tick = token_rate.unwrap_or(0.0);
        let token_rate_per_sec = token_rate_per_tick / (interval_ms as f64 / 1000.0);
        let sessions_total = status
            .get("sessions_total")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let sessions_active = status
            .get("sessions_active")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let max_context_pct = max_agent_context_pct(status);
        let activity = if sessions_active > 0 || token_rate_per_tick > 0.0 {
            AgentActivity::Active
        } else if sessions_total > 0 {
            AgentActivity::Idle
        } else {
            AgentActivity::Unknown
        };

        Self {
            activity,
            token_rate_known,
            token_rate_per_tick,
            token_rate_per_sec,
            interval_ms,
            sessions_total,
            sessions_active,
            max_context_pct,
        }
    }
}

pub const DEFAULT_STUCK_BURN_MIN_RATE_PER_SEC: f64 = 0.1;
pub const DEFAULT_STUCK_BURN_TIMEOUT_SECS: u64 = 90;
pub const DEFAULT_ANDROID_ALERT_COOLDOWN_SECS: u64 = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentBurnState {
    Unknown,
    Idle,
    Active,
    Runaway,
    Recovery,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AgentBurnConfig {
    pub min_rate_per_sec: f64,
    pub timeout_secs: u64,
}

impl Default for AgentBurnConfig {
    fn default() -> Self {
        Self {
            min_rate_per_sec: DEFAULT_STUCK_BURN_MIN_RATE_PER_SEC,
            timeout_secs: DEFAULT_STUCK_BURN_TIMEOUT_SECS,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentBurnEvent {
    pub state: AgentBurnState,
    pub burning_for_secs: Option<u64>,
    pub token_rate_per_sec: f64,
}

#[derive(Debug, Clone)]
pub struct AgentBurnDetector {
    config: AgentBurnConfig,
    burning_since_secs: Option<u64>,
    last_state: AgentBurnState,
}

impl AgentBurnDetector {
    pub fn new(config: AgentBurnConfig) -> Self {
        Self {
            config,
            burning_since_secs: None,
            last_state: AgentBurnState::Unknown,
        }
    }

    pub fn observe(&mut self, pulse: &AgentPulse, now_secs: u64) -> AgentBurnEvent {
        let burning = pulse.token_rate_known
            && pulse.token_rate_per_sec >= self.config.min_rate_per_sec
            && matches!(pulse.activity, AgentActivity::Active);

        if !burning {
            self.burning_since_secs = None;
            let state = match pulse.activity {
                AgentActivity::Idle => {
                    if matches!(
                        self.last_state,
                        AgentBurnState::Active | AgentBurnState::Runaway
                    ) {
                        AgentBurnState::Recovery
                    } else {
                        AgentBurnState::Idle
                    }
                }
                AgentActivity::Unknown => AgentBurnState::Unknown,
                AgentActivity::Active => AgentBurnState::Active,
            };
            self.last_state = state;
            return AgentBurnEvent {
                state,
                burning_for_secs: None,
                token_rate_per_sec: pulse.token_rate_per_sec,
            };
        }

        let started = *self.burning_since_secs.get_or_insert(now_secs);
        let burning_for_secs = now_secs.saturating_sub(started);
        let state = if burning_for_secs >= self.config.timeout_secs {
            AgentBurnState::Runaway
        } else {
            AgentBurnState::Active
        };
        self.last_state = state;

        AgentBurnEvent {
            state,
            burning_for_secs: Some(burning_for_secs),
            token_rate_per_sec: pulse.token_rate_per_sec,
        }
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

// ── API (now in api.rs) ─────────────────────────────────────────────────────
// HttpClient, Router, fetch_me, load_mock moved to api.rs

// ── Parsing / Summarize (now in parse.rs) ────────────────────────────────────
// demo_payload, summarize_me, etc. moved to parse.rs

// ── Utility ─────────────────────────────────────────────────────────────────

pub fn to_number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.parse::<f64>().ok(),
        _ => None,
    }
}

// ── Formatting ──────────────────────────────────────────────────────────────

pub fn format_duration_secs(seconds: i64) -> String {
    match seconds {
        seconds if seconds < 60 => format!("через {seconds}с"),
        seconds if seconds < 3600 => format!("через {}м", seconds / 60),
        seconds if seconds < 86_400 => {
            format!("через {}ч {}м", seconds / 3600, (seconds % 3600) / 60)
        }
        seconds => format!("через {}д {}ч", seconds / 86_400, (seconds % 86_400) / 3600),
    }
}

pub fn format_duration_opt(seconds: Option<i64>) -> String {
    match seconds {
        None => "unknown".to_string(),
        Some(seconds) => format_duration_secs(seconds),
    }
}

pub fn format_percent(value: f64) -> String {
    format!("{}%", one_decimal(value))
}

pub fn one_decimal(value: f64) -> String {
    format!("{value:.1}").replace('.', ",")
}

pub fn short_number(value: f64) -> String {
    let abs = value.abs();
    if abs >= 1_000_000_000.0 {
        format!("{}B", one_decimal(value / 1_000_000_000.0))
    } else if abs >= 1_000_000.0 {
        format!("{}M", one_decimal(value / 1_000_000.0))
    } else if abs >= 1_000.0 {
        format!("{}K", one_decimal(value / 1_000.0))
    } else if value.fract().abs() < f64::EPSILON {
        format!("{}", value as i64)
    } else {
        one_decimal(value)
    }
}

pub fn short_rate(value: f64) -> String {
    if value >= 1000.0 {
        short_number(value)
    } else {
        one_decimal(value)
    }
}

pub fn compact_number(value: f64) -> String {
    if (value.fract()).abs() < f64::EPSILON {
        format!("{}", value as i64)
    } else {
        format!("{value:.2}")
    }
}

pub fn metric_text(label: &str, metric: Option<&Metric>) -> String {
    match metric {
        Some(metric) => format!(
            "{label}: {}/{} ({}%, осталось {})",
            short_number(metric.used),
            short_number(metric.limit),
            one_decimal(metric.percent),
            short_number(metric.remaining)
        ),
        None => format!("{label}: н/д"),
    }
}

pub fn metric_text_en(label: &str, metric: Option<&Metric>) -> String {
    match metric {
        Some(metric) => format!(
            "{label} {:.1}% left {}",
            metric.percent,
            short_number(metric.remaining)
        ),
        None => format!("{label} n/a"),
    }
}

pub fn format_metric(metric: &Metric) -> String {
    format!(
        "{}/{} ({:.1}%, left {})",
        compact_number(metric.used),
        compact_number(metric.limit),
        metric.percent,
        compact_number(metric.remaining)
    )
}

pub fn value_string(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::Number(number) => Some(number.to_string()),
        Value::String(text) => Some(text.clone()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

// ── Dashboard helpers ───────────────────────────────────────────────────────

pub fn dashboard_status(windows: &[WindowState]) -> String {
    let peak = windows
        .iter()
        .map(|window| window.percent)
        .fold(0.0, f64::max);
    let level = window_level_from_peak(peak);
    format!(
        "квота: {level} | макс {} | обновлено {}",
        format_percent(peak),
        Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
    )
}

fn window_level_from_peak(peak: f64) -> &'static str {
    if peak >= DEFAULT_DANGER_THRESHOLD {
        "лимит"
    } else if peak >= DEFAULT_WARNING_THRESHOLD {
        "внимание"
    } else {
        "норма"
    }
}

pub fn summary_to_json(
    windows: &[WindowState],
    abtop: Option<&Value>,
    daily: Option<&DailyState>,
) -> Value {
    json!({
        "source": "vibemode",
        "windows": windows.iter().map(window_to_json).collect::<Vec<_>>(),
        "abtop": abtop.cloned().unwrap_or(Value::Null),
        "daily": daily.map(|d| json!(d)).unwrap_or_else(|| json!({
            "spent_today": 0.0,
            "daily_limit": 0.0,
            "percent": 0.0,
            "level": "normal"
        })),
    })
}

pub fn summary_to_json_with_stale(
    windows: &[WindowState],
    abtop: Option<&Value>,
    daily: Option<&DailyState>,
    stale: bool,
    latency_ms: u64,
    active_endpoint: &str,
) -> Value {
    let mut obj = json!({
        "source": "vibemode",
        "windows": windows.iter().map(window_to_json).collect::<Vec<_>>(),
        "abtop": abtop.cloned().unwrap_or(Value::Null),
        "daily": daily.map(|d| json!(d)).unwrap_or_else(|| json!({
            "spent_today": 0.0,
            "daily_limit": 0.0,
            "percent": 0.0,
            "level": "normal"
        })),
    });
    if let Some(map) = obj.as_object_mut() {
        if stale {
            map.insert("stale".to_string(), Value::Bool(true));
        }
        map.insert(
            "latency_ms".to_string(),
            Value::Number(serde_json::Number::from(latency_ms)),
        );
        map.insert(
            "active_endpoint".to_string(),
            Value::String(active_endpoint.to_string()),
        );
        if let Some(offline_mins) = get_offline_duration_min() {
            map.insert(
                "api_status".to_string(),
                Value::String("offline".to_string()),
            );
            map.insert(
                "offline_duration_min".to_string(),
                Value::Number(serde_json::Number::from(offline_mins)),
            );
        } else {
            map.insert(
                "api_status".to_string(),
                Value::String("online".to_string()),
            );
        }
    }
    obj
}

fn window_to_json(window: &WindowState) -> Value {
    json!({
        "window": window.key,
        "level": window.level,
        "reset": window.reset,
        "reset_in_seconds": window.reset_in_seconds,
        "credits": metric_to_json(window.credits.as_ref()),
        "requests": metric_to_json(window.requests.as_ref()),
    })
}

fn metric_to_json(metric: Option<&Metric>) -> Value {
    match metric {
        Some(metric) => json!({
            "used": metric.used,
            "limit": metric.limit,
            "remaining": metric.remaining,
            "percent": metric.percent,
        }),
        None => Value::Null,
    }
}

// ── Agent status ────────────────────────────────────────────────────────────

pub fn read_agent_status(binary: &str) -> AgentStatus {
    let Ok(output) = std::process::Command::new(binary)
        .arg("--status-json")
        .output()
    else {
        return AgentStatus {
            summary: "агенты: abtop не найден; задайте ABTOP_BIN".to_string(),
            token_rate: "токены/мин: нет данных abtop".to_string(),
        };
    };
    if !output.status.success() {
        return AgentStatus {
            summary: "агенты: статус abtop недоступен".to_string(),
            token_rate: "токены/мин: нет данных abtop".to_string(),
        };
    }
    let Ok(parsed) = serde_json::from_slice::<Value>(&output.stdout) else {
        return AgentStatus {
            summary: "агенты: abtop вернул невалидный JSON".to_string(),
            token_rate: "токены/мин: нет данных abtop".to_string(),
        };
    };
    let pulse = AgentPulse::from_abtop_status(&parsed);
    let ctx = pulse
        .max_context_pct
        .map(|value| format!("{value:.0}%"))
        .unwrap_or_else(|| "н/д".to_string());

    AgentStatus {
        summary: format!(
            "агенты: сессий {}, активных {}, контекст макс. {ctx}",
            pulse.sessions_total, pulse.sessions_active
        ),
        token_rate: if pulse.token_rate_known {
            format!("токены/мин: {}", short_rate(pulse.token_rate_per_tick))
        } else {
            "токены/мин: нет данных abtop".to_string()
        },
    }
}

pub fn read_abtop_status(binary: &str) -> Option<Value> {
    let output = std::process::Command::new(binary)
        .arg("--status-json")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let parsed: Value = serde_json::from_slice(&output.stdout).ok()?;
    parsed.is_object().then_some(parsed)
}

fn summed_agent_token_rate(parsed: &Value) -> Option<f64> {
    parsed
        .get("agents")
        .and_then(Value::as_array)
        .and_then(|agents| {
            let mut total = 0.0;
            let mut seen = false;
            for agent in agents {
                if let Some(rate) = agent.get("token_rate").and_then(to_number) {
                    total += rate;
                    seen = true;
                }
            }
            seen.then_some(total)
        })
}

fn max_agent_context_pct(parsed: &Value) -> Option<f64> {
    parsed
        .get("agents")
        .and_then(Value::as_array)
        .and_then(|agents| {
            agents
                .iter()
                .filter_map(|agent| agent.get("max_context_pct").and_then(to_number))
                .fold(None, |peak: Option<f64>, value| {
                    Some(peak.map_or(value, |peak| peak.max(value)))
                })
        })
}

// ── .env ────────────────────────────────────────────────────────────────────

pub fn config_value(key: &str, dotenv: &HashMap<String, String>) -> Option<String> {
    let keys = if key == "NEUROGATE_API_KEY"
        || key == "VIBEMODE_API_KEY"
        || key == "VIBEMOD_API_KEY"
    {
        vec!["VIBEMODE_API_KEY", "VIBEMOD_API_KEY", "NEUROGATE_API_KEY"]
    } else if key == "NEUROGATE_API_BASE" || key == "VIBEMODE_API_BASE" || key == "VIBEMOD_API_BASE"
    {
        vec![
            "VIBEMODE_API_BASE",
            "VIBEMOD_API_BASE",
            "NEUROGATE_API_BASE",
        ]
    } else {
        vec![key]
    };

    for k in &keys {
        if let Some(val) = env::var(k).ok().filter(|v| !v.is_empty()) {
            if let Some(suffix) = k.strip_prefix("NEUROGATE_") {
                warn_deprecated_env_once(k, suffix);
            }
            return Some(val);
        }
        if let Some(val) = dotenv.get(*k).cloned().filter(|v| !v.is_empty()) {
            if let Some(suffix) = k.strip_prefix("NEUROGATE_") {
                warn_deprecated_env_once(k, suffix);
            }
            return Some(val);
        }
    }
    None
}

fn warn_deprecated_env_once(key: &str, suffix: &str) {
    if remember_deprecated_env_warning(key) {
        eprintln!("warning: {key} is deprecated, rename to VIBEMODE_{suffix}");
    }
}

fn remember_deprecated_env_warning(key: &str) -> bool {
    let warnings = DEPRECATED_ENV_WARNINGS.get_or_init(|| Mutex::new(HashSet::new()));
    warnings.lock().unwrap().insert(key.to_string())
}

pub fn find_dotenv_custom(explicit: Option<&PathBuf>) -> Option<PathBuf> {
    if let Some(path) = explicit {
        if path.is_file() {
            return Some(path.clone());
        }
        return None;
    }

    let cwd_env = PathBuf::from(".env");
    if cwd_env.is_file() {
        return Some(cwd_env);
    }

    if let Some(exe_env) = env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(".env")))
        .filter(|exe_env| exe_env.is_file())
    {
        return Some(exe_env);
    }

    // Fallback to ~/.vimit/.env
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .ok();
    if let Some(home_path) = home {
        let home_env = PathBuf::from(home_path).join(".vimit").join(".env");
        if home_env.is_file() {
            return Some(home_env);
        }
    }

    None
}

pub fn load_dotenv_custom(explicit: Option<&PathBuf>) -> Result<HashMap<String, String>, String> {
    let Some(path) = find_dotenv_custom(explicit) else {
        return Ok(HashMap::new());
    };
    let raw = fs::read_to_string(&path)
        .map_err(|error| format!("cannot read env file {}: {error}", path.display()))?;
    parse_dotenv(&raw).map_err(|error| format!("{}: {error}", path.display()))
}

pub fn parse_dotenv(raw: &str) -> Result<HashMap<String, String>, String> {
    let mut values = HashMap::new();
    for (index, line) in raw.lines().enumerate() {
        let mut line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("export ") {
            line = rest.trim_start();
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(format!("line {} is not KEY=VALUE", index + 1));
        };
        values.insert(
            key.trim().to_string(),
            unquote_env_value(value.trim()).to_string(),
        );
    }
    Ok(values)
}

pub fn unquote_env_value(value: &str) -> &str {
    if value.len() >= 2 {
        let bytes = value.as_bytes();
        if (bytes[0] == b'"' && bytes[value.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[value.len() - 1] == b'\'')
        {
            return &value[1..value.len() - 1];
        }
    }
    value
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_summary_has_no_account_identity() {
        let payload = json!({
            "id": "usr_demo",
            "usage": {"rows": [{"credits5Hours": 39, "creditLimit5Hours": 50}]}
        });
        let encoded = summary_to_json(&summarize_me(&payload, 75.0, 90.0), None, None).to_string();

        assert!(encoded.contains("\"source\":\"vibemode\""));
        assert!(!encoded.contains("usr_demo"));
    }

    #[test]
    fn parses_dotenv_without_leaking_comments() {
        let parsed = parse_dotenv(
            r#"
            # comment
            export NEUROGATE_API_KEY="demo"
            NEUROGATE_API_BASE=https://r-api.vibemod.pro
            ABTOP_BIN='abtop'
            "#,
        )
        .unwrap();

        assert_eq!(parsed.get("NEUROGATE_API_KEY").unwrap(), "demo");
        assert_eq!(
            parsed.get("NEUROGATE_API_BASE").unwrap(),
            "https://r-api.vibemod.pro"
        );
        assert_eq!(parsed.get("ABTOP_BIN").unwrap(), "abtop");
    }

    #[test]
    fn deprecated_env_warning_is_recorded_once_per_key() {
        let key = format!("NEUROGATE_TEST_{}", std::process::id());

        assert!(remember_deprecated_env_warning(&key));
        assert!(!remember_deprecated_env_warning(&key));
    }

    #[test]
    fn agent_pulse_marks_active_and_converts_tokens_per_second() {
        let pulse = AgentPulse::from_abtop_status(&json!({
            "interval_ms": 2000,
            "token_rate": 42.0,
            "sessions_total": 2,
            "sessions_active": 1,
            "agents": [
                {"agent_cli": "codex", "max_context_pct": 41.0},
                {"agent_cli": "claude", "max_context_pct": 73.0}
            ]
        }));

        assert_eq!(pulse.activity, AgentActivity::Active);
        assert!(pulse.token_rate_known);
        assert_eq!(pulse.token_rate_per_tick, 42.0);
        assert_eq!(pulse.token_rate_per_sec, 21.0);
        assert_eq!(pulse.sessions_total, 2);
        assert_eq!(pulse.sessions_active, 1);
        assert_eq!(pulse.max_context_pct, Some(73.0));
    }

    #[test]
    fn agent_pulse_marks_idle_when_sessions_wait_without_rate() {
        let pulse = AgentPulse::from_abtop_status(&json!({
            "interval_ms": 2000,
            "token_rate": 0.0,
            "sessions_total": 1,
            "sessions_active": 0
        }));

        assert_eq!(pulse.activity, AgentActivity::Idle);
        assert!(pulse.token_rate_known);
        assert_eq!(pulse.token_rate_per_sec, 0.0);
    }

    #[test]
    fn agent_pulse_uses_agent_rate_fallback() {
        let pulse = AgentPulse::from_abtop_status(&json!({
            "interval_ms": 1000,
            "sessions_total": 2,
            "sessions_active": 0,
            "agents": [
                {"agent_cli": "codex", "token_rate": 5.5},
                {"agent_cli": "claude", "token_rate": 2.5}
            ]
        }));

        assert_eq!(pulse.activity, AgentActivity::Active);
        assert!(pulse.token_rate_known);
        assert_eq!(pulse.token_rate_per_tick, 8.0);
        assert_eq!(pulse.token_rate_per_sec, 8.0);
    }

    #[test]
    fn agent_pulse_marks_unknown_without_sessions_or_rate() {
        let pulse = AgentPulse::from_abtop_status(&json!({}));

        assert_eq!(pulse.activity, AgentActivity::Unknown);
        assert!(!pulse.token_rate_known);
        assert_eq!(pulse.interval_ms, 1000);
        assert_eq!(pulse.token_rate_per_tick, 0.0);
    }

    #[test]
    fn burn_detector_keeps_short_activity_normal() {
        let mut detector = AgentBurnDetector::new(AgentBurnConfig {
            min_rate_per_sec: 1.0,
            timeout_secs: 60,
        });
        let pulse = AgentPulse::from_abtop_status(&json!({
            "interval_ms": 1000,
            "token_rate": 10.0,
            "sessions_total": 1,
            "sessions_active": 1
        }));

        let first = detector.observe(&pulse, 100);
        let later = detector.observe(&pulse, 130);

        assert_eq!(first.state, AgentBurnState::Active);
        assert_eq!(first.burning_for_secs, Some(0));
        assert_eq!(later.state, AgentBurnState::Active);
        assert_eq!(later.burning_for_secs, Some(30));
    }

    #[test]
    fn burn_detector_marks_runaway_after_timeout() {
        let mut detector = AgentBurnDetector::new(AgentBurnConfig {
            min_rate_per_sec: 1.0,
            timeout_secs: 60,
        });
        let pulse = AgentPulse::from_abtop_status(&json!({
            "interval_ms": 1000,
            "token_rate": 10.0,
            "sessions_total": 1,
            "sessions_active": 1
        }));

        detector.observe(&pulse, 100);
        let event = detector.observe(&pulse, 161);

        assert_eq!(event.state, AgentBurnState::Runaway);
        assert_eq!(event.burning_for_secs, Some(61));
        assert_eq!(event.token_rate_per_sec, 10.0);
    }

    #[test]
    fn burn_detector_switches_at_timeout_boundary_and_recovers() {
        let mut detector = AgentBurnDetector::new(AgentBurnConfig {
            min_rate_per_sec: 1.0,
            timeout_secs: 60,
        });
        let burning = AgentPulse::from_abtop_status(&json!({
            "interval_ms": 1000,
            "token_rate": 10.0,
            "sessions_total": 1,
            "sessions_active": 1
        }));
        let idle = AgentPulse::from_abtop_status(&json!({
            "interval_ms": 1000,
            "token_rate": 0.0,
            "sessions_total": 1,
            "sessions_active": 0
        }));

        assert_eq!(
            detector.observe(&burning, 100).state,
            AgentBurnState::Active
        );

        let before_timeout = detector.observe(&burning, 159);
        assert_eq!(before_timeout.state, AgentBurnState::Active);
        assert_eq!(before_timeout.burning_for_secs, Some(59));

        let at_timeout = detector.observe(&burning, 160);
        assert_eq!(at_timeout.state, AgentBurnState::Runaway);
        assert_eq!(at_timeout.burning_for_secs, Some(60));

        let recovery = detector.observe(&idle, 161);
        assert_eq!(recovery.state, AgentBurnState::Recovery);
        assert_eq!(recovery.burning_for_secs, None);
    }

    #[test]
    fn burn_detector_recovers_when_rate_drops_to_zero() {
        let mut detector = AgentBurnDetector::new(AgentBurnConfig {
            min_rate_per_sec: 1.0,
            timeout_secs: 60,
        });
        let burning = AgentPulse::from_abtop_status(&json!({
            "interval_ms": 1000,
            "token_rate": 10.0,
            "sessions_total": 1,
            "sessions_active": 1
        }));
        let idle = AgentPulse::from_abtop_status(&json!({
            "interval_ms": 1000,
            "token_rate": 0.0,
            "sessions_total": 1,
            "sessions_active": 0
        }));

        detector.observe(&burning, 100);
        detector.observe(&burning, 161);
        let event = detector.observe(&idle, 170);

        assert_eq!(event.state, AgentBurnState::Recovery);
        assert_eq!(event.burning_for_secs, None);
    }

    #[test]
    fn burn_detector_does_not_alarm_on_unknown_rate() {
        let mut detector = AgentBurnDetector::new(AgentBurnConfig {
            min_rate_per_sec: 1.0,
            timeout_secs: 60,
        });
        let pulse = AgentPulse::from_abtop_status(&json!({
            "sessions_total": 1,
            "sessions_active": 1
        }));

        let event = detector.observe(&pulse, 100);

        assert_eq!(event.state, AgentBurnState::Active);
        assert_eq!(event.burning_for_secs, None);
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
}
