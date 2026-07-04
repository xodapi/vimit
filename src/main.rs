#![allow(clippy::collapsible_if)]

use std::env;
#[cfg(windows)]
use std::io::{self, Write};
use std::thread;
use std::time::Duration;

use vimit::{self as ng, VERSION, cli};

use cli::accounts::AccountsConfig;
use cli::args::{Args, FailOn, parse_args};
use cli::cache::CacheStore;
use cli::config::{Config, MergedConfig};
use cli::monitor::run_monitor;
use cli::notify::Notifier;
use cli::output::run_once;
use cli::trends::{TrendStore, print_trends_human, print_trends_json};

fn main() {
    let code = match real_main() {
        Ok(code) => code,
        Err(message) => {
            let message = enhance_error(&message);
            eprintln!("vimit: {message}");
            2
        }
    };
    pause_before_exit_if_own_console();
    std::process::exit(code);
}

#[cfg(windows)]
fn pause_before_exit_if_own_console() {
    if windows_console_process_count() <= 1 {
        eprintln!();
        eprint!("Press Enter to exit...");
        let _ = io::stderr().flush();
        let mut line = String::new();
        let _ = io::stdin().read_line(&mut line);
    }
}

#[cfg(not(windows))]
fn pause_before_exit_if_own_console() {}

#[cfg(windows)]
fn windows_console_process_count() -> u32 {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetConsoleProcessList(process_list: *mut u32, process_count: u32) -> u32;
    }

    let mut processes = [0_u32; cli::constants::MAX_CONSOLE_PROCESSES];
    unsafe { GetConsoleProcessList(processes.as_mut_ptr(), processes.len() as u32) }
}

fn enhance_error(msg: &str) -> String {
    if msg.contains("cannot reach VibeMode API") {
        format!("{msg}\n  hint: check your internet connection, VPN, and VIBEMODE_API_BASE")
    } else if msg.contains("cannot read env file") {
        format!("{msg}\n  hint: create a .env file or use --demo")
    } else if msg.contains("cannot read mock payload") {
        format!("{msg}\n  hint: check the path passed to --mock")
    } else if msg.contains("mock payload is invalid") {
        format!("{msg}\n  hint: the --mock file must be a valid JSON object")
    } else if msg.contains("cannot initialize HTTP client") {
        format!(
            "{}\n  hint: check your system's TLS/CA certificates or proxy settings",
            msg
        )
    } else if msg.contains("VIBEMODE_API_KEY is required") {
        format!(
            "{}\n  hint: set VIBEMODE_API_KEY, use --demo, or run vimit --init",
            msg
        )
    } else {
        msg.to_string()
    }
}

fn real_main() -> Result<i32, String> {
    let cli_args = parse_args(env::args().skip(1))?;
    if cli_args.help {
        cli::args::print_help();
        return Ok(0);
    }
    if cli_args.version {
        println!("vimit {VERSION}");
        return Ok(0);
    }

    let config = Config::load(cli_args.config.as_ref())?;
    let accounts = AccountsConfig::load()?;

    if cli_args.list_accounts {
        let names = accounts.list_names();
        if names.is_empty() {
            println!("no accounts configured (create ~/.config/vimit/accounts.toml)");
        } else {
            println!("available accounts:");
            for name in &names {
                println!("  {name}");
            }
        }
        return Ok(0);
    }

    if cli_args.update {
        return cli::update::check_and_update(cli_args.update_check).map(|_| 0);
    }

    if cli_args.doctor {
        return cli::doctor::run_doctor();
    }

    if cli_args.init {
        return cli::init::run_init();
    }

    if cli_args.mcp {
        return cli::mcp::run_mcp();
    }

    let mut merged = config.merge_with_defaults()?;
    if let Some(ref account_name) = cli_args.account {
        let account = accounts.resolve(account_name)?;
        if let Some(api_key_env) = account.api_key_env {
            merged.api_key_env = api_key_env;
        }
        if let Some(api_base) = account.api_base {
            merged.api_base = Some(api_base);
        }
    }

    let mut args = merge_args_with_config(cli_args, &merged);
    if args.vpn && args.api_base.is_none() {
        args.api_base = Some(ng::VPN_API_BASE.to_string());
    }
    if args.overlay {
        let current = env::current_exe().map_err(|e| format!("cannot find current exe: {e}"))?;
        #[cfg(windows)]
        let gui_path = current.with_file_name("vimit-gui.exe");
        #[cfg(not(windows))]
        let gui_path = current.with_file_name("vimit-gui");
        if !gui_path.exists() {
            return Err("vimit-gui executable not found in the same directory".to_string());
        }
        let mut cmd = std::process::Command::new(gui_path);
        cmd.arg("--overlay");
        if args.demo {
            cmd.arg("--demo");
        }
        if args.output == cli::args::OutputMode::Compact {
            cmd.arg("--compact");
        }
        if let Some(mock) = &args.mock {
            cmd.arg("--mock").arg(mock);
        }
        let _ = cmd
            .spawn()
            .map_err(|e| format!("failed to launch vimit-gui: {e}"))?;
        return Ok(0);
    }

    if args.trend {
        let store = TrendStore::open_readonly()?;
        match store {
            Some(s) => {
                let days = s.query_trends(args.trend_days)?;
                match args.output {
                    cli::args::OutputMode::Json => print_trends_json(&days),
                    _ => print_trends_human(&days),
                }
            }
            None => {
                println!("No trend data found. Run vimit a few times to collect snapshots.");
            }
        }
        return Ok(0);
    }

    let trends = TrendStore::open()?;
    let cache = CacheStore::open()?;
    let mut notifier = Notifier::new(args.notify);
    let http = ng::HttpClient::new(ng::USER_AGENT)?;
    let mut router = ng::Router::new(
        args.api_base
            .clone()
            .unwrap_or_else(|| ng::DEFAULT_API_BASE.to_string()),
        ng::api_fallbacks_for(
            args.api_base.as_deref().unwrap_or(ng::DEFAULT_API_BASE),
            args.auto_failover,
        ),
    );
    cli::update::start_background_check();

    if args.monitor {
        let account_names = accounts.list_names();
        let account_configs: Vec<cli::accounts::AccountConfig> = account_names
            .iter()
            .filter_map(|name| accounts.resolve(name).ok())
            .collect();
        let initial_idx = args
            .account
            .as_ref()
            .and_then(|name| account_names.iter().position(|n| n == name))
            .unwrap_or(0);
        return run_monitor(
            &args,
            &mut notifier,
            &account_names,
            &account_configs,
            initial_idx,
            trends.as_ref(),
            cache.as_ref(),
        );
    }

    loop {
        let dotenv = load_config(&args)?;
        let code = run_once(
            &args,
            &dotenv,
            &mut notifier,
            &http,
            trends.as_ref(),
            cache.as_ref(),
            Some(&mut router),
        )?;
        if args.watch == 0 {
            if let Some(latest) = cli::update::latest_checked_version() {
                eprintln!();
                eprintln!(
                    "⚠️  Доступно обновление vimit: v{}! Запустите `vimit update` для установки.",
                    latest
                );
            }
            return Ok(code);
        }
        if args.fail_on != FailOn::Never && code != 0 {
            return Ok(code);
        }
        thread::sleep(Duration::from_secs(args.watch));
    }
}

fn merge_args_with_config(args: Args, merged: &MergedConfig) -> Args {
    Args {
        api_base: args.api_base.or_else(|| merged.api_base.clone()),
        api_key_env: if args.api_key_env == cli::constants::DEFAULT_API_KEY_ENV {
            merged.api_key_env.clone()
        } else {
            args.api_key_env
        },
        env_file: args.env_file.or_else(|| merged.env_file.clone()),
        demo: args.demo || merged.demo,
        mock: args.mock.or_else(|| merged.mock.clone()),
        output: args.output,
        daily_limit: args.daily_limit,
        overlay: args.overlay,
        monitor: args.monitor || merged.monitor,
        preset: if args.preset == cli::args::Preset::Full
            && merged.preset != cli::args::Preset::Full
        {
            merged.preset
        } else {
            args.preset
        },
        theme: if args.theme == cli::theme::Theme::Btop && merged.theme != cli::theme::Theme::Btop {
            merged.theme
        } else {
            args.theme
        },
        with_abtop: args.with_abtop || merged.with_abtop,
        notify: args.notify || merged.notify,
        watch: if args.watch == 0 && merged.watch != 0 {
            merged.watch
        } else {
            args.watch
        },
        fail_on: if args.fail_on == FailOn::Never && merged.fail_on != FailOn::Never {
            merged.fail_on
        } else {
            args.fail_on
        },
        warning_threshold: if (args.warning_threshold - cli::constants::DEFAULT_WARNING_THRESHOLD)
            .abs()
            < f64::EPSILON
            && (merged.warning_threshold - cli::constants::DEFAULT_WARNING_THRESHOLD).abs()
                > f64::EPSILON
        {
            merged.warning_threshold
        } else {
            args.warning_threshold
        },
        danger_threshold: if (args.danger_threshold - cli::constants::DEFAULT_DANGER_THRESHOLD)
            .abs()
            < f64::EPSILON
            && (merged.danger_threshold - cli::constants::DEFAULT_DANGER_THRESHOLD).abs()
                > f64::EPSILON
        {
            merged.danger_threshold
        } else {
            args.danger_threshold
        },
        window_thresholds: if args.window_thresholds.is_empty()
            && !merged.window_thresholds.is_empty()
        {
            merged.window_thresholds.clone()
        } else {
            args.window_thresholds
        },
        account: args.account,
        list_accounts: args.list_accounts,
        doctor: args.doctor,
        init: args.init,
        mcp: args.mcp,
        vpn: args.vpn,
        auto_failover: args.auto_failover && merged.auto_failover,
        no_cache: args.no_cache,
        trend: args.trend,
        trend_days: args.trend_days,
        update: args.update,
        update_check: args.update_check,
        help: args.help,
        version: args.version,
        config: args.config,
    }
}

fn load_config(args: &cli::args::Args) -> Result<ng::RuntimeConfig, String> {
    if args.demo || args.mock.is_some() {
        let dotenv = ng::load_dotenv_custom(args.env_file.as_ref())?;
        return Ok(ng::RuntimeConfig {
            api_base: args
                .api_base
                .clone()
                .unwrap_or_else(|| ng::DEFAULT_API_BASE.to_string()),
            api_key: String::new(),
            abtop_bin: ng::config_value("ABTOP_BIN", &dotenv)
                .unwrap_or_else(|| ng::DEFAULT_ABTOP_BIN.to_string()),
            auto_failover: args.auto_failover,
        });
    }

    let mut config = ng::RuntimeConfig::from_dotenv(
        args.api_base.clone(),
        &args.api_key_env,
        args.env_file.as_ref(),
    )?;
    config.auto_failover = args.auto_failover;
    Ok(config)
}
