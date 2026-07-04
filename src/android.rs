#[cfg(all(target_os = "android", feature = "android-gui"))]
slint::include_modules!();

#[cfg(all(target_os = "android", feature = "android-gui"))]
use slint::{ComponentHandle, SharedString, Weak};

use crate::{AgentBurnEvent, AgentBurnState, DEFAULT_ANDROID_ALERT_COOLDOWN_SECS};
#[cfg(all(target_os = "android", feature = "android-gui"))]
use crate::{
    DEFAULT_API_BASE, DEFAULT_DANGER_THRESHOLD, DEFAULT_WARNING_THRESHOLD, HttpClient, Router,
    USER_AGENT_GUI, WindowState, api_fallbacks_for, dashboard_status, demo_payload, format_percent,
    metric_text, peak_percent, short_number, summarize_me,
};
#[cfg(all(target_os = "android", feature = "android-gui"))]
use std::fs;
#[cfg(all(target_os = "android", feature = "android-gui"))]
use std::path::PathBuf;
#[cfg(all(target_os = "android", feature = "android-gui"))]
use std::sync::Mutex;

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

#[cfg(test)]
mod tests {
    use super::*;

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
