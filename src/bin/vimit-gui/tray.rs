use std::cell::RefCell;

use tray_icon::{
    Icon, TrayIcon, TrayIconBuilder,
    menu::{Menu, MenuId, MenuItem},
};

use crate::ng;

thread_local! {
    static TRAY_ICON: RefCell<Option<TrayIcon>> = const { RefCell::new(None) };
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
pub(crate) struct TrayIds {
    pub(crate) show: MenuId,
    pub(crate) mini: MenuId,
    pub(crate) quit: MenuId,
}

pub(crate) fn try_init_tray() -> Option<TrayIds> {
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

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TrayStatus {
    pub(crate) color: (u8, u8, u8),
    pub(crate) tooltip: String,
}

pub(crate) fn tray_status_from_dashboard(source: &str, windows: &[ng::WindowState]) -> TrayStatus {
    let max_percent = windows
        .iter()
        .map(|window| window.percent)
        .fold(0.0f64, f64::max);
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

pub(crate) fn update_tray_status(status: TrayStatus) {
    TRAY_ICON.with(|cell| {
        if let Some(ref mut tray) = *cell.borrow_mut() {
            let _ = tray.set_icon(Some(create_status_icon(status.color)));
            let _ = tray.set_tooltip(Some(status.tooltip));
        }
    });
}
