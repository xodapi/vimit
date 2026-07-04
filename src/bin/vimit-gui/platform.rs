use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use serde_json::Value;

use crate::ng;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub(crate) fn suppress_windows_console(command: &mut Command) {
    #[cfg(windows)]
    {
        command.creation_flags(CREATE_NO_WINDOW);
    }
}

pub(crate) fn spawn_mini_overlay(force_demo: bool) -> Result<(), String> {
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

pub(crate) fn ensure_config_dir() -> Result<PathBuf, String> {
    let dir = config_dir_path()?;
    std::fs::create_dir_all(&dir)
        .map_err(|error| format!("cannot create config directory {}: {error}", dir.display()))?;
    Ok(dir)
}

pub(crate) fn ensure_env_template() -> Result<PathBuf, String> {
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

pub(crate) fn ensure_accounts_template() -> Result<PathBuf, String> {
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

pub(crate) fn open_path(path: &Path) -> Result<(), String> {
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

pub(crate) fn read_agent_status_for_gui(binary: &str) -> ng::AgentStatus {
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
