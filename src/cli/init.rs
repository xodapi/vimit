use crate as vimit;
use std::io::{self, Write};
use std::path::Path;

use super::config::dirs_or_default;

pub fn run_init() -> Result<i32, String> {
    let home = dirs_or_default().ok_or_else(|| "cannot determine home directory".to_string())?;
    let config_dir = if cfg!(windows) {
        home.join("vimit")
    } else {
        home.join(".config").join("vimit")
    };

    println!("vimit init — interactive setup");
    println!();

    // Create config directory
    if config_dir.is_dir() {
        println!("  config directory: {} (exists)", config_dir.display());
    } else {
        print!("  creating config directory: {}... ", config_dir.display());
        io::stdout().flush().unwrap();
        std::fs::create_dir_all(&config_dir)
            .map_err(|e| format!("cannot create config dir: {e}"))?;
        println!("done");
    }

    // Create config.toml
    let config_file = config_dir.join("config.toml");
    if config_file.is_file() {
        println!(
            "  config.toml: {} (exists, skipping)",
            config_file.display()
        );
    } else {
        let default_config = r#"# vimit configuration
# See https://github.com/xodapi/vimit for docs

# Default thresholds
# warning = 75
# danger = 90

# Color theme (btop, dracula, catppuccin, tokyo-night, gruvbox, nord, ...)
# theme = "btop"

# Monitor preset (full, compact, mini)
# preset = "full"

# Notify on threshold breach
# notify = false

# Poll interval in seconds (0 = single run)
# watch = 0

# API cache TTL in seconds; set to 0 to expire cached responses immediately.
# cache_ttl_secs = 30
"#;
        print!("  creating config.toml... ");
        io::stdout().flush().unwrap();
        std::fs::write(&config_file, default_config)
            .map_err(|e| format!("cannot write config.toml: {e}"))?;
        println!("done");
    }

    // Optionally create .env
    let env_file = config_dir.join(".env");
    if env_file.is_file() {
        println!("  .env: {} (exists, skipping)", env_file.display());
    } else {
        print!("\n  ? create .env with VIBEMODE_API_KEY? [y/N] ");
        io::stdout().flush().unwrap();
        let mut answer = String::new();
        io::stdin().read_line(&mut answer).unwrap();
        let answer = answer.trim().to_lowercase();
        if answer == "y" || answer == "yes" {
            print!("  ? enter your API key: ");
            io::stdout().flush().unwrap();
            let mut key = String::new();
            io::stdin().read_line(&mut key).unwrap();
            let key = key.trim().to_string();
            if !key.is_empty() {
                let env_content = format!(
                    "VIBEMODE_API_KEY={key}\n# VIBEMODE_API_BASE=https://r-api.vibemod.pro\n"
                );
                write_secret_env_file(&env_file, env_content.as_bytes())
                    .map_err(|e| format!("cannot write .env: {e}"))?;
                println!("  .env created with VIBEMODE_API_KEY");
                println!("  hint: you can also set the env var directly or use --api-key-env");
            } else {
                println!("  skipping — empty key");
            }
        }
    }

    // Optionally test connection
    print!("\n  ? test API connection now? [Y/n] ");
    io::stdout().flush().unwrap();
    let mut answer = String::new();
    io::stdin().read_line(&mut answer).unwrap();
    let answer = answer.trim().to_lowercase();
    if answer != "n" && answer != "no" {
        test_connection()?;
    }

    println!();
    println!("  setup complete!");
    println!("  run 'vimit --monitor' to start the dashboard");
    println!("  or 'vimit --doctor' for diagnostics");
    Ok(0)
}

fn test_connection() -> Result<(), String> {
    let dotenv = vimit::load_dotenv_custom(None).unwrap_or_default();
    let api_key = vimit::config_value("VIBEMODE_API_KEY", &dotenv).unwrap_or_default();
    if api_key.is_empty() {
        println!("  skipping connection test: no API key found");
        return Ok(());
    }
    let api_base = vimit::config_value("VIBEMODE_API_BASE", &dotenv)
        .unwrap_or_else(|| vimit::DEFAULT_API_BASE.to_string());
    print!("  testing {api_base}... ");
    io::stdout().flush().unwrap();
    let http = vimit::HttpClient::new(vimit::USER_AGENT)?;
    match http.fetch_me(&api_key, &api_base) {
        Ok(payload) => {
            let windows = vimit::summarize_me(&payload, 75.0, 90.0);
            println!("OK ({} window(s))", windows.len());
            Ok(())
        }
        Err(e) => {
            println!("FAILED");
            println!("  error: {e}");
            println!("  hint: check your API key and network");
            Ok(())
        }
    }
}

fn write_secret_env_file(path: &Path, content: &[u8]) -> std::io::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }

    let mut file = options.open(path)?;
    file.write_all(content)
}

#[cfg(test)]
mod tests {
    use super::write_secret_env_file;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn secret_env_file_uses_create_new() {
        let path = std::env::temp_dir().join(format!(
            "vimit-init-create-new-{}.env",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        write_secret_env_file(&path, b"VIBEMODE_API_KEY=test\n").unwrap();
        let err = write_secret_env_file(&path, b"VIBEMODE_API_KEY=other\n").unwrap_err();

        assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
        let _ = std::fs::remove_file(path);
    }

    #[cfg(unix)]
    #[test]
    fn secret_env_file_is_owner_only_on_unix() {
        use std::os::unix::fs::PermissionsExt;

        let path = std::env::temp_dir().join(format!(
            "vimit-init-owner-only-{}.env",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        write_secret_env_file(&path, b"VIBEMODE_API_KEY=test\n").unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;

        assert_eq!(mode, 0o600);
        let _ = std::fs::remove_file(path);
    }
}
