use serde_json::Value;

use crate as ng;

use super::args::{FailOn, OutputMode};
use super::cache::CacheStore;
use super::trends::TrendStore;

pub fn run_once(
    args: &super::args::Args,
    config: &ng::RuntimeConfig,
    notifier: &mut super::notify::Notifier,
    http: &ng::HttpClient,
    trends: Option<&TrendStore>,
    cache: Option<&CacheStore>,
    router: Option<&mut ng::Router>,
) -> Result<i32, String> {
    let mut daily_file = crate::cli::daily::DailyFile::load();
    let snapshot =
        super::monitor::collect_status(args, config, http, cache, router, &mut daily_file)?;
    if let Some(store) = trends {
        let _ = store.save_snapshot(&snapshot.windows, snapshot.fetched_at);
    }
    let status = ng::summary_to_json_with_stale(
        &snapshot.windows,
        snapshot.abtop.as_ref(),
        snapshot.daily.as_ref(),
        snapshot.stale,
        snapshot.latency_ms,
        &snapshot.api_endpoint,
        snapshot.offline_duration_min,
    );

    match args.output {
        OutputMode::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&status)
                    .map_err(|error| format!("cannot render JSON: {error}"))?
            );
        }
        OutputMode::Compact => print_compact(
            &snapshot.windows,
            snapshot.abtop.as_ref(),
            snapshot.stale,
            snapshot.offline_duration_min,
        ),
        OutputMode::Human => print_human(
            &snapshot.windows,
            snapshot.abtop.as_ref(),
            snapshot.stale,
            snapshot.offline_duration_min,
        ),
    }

    notifier.check_windows(&snapshot.windows);
    if args.ci_annotate {
        print_ci_annotations(&snapshot.windows);
    }

    Ok(exit_code(&snapshot.windows, args.fail_on))
}

fn color_level(level: &str) -> String {
    let code = match level {
        "danger" => "31",
        "warning" => "33",
        _ => "32",
    };
    format!("\x1b[{code}m{level}\x1b[0m")
}

fn color_percent(percent: f64) -> String {
    let code = if percent >= 90.0 {
        "31"
    } else if percent >= 75.0 {
        "33"
    } else {
        "32"
    };
    format!("\x1b[{code}m{:.0}%\x1b[0m", percent)
}

pub fn print_human(
    windows: &[ng::WindowState],
    abtop: Option<&Value>,
    stale: bool,
    offline_min: Option<u64>,
) {
    println!("VibeMode limits{}", human_status_suffix(stale, offline_min));
    if windows.is_empty() {
        println!("  usage rows not found in /v1/me response");
    }
    for window in windows {
        let peak = ng::peak_percent(window.credits.as_ref(), window.requests.as_ref());
        let peak_str = peak
            .map(|p| format!(" peak {}", color_percent(p)))
            .unwrap_or_default();
        println!(
            "  {:<4} {} reset {}{}",
            window.key,
            color_level(&window.level),
            ng::format_duration_opt(window.reset_in_seconds),
            peak_str,
        );
        if let Some(metric) = &window.credits {
            println!("       credits  {}", ng::format_metric(metric));
        }
        if let Some(metric) = &window.requests {
            println!("       requests {}", ng::format_metric(metric));
        }
    }

    if let Some(abtop) = abtop {
        if let Some(agents) = abtop.get("agents").and_then(Value::as_array) {
            if !agents.is_empty() {
                println!("\nLocal agents from abtop");
                for agent in agents {
                    println!("  {}", format_agent(agent));
                }
            }
        }
    }
}

pub fn print_compact(
    windows: &[ng::WindowState],
    abtop: Option<&Value>,
    stale: bool,
    offline_min: Option<u64>,
) {
    let mut parts = vec!["NG".to_string()];
    if stale {
        parts.push("stale-cache".to_string());
    }
    if let Some(min) = offline_min {
        parts.push(format!("offline:{}m", min));
    }
    for window in windows {
        let peak = ng::peak_percent(window.credits.as_ref(), window.requests.as_ref())
            .map(|value| format!("{value:.0}%"))
            .unwrap_or_else(|| "n/a".to_string());
        parts.push(format!("{}:{}:{}", window.key, window.level, peak));
    }
    if let Some(abtop) = abtop {
        if let Some(agents) = abtop.get("agents").and_then(Value::as_array) {
            for agent in agents {
                let agent_cli = agent
                    .get("agent_cli")
                    .and_then(Value::as_str)
                    .unwrap_or("agent");
                if let Some(ctx) = agent.get("max_context_pct").and_then(ng::to_number) {
                    parts.push(format!("{agent_cli}:ctx{ctx:.0}%"));
                }
            }
        }
    }
    println!("{}", parts.join(" "));
}

fn human_status_suffix(stale: bool, offline_min: Option<u64>) -> String {
    match (stale, offline_min) {
        (true, Some(min)) => format!(" (stale cache; API offline {min}m)"),
        (true, None) => " (stale cache)".to_string(),
        (false, Some(min)) => format!(" (API offline {min}m)"),
        (false, None) => String::new(),
    }
}

fn format_agent(agent: &Value) -> String {
    let agent_cli = agent
        .get("agent_cli")
        .and_then(Value::as_str)
        .unwrap_or("agent");
    let sessions = ng::value_string(agent.get("sessions")).unwrap_or_else(|| "?".to_string());
    let active = ng::value_string(agent.get("active")).unwrap_or_else(|| "?".to_string());
    let tokens = ng::value_string(agent.get("active_tokens")).unwrap_or_else(|| "?".to_string());
    let context = agent
        .get("max_context_pct")
        .and_then(ng::to_number)
        .map(|value| format!("{value:.0}%"))
        .unwrap_or_else(|| "n/a".to_string());
    format!(
        "{agent_cli:<8} sessions {sessions:<2} active {active:<2} ctx max {context} tokens {tokens}"
    )
}

fn print_ci_annotations(windows: &[ng::WindowState]) {
    for annotation in ci_annotations(windows) {
        eprintln!("{}", annotation);
    }
}

fn ci_annotations(windows: &[ng::WindowState]) -> Vec<String> {
    windows
        .iter()
        .filter_map(|window| match window.level.as_str() {
            "warning" => Some(render_ci_annotation("warning", window)),
            "danger" => Some(render_ci_annotation("error", window)),
            _ => None,
        })
        .collect()
}

fn render_ci_annotation(kind: &str, window: &ng::WindowState) -> String {
    let peak = ng::peak_percent(window.credits.as_ref(), window.requests.as_ref()).unwrap_or(0.0);
    let message = format!(
        "{} window hit {} at {:.1}% (reset {})",
        window.key,
        window.level,
        peak,
        ng::format_duration_opt(window.reset_in_seconds)
    );
    format!(
        "::{} title={}::{}",
        kind,
        escape_github_annotation("vimit threshold breach"),
        escape_github_annotation(&message)
    )
}

fn escape_github_annotation(value: &str) -> String {
    value
        .replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

fn exit_code(windows: &[ng::WindowState], fail_on: FailOn) -> i32 {
    match fail_on {
        FailOn::Never => 0,
        FailOn::Danger => {
            if windows.iter().any(|window| window.level == "danger") {
                3
            } else {
                0
            }
        }
        FailOn::Warning => {
            if windows
                .iter()
                .any(|window| matches!(window.level.as_str(), "warning" | "danger"))
            {
                2
            } else {
                0
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fail_on_warning_catches_warning_and_danger() {
        let windows = vec![
            ng::WindowState {
                key: "5h",
                credits: Some(ng::Metric {
                    used: 10.0,
                    limit: 100.0,
                    remaining: 90.0,
                    percent: 10.0,
                }),
                requests: None,
                reset: "unknown".to_string(),
                reset_in_seconds: None,
                level: "ok".to_string(),
                percent: 10.0,
            },
            ng::WindowState {
                key: "7d",
                credits: Some(ng::Metric {
                    used: 95.0,
                    limit: 100.0,
                    remaining: 5.0,
                    percent: 95.0,
                }),
                requests: None,
                reset: "unknown".to_string(),
                reset_in_seconds: None,
                level: "danger".to_string(),
                percent: 95.0,
            },
        ];

        assert_eq!(exit_code(&windows, FailOn::Warning), 2);
        assert_eq!(exit_code(&windows, FailOn::Danger), 3);
        assert_eq!(exit_code(&windows, FailOn::Never), 0);
    }

    #[test]
    fn ci_annotations_emit_warning_and_error_entries() {
        let windows = vec![
            ng::WindowState {
                key: "5h",
                credits: Some(ng::Metric {
                    used: 39.0,
                    limit: 50.0,
                    remaining: 11.0,
                    percent: 78.0,
                }),
                requests: None,
                reset: "unknown".to_string(),
                reset_in_seconds: Some(3600),
                level: "warning".to_string(),
                percent: 78.0,
            },
            ng::WindowState {
                key: "7d",
                credits: Some(ng::Metric {
                    used: 95.0,
                    limit: 100.0,
                    remaining: 5.0,
                    percent: 95.0,
                }),
                requests: None,
                reset: "unknown".to_string(),
                reset_in_seconds: Some(120),
                level: "danger".to_string(),
                percent: 95.0,
            },
            ng::WindowState {
                key: "30d",
                credits: None,
                requests: None,
                reset: "unknown".to_string(),
                reset_in_seconds: None,
                level: "ok".to_string(),
                percent: 0.0,
            },
        ];

        let annotations = ci_annotations(&windows);

        assert_eq!(annotations.len(), 2);
        assert!(annotations[0].starts_with("::warning "));
        assert!(annotations[0].contains("5h window hit warning at 78.0%"));
        assert!(annotations[1].starts_with("::error "));
        assert!(annotations[1].contains("7d window hit danger at 95.0%"));
    }

    #[test]
    fn ci_annotation_escaping_matches_github_command_rules() {
        let escaped = escape_github_annotation("danger 95%\nnext\rline");

        assert_eq!(escaped, "danger 95%25%0Anext%0Dline");
    }

    #[test]
    fn human_status_suffix_separates_stale_and_offline() {
        assert_eq!(human_status_suffix(false, None), "");
        assert_eq!(human_status_suffix(true, None), " (stale cache)");
        assert_eq!(human_status_suffix(false, Some(3)), " (API offline 3m)");
        assert_eq!(
            human_status_suffix(true, Some(3)),
            " (stale cache; API offline 3m)"
        );
    }
}
