use std::collections::HashMap;
use std::path::PathBuf;

use crate::VERSION;

use super::constants;
use super::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailOn {
    Never,
    Warning,
    Danger,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    Human,
    Json,
    Compact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    Full,
    Compact,
    Mini,
}

#[derive(Debug)]
pub struct Args {
    pub api_base: Option<String>,
    pub api_key_env: String,
    pub env_file: Option<PathBuf>,
    pub demo: bool,
    pub mock: Option<String>,
    pub output: OutputMode,
    pub monitor: bool,
    pub preset: Preset,
    pub theme: Theme,
    pub with_abtop: bool,
    pub notify: bool,
    pub watch: u64,
    pub fail_on: FailOn,
    pub warning_threshold: f64,
    pub danger_threshold: f64,
    pub window_thresholds: HashMap<String, (f64, f64)>,
    pub account: Option<String>,
    pub list_accounts: bool,
    pub doctor: bool,
    pub init: bool,
    pub mcp: bool,
    pub vpn: bool,
    pub auto_failover: bool,
    pub no_cache: bool,
    pub trend: bool,
    pub trend_days: u64,
    pub update: bool,
    pub update_check: bool,
    pub help: bool,
    pub version: bool,
    pub config: Option<PathBuf>,
    pub daily_limit: Option<f64>,
    pub overlay: bool,
}

pub fn parse_args<I>(args: I) -> Result<Args, String>
where
    I: IntoIterator<Item = String>,
{
    let mut parsed = Args {
        api_base: None,
        api_key_env: constants::DEFAULT_API_KEY_ENV.to_string(),
        env_file: None,
        demo: false,
        mock: None,
        output: OutputMode::Human,
        monitor: false,
        preset: Preset::Full,
        theme: Theme::Btop,
        with_abtop: false,
        notify: false,
        watch: 0,
        fail_on: FailOn::Never,
        warning_threshold: constants::DEFAULT_WARNING_THRESHOLD,
        danger_threshold: constants::DEFAULT_DANGER_THRESHOLD,
        window_thresholds: HashMap::new(),
        account: None,
        list_accounts: false,
        doctor: false,
        init: false,
        mcp: false,
        vpn: false,
        auto_failover: true,
        no_cache: false,
        trend: false,
        trend_days: 30,
        update: false,
        update_check: false,
        help: false,
        version: false,
        config: None,
        daily_limit: None,
        overlay: false,
    };

    let mut iter = args.into_iter().peekable();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => parsed.help = true,
            "-V" | "--version" => parsed.version = true,
            "--daily-limit" => {
                let value = next_value(&mut iter, "--daily-limit")?;
                parsed.daily_limit = Some(
                    value
                        .parse::<f64>()
                        .map_err(|_| "--daily-limit must be a number".to_string())?,
                );
            }
            "--config" => parsed.config = Some(PathBuf::from(next_value(&mut iter, "--config")?)),
            "--account" => {
                parsed.account = Some(next_value(&mut iter, "--account")?);
            }
            "--list-accounts" => parsed.list_accounts = true,
            "--doctor" => parsed.doctor = true,
            "--init" => parsed.init = true,
            "--mcp" => parsed.mcp = true,
            "--vpn" => parsed.vpn = true,
            "--no-failover" => parsed.auto_failover = false,
            "--no-cache" => parsed.no_cache = true,
            "--trend" => parsed.trend = true,
            "--days" => {
                let value = next_value(&mut iter, "--days")?;
                parsed.trend_days = value
                    .parse::<u64>()
                    .map_err(|_| "--days must be a positive integer".to_string())?;
                if parsed.trend_days == 0 {
                    return Err("--days must be a positive integer".to_string());
                }
            }
            "--demo" => parsed.demo = true,
            "--overlay" => parsed.overlay = true,
            "--json" => parsed.output = set_output_mode(parsed.output, OutputMode::Json)?,
            "--compact" => parsed.output = set_output_mode(parsed.output, OutputMode::Compact)?,
            "--monitor" => parsed.monitor = true,
            "--preset" => {
                parsed.preset = match next_value(&mut iter, "--preset")?.as_str() {
                    "full" => Preset::Full,
                    "compact" => Preset::Compact,
                    "mini" => Preset::Mini,
                    other => {
                        return Err(format!(
                            "--preset must be one of: full, compact, mini; got {other}"
                        ));
                    }
                };
            }
            "--theme" => {
                let name = next_value(&mut iter, "--theme")?;
                parsed.theme = Theme::from_name(&name).ok_or_else(|| {
                    let valid: Vec<&str> = Theme::all().iter().map(|t| t.name()).collect();
                    format!("--theme must be one of: {}; got {name}", valid.join(", "))
                })?;
            }
            "--with-abtop" => parsed.with_abtop = true,
            "--notify" => parsed.notify = true,
            "--api-base" => parsed.api_base = Some(next_value(&mut iter, "--api-base")?),
            "--api-key-env" => parsed.api_key_env = next_value(&mut iter, "--api-key-env")?,
            "--env-file" => {
                parsed.env_file = Some(PathBuf::from(next_value(&mut iter, "--env-file")?))
            }
            "--mock" => parsed.mock = Some(next_value(&mut iter, "--mock")?),
            "--warning" => {
                parsed.warning_threshold =
                    parse_percent(&next_value(&mut iter, "--warning")?, "--warning")?;
            }
            "--danger" => {
                parsed.danger_threshold =
                    parse_percent(&next_value(&mut iter, "--danger")?, "--danger")?;
            }
            "--threshold" => {
                let value = next_value(&mut iter, "--threshold")?;
                parsed.window_thresholds = parse_window_thresholds(&value)?;
            }
            "--watch" => {
                let value = next_value(&mut iter, "--watch")?;
                parsed.watch = value.parse::<u64>().map_err(|_| {
                    "--watch must be a non-negative integer number of seconds".to_string()
                })?;
            }
            "--fail-on" => {
                parsed.fail_on = match next_value(&mut iter, "--fail-on")?.as_str() {
                    "never" => FailOn::Never,
                    "warning" => FailOn::Warning,
                    "danger" => FailOn::Danger,
                    other => {
                        return Err(format!(
                            "--fail-on must be one of: never, warning, danger; got {other}"
                        ));
                    }
                };
            }
            "update" => {
                parsed.update = true;
                if let Some(next) = iter.peek() {
                    if next == "--check" {
                        parsed.update_check = true;
                        let _ = iter.next();
                    }
                }
            }
            other => {
                return Err(format!(
                    "unknown argument: {other}\n  hint: use --help to see available options"
                ));
            }
        }
    }

    if parsed.demo && parsed.mock.is_some() {
        return Err("--demo and --mock are mutually exclusive".to_string());
    }
    if parsed.monitor && parsed.output != OutputMode::Human {
        return Err("--monitor cannot be combined with --json or --compact".to_string());
    }
    if parsed.warning_threshold >= parsed.danger_threshold {
        return Err("--warning must be lower than --danger".to_string());
    }
    Ok(parsed)
}

fn set_output_mode(current: OutputMode, next: OutputMode) -> Result<OutputMode, String> {
    if current != OutputMode::Human && current != next {
        return Err("--json and --compact are mutually exclusive".to_string());
    }
    Ok(next)
}

fn parse_percent(value: &str, option: &str) -> Result<f64, String> {
    let percent = value
        .trim_end_matches('%')
        .parse::<f64>()
        .map_err(|_| format!("{option} must be a percentage number"))?;
    if !(0.0..=100.0).contains(&percent) {
        return Err(format!("{option} must be between 0 and 100"));
    }
    Ok(percent)
}

fn next_value<I>(iter: &mut I, option: &str) -> Result<String, String>
where
    I: Iterator<Item = String>,
{
    iter.next()
        .ok_or_else(|| format!("{option} requires a value"))
}

pub fn parse_window_thresholds(value: &str) -> Result<HashMap<String, (f64, f64)>, String> {
    let mut result = HashMap::new();
    for entry in value.split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        let (key, thresholds) = entry
            .split_once('=')
            .ok_or_else(|| format!("--threshold format: KEY=WARNING[:DANGER], got '{entry}'"))?;
        let key = key.trim().to_string();
        if !["5h", "24h", "7d", "30d"].contains(&key.as_str()) {
            return Err(format!(
                "--threshold key must be one of: 5h, 24h, 7d, 30d; got '{key}'"
            ));
        }
        let parts: Vec<&str> = thresholds.split(':').collect();
        let warning = parse_percent(parts[0], "--threshold")?;
        let danger = if parts.len() > 1 {
            parse_percent(parts[1], "--threshold")?
        } else {
            90.0
        };
        if warning >= danger {
            return Err(format!(
                "--threshold: warning ({warning}) must be lower than danger ({danger}) for {key}"
            ));
        }
        result.insert(key, (warning, danger));
    }
    Ok(result)
}

pub fn print_help() {
    println!(
        "\
vimit {VERSION}

Safe VibeMode quota monitor for Codex/Droid workflows.

USAGE:
  vimit [OPTIONS]

OPTIONS:
      --demo                 Use built-in demo data without a key or network
      --mock <PATH>          Read a saved /v1/me JSON payload instead of calling VibeMode
      --json                 Print machine-readable JSON
      --compact              Print one-line output for widgets/status bars
      --monitor              Full-screen live dashboard, abtop-style
      --preset <LAYOUT>      Monitor layout: full (default), compact, mini
      --theme <THEME>        Color theme: btop (default), dracula, catppuccin, tokyo-night,
                             gruvbox, nord, high-contrast, protanopia, deuteranopia,
                             tritanopia, solarized, monokai
      --with-abtop           Merge local abtop --status-json output if available
      --notify               Desktop alert when a window enters warning/danger
      --watch <SECONDS>      Poll every N seconds; default is 5 in --monitor
      --fail-on <LEVEL>      Exit non-zero on threshold: never, warning, danger
      --warning <PCT>        Warning threshold percentage [default: 75]
      --danger <PCT>         Danger threshold percentage [default: 90]
      --threshold <SPEC>     Per-window thresholds, e.g. 5h=80:95,7d=90
                             Format: KEY=WARNING[:DANGER] where KEY is 5h,24h,7d,30d
      --daily-limit <N>      Set daily credit limit (0 = auto calculate based on 7d limit)
      --env-file <PATH>      Load .env file explicitly
      --config <PATH>        Load config file (default: ~/.config/vimit/config.toml)
      --account <NAME>       Use account profile from accounts.toml
      --list-accounts        List available account profiles
      --doctor               Run system diagnostics
      --init                 Interactive setup wizard
      --mcp                  Start MCP server on stdio
      --trend                Show 30-day usage trends (requires saved snapshots)
      --days <N>             Days of trend history to show [default: 30]
  
  Commands:
      update                 Update vimit to the latest version from GitHub Releases
      update --check         Check for updates without installing
      --api-base <URL>       API base URL [env: VIBEMODE_API_BASE]
      --api-key-env <NAME>   API key environment variable [default: VIBEMODE_API_KEY]
      --vpn                  Switch to fallback endpoint (r-api.vibemod.pro)
      --no-failover          Disable automatic api/r-api endpoint failover
  -V, --version              Print version
  -h, --help                 Print help

.env lookup:
  1. --env-file <PATH>
  2. .env in the current directory
  3. .env next to the vimit executable
"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monitor_rejects_machine_output_modes() {
        let error = parse_args(["--monitor".to_string(), "--json".to_string()]).unwrap_err();
        assert!(error.contains("--monitor cannot be combined"));
    }

    #[test]
    fn notify_flag_is_parsed() {
        let args = parse_args([
            "--notify".to_string(),
            "--watch".to_string(),
            "60".to_string(),
        ])
        .unwrap();
        assert!(args.notify);
        assert_eq!(args.watch, 60);
        assert!(args.auto_failover);
    }

    #[test]
    fn no_failover_disables_auto_endpoint_switching() {
        let args = parse_args(["--no-failover".to_string()]).unwrap();
        assert!(!args.auto_failover);
    }

    #[test]
    fn demo_and_mock_are_mutually_exclusive() {
        let error = parse_args([
            "--demo".to_string(),
            "--mock".to_string(),
            "test.json".to_string(),
        ])
        .unwrap_err();
        assert!(error.contains("--demo and --mock"));
    }

    #[test]
    fn warning_must_be_lower_than_danger() {
        let error = parse_args([
            "--warning".to_string(),
            "90".to_string(),
            "--danger".to_string(),
            "80".to_string(),
        ])
        .unwrap_err();
        assert!(error.contains("--warning must be lower"));
    }

    #[test]
    fn unknown_argument_is_rejected() {
        let error = parse_args(["--unknown".to_string()]).unwrap_err();
        assert!(error.contains("unknown argument:"));
        assert!(error.contains("--help"));
    }

    #[test]
    fn fail_on_parses_valid_values() {
        let args = parse_args(["--fail-on".to_string(), "danger".to_string()]).unwrap();
        assert!(matches!(args.fail_on, FailOn::Danger));
    }

    #[test]
    fn fail_on_rejects_invalid_value() {
        let error = parse_args(["--fail-on".to_string(), "invalid".to_string()]).unwrap_err();
        assert!(error.contains("--fail-on must be one of"));
    }

    #[test]
    fn watch_rejects_non_integer() {
        let error = parse_args(["--watch".to_string(), "abc".to_string()]).unwrap_err();
        assert!(error.contains("--watch must be a non-negative"));
    }

    #[test]
    fn window_thresholds_parsed_correctly() {
        let args =
            parse_args(["--threshold".to_string(), "5h=80:95,7d=85:95".to_string()]).unwrap();
        assert_eq!(args.window_thresholds.get("5h"), Some(&(80.0, 95.0)));
        assert_eq!(args.window_thresholds.get("7d"), Some(&(85.0, 95.0)));
    }

    #[test]
    fn window_threshold_rejects_invalid_key() {
        let error = parse_args(["--threshold".to_string(), "1h=80".to_string()]).unwrap_err();
        assert!(error.contains("must be one of: 5h, 24h, 7d, 30d"));
    }

    #[test]
    fn window_threshold_rejects_bad_format() {
        let error = parse_args(["--threshold".to_string(), "5h80".to_string()]).unwrap_err();
        assert!(error.contains("format"));
    }

    #[test]
    fn preset_parses_valid_values() {
        let args = parse_args(["--preset".to_string(), "compact".to_string()]).unwrap();
        assert!(matches!(args.preset, Preset::Compact));
    }

    #[test]
    fn preset_rejects_invalid_value() {
        let error = parse_args(["--preset".to_string(), "wide".to_string()]).unwrap_err();
        assert!(error.contains("--preset must be one of"));
    }
}
