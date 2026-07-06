use self_update::cargo_crate_version;
use self_update::update::{Release, ReleaseAsset, ReleaseUpdate};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;
use std::path::PathBuf;
use std::time::SystemTime;

use super::config::dirs_or_default;

const STATE_FILE_NAME: &str = "state.json";
const REPO_OWNER: &str = "xodapi";
const REPO_NAME: &str = "vimit";
const BIN_NAME: &str = "vimit";

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct State {
    last_update_check: Option<u64>, // timestamp in seconds
    latest_available_version: Option<String>,
    #[serde(default = "default_true")]
    auto_update_check: bool,
    #[serde(default = "default_true")]
    auto_api_failover: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            last_update_check: None,
            latest_available_version: None,
            auto_update_check: true,
            auto_api_failover: true,
        }
    }
}

fn state_path() -> Option<PathBuf> {
    let home = dirs_or_default()?;
    let config_dir = if cfg!(windows) {
        home.join("vimit")
    } else {
        home.join(".config").join("vimit")
    };
    Some(config_dir.join(STATE_FILE_NAME))
}

fn load_state() -> State {
    let Some(path) = state_path() else {
        return State::default();
    };
    if !path.is_file() {
        return State::default();
    }
    let raw = fs::read_to_string(&path).unwrap_or_default();
    serde_json::from_str(&raw).unwrap_or_default()
}

fn save_state(state: &State) {
    let Some(path) = state_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(raw) = serde_json::to_string_pretty(state) {
        let _ = fs::write(path, raw);
    }
}

pub fn check_and_update(check_only: bool) -> Result<(), String> {
    let current_version = cargo_crate_version!();

    let mut builder = self_update::backends::github::Update::configure();
    builder
        .repo_owner(REPO_OWNER)
        .repo_name(REPO_NAME)
        .bin_name(BIN_NAME)
        .current_version(current_version);

    if check_only {
        println!(
            "Проверка обновлений для vimit (текущая версия: v{})...",
            current_version
        );
        let latest = builder
            .build()
            .map_err(|e| format!("Ошибка конфигурации обновления: {e}"))?
            .get_latest_release()
            .map_err(|e| format!("Не удалось получить последний релиз: {e}"))?;

        if self_update::version::bump_is_greater(current_version, &latest.version).unwrap_or(false)
        {
            println!("Доступна новая версия: v{}!", latest.version);
            println!("Запустите `vimit update` для установки.");

            // Update state cache as well since we manually checked
            let mut state = load_state();
            let now = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            state.last_update_check = Some(now);
            state.latest_available_version = Some(latest.version);
            save_state(&state);
        } else {
            println!(
                "У вас уже установлена последняя версия: v{}.",
                current_version
            );
        }
        Ok(())
    } else {
        println!("Запуск обновления vimit v{}...", current_version);
        let status = builder
            .build()
            .map_err(|e| format!("Ошибка конфигурации обновления: {e}"))?
            .update_with_sha256()
            .map_err(|e| format!("Ошибка при установке обновления: {e}"))?;

        if status.updated() {
            println!("Успешно обновлено до версии v{}!", status.version());
            // Clear state cache to avoid showing notification on next run
            let mut state = load_state();
            state.latest_available_version = None;
            save_state(&state);
        } else {
            println!(
                "У вас уже установлена актуальная версия v{}.",
                status.version()
            );
        }
        Ok(())
    }
}

pub fn start_background_check() {
    std::thread::spawn(move || {
        let current_version = cargo_crate_version!();
        let mut state = load_state();
        if !state.auto_update_check {
            return;
        }
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // 24 hours = 86400 seconds
        if state
            .last_update_check
            .filter(|&last_check| now >= last_check && now - last_check < 86400)
            .is_some()
        {
            return;
        }

        // Query github release
        let mut builder = self_update::backends::github::Update::configure();
        builder
            .repo_owner(REPO_OWNER)
            .repo_name(REPO_NAME)
            .bin_name(BIN_NAME)
            .current_version(current_version);

        if let Ok(latest) = builder
            .build()
            .and_then(|updater| updater.get_latest_release())
        {
            state.last_update_check = Some(now);
            if self_update::version::bump_is_greater(current_version, &latest.version)
                .unwrap_or(false)
            {
                state.latest_available_version = Some(latest.version);
            } else {
                state.latest_available_version = None;
            }
            save_state(&state);
        }
    });
}

pub fn latest_checked_version() -> Option<String> {
    let current_version = cargo_crate_version!();
    let state = load_state();
    if let Some(ver) = state
        .latest_available_version
        .filter(|ver| self_update::version::bump_is_greater(current_version, ver).unwrap_or(false))
    {
        return Some(ver);
    }
    None
}

pub fn is_auto_check_enabled() -> bool {
    load_state().auto_update_check
}

pub fn set_auto_check_enabled(enabled: bool) {
    let mut state = load_state();
    state.auto_update_check = enabled;
    save_state(&state);
}

pub fn is_auto_api_failover_enabled() -> bool {
    load_state().auto_api_failover
}

pub fn set_auto_api_failover_enabled(enabled: bool) {
    let mut state = load_state();
    state.auto_api_failover = enabled;
    save_state(&state);
}

trait Sha256VerifiedUpdate {
    fn update_with_sha256(&self) -> self_update::errors::Result<self_update::Status>;
}

impl<T: ReleaseUpdate + ?Sized> Sha256VerifiedUpdate for T {
    fn update_with_sha256(&self) -> self_update::errors::Result<self_update::Status> {
        let current_version = self.current_version();
        println!("Checking target-arch... {}", self.target());
        println!("Checking current version... v{}", current_version);
        print!("Checking latest released version... ");
        io::stdout().flush()?;

        let release = self.get_latest_release()?;
        println!("v{}", release.version);
        if !self_update::version::bump_is_greater(&current_version, &release.version)? {
            return Ok(self_update::Status::UpToDate(current_version));
        }

        println!(
            "New release found! v{} --> v{}",
            current_version, release.version
        );
        let qualifier =
            if self_update::version::bump_is_compatible(&current_version, &release.version)? {
                ""
            } else {
                "*NOT* "
            };
        println!("New release is {}compatible", qualifier);

        let target = self.target();
        let target_asset = archive_asset_for(&release, &target, self.identifier().as_deref())
            .ok_or_else(|| {
                self_update::errors::Error::Release(format!(
                    "No archive asset found for target: `{target}`"
                ))
            })?;
        let checksum_asset = checksum_asset_for(&release, &target_asset).ok_or_else(|| {
            self_update::errors::Error::Release(format!(
                "No SHA-256 checksum asset found for `{}`",
                target_asset.name
            ))
        })?;

        println!("\n{} release status:", self.bin_name());
        println!("  * Current exe: {:?}", self.bin_install_path());
        println!("  * New exe release: {:?}", target_asset.name);
        println!("  * New exe checksum: {:?}", checksum_asset.name);
        println!("  * New exe download url: {:?}", target_asset.download_url);
        println!(
            "\nThe new release will be downloaded, verified, extracted, and the existing binary will be replaced."
        );
        confirm_update()?;

        let tmp_archive_dir = self_update::TempDir::new()?;
        let tmp_archive_path = tmp_archive_dir.path().join(&target_asset.name);
        let mut tmp_archive = fs::File::create(&tmp_archive_path)?;
        let mut archive_download = release_download(&target_asset.download_url);
        archive_download.show_progress(self.show_download_progress());
        archive_download.set_progress_style(self.progress_template(), self.progress_chars());
        println!("Downloading...");
        archive_download.download_to(&mut tmp_archive)?;
        drop(tmp_archive);

        let tmp_checksum_path = tmp_archive_dir.path().join(&checksum_asset.name);
        let mut tmp_checksum = fs::File::create(&tmp_checksum_path)?;
        release_download(&checksum_asset.download_url).download_to(&mut tmp_checksum)?;
        drop(tmp_checksum);

        println!("Verifying SHA-256 checksum...");
        verify_archive_checksum(&tmp_archive_path, &fs::read_to_string(&tmp_checksum_path)?)
            .map_err(self_update::errors::Error::Update)?;

        print!("Extracting archive... ");
        io::stdout().flush()?;
        self_update::Extract::from_source(&tmp_archive_path)
            .extract_file(tmp_archive_dir.path(), self.bin_path_in_archive())?;
        println!("Done");

        let new_exe = tmp_archive_dir.path().join(self.bin_path_in_archive());
        print!("Replacing binary file... ");
        io::stdout().flush()?;
        self_update::self_replace::self_replace(new_exe)?;
        println!("Done");

        Ok(self_update::Status::Updated(release.version))
    }
}

fn confirm_update() -> self_update::errors::Result<()> {
    print!("Do you want to continue? [Y/n] ");
    io::stdout().flush()?;

    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    let answer = answer.trim().to_lowercase();
    if !answer.is_empty() && answer != "y" {
        return Err(self_update::errors::Error::Update("Update aborted".into()));
    }
    Ok(())
}

fn release_download(url: &str) -> self_update::Download {
    let mut download = self_update::Download::from_url(url);
    download.set_header(
        reqwest::header::ACCEPT,
        "application/octet-stream"
            .parse()
            .expect("valid accept header"),
    );
    download
}

fn archive_asset_for(
    release: &Release,
    target: &str,
    identifier: Option<&str>,
) -> Option<ReleaseAsset> {
    release
        .assets
        .iter()
        .find(|asset| {
            asset.name.contains(target)
                && !asset.name.ends_with(".sha256")
                && (asset.name.ends_with(".tar.gz") || asset.name.ends_with(".zip"))
                && identifier
                    .map(|identifier| asset.name.contains(identifier))
                    .unwrap_or(true)
        })
        .cloned()
}

fn checksum_asset_for(release: &Release, archive_asset: &ReleaseAsset) -> Option<ReleaseAsset> {
    let checksum_name = format!("{}.sha256", archive_asset.name);
    release
        .assets
        .iter()
        .find(|asset| asset.name == checksum_name)
        .cloned()
}

fn verify_archive_checksum(archive_path: &Path, checksum_text: &str) -> Result<(), String> {
    let expected = parse_sha256_checksum(checksum_text)?;
    let actual =
        sha256_hex(archive_path).map_err(|e| format!("Не удалось прочитать архив: {e}"))?;
    if actual.eq_ignore_ascii_case(&expected) {
        Ok(())
    } else {
        Err(format!(
            "SHA-256 mismatch for downloaded release archive: expected {expected}, got {actual}"
        ))
    }
}

fn parse_sha256_checksum(text: &str) -> Result<String, String> {
    let first_token = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .and_then(|line| line.split_whitespace().next())
        .ok_or_else(|| "SHA-256 checksum file is empty".to_string())?;

    if first_token.len() == 64 && first_token.chars().all(|ch| ch.is_ascii_hexdigit()) {
        Ok(first_token.to_ascii_lowercase())
    } else {
        Err("SHA-256 checksum file does not start with a 64-character hex digest".to_string())
    }
}

fn sha256_hex(path: &Path) -> io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut hex, "{byte:02x}").expect("writing to String cannot fail");
    }
    Ok(hex)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(name: &str) -> ReleaseAsset {
        ReleaseAsset {
            name: name.to_string(),
            download_url: format!("https://example.invalid/{name}"),
        }
    }

    #[test]
    fn parse_sha256_checksum_accepts_release_workflow_format() {
        let checksum = "61d825595262940fb32c184e8abd8e53c68f0a0db1c4a3f9096868cf3e5b4531  vimit-v0.6.4-x86_64-pc-windows-msvc.zip\n";

        assert_eq!(
            parse_sha256_checksum(checksum).unwrap(),
            "61d825595262940fb32c184e8abd8e53c68f0a0db1c4a3f9096868cf3e5b4531"
        );
    }

    #[test]
    fn parse_sha256_checksum_rejects_invalid_digest() {
        let err = parse_sha256_checksum("not-a-sha  file.zip").unwrap_err();

        assert!(err.contains("64-character hex digest"));
    }

    #[test]
    fn archive_asset_for_ignores_checksum_sidecar() {
        let release = Release {
            assets: vec![
                asset("vimit-v0.6.4-x86_64-pc-windows-msvc.zip.sha256"),
                asset("vimit-v0.6.4-x86_64-pc-windows-msvc.zip"),
            ],
            ..Release::default()
        };

        let selected = archive_asset_for(&release, "x86_64-pc-windows-msvc", None).unwrap();

        assert_eq!(selected.name, "vimit-v0.6.4-x86_64-pc-windows-msvc.zip");
    }

    #[test]
    fn checksum_asset_for_requires_exact_archive_sidecar() {
        let archive = asset("vimit-v0.6.4-x86_64-unknown-linux-gnu.tar.gz");
        let release = Release {
            assets: vec![
                archive.clone(),
                asset("vimit-v0.6.4-x86_64-unknown-linux-gnu.tar.gz.sha256"),
                asset("vimit-v0.6.4-aarch64-unknown-linux-gnu.tar.gz.sha256"),
            ],
            ..Release::default()
        };

        let selected = checksum_asset_for(&release, &archive).unwrap();

        assert_eq!(
            selected.name,
            "vimit-v0.6.4-x86_64-unknown-linux-gnu.tar.gz.sha256"
        );
    }
}
