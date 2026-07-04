mod helpers;
mod render;
mod state;
#[cfg(test)]
mod text;

#[cfg(test)]
mod tests;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::collections::HashMap;
use std::io;
use std::time::{Duration, Instant};

use crate::{self as ng, cli::cache::CacheStore};

use super::accounts::AccountConfig;
use super::args::Args;
use super::notify::Notifier;
use super::trends::TrendStore;
use render::draw_frame;
pub use state::{PanelState, StatusSnapshot, collect_status};
use state::{TerminalGuard, WindowHistory, load_config, monitor_interval};

#[cfg(test)]
pub use text::{
    fit_text, hbar, panel_bottom, panel_line, panel_top, render_monitor, render_monitor_lines,
};

pub fn run_monitor(
    args: &Args,
    notifier: &mut Notifier,
    account_names: &[String],
    account_configs: &[AccountConfig],
    initial_account_idx: usize,
    trends: Option<&TrendStore>,
    cache: Option<&CacheStore>,
) -> Result<i32, String> {
    let interval_secs = monitor_interval(args);
    let interval = Duration::from_secs(interval_secs);

    crossterm::terminal::enable_raw_mode()
        .map_err(|error| format!("cannot enable terminal raw mode: {error}"))?;
    let mut stdout = io::stdout();
    crossterm::execute!(stdout, crossterm::terminal::EnterAlternateScreen)
        .map_err(|error| format!("cannot enter alternate screen: {error}"))?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal =
        Terminal::new(backend).map_err(|error| format!("cannot initialize terminal: {error}"))?;

    let _guard = TerminalGuard;

    let has_accounts = !account_names.is_empty();
    let total_accounts = account_names.len();
    let mut cur_account = if has_accounts {
        initial_account_idx.min(total_accounts - 1)
    } else {
        0
    };

    let mut last_snapshot = None::<StatusSnapshot>;
    let mut last_error = None::<String>;
    let mut force_refresh = true;
    let mut next_refresh = Instant::now();
    let mut window_history: HashMap<&str, WindowHistory> = HashMap::new();
    let mut trend_days = Vec::new();
    let http = ng::HttpClient::new(ng::USER_AGENT)?;
    let mut panels = PanelState {
        vpn_mode: args.vpn,
        ..PanelState::default()
    };

    let mut account: Option<&AccountConfig> = None;
    let mut router = ng::Router::new(
        ng::DEFAULT_API_BASE.to_string(),
        ng::api_fallbacks_for(ng::DEFAULT_API_BASE, args.auto_failover),
    );
    let mut daily_file = crate::cli::daily::DailyFile::load();

    loop {
        let now = Instant::now();
        if force_refresh || now >= next_refresh {
            account = if has_accounts && total_accounts > 1 {
                Some(&account_configs[cur_account])
            } else {
                None
            };
            match load_config(args, account, panels.vpn_mode).and_then(|config| {
                collect_status(
                    args,
                    &config,
                    &http,
                    cache,
                    Some(&mut router),
                    &mut daily_file,
                )
            }) {
                Ok(snapshot) => {
                    if let Some(store) = trends {
                        let _ = store.save_snapshot(&snapshot.windows, snapshot.fetched_at);
                        if let Ok(days) = store.query_trends(30) {
                            trend_days = days;
                        }
                    }
                    for window in &snapshot.windows {
                        let hist = window_history
                            .entry(window.key)
                            .or_insert_with(WindowHistory::new);
                        let peak =
                            ng::peak_percent(window.credits.as_ref(), window.requests.as_ref())
                                .unwrap_or(0.0);
                        hist.record(peak);
                    }
                    notifier.check_windows(&snapshot.windows);
                    last_snapshot = Some(snapshot);
                    last_error = None;
                }
                Err(error) => {
                    last_error = Some(error);
                }
            }
            next_refresh = Instant::now() + interval;
            force_refresh = false;
        }

        let next_refresh_secs = next_refresh
            .saturating_duration_since(Instant::now())
            .as_secs();

        terminal
            .draw(|frame| {
                draw_frame(
                    frame,
                    last_snapshot.as_ref(),
                    last_error.as_deref(),
                    interval_secs,
                    next_refresh_secs,
                    args.with_abtop,
                    args.warning_threshold,
                    &window_history,
                    args.preset,
                    args.theme,
                    &panels,
                    account_names,
                    cur_account,
                    &trend_days,
                );
            })
            .map_err(|error| format!("cannot draw terminal frame: {error}"))?;

        if event::poll(Duration::from_millis(200))
            .map_err(|error| format!("cannot read terminal events: {error}"))?
        {
            match event::read().map_err(|error| format!("cannot read terminal event: {error}"))? {
                Event::Key(key) if key.kind != KeyEventKind::Release => match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(0),
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Ok(130);
                    }
                    KeyCode::Char('r') => {
                        if let Some(store) = cache
                            && let Ok(config) = load_config(args, account, panels.vpn_mode)
                        {
                            let _ = store.remove(&config.api_key, &config.api_base);
                        }
                        force_refresh = true;
                    }
                    KeyCode::Char('?') => panels.show_help = !panels.show_help,
                    KeyCode::Char('1') => panels.toggle(1),
                    KeyCode::Char('2') => panels.toggle(2),
                    KeyCode::Char('3') => panels.toggle(3),
                    KeyCode::Char('4') => panels.toggle(4),
                    KeyCode::Char('5') => panels.toggle(5),
                    KeyCode::Tab if has_accounts && total_accounts > 1 => {
                        cur_account = (cur_account + 1) % total_accounts;
                        force_refresh = true;
                    }
                    KeyCode::Char('p') => {
                        panels.vpn_mode = !panels.vpn_mode;
                        force_refresh = true;
                    }
                    _ => {}
                },
                Event::Resize(_, _) => {}
                _ => {}
            }
        }
    }
}
