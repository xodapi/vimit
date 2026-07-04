use serde_json::Value;
use std::collections::VecDeque;
use std::io;
use std::time::Instant;

use crate::{self as ng, cli::cache::CacheStore};

use super::super::accounts::AccountConfig;
use super::super::args::Args;

const SPARKLINE_LEN: usize = 20;

#[derive(Debug, Clone)]
pub struct PanelState {
    pub show_header: bool,
    pub show_quota: bool,
    pub show_alerts: bool,
    pub show_agents: bool,
    pub show_trends: bool,
    pub show_footer: bool,
    pub show_help: bool,
    pub vpn_mode: bool,
}

impl Default for PanelState {
    fn default() -> Self {
        Self {
            show_header: true,
            show_quota: true,
            show_alerts: true,
            show_agents: true,
            show_trends: false,
            show_footer: true,
            show_help: false,
            vpn_mode: false,
        }
    }
}

impl PanelState {
    pub fn toggle(&mut self, panel: u8) {
        match panel {
            1 => self.show_header = !self.show_header,
            2 => self.show_quota = !self.show_quota,
            3 => self.show_alerts = !self.show_alerts,
            4 => self.show_agents = !self.show_agents,
            5 => self.show_trends = !self.show_trends,
            6 => self.show_footer = !self.show_footer,
            _ => {}
        }
    }
}

#[allow(dead_code)]
pub struct StatusSnapshot {
    pub windows: Vec<ng::WindowState>,
    pub abtop: Option<Value>,
    pub daily: Option<crate::DailyState>,
    pub fetched_at: chrono::DateTime<chrono::Utc>,
    pub stale: bool,
    pub latency_ms: u64,
    pub api_endpoint: String,
    pub offline_duration_min: Option<u64>,
}

pub fn collect_status(
    args: &Args,
    config: &ng::RuntimeConfig,
    http: &ng::HttpClient,
    cache: Option<&CacheStore>,
    mut router: Option<&mut ng::Router>,
    daily_file: &mut crate::cli::daily::DailyFile,
) -> Result<StatusSnapshot, String> {
    let fetch_result: Result<(serde_json::Value, u64, String), String> = if args.demo {
        Ok((ng::demo_payload(), 0, "demo".to_string()))
    } else if let Some(path) = &args.mock {
        ng::load_mock(path).map(|v| (v, 0, "mock".to_string()))
    } else {
        let start = Instant::now();
        let result = if config.auto_failover
            && config.api_base.trim_end_matches('/') == ng::DEFAULT_API_BASE
            && let Some(ref mut r) = router
        {
            http.fetch_me_with_retry(&config.api_key, r, &config.api_base)
        } else {
            let mut fallback_router = ng::Router::new(
                config.api_base.clone(),
                ng::api_fallbacks_for(&config.api_base, config.auto_failover),
            );
            http.fetch_me_with_retry(&config.api_key, &mut fallback_router, &config.api_base)
        };
        let elapsed = start.elapsed().as_millis() as u64;
        match result {
            Ok((payload, label)) => {
                if let Some(store) = cache {
                    let _ = store.set(&config.api_key, &config.api_base, &payload);
                }
                Ok((payload, elapsed, label))
            }
            Err(error) => {
                if !args.no_cache
                    && let Some(store) = cache
                    && let Some((cached, _when)) = store.get(&config.api_key, &config.api_base)
                {
                    let label = router
                        .as_ref()
                        .map(|r| r.active_label().to_string())
                        .unwrap_or_else(|| "cached".to_string());
                    let mut snapshot = StatusSnapshot {
                        stale: true,
                        latency_ms: elapsed,
                        api_endpoint: label,
                        ..snapshot_from_payload(args, config, cached)
                    };
                    if let Some(w7d) = snapshot.windows.clone().iter().find(|w| w.key == "7d")
                        && let Some(c) = w7d.credits.as_ref()
                    {
                        let daily_state = daily_file.get_state(c.limit, args.daily_limit);
                        snapshot.windows.insert(
                            0,
                            ng::WindowState {
                                key: "today",
                                level: daily_state.level.clone(),
                                reset: "End of Day".to_string(),
                                reset_in_seconds: None,
                                credits: Some(ng::Metric {
                                    used: daily_state.spent_today,
                                    limit: daily_state.daily_limit,
                                    remaining: daily_state.daily_limit - daily_state.spent_today,
                                    percent: daily_state.percent,
                                }),
                                requests: None,
                                percent: daily_state.percent,
                            },
                        );
                        snapshot.daily = Some(daily_state);
                    }
                    return Ok(snapshot);
                }
                Err(error)
            }
        }
    };

    let (payload, latency_ms, api_endpoint) = fetch_result?;
    let mut snapshot = StatusSnapshot {
        stale: false,
        latency_ms,
        api_endpoint,
        ..snapshot_from_payload(args, config, payload)
    };
    if let Some(w7d) = snapshot.windows.clone().iter().find(|w| w.key == "7d")
        && let Some(c) = w7d.credits.as_ref()
    {
        if let Ok(updated_file) = crate::cli::daily::DailyFile::atomic_update(c.remaining) {
            *daily_file = updated_file;
        }
        let daily_state = daily_file.get_state(c.limit, args.daily_limit);

        snapshot.windows.insert(
            0,
            ng::WindowState {
                key: "today",
                level: daily_state.level.clone(),
                reset: "End of Day".to_string(),
                reset_in_seconds: None,
                credits: Some(ng::Metric {
                    used: daily_state.spent_today,
                    limit: daily_state.daily_limit,
                    remaining: daily_state.daily_limit - daily_state.spent_today,
                    percent: daily_state.percent,
                }),
                requests: None,
                percent: daily_state.percent,
            },
        );

        snapshot.daily = Some(daily_state);
    }
    Ok(snapshot)
}

fn snapshot_from_payload(
    args: &Args,
    config: &ng::RuntimeConfig,
    payload: serde_json::Value,
) -> StatusSnapshot {
    let windows = ng::summarize_me_with_thresholds(
        &payload,
        args.warning_threshold,
        args.danger_threshold,
        &args.window_thresholds,
    );
    let abtop = if args.with_abtop {
        ng::read_abtop_status(&config.abtop_bin)
    } else {
        None
    };
    StatusSnapshot {
        windows,
        abtop,
        daily: None,
        fetched_at: chrono::Utc::now(),
        stale: false,
        latency_ms: 0,
        api_endpoint: "r-api".to_string(),
        offline_duration_min: ng::get_offline_duration_min(),
    }
}

pub(super) fn load_config(
    args: &Args,
    account: Option<&AccountConfig>,
    vpn_mode: bool,
) -> Result<ng::RuntimeConfig, String> {
    let (api_base, api_key_env) = match account {
        Some(acct) => (
            acct.api_base.clone().or_else(|| args.api_base.clone()),
            acct.api_key_env.as_deref().unwrap_or(&args.api_key_env),
        ),
        None => {
            let base = if vpn_mode {
                args.api_base
                    .clone()
                    .or_else(|| Some(ng::VPN_API_BASE.to_string()))
            } else {
                args.api_base.clone()
            };
            (base, args.api_key_env.as_str())
        }
    };
    let mut config = ng::RuntimeConfig::from_dotenv(api_base, api_key_env, args.env_file.as_ref())?;
    config.auto_failover = args.auto_failover;
    Ok(config)
}

pub(super) fn monitor_interval(args: &Args) -> u64 {
    if args.watch == 0 {
        super::super::constants::DEFAULT_WATCH_INTERVAL
    } else {
        args.watch.max(1)
    }
}

pub(super) struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(io::stdout(), crossterm::terminal::LeaveAlternateScreen);
    }
}

pub(super) struct WindowHistory {
    values: VecDeque<f64>,
}

impl WindowHistory {
    pub(super) fn new() -> Self {
        Self {
            values: VecDeque::with_capacity(SPARKLINE_LEN),
        }
    }

    pub(super) fn record(&mut self, peak: f64) {
        if self.values.len() >= SPARKLINE_LEN {
            self.values.pop_front();
        }
        self.values.push_back(peak);
    }

    pub(super) fn sparkline_values(&self) -> Vec<u64> {
        self.values.iter().map(|v| *v as u64).collect()
    }
}
