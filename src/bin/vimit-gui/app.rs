use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use slint::{ComponentHandle, PhysicalPosition, PhysicalSize, SharedString, Timer, TimerMode};
use tray_icon::{MouseButton, TrayIconEvent, menu::MenuEvent};

use crate::AppWindow;
use crate::config::{GuiAccount, gui_load_dotenv, load_gui_accounts, runtime_config};
use crate::dashboard::start_refresh;
use crate::ng;
use crate::overlay::{
    OverlayHistory, creature_node_count_for_skin, creature_path_commands_for_skin,
    creature_state_from_str, overlay_phase_step_for_activity, set_overlay_window_size,
};
use crate::platform::{
    ensure_accounts_template, ensure_config_dir, ensure_env_template, open_path, spawn_mini_overlay,
};
use crate::tray::try_init_tray;

pub(crate) fn run() {
    let args: Vec<String> = std::env::args().collect();
    let is_overlay = args.iter().any(|arg| arg == "--overlay");
    let is_compact = args.iter().any(|arg| arg == "--compact");
    let force_demo = args.iter().any(|arg| arg == "--demo");
    let mock_path = Arc::new(
        args.iter()
            .position(|arg| arg == "--mock")
            .and_then(|index| args.get(index + 1))
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
            let next = (app.get_overlay_pulse_phase()
                + overlay_phase_step_for_activity(
                    state,
                    app.get_overlay_activity_energy(),
                    app.get_overlay_activity_burst(),
                ))
                % 360.0;
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
                app.set_overlay_reset_text(crate::overlay::format_overlay_countdown(next).into());
            }
        }
    });

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

    let http =
        Arc::new(ng::HttpClient::new(ng::USER_AGENT_GUI).expect("cannot initialize HTTP client"));

    let auto_api_failover = Arc::new(AtomicBool::new(
        ng::cli::update::is_auto_api_failover_enabled(),
    ));
    let router = Arc::new(Mutex::new(ng::Router::new(
        ng::DEFAULT_API_BASE.to_string(),
        ng::api_fallbacks_for(
            ng::DEFAULT_API_BASE,
            auto_api_failover.load(Ordering::Relaxed),
        ),
    )));

    let refresh_gen = Arc::new(AtomicU64::new(0));
    let is_refreshing = Arc::new(AtomicBool::new(false));
    let overlay_history = Arc::new(Mutex::new(OverlayHistory::default()));
    let activity_tracker = Arc::new(Mutex::new(ng::ActivityTracker::default()));
    let demo_mode = Arc::new(AtomicBool::new(force_demo));

    let (account_names, account_configs) = load_gui_accounts();
    let current_acct = Arc::new(Mutex::new(None::<GuiAccount>));

    if !account_names.is_empty() {
        let items: Vec<SharedString> = account_names
            .iter()
            .map(|name| name.as_str().into())
            .collect();
        let model = slint::ModelRc::new(std::rc::Rc::new(slint::VecModel::from(items)));
        app.set_account_names(model);
        app.set_current_account(account_names[0].as_str().into());
        {
            let mut current = current_acct.lock().unwrap();
            *current = Some(account_configs[0].clone());
        }

        let state = current_acct.clone();
        let configs = account_configs.clone();
        let weak_app = app.as_weak();
        app.on_account_changed(move |name| {
            if let Some(index) = account_names
                .iter()
                .position(|candidate| candidate == name.as_str())
            {
                {
                    let mut current = state.lock().unwrap();
                    *current = Some(configs[index].clone());
                }
                if let Some(app) = weak_app.upgrade() {
                    app.invoke_refresh_requested();
                }
            }
        });
    }

    let weak = app.as_weak();
    let http_clone = http.clone();
    let account = current_acct.clone();
    let router_clone = router.clone();
    let generation = refresh_gen.clone();
    let refreshing = is_refreshing.clone();
    let history = overlay_history.clone();
    let activity = activity_tracker.clone();
    let demo_mode_refresh = demo_mode.clone();
    let mock_path_refresh = mock_path.clone();
    let failover_refresh = auto_api_failover.clone();
    app.on_refresh_requested(move || {
        start_refresh(
            weak.clone(),
            demo_mode_refresh.load(Ordering::Relaxed),
            mock_path_refresh.clone(),
            http_clone.clone(),
            account.clone(),
            router_clone.clone(),
            ng::DEFAULT_WARNING_THRESHOLD,
            ng::DEFAULT_DANGER_THRESHOLD,
            generation.clone(),
            refreshing.clone(),
            history.clone(),
            activity.clone(),
            failover_refresh.clone(),
        );
    });

    let weak = app.as_weak();
    let http_clone = http.clone();
    let account = current_acct.clone();
    let router_clone = router.clone();
    let generation = refresh_gen.clone();
    let refreshing = is_refreshing.clone();
    let history = overlay_history.clone();
    let activity = activity_tracker.clone();
    let demo_mode_refresh = demo_mode.clone();
    let mock_path_refresh = mock_path.clone();
    let failover_refresh = auto_api_failover.clone();
    app.on_demo_requested(move || {
        demo_mode_refresh.store(true, Ordering::Relaxed);
        start_refresh(
            weak.clone(),
            true,
            mock_path_refresh.clone(),
            http_clone.clone(),
            account.clone(),
            router_clone.clone(),
            ng::DEFAULT_WARNING_THRESHOLD,
            ng::DEFAULT_DANGER_THRESHOLD,
            generation.clone(),
            refreshing.clone(),
            history.clone(),
            activity.clone(),
            failover_refresh.clone(),
        );
    });

    let weak = app.as_weak();
    let http_clone = http.clone();
    let account = current_acct.clone();
    let router_clone = router.clone();
    let generation = refresh_gen.clone();
    let refreshing = is_refreshing.clone();
    let history = overlay_history.clone();
    let activity = activity_tracker.clone();
    let demo_mode_refresh = demo_mode.clone();
    let mock_path_refresh = mock_path.clone();
    let failover_refresh = auto_api_failover.clone();
    app.on_settings_changed(move |warning, danger| {
        start_refresh(
            weak.clone(),
            demo_mode_refresh.load(Ordering::Relaxed),
            mock_path_refresh.clone(),
            http_clone.clone(),
            account.clone(),
            router_clone.clone(),
            warning as f64,
            danger as f64,
            generation.clone(),
            refreshing.clone(),
            history.clone(),
            activity.clone(),
            failover_refresh.clone(),
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
    let account = current_acct.clone();
    let router_clone = router.clone();
    let generation = refresh_gen.clone();
    let refreshing = is_refreshing.clone();
    let history = overlay_history.clone();
    let activity = activity_tracker.clone();
    let demo_mode_refresh = demo_mode.clone();
    let mock_path_refresh = mock_path.clone();
    let failover_refresh = auto_api_failover.clone();
    timer.start(TimerMode::Repeated, Duration::from_secs(10), move || {
        start_refresh(
            weak.clone(),
            demo_mode_refresh.load(Ordering::Relaxed),
            mock_path_refresh.clone(),
            http_clone.clone(),
            account.clone(),
            router_clone.clone(),
            ng::DEFAULT_WARNING_THRESHOLD,
            ng::DEFAULT_DANGER_THRESHOLD,
            generation.clone(),
            refreshing.clone(),
            history.clone(),
            activity.clone(),
            failover_refresh.clone(),
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
        activity_tracker.clone(),
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
                    Err(error) => {
                        let _ = weak_clone.upgrade_in_event_loop(move |app| {
                            app.set_status_text(format!("Ошибка проверки: {}", error).into());
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
                Err(error) => {
                    let _ = weak_clone.upgrade_in_event_loop(move |app| {
                        app.set_status_text(format!("Ошибка обновления: {}", error).into());
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
