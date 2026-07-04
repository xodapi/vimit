use std::env;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

fn main() {
    let mut timeout_secs = 900_u64;
    let mut heartbeat_secs = 30_u64;
    let mut command_start = None;
    let args: Vec<String> = env::args().skip(1).collect();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--timeout-secs" => {
                index += 1;
                timeout_secs = parse_u64(args.get(index), "--timeout-secs");
            }
            "--heartbeat-secs" => {
                index += 1;
                heartbeat_secs = parse_u64(args.get(index), "--heartbeat-secs");
            }
            "--" => {
                command_start = Some(index + 1);
                break;
            }
            _ => {
                command_start = Some(index);
                break;
            }
        }
        index += 1;
    }

    let Some(command_start) = command_start else {
        eprintln!(
            "usage: guarded-run [--timeout-secs N] [--heartbeat-secs N] -- <command> [args...]"
        );
        std::process::exit(2);
    };
    if command_start >= args.len() {
        eprintln!("guarded-run: missing command");
        std::process::exit(2);
    }

    let mut child = Command::new(&args[command_start])
        .args(&args[command_start + 1..])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap_or_else(|error| {
            eprintln!(
                "guarded-run: cannot start '{}': {error}",
                args[command_start]
            );
            std::process::exit(127);
        });

    let started = Instant::now();
    let timeout = Duration::from_secs(timeout_secs);
    let heartbeat = Duration::from_secs(heartbeat_secs.max(1));
    let mut next_heartbeat = heartbeat;

    loop {
        match child.try_wait() {
            Ok(Some(status)) => std::process::exit(status.code().unwrap_or(1)),
            Ok(None) => {}
            Err(error) => {
                eprintln!("guarded-run: cannot read child status: {error}");
                let _ = child.kill();
                std::process::exit(1);
            }
        }

        let elapsed = started.elapsed();
        if elapsed >= timeout {
            eprintln!(
                "guarded-run: timeout after {}s; killing '{}'",
                elapsed.as_secs(),
                args[command_start]
            );
            let _ = child.kill();
            let _ = child.wait();
            std::process::exit(124);
        }

        if elapsed >= next_heartbeat {
            eprintln!(
                "guarded-run: still running '{}' after {}s (timeout {}s)",
                args[command_start],
                elapsed.as_secs(),
                timeout_secs
            );
            next_heartbeat += heartbeat;
        }

        thread::sleep(Duration::from_secs(1));
    }
}

fn parse_u64(value: Option<&String>, name: &str) -> u64 {
    let Some(value) = value else {
        eprintln!("guarded-run: missing value for {name}");
        std::process::exit(2);
    };
    value.parse::<u64>().unwrap_or_else(|_| {
        eprintln!("guarded-run: invalid integer for {name}: {value}");
        std::process::exit(2);
    })
}
