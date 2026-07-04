use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

static CLI_TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn cli_test_lock() -> std::sync::MutexGuard<'static, ()> {
    CLI_TEST_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .expect("cli integration test lock should not be poisoned")
}

struct TestHome {
    root: PathBuf,
    appdata: PathBuf,
    home: PathBuf,
}

impl TestHome {
    fn new() -> Self {
        let unique = format!(
            "vimit-cli-it-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        let appdata = root.join("appdata");
        let home = root.join("home");
        fs::create_dir_all(&appdata).unwrap();
        fs::create_dir_all(&home).unwrap();

        let state_dir = if cfg!(windows) {
            appdata.join("vimit")
        } else {
            home.join(".config").join("vimit")
        };
        fs::create_dir_all(&state_dir).unwrap();
        fs::write(
            state_dir.join("state.json"),
            r#"{"auto_update_check":false,"auto_api_failover":true}"#,
        )
        .unwrap();

        Self {
            root,
            appdata,
            home,
        }
    }

    fn command(&self) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_vimit"));
        cmd.current_dir(env!("CARGO_MANIFEST_DIR"));
        if cfg!(windows) {
            cmd.env("APPDATA", &self.appdata);
            cmd.env("USERPROFILE", &self.home);
        } else {
            cmd.env("HOME", &self.home);
        }
        cmd.env_remove("VIBEMODE_API_KEY");
        cmd.env_remove("NEUROGATE_API_KEY");
        cmd
    }
}

impl Drop for TestHome {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn fixture_path(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(path)
}

fn stdout_json(output: &std::process::Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("stdout should be valid json")
}

#[test]
fn demo_json_runs_real_cli_binary() {
    let _lock = cli_test_lock();
    let test_home = TestHome::new();
    let output = test_home
        .command()
        .args(["--demo", "--json"])
        .output()
        .expect("demo command should run");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json = stdout_json(&output);
    assert_eq!(json["active_endpoint"], "demo");
    assert_eq!(json["source"], "vibemode");
    assert_eq!(json["api_status"], "online");
    assert!(
        json["windows"]
            .as_array()
            .is_some_and(|windows| !windows.is_empty())
    );
}

#[test]
fn mock_json_runs_real_cli_binary() {
    let _lock = cli_test_lock();
    let test_home = TestHome::new();
    let fixture = fixture_path("tests/fixtures/me.json");
    let output = test_home
        .command()
        .args(["--mock", fixture.to_str().unwrap(), "--json"])
        .output()
        .expect("mock command should run");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json = stdout_json(&output);
    assert_eq!(json["active_endpoint"], "mock");
    assert_eq!(json["source"], "vibemode");
    assert_eq!(json["windows"][1]["window"], "5h");
    assert_eq!(json["windows"][1]["credits"]["used"], 39.0);
}

#[test]
fn invalid_args_return_error_from_cli() {
    let _lock = cli_test_lock();
    let test_home = TestHome::new();
    let fixture = fixture_path("tests/fixtures/me.json");
    let output = test_home
        .command()
        .args(["--demo", "--mock", fixture.to_str().unwrap()])
        .output()
        .expect("invalid command should run");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--demo and --mock are mutually exclusive"));
}

#[test]
fn cli_output_does_not_leak_api_keys() {
    let _lock = cli_test_lock();
    let test_home = TestHome::new();
    let fixture = fixture_path("tests/fixtures/me.json");
    let secret = "vm_test_secret_123456789";
    let output = test_home
        .command()
        .env("VIBEMODE_API_KEY", secret)
        .args(["--mock", fixture.to_str().unwrap(), "--json"])
        .output()
        .expect("mock command with env should run");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stdout.contains(secret), "stdout leaked secret: {stdout}");
    assert!(!stderr.contains(secret), "stderr leaked secret: {stderr}");
}
