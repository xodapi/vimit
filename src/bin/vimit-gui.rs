#![cfg_attr(windows, windows_subsystem = "windows")]
#![allow(clippy::collapsible_if)]

use serde_json::Value;
use slint::{
    ComponentHandle, PhysicalPosition, PhysicalSize, SharedString, Timer, TimerMode, Weak,
};
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};
use tray_icon::{
    Icon, MouseButton, TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{Menu, MenuEvent, MenuId, MenuItem},
};

use vimit as ng;

slint::include_modules!();

thread_local! {
    static TRAY_ICON: RefCell<Option<TrayIcon>> = const { RefCell::new(None) };
}

static CREATURE_SOUND_LEVEL: OnceLock<Mutex<Option<String>>> = OnceLock::new();

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    #[link_name = "Beep"]
    fn windows_beep(dw_freq: u32, dw_duration: u32) -> i32;
}

const OVERLAY_RATE_WINDOW: Duration = Duration::from_secs(5 * 60);
const OVERLAY_HISTORY_RETENTION: Duration = Duration::from_secs(30 * 60);
const OVERLAY_SPARK_SAMPLES: usize = 15;
const OVERLAY_COMPACT_SIZE: (f32, f32) = (260.0, 52.0);
const OVERLAY_FULL_SIZE: (f32, f32) = (340.0, 380.0);
const CREATURE_MIN_POINTS: usize = 8;
const CREATURE_MAX_POINTS: usize = 128;
const CREATURE_ORGANIC_MAX_POINTS: usize = 64;
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn suppress_windows_console(_command: &mut Command) {
    #[cfg(windows)]
    {
        _command.creation_flags(CREATE_NO_WINDOW);
    }
}

fn overlay_logical_size(compact: bool) -> (f32, f32) {
    if compact {
        OVERLAY_COMPACT_SIZE
    } else {
        OVERLAY_FULL_SIZE
    }
}

fn set_overlay_window_size(app: &AppWindow, compact: bool) {
    let (width, height) = overlay_logical_size(compact);
    let scale = app.window().scale_factor();
    app.window().set_size(PhysicalSize::new(
        (width * scale).round() as u32,
        (height * scale).round() as u32,
    ));
}

fn creature_node_count(percent: f32) -> usize {
    let normalized = (percent.clamp(0.0, 100.0) / 100.0) as f64;
    (CREATURE_MIN_POINTS + (normalized * 120.0).floor() as usize)
        .clamp(CREATURE_MIN_POINTS, CREATURE_MAX_POINTS)
}

fn organic_creature_node_count(percent: f32) -> usize {
    let normalized = percent.clamp(0.0, 100.0) / 100.0;
    (CREATURE_MIN_POINTS
        + (normalized * (CREATURE_ORGANIC_MAX_POINTS - CREATURE_MIN_POINTS) as f32).floor()
            as usize)
        .clamp(CREATURE_MIN_POINTS, CREATURE_ORGANIC_MAX_POINTS)
}

fn creature_node_count_for_skin(percent: f32, skin: i32) -> usize {
    if skin == 1 {
        organic_creature_node_count(percent)
    } else {
        creature_node_count(percent)
    }
}

fn creature_path_commands_for_skin(percent: f32, phase: f32, skin: i32) -> String {
    if skin == 1 {
        organic_creature_path_commands(percent, phase)
    } else {
        creature_path_commands(percent, phase)
    }
}

fn creature_path_commands(percent: f32, phase: f32) -> String {
    let count = creature_node_count(percent);
    let center_x = 52.0f32;
    let center_y = 44.0f32;
    let base_radius = 26.0 + percent.clamp(0.0, 100.0) * 0.10;
    let mut points = Vec::with_capacity(count);

    for idx in 0..count {
        let angle = (idx as f32 / count as f32) * std::f32::consts::TAU;
        let jitter = (phase.to_radians() * 1.7 + idx as f32 * 1.618).sin() * 3.4
            + (phase.to_radians() * 0.7 + idx as f32 * 2.413).cos() * 1.8;
        let radius = (base_radius + jitter).clamp(22.0, 39.0);
        points.push((
            center_x + radius * angle.cos(),
            center_y + radius * angle.sin(),
        ));
    }

    smooth_closed_path(&points)
}

fn organic_creature_path_commands(percent: f32, phase: f32) -> String {
    let count = organic_creature_node_count(percent);
    let normalized = percent.clamp(0.0, 100.0) / 100.0;
    let center_x = 52.0f32;
    let center_y = 44.0f32;
    let base_radius = 24.0 + normalized * 10.0;
    let amplitude = 3.0 + normalized * 5.0;
    let waves = 3.0 + (normalized * 5.0).round();
    let speed = if percent >= 90.0 {
        3.0
    } else if percent >= 75.0 {
        1.7
    } else {
        0.7
    };
    let time = phase.to_radians();
    let mut points = Vec::with_capacity(count);

    for idx in 0..count {
        let theta = (idx as f32 / count as f32) * std::f32::consts::TAU;
        let primary = (waves * theta + speed * time).sin();
        let secondary = ((waves * 0.5 + 1.0) * theta - speed * time * 0.65).sin() * 0.35;
        let breath = (time * 0.4).sin() * (1.4 + normalized * 1.8);
        let radius = (base_radius + amplitude * (primary + secondary) + breath).clamp(20.0, 39.0);
        points.push((
            center_x + radius * theta.cos(),
            center_y + radius * theta.sin(),
        ));
    }

    smooth_closed_path(&points)
}

fn smooth_closed_path(points: &[(f32, f32)]) -> String {
    let count = points.len();
    let mut commands = String::with_capacity(count * 34);
    let (start_x, start_y) = points[0];
    commands.push_str(&format!("M {:.2} {:.2} ", start_x, start_y));
    for idx in 0..count {
        let p0 = points[(idx + count - 1) % count];
        let p1 = points[idx];
        let p2 = points[(idx + 1) % count];
        let p3 = points[(idx + 2) % count];
        let c1 = (p1.0 + (p2.0 - p0.0) / 6.0, p1.1 + (p2.1 - p0.1) / 6.0);
        let c2 = (p2.0 - (p3.0 - p1.0) / 6.0, p2.1 - (p3.1 - p1.1) / 6.0);
        commands.push_str(&format!(
            "C {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} ",
            c1.0, c1.1, c2.0, c2.1, p2.0, p2.1
        ));
    }
    commands.push('Z');
    commands
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CreatureSound {
    Warning,
    Danger,
    Recovery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CreatureState {
    Sleeping,
    Awake,
    Alert,
    Critical,
    Recovery,
}

impl CreatureState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Sleeping => "sleeping",
            Self::Awake => "awake",
            Self::Alert => "alert",
            Self::Critical => "critical",
            Self::Recovery => "recovery",
        }
    }
}

fn creature_state_for(
    percent: f32,
    credit_rate: f32,
    reset_detected: bool,
    samples_len: usize,
) -> CreatureState {
    if reset_detected {
        return CreatureState::Recovery;
    }
    if samples_len > 1 && credit_rate <= 0.05 {
        return CreatureState::Sleeping;
    }
    if percent >= 90.0 {
        CreatureState::Critical
    } else if percent >= 75.0 {
        CreatureState::Alert
    } else {
        CreatureState::Awake
    }
}

fn overlay_phase_step(state: CreatureState) -> f32 {
    match state {
        CreatureState::Sleeping => 2.0,
        CreatureState::Awake => 7.0,
        CreatureState::Alert => 9.0,
        CreatureState::Critical => 13.0,
        CreatureState::Recovery => 16.0,
    }
}

fn creature_state_from_str(value: &str) -> CreatureState {
    match value {
        "sleeping" => CreatureState::Sleeping,
        "alert" => CreatureState::Alert,
        "critical" => CreatureState::Critical,
        "recovery" => CreatureState::Recovery,
        _ => CreatureState::Awake,
    }
}

fn level_rank(level: &str) -> u8 {
    match level {
        "danger" => 2,
        "warning" => 1,
        _ => 0,
    }
}

fn creature_sound_for_transition(previous: Option<&str>, current: &str) -> Option<CreatureSound> {
    let previous = previous?;
    let previous_rank = level_rank(previous);
    let current_rank = level_rank(current);

    if current_rank == previous_rank {
        return None;
    }
    if current_rank == 2 && previous_rank < 2 {
        Some(CreatureSound::Danger)
    } else if current_rank == 1 && previous_rank == 0 {
        Some(CreatureSound::Warning)
    } else if previous_rank == 2 && current_rank == 0 {
        Some(CreatureSound::Recovery)
    } else {
        None
    }
}

fn maybe_play_creature_sound(current_level: &str) {
    let state = CREATURE_SOUND_LEVEL.get_or_init(|| Mutex::new(None));
    let mut previous = state.lock().unwrap();
    let sound = creature_sound_for_transition(previous.as_deref(), current_level);
    *previous = Some(current_level.to_string());
    drop(previous);

    if let Some(sound) = sound {
        thread::spawn(move || play_creature_sound_blocking(sound));
    }
}

fn play_creature_sound_blocking(sound: CreatureSound) {
    #[cfg(windows)]
    unsafe {
        match sound {
            CreatureSound::Warning => {
                let _ = windows_beep(440, 200);
            }
            CreatureSound::Danger => {
                let _ = windows_beep(440, 150);
                let _ = windows_beep(220, 150);
            }
            CreatureSound::Recovery => {
                let _ = windows_beep(880, 150);
            }
        }
    }

    #[cfg(not(windows))]
    {
        let _ = sound;
    }
}

fn create_status_icon(color: (u8, u8, u8)) -> Icon {
    let width = 32;
    let height = 32;
    let mut rgba = vec![0u8; width * height * 4];

    let center_x = 16.0;
    let center_y = 16.0;
    let radius = 10.0;

    for y in 0..height {
        for x in 0..width {
            let dx = x as f32 - center_x;
            let dy = y as f32 - center_y;
            let dist = (dx * dx + dy * dy).sqrt();

            let idx = (y * width + x) * 4;
            if dist <= radius {
                rgba[idx] = color.0;
                rgba[idx + 1] = color.1;
                rgba[idx + 2] = color.2;
                rgba[idx + 3] = 255;
            } else if dist <= radius + 1.0 {
                let alpha = ((1.0 - (dist - radius)) * 255.0) as u8;
                rgba[idx] = color.0;
                rgba[idx + 1] = color.1;
                rgba[idx + 2] = color.2;
                rgba[idx + 3] = alpha;
            }
        }
    }

    Icon::from_rgba(rgba, width as u32, height as u32).expect("failed to create tray icon")
}

#[derive(Clone)]
struct TrayIds {
    show: MenuId,
    mini: MenuId,
    quit: MenuId,
}

fn try_init_tray() -> Option<TrayIds> {
    let tray_menu = Menu::new();
    let show_item = MenuItem::new("Показать окно", true, None);
    let mini_item = MenuItem::new("Мини-окно", true, None);
    let quit_item = MenuItem::new("Выход", true, None);

    for item in [&show_item, &mini_item, &quit_item] {
        if let Err(error) = tray_menu.append(item) {
            eprintln!("vimit-gui: tray menu unavailable: {error}");
            return None;
        }
    }

    let ids = TrayIds {
        show: show_item.id().clone(),
        mini: mini_item.id().clone(),
        quit: quit_item.id().clone(),
    };

    let tray_icon_instance = match TrayIconBuilder::new()
        .with_menu(Box::new(tray_menu))
        .with_tooltip("VibeMode Control")
        .with_icon(create_status_icon((141, 150, 170)))
        .build()
    {
        Ok(icon) => icon,
        Err(error) => {
            eprintln!("vimit-gui: tray unavailable: {error}");
            return None;
        }
    };

    TRAY_ICON.with(|cell| {
        *cell.borrow_mut() = Some(tray_icon_instance);
    });

    Some(ids)
}

fn spawn_mini_overlay(force_demo: bool) -> Result<(), String> {
    let current =
        std::env::current_exe().map_err(|error| format!("cannot find GUI exe: {error}"))?;
    let mut command = Command::new(current);
    command.arg("--overlay").arg("--compact");
    if force_demo {
        command.arg("--demo");
    }
    suppress_windows_console(&mut command);
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("failed to launch mini overlay: {error}"))
}

fn config_dir_path() -> Result<PathBuf, String> {
    let home = if cfg!(windows) {
        std::env::var("APPDATA")
            .ok()
            .map(PathBuf::from)
            .or_else(|| std::env::var("USERPROFILE").ok().map(PathBuf::from))
    } else {
        std::env::var("HOME").ok().map(PathBuf::from)
    };
    let home =
        home.ok_or_else(|| "Не удалось определить домашнюю папку пользователя".to_string())?;
    Ok(if cfg!(windows) {
        home.join("vimit")
    } else {
        home.join(".config").join("vimit")
    })
}

fn ensure_config_dir() -> Result<PathBuf, String> {
    let dir = config_dir_path()?;
    std::fs::create_dir_all(&dir)
        .map_err(|error| format!("cannot create config directory {}: {error}", dir.display()))?;
    Ok(dir)
}

fn ensure_env_template() -> Result<PathBuf, String> {
    let dir = ensure_config_dir()?;
    let env_file = dir.join(".env");
    if !env_file.exists() {
        std::fs::write(
            &env_file,
            "# VibeMode настройки\n# Вставьте ключ без кавычек:\nVIBEMODE_API_KEY=\n# VIBEMODE_API_BASE=https://r-api.vibemod.pro\n",
        )
        .map_err(|error| format!("cannot write {}: {error}", env_file.display()))?;
    }
    Ok(env_file)
}

fn ensure_accounts_template() -> Result<PathBuf, String> {
    let dir = ensure_config_dir()?;
    let accounts_file = dir.join("accounts.toml");
    if !accounts_file.exists() {
        std::fs::write(
            &accounts_file,
            "# VibeMode accounts\n# [accounts.default]\n# api_key_env = \"VIBEMODE_API_KEY\"\n# api_base = \"https://r-api.vibemod.pro\"\n",
        )
        .map_err(|error| format!("cannot write {}: {error}", accounts_file.display()))?;
    }
    Ok(accounts_file)
}

fn open_path(path: &Path) -> Result<(), String> {
    #[cfg(windows)]
    let mut command = {
        let mut command = Command::new("explorer");
        command.arg(path);
        command
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        command.arg(path);
        command
    };
    #[cfg(target_os = "linux")]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(path);
        command
    };
    suppress_windows_console(&mut command);
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("cannot open {}: {error}", path.display()))
}

fn read_agent_status_for_gui(binary: &str) -> ng::AgentStatus {
    if binary.trim().is_empty() {
        return ng::AgentStatus {
            summary: "агенты: abtop отключён; задайте ABTOP_BIN".to_string(),
            token_rate: "токены/мин: нет данных abtop".to_string(),
        };
    }

    let mut command = Command::new(binary);
    command.arg("--status-json");
    suppress_windows_console(&mut command);
    let Ok(output) = command.output() else {
        return ng::AgentStatus {
            summary: "агенты: abtop не найден; задайте ABTOP_BIN".to_string(),
            token_rate: "токены/мин: нет данных abtop".to_string(),
        };
    };
    if !output.status.success() {
        return ng::AgentStatus {
            summary: "агенты: статус abtop недоступен".to_string(),
            token_rate: "токены/мин: нет данных abtop".to_string(),
        };
    }
    let Ok(parsed) = serde_json::from_slice::<Value>(&output.stdout) else {
        return ng::AgentStatus {
            summary: "агенты: abtop вернул невалидный JSON".to_string(),
            token_rate: "токены/мин: нет данных abtop".to_string(),
        };
    };
    let sessions = parsed
        .get("sessions_total")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let active = parsed
        .get("sessions_active")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let ctx = parsed
        .get("agents")
        .and_then(Value::as_array)
        .and_then(|agents| {
            agents
                .iter()
                .filter_map(|agent| agent.get("max_context_pct").and_then(ng::to_number))
                .fold(None, |peak: Option<f64>, value| {
                    Some(peak.map_or(value, |peak| peak.max(value)))
                })
        })
        .map(|value| format!("{value:.0}%"))
        .unwrap_or_else(|| "н/д".to_string());
    let token_rate = parsed
        .get("token_rate")
        .and_then(ng::to_number)
        .or_else(|| summed_agent_token_rate_for_gui(&parsed));

    ng::AgentStatus {
        summary: format!("агенты: сессий {sessions}, активных {active}, контекст макс. {ctx}"),
        token_rate: token_rate
            .map(|value| format!("токены/мин: {}", ng::short_rate(value)))
            .unwrap_or_else(|| "токены/мин: нет данных abtop".to_string()),
    }
}

fn summed_agent_token_rate_for_gui(parsed: &Value) -> Option<f64> {
    parsed
        .get("agents")
        .and_then(Value::as_array)
        .and_then(|agents| {
            let mut total = 0.0;
            let mut seen = false;
            for agent in agents {
                if let Some(rate) = agent.get("token_rate").and_then(ng::to_number) {
                    total += rate;
                    seen = true;
                }
            }
            seen.then_some(total)
        })
}

#[derive(Debug, Clone, Default)]
struct GuiAccount {
    api_key_env: Option<String>,
    api_base: Option<String>,
}

fn load_gui_accounts() -> (Vec<String>, Vec<GuiAccount>) {
    if let Ok(config) = ng::cli::accounts::AccountsConfig::load() {
        let names = config.list_names();
        let configs: Vec<GuiAccount> = names
            .iter()
            .filter_map(|n| config.resolve(n).ok())
            .map(|r| GuiAccount {
                api_key_env: r.api_key_env.clone(),
                api_base: r.api_base.clone(),
            })
            .collect();
        (names, configs)
    } else {
        (vec![], vec![])
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let is_overlay = args.iter().any(|arg| arg == "--overlay");
    let is_compact = args.iter().any(|arg| arg == "--compact");
    let force_demo = args.iter().any(|arg| arg == "--demo");
    let mock_path = Arc::new(
        args.iter()
            .position(|arg| arg == "--mock")
            .and_then(|idx| args.get(idx + 1))
            .cloned(),
    );

    ng::cli::update::start_background_check();
    let app = AppWindow::new().expect("cannot initialize Slint window");

    if is_overlay {
        app.set_is_overlay(true);
        if is_compact {
            app.set_is_compact(true);
        }
        set_overlay_window_size(&app, is_compact);
    }

    let tray_ids = try_init_tray();
    let has_tray = tray_ids.is_some();

    // Minimize to tray on close request
    let weak = app.as_weak();
    app.window().on_close_requested(move || {
        if has_tray && let Some(app) = weak.upgrade() {
            let _ = app.hide();
            return slint::CloseRequestResponse::KeepWindowShown;
        }
        slint::CloseRequestResponse::HideWindow
    });

    app.on_close_overlay(move || {
        let _ = slint::quit_event_loop();
    });

    let weak = app.as_weak();
    app.on_overlay_compact_changed(move |compact| {
        if let Some(app) = weak.upgrade() {
            set_overlay_window_size(&app, compact);
        }
    });

    let drag_origin = Arc::new(Mutex::new(None::<PhysicalPosition>));
    let resize_origin = Arc::new(Mutex::new(None::<PhysicalSize>));

    let weak = app.as_weak();
    let drag_origin_start = drag_origin.clone();
    app.on_overlay_drag_start(move || {
        if let Some(app) = weak.upgrade() {
            *drag_origin_start.lock().unwrap() = Some(app.window().position());
        }
    });

    let weak = app.as_weak();
    let drag_origin_move = drag_origin.clone();
    app.on_overlay_drag_move(move |dx, dy| {
        if let Some(app) = weak.upgrade() {
            if let Some(origin) = *drag_origin_move.lock().unwrap() {
                let scale = app.window().scale_factor();
                app.window().set_position(PhysicalPosition::new(
                    origin.x + (dx * scale) as i32,
                    origin.y + (dy * scale) as i32,
                ));
            }
        }
    });

    let weak = app.as_weak();
    let resize_origin_start = resize_origin.clone();
    app.on_overlay_resize_start(move || {
        if let Some(app) = weak.upgrade() {
            *resize_origin_start.lock().unwrap() = Some(app.window().size());
        }
    });

    let weak = app.as_weak();
    let resize_origin_move = resize_origin.clone();
    app.on_overlay_resize_move(move |dx, dy| {
        if let Some(app) = weak.upgrade() {
            if let Some(origin) = *resize_origin_move.lock().unwrap() {
                let scale = app.window().scale_factor();
                let min_width = if app.get_is_compact() { 220.0 } else { 300.0 };
                let min_height = if app.get_is_compact() { 48.0 } else { 340.0 };
                let width = (origin.width as f32 + dx * scale).max(min_width * scale) as u32;
                let height = (origin.height as f32 + dy * scale).max(min_height * scale) as u32;
                app.window().set_size(PhysicalSize::new(width, height));
            }
        }
    });

    let pulse_timer = Timer::default();
    let weak = app.as_weak();
    pulse_timer.start(TimerMode::Repeated, Duration::from_millis(80), move || {
        if let Some(app) = weak.upgrade() {
            let state = creature_state_from_str(app.get_overlay_creature_state().as_str());
            let next = (app.get_overlay_pulse_phase() + overlay_phase_step(state)) % 360.0;
            app.set_overlay_pulse_phase(next);
            let percent = app.get_overlay_creature_percent();
            let skin = app.get_overlay_creature_skin();
            app.set_overlay_creature_points(creature_node_count_for_skin(percent, skin) as i32);
            app.set_overlay_creature_path(
                creature_path_commands_for_skin(percent, next, skin).into(),
            );
        }
    });

    let countdown_timer = Timer::default();
    let weak = app.as_weak();
    countdown_timer.start(TimerMode::Repeated, Duration::from_secs(1), move || {
        if let Some(app) = weak.upgrade() {
            let seconds = app.get_overlay_reset_seconds();
            if seconds > 0 {
                let next = seconds - 1;
                app.set_overlay_reset_seconds(next);
                app.set_overlay_reset_text(format_overlay_countdown(next).into());
            }
        }
    });

    // Listen for Tray Events
    let tray_timer = Timer::default();
    let weak = app.as_weak();
    tray_timer.start(TimerMode::Repeated, Duration::from_millis(100), move || {
        if let Some(ids) = tray_ids.as_ref() {
            if let (Ok(event), Some(app)) = (MenuEvent::receiver().try_recv(), weak.upgrade()) {
                if event.id == ids.show {
                    let _ = app.show();
                } else if event.id == ids.mini {
                    let demo = app.get_active_endpoint_label().as_str() == "demo";
                    if let Err(error) = spawn_mini_overlay(demo) {
                        app.set_error_text(error.into());
                    }
                } else if event.id == ids.quit {
                    let _ = slint::quit_event_loop();
                }
            }
        }
        if let (Ok(event), Some(app)) = (TrayIconEvent::receiver().try_recv(), weak.upgrade()) {
            match event {
                TrayIconEvent::Click { button, .. } => {
                    if button == MouseButton::Left {
                        let _ = app.show();
                    }
                }
                TrayIconEvent::DoubleClick { .. } => {
                    let _ = app.show();
                }
                _ => {}
            }
        }
    });

    let http = std::sync::Arc::new(
        ng::HttpClient::new(ng::USER_AGENT_GUI).expect("cannot initialize HTTP client"),
    );

    let auto_api_failover = Arc::new(AtomicBool::new(
        ng::cli::update::is_auto_api_failover_enabled(),
    ));
    let router = std::sync::Arc::new(std::sync::Mutex::new(ng::Router::new(
        ng::DEFAULT_API_BASE.to_string(),
        ng::api_fallbacks_for(
            ng::DEFAULT_API_BASE,
            auto_api_failover.load(Ordering::Relaxed),
        ),
    )));

    let refresh_gen = Arc::new(AtomicU64::new(0));
    let is_refreshing = Arc::new(AtomicBool::new(false));
    let overlay_history = Arc::new(Mutex::new(OverlayHistory::default()));
    let demo_mode = Arc::new(AtomicBool::new(force_demo));

    let (account_names, account_configs) = load_gui_accounts();
    let current_acct = Arc::new(Mutex::new(None::<GuiAccount>));

    if !account_names.is_empty() {
        let slist: Vec<SharedString> = account_names.iter().map(|n| n.as_str().into()).collect();
        let arr = slint::ModelRc::new(std::rc::Rc::new(slint::VecModel::from(slist)));
        app.set_account_names(arr);
        app.set_current_account(account_names[0].as_str().into());
        {
            let mut cur = current_acct.lock().unwrap();
            *cur = Some(account_configs[0].clone());
        }

        let state = current_acct.clone();
        let configs = account_configs.clone();
        let weak_app = app.as_weak();
        app.on_account_changed(move |name| {
            if let Some(idx) = account_names.iter().position(|n| n == name.as_str()) {
                {
                    let mut cur = state.lock().unwrap();
                    *cur = Some(configs[idx].clone());
                }
                if let Some(app) = weak_app.upgrade() {
                    app.invoke_refresh_requested();
                }
            }
        });
    }

    let weak = app.as_weak();
    let http_clone = http.clone();
    let acct = current_acct.clone();
    let router_clone = router.clone();
    let gen1 = refresh_gen.clone();
    let is_ref1 = is_refreshing.clone();
    let history1 = overlay_history.clone();
    let demo_mode1 = demo_mode.clone();
    let mock_path1 = mock_path.clone();
    let failover1 = auto_api_failover.clone();
    app.on_refresh_requested(move || {
        start_refresh(
            weak.clone(),
            demo_mode1.load(Ordering::Relaxed),
            mock_path1.clone(),
            http_clone.clone(),
            acct.clone(),
            router_clone.clone(),
            ng::DEFAULT_WARNING_THRESHOLD,
            ng::DEFAULT_DANGER_THRESHOLD,
            gen1.clone(),
            is_ref1.clone(),
            history1.clone(),
            failover1.clone(),
        );
    });

    let weak = app.as_weak();
    let http_clone = http.clone();
    let acct = current_acct.clone();
    let router_clone = router.clone();
    let gen2 = refresh_gen.clone();
    let is_ref2 = is_refreshing.clone();
    let history2 = overlay_history.clone();
    let demo_mode2 = demo_mode.clone();
    let mock_path2 = mock_path.clone();
    let failover2 = auto_api_failover.clone();
    app.on_demo_requested(move || {
        demo_mode2.store(true, Ordering::Relaxed);
        start_refresh(
            weak.clone(),
            true,
            mock_path2.clone(),
            http_clone.clone(),
            acct.clone(),
            router_clone.clone(),
            ng::DEFAULT_WARNING_THRESHOLD,
            ng::DEFAULT_DANGER_THRESHOLD,
            gen2.clone(),
            is_ref2.clone(),
            history2.clone(),
            failover2.clone(),
        );
    });

    let weak = app.as_weak();
    let http_clone = http.clone();
    let acct = current_acct.clone();
    let router_clone = router.clone();
    let gen3 = refresh_gen.clone();
    let is_ref3 = is_refreshing.clone();
    let history3 = overlay_history.clone();
    let demo_mode3 = demo_mode.clone();
    let mock_path3 = mock_path.clone();
    let failover3 = auto_api_failover.clone();
    app.on_settings_changed(move |warning, danger| {
        start_refresh(
            weak.clone(),
            demo_mode3.load(Ordering::Relaxed),
            mock_path3.clone(),
            http_clone.clone(),
            acct.clone(),
            router_clone.clone(),
            warning as f64,
            danger as f64,
            gen3.clone(),
            is_ref3.clone(),
            history3.clone(),
            failover3.clone(),
        );
    });

    let router_for_failover = router.clone();
    let failover_for_toggle = auto_api_failover.clone();
    app.on_auto_api_failover_changed(move |enabled| {
        ng::cli::update::set_auto_api_failover_enabled(enabled);
        failover_for_toggle.store(enabled, Ordering::Relaxed);
        let mut router = router_for_failover.lock().unwrap();
        *router = ng::Router::new(
            ng::DEFAULT_API_BASE.to_string(),
            ng::api_fallbacks_for(ng::DEFAULT_API_BASE, enabled),
        );
    });

    let timer = Timer::default();
    let weak = app.as_weak();
    let http_clone = http.clone();
    let acct = current_acct.clone();
    let router_clone = router.clone();
    let gen4 = refresh_gen.clone();
    let is_ref4 = is_refreshing.clone();
    let history4 = overlay_history.clone();
    let demo_mode4 = demo_mode.clone();
    let mock_path4 = mock_path.clone();
    let failover4 = auto_api_failover.clone();
    timer.start(TimerMode::Repeated, Duration::from_secs(10), move || {
        start_refresh(
            weak.clone(),
            demo_mode4.load(Ordering::Relaxed),
            mock_path4.clone(),
            http_clone.clone(),
            acct.clone(),
            router_clone.clone(),
            ng::DEFAULT_WARNING_THRESHOLD,
            ng::DEFAULT_DANGER_THRESHOLD,
            gen4.clone(),
            is_ref4.clone(),
            history4.clone(),
            failover4.clone(),
        );
    });

    start_refresh(
        app.as_weak(),
        demo_mode.load(Ordering::Relaxed),
        mock_path.clone(),
        http,
        current_acct.clone(),
        router.clone(),
        ng::DEFAULT_WARNING_THRESHOLD,
        ng::DEFAULT_DANGER_THRESHOLD,
        refresh_gen.clone(),
        is_refreshing.clone(),
        overlay_history.clone(),
        auto_api_failover.clone(),
    );

    if !force_demo
        && mock_path.is_none()
        && let Ok(dotenv) = gui_load_dotenv()
    {
        let config = runtime_config(
            &dotenv,
            &current_acct,
            auto_api_failover.load(Ordering::Relaxed),
        );
        if config.api_key.is_empty() {
            app.set_needs_setup(true);
        }
    }

    app.set_auto_update_check(ng::cli::update::is_auto_check_enabled());
    app.set_auto_api_failover(auto_api_failover.load(Ordering::Relaxed));

    app.on_auto_update_changed(|enabled| {
        ng::cli::update::set_auto_check_enabled(enabled);
    });

    let weak = app.as_weak();
    app.on_launch_mini_overlay(move || {
        if let Some(app) = weak.upgrade() {
            let demo = app.get_active_endpoint_label().as_str() == "demo";
            match spawn_mini_overlay(demo) {
                Ok(()) => {
                    app.set_status_text("Мини-окно запущено, его можно перетащить мышью".into())
                }
                Err(error) => app.set_error_text(error.into()),
            }
        }
    });

    let weak = app.as_weak();
    app.on_check_updates(move || {
        let weak_clone = weak.clone();
        thread::spawn(move || {
            let _ = weak_clone.upgrade_in_event_loop(|app| {
                app.set_status_text("Проверка обновлений...".into());
            });
            let current_version = ng::VERSION;
            let mut builder = self_update::backends::github::Update::configure();
            builder
                .repo_owner("xodapi")
                .repo_name("vimit")
                .bin_name("vimit")
                .current_version(current_version);

            if let Ok(updater) = builder.build() {
                match updater.get_latest_release() {
                    Ok(latest) => {
                        let is_greater =
                            self_update::version::bump_is_greater(current_version, &latest.version)
                                .unwrap_or(false);
                        let _ = weak_clone.upgrade_in_event_loop(move |app| {
                            if is_greater {
                                app.set_new_version_label(latest.version.clone().into());
                                app.set_status_text(
                                    format!("Доступно обновление: v{}", latest.version).into(),
                                );
                            } else {
                                app.set_new_version_label("".into());
                                app.set_status_text(
                                    format!("У вас последняя версия v{}", current_version).into(),
                                );
                            }
                        });
                    }
                    Err(e) => {
                        let _ = weak_clone.upgrade_in_event_loop(move |app| {
                            app.set_status_text(format!("Ошибка проверки: {}", e).into());
                        });
                    }
                }
            }
        });
    });

    let weak = app.as_weak();
    app.on_update_now(move || {
        let weak_clone = weak.clone();
        thread::spawn(move || {
            let _ = weak_clone.upgrade_in_event_loop(|app| {
                app.set_status_text("Скачивание обновления...".into());
            });
            match ng::cli::update::check_and_update(false) {
                Ok(_) => {
                    let _ = weak_clone.upgrade_in_event_loop(|app| {
                        app.set_status_text(
                            "Обновление установлено! Перезапустите приложение.".into(),
                        );
                        app.set_new_version_label("".into());
                    });
                }
                Err(e) => {
                    let _ = weak_clone.upgrade_in_event_loop(move |app| {
                        app.set_status_text(format!("Ошибка обновления: {}", e).into());
                    });
                }
            }
        });
    });

    let weak = app.as_weak();
    app.on_open_config_dir(move || {
        if let Some(app) = weak.upgrade() {
            match ensure_config_dir().and_then(|path| open_path(&path).map(|()| path)) {
                Ok(path) => {
                    app.set_setup_status_text(
                        format!("Папка настроек открыта: {}", path.display()).into(),
                    );
                    app.set_footer_text(format!("config: {}", path.display()).into());
                }
                Err(error) => app.set_setup_status_text(error.into()),
            }
        }
    });

    let weak = app.as_weak();
    app.on_setup_create_config(move || {
        if let Some(app) = weak.upgrade() {
            match ensure_env_template() {
                Ok(path) => {
                    app.set_setup_status_text(
                        format!(
                            "Создан файл настроек: {}. Откройте его и вставьте VIBEMODE_API_KEY.",
                            path.display()
                        )
                        .into(),
                    );
                    app.set_footer_text(format!("config: {}", path.display()).into());
                }
                Err(error) => app.set_setup_status_text(error.into()),
            }
        }
    });

    let weak = app.as_weak();
    app.on_setup_open_env_file(move || {
        if let Some(app) = weak.upgrade() {
            match ensure_env_template().and_then(|path| open_path(&path).map(|()| path)) {
                Ok(path) => {
                    app.set_setup_status_text(
                        format!(
                            "Откройте {}, вставьте ключ и нажмите Проверить.",
                            path.display()
                        )
                        .into(),
                    );
                }
                Err(error) => app.set_setup_status_text(error.into()),
            }
        }
    });

    let weak = app.as_weak();
    app.on_open_accounts_config(move || {
        if let Some(app) = weak.upgrade() {
            match ensure_accounts_template().and_then(|path| open_path(&path).map(|()| path)) {
                Ok(path) => {
                    app.set_status_text(
                        format!("Открыт accounts config: {}", path.display()).into(),
                    );
                }
                Err(error) => app.set_error_text(error.into()),
            }
        }
    });

    app.run().expect("Slint event loop failed");
}

struct GuiDashboardResult {
    dashboard: ng::Dashboard,
    active_endpoint_label: String,
    five_trend_data: Vec<f32>,
    day_trend_data: Vec<f32>,
    week_trend_data: Vec<f32>,
    month_trend_data: Vec<f32>,
    overlay: OverlayState,
}

#[derive(Default)]
struct OverlayHistory {
    observations: VecDeque<OverlayUsagePoint>,
    samples: VecDeque<f32>,
}

struct OverlayUsagePoint {
    at: Instant,
    used: f64,
}

impl OverlayHistory {
    fn record_usage(&mut self, at: Instant, used: f64) -> (f32, bool) {
        let reset_detected = self
            .observations
            .back()
            .is_some_and(|latest| used < latest.used);
        self.observations.push_back(OverlayUsagePoint { at, used });
        self.trim_observations(at);

        let rate = self.average_credit_rate(at);
        if self.samples.len() == OVERLAY_SPARK_SAMPLES {
            self.samples.pop_front();
        }
        self.samples.push_back(rate);
        (rate, reset_detected)
    }

    fn average_credit_rate(&self, now: Instant) -> f32 {
        let Some(latest) = self.observations.back() else {
            return 0.0;
        };

        let baseline = self
            .observations
            .iter()
            .find(|point| within_duration(now, point.at, OVERLAY_RATE_WINDOW))
            .unwrap_or(latest);

        let elapsed_min = OVERLAY_RATE_WINDOW.as_secs_f64() / 60.0;
        ((latest.used - baseline.used).max(0.0) / elapsed_min) as f32
    }

    fn trim_observations(&mut self, now: Instant) {
        while self
            .observations
            .front()
            .is_some_and(|point| !within_duration(now, point.at, OVERLAY_HISTORY_RETENTION))
        {
            self.observations.pop_front();
        }
    }
}

fn within_duration(now: Instant, earlier: Instant, duration: Duration) -> bool {
    now.checked_duration_since(earlier)
        .is_some_and(|elapsed| elapsed <= duration)
}

struct OverlayState {
    credit_rate_text: String,
    token_rate_text: String,
    percent_hour_text: String,
    spark_data: Vec<f32>,
    creature_percent: f32,
    creature_state: CreatureState,
    delta_text: String,
    delta_level: String,
    reset_label: String,
    reset_seconds: i32,
}

#[allow(clippy::too_many_arguments)]
fn start_refresh(
    app: Weak<AppWindow>,
    demo: bool,
    mock_path: Arc<Option<String>>,
    http: Arc<ng::HttpClient>,
    account: Arc<Mutex<Option<GuiAccount>>>,
    router: Arc<Mutex<ng::Router>>,
    warning: f64,
    danger: f64,
    generation: Arc<AtomicU64>,
    is_refreshing: Arc<std::sync::atomic::AtomicBool>,
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
fn apply_dashboard(app: &AppWindow, result: Result<GuiDashboardResult, String>) {
    if let Ok(res) = result.as_ref() {
        update_tray_status(tray_status_from_dashboard(
            &res.dashboard.source,
            &res.dashboard.windows,
        ));
    }

    let offline_min = ng::get_offline_duration_min()
        .map(|m| m as i32)
        .unwrap_or(-1);
    app.set_api_offline_min(offline_min);

    match result {
        Ok(res) => {
            let dashboard = res.dashboard;
            let creature_level = dashboard
                .windows
                .iter()
                .find(|window| window.key == "5h")
                .map(|window| window.level.as_str())
                .unwrap_or("ok");
            if res.overlay.creature_state != CreatureState::Sleeping {
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
                .and_then(|s| s.replace(',', ".").parse::<f32>().ok())
                .unwrap_or(0.0);
            app.set_token_rate_raw(raw_rate);
            app.set_active_endpoint_label(res.active_endpoint_label.into());
            app.set_overlay_credit_rate_text(res.overlay.credit_rate_text.into());
            app.set_overlay_token_rate_text(res.overlay.token_rate_text.into());
            app.set_overlay_percent_hour_text(res.overlay.percent_hour_text.into());
            app.set_overlay_delta_text(res.overlay.delta_text.into());
            app.set_overlay_delta_level(res.overlay.delta_level.into());
            app.set_overlay_reset_label(res.overlay.reset_label.into());
            app.set_overlay_reset_seconds(res.overlay.reset_seconds);
            app.set_overlay_reset_text(format_overlay_countdown(res.overlay.reset_seconds).into());
            app.set_overlay_creature_percent(res.overlay.creature_percent);
            app.set_overlay_creature_state(res.overlay.creature_state.as_str().into());
            let skin = app.get_overlay_creature_skin();
            app.set_overlay_creature_points(creature_node_count_for_skin(
                res.overlay.creature_percent,
                skin,
            ) as i32);
            app.set_overlay_creature_path(
                creature_path_commands_for_skin(
                    res.overlay.creature_percent,
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
                |v: Vec<f32>| slint::ModelRc::new(std::rc::Rc::new(slint::VecModel::from(v)));
            app.set_five_trend_data(create_model(res.five_trend_data));
            app.set_day_trend_data(create_model(res.day_trend_data));
            app.set_week_trend_data(create_model(res.week_trend_data));
            app.set_month_trend_data(create_model(res.month_trend_data));
            app.set_overlay_spark_data(create_model(res.overlay.spark_data));

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

#[derive(Debug, Clone, PartialEq)]
struct TrayStatus {
    color: (u8, u8, u8),
    tooltip: String,
}

fn tray_status_from_dashboard(source: &str, windows: &[ng::WindowState]) -> TrayStatus {
    let max_percent = windows.iter().map(|w| w.percent).fold(0.0f64, f64::max);
    let color = if max_percent >= 90.0 {
        (214, 111, 115)
    } else if max_percent >= 75.0 {
        (202, 164, 95)
    } else if max_percent > 0.0 {
        (120, 173, 132)
    } else {
        (141, 150, 170)
    };

    let mut tooltip_parts = Vec::new();
    let source = source.trim_start_matches("источник: ").trim();
    if !source.is_empty() {
        tooltip_parts.push(source.to_string());
    }
    for window in windows {
        tooltip_parts.push(format!(
            "{}: {} ({:.1}%)",
            window.key, window.level, window.percent
        ));
    }
    let tooltip = if tooltip_parts.is_empty() {
        "VibeMode Control".to_string()
    } else {
        format!("VibeMode Control\n{}", tooltip_parts.join("\n"))
    };

    TrayStatus { color, tooltip }
}

fn update_tray_status(status: TrayStatus) {
    TRAY_ICON.with(|cell| {
        if let Some(ref mut tray) = *cell.borrow_mut() {
            let _ = tray.set_icon(Some(create_status_icon(status.color)));
            let _ = tray.set_tooltip(Some(status.tooltip));
        }
    });
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
    let level: SharedString = window.level.clone().into();
    let reset: SharedString = window.reset.clone().into();
    let credits: SharedString = ng::metric_text("кредиты", window.credits.as_ref()).into();
    let requests: SharedString = ng::metric_text("запросы", window.requests.as_ref()).into();
    let percent_text: SharedString = ng::format_percent(window.percent).into();
    let percent = window.percent as f32;
    let credit_percent =
        ng::peak_percent(window.credits.as_ref(), window.requests.as_ref()).unwrap_or(0.0) as f32;
    let request_percent =
        ng::peak_percent(window.credits.as_ref(), window.requests.as_ref()).unwrap_or(0.0) as f32;

    let donut_remaining = match window.credits.as_ref() {
        Some(m) => ng::short_number(m.remaining),
        None => "н/д".to_string(),
    };
    let donut_limit = match window.credits.as_ref() {
        Some(m) => ng::short_number(m.limit),
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

fn gui_load_dotenv() -> Result<HashMap<String, String>, String> {
    ng::load_dotenv_custom(None).or_else(|error| {
        eprintln!("vimit-gui: {error}");
        Ok(HashMap::new())
    })
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
            let (val, label) = if config.auto_failover
                && config.api_base.trim_end_matches('/') == ng::DEFAULT_API_BASE
            {
                let mut r = router.lock().unwrap();
                http.fetch_me_with_retry(&config.api_key, &mut r, &config.api_base)?
            } else {
                let mut r = ng::Router::new(
                    config.api_base.clone(),
                    ng::api_fallbacks_for(&config.api_base, config.auto_failover),
                );
                http.fetch_me_with_retry(&config.api_key, &mut r, &config.api_base)?
            };
            let endpoint = endpoint_for_label(&label, &config.api_base);
            (
                val,
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

    // Save snapshot to TrendStore and load trend data
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
                if let Some(w) = day.windows.iter().find(|w| w.key == "5h") {
                    five_trend_data.push(w.peak_max as f32);
                }
                if let Some(w) = day.windows.iter().find(|w| w.key == "24h") {
                    day_trend_data.push(w.peak_max as f32);
                }
                if let Some(w) = day.windows.iter().find(|w| w.key == "7d") {
                    week_trend_data.push(w.peak_max as f32);
                }
                if let Some(w) = day.windows.iter().find(|w| w.key == "30d") {
                    month_trend_data.push(w.peak_max as f32);
                }
            }
        }
    }

    // Generate nice mock sparkline values if we have no historical snapshots yet
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

fn build_overlay_state(
    windows: &[ng::WindowState],
    token_rate_text: &str,
    history: &Arc<Mutex<OverlayHistory>>,
) -> OverlayState {
    let now = Instant::now();
    let five = windows.iter().find(|window| window.key == "5h");
    let current_used = five.and_then(|window| window.credits.as_ref().map(|metric| metric.used));
    let credit_limit = five
        .and_then(|window| window.credits.as_ref().map(|metric| metric.limit))
        .unwrap_or(0.0);

    let mut history = history.lock().unwrap();
    let (credit_rate, reset_detected) = current_used
        .map(|used| history.record_usage(now, used))
        .unwrap_or_default();

    let samples: Vec<f32> = history.samples.iter().copied().collect();
    let spark_data = scale_samples(&samples);
    let token_rate = parse_rate_value(token_rate_text).unwrap_or(credit_rate * 750.0);
    let creature_percent = five.map(|window| window.percent as f32).unwrap_or(0.0);
    let creature_state = creature_state_for(
        creature_percent,
        credit_rate,
        reset_detected,
        history.samples.len(),
    );
    let percent_hour = if credit_limit > 0.0 {
        (credit_rate as f64 / credit_limit * 60.0 * 100.0) as f32
    } else {
        0.0
    };
    let (delta_text, delta_level) = overlay_delta(&samples);
    let (reset_label, reset_seconds) = nearest_reset(windows);

    OverlayState {
        credit_rate_text: format!("{} кред/мин", one_decimal_local(credit_rate)),
        token_rate_text: format!("{} токены/мин", one_decimal_local(token_rate)),
        percent_hour_text: format!("{}%/час", one_decimal_local(percent_hour)),
        spark_data,
        creature_percent,
        creature_state,
        delta_text,
        delta_level,
        reset_label,
        reset_seconds,
    }
}

fn scale_samples(samples: &[f32]) -> Vec<f32> {
    let max = samples.iter().copied().fold(0.0f32, f32::max).max(1.0);
    samples
        .iter()
        .map(|value| ((*value / max) * 100.0).clamp(2.0, 100.0))
        .collect()
}

fn overlay_delta(samples: &[f32]) -> (String, String) {
    let Some((&latest, previous)) = samples.split_last() else {
        return ("стабильно".to_string(), "ok".to_string());
    };
    if previous.is_empty() {
        return ("старт".to_string(), "ok".to_string());
    }
    let avg = previous.iter().sum::<f32>() / previous.len() as f32;
    if avg <= 0.05 {
        if latest <= 0.05 {
            return ("стабильно".to_string(), "ok".to_string());
        }
        return ("новый расход".to_string(), "warning".to_string());
    }
    let delta = ((latest - avg) / avg) * 100.0;
    let level = if delta >= 50.0 {
        "danger"
    } else if delta >= 15.0 {
        "warning"
    } else {
        "ok"
    };
    let sign = if delta >= 0.0 { "+" } else { "" };
    (
        format!("{sign}{}% к норме", one_decimal_local(delta)),
        level.to_string(),
    )
}

fn nearest_reset(windows: &[ng::WindowState]) -> (String, i32) {
    let nearest = windows
        .iter()
        .filter_map(|window| {
            window
                .reset_in_seconds
                .map(|seconds| (window.key, seconds.max(0)))
        })
        .min_by_key(|(_, seconds)| *seconds);

    match nearest {
        Some((key, seconds)) => (
            format!("сброс {key} окна"),
            seconds.min(i32::MAX as i64) as i32,
        ),
        None => ("сброс окна".to_string(), -1),
    }
}

fn format_overlay_countdown(seconds: i32) -> String {
    if seconds < 0 {
        "unknown".to_string()
    } else {
        ng::format_duration_secs(seconds as i64)
    }
}

fn parse_rate_value(text: &str) -> Option<f32> {
    text.split_whitespace()
        .next()
        .and_then(|value| value.replace(',', ".").parse::<f32>().ok())
}

fn one_decimal_local(value: f32) -> String {
    format!("{value:.1}").replace('.', ",")
}

fn runtime_config(
    dotenv: &HashMap<String, String>,
    account: &Arc<Mutex<Option<GuiAccount>>>,
    auto_failover: bool,
) -> ng::RuntimeConfig {
    let acct = account.lock().unwrap();
    let (api_key_env, api_base_override) = match acct.as_ref() {
        Some(a) => (
            a.api_key_env.as_deref().unwrap_or("VIBEMODE_API_KEY"),
            a.api_base.clone(),
        ),
        None => ("VIBEMODE_API_KEY", None),
    };
    ng::RuntimeConfig {
        api_base: api_base_override
            .or_else(|| ng::config_value("VIBEMODE_API_BASE", dotenv))
            .unwrap_or_else(|| ng::DEFAULT_API_BASE.to_string()),
        api_key: ng::config_value(api_key_env, dotenv).unwrap_or_default(),
        abtop_bin: ng::config_value("ABTOP_BIN", dotenv).unwrap_or_default(),
        auto_failover,
    }
}

fn endpoint_for_label(label: &str, configured_api_base: &str) -> String {
    match label {
        "api" => ng::DEFAULT_API_BASE.to_string(),
        "r-api" => ng::FALLBACK_API_BASE.to_string(),
        _ => configured_api_base.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            endpoint_for_label("api", "https://custom.example"),
            ng::DEFAULT_API_BASE
        );
        assert_eq!(
            endpoint_for_label("r-api", "https://custom.example"),
            ng::FALLBACK_API_BASE
        );
        assert_eq!(
            endpoint_for_label("custom", "https://custom.example"),
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
