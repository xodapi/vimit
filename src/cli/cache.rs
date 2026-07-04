use redb::{Database, ReadableTable, TableDefinition};
use serde_json::Value;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("api_cache");
pub const DEFAULT_TTL_SECS: u64 = 30;

pub struct CacheEntry {
    pub cache_key: String,
    pub api_base: String,
    pub cached_at: String,
    pub age_secs: u64,
    pub payload: Value,
}

pub struct CacheStore {
    db: Database,
    ttl: Duration,
}

impl CacheStore {
    pub fn open() -> Result<Option<Self>, String> {
        let path = match default_cache_path() {
            Some(p) => p,
            None => return Ok(None),
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create cache directory: {e}"))?;
        }
        let db =
            Database::create(&path).map_err(|e| format!("cannot create cache database: {e}"))?;
        let tx = db
            .begin_write()
            .map_err(|e| format!("cache write tx failed: {e}"))?;
        let _ = tx.open_table(TABLE);
        tx.commit()
            .map_err(|e| format!("cache commit failed: {e}"))?;
        Ok(Some(Self {
            db,
            ttl: Duration::from_secs(DEFAULT_TTL_SECS),
        }))
    }

    pub fn set_ttl(&mut self, secs: u64) {
        self.ttl = Duration::from_secs(secs);
    }

    pub fn get(&self, api_key: &str, api_base: &str) -> Option<(Value, Instant)> {
        let key = cache_key(api_key, api_base);
        let tx = self.db.begin_read().ok()?;
        let table = tx.open_table(TABLE).ok()?;
        let entry = table.get(key.as_bytes()).ok()??;
        let bytes = entry.value();
        let parsed: Value = serde_json::from_slice(bytes).ok()?;
        let cached_at_str = parsed.get("cached_at")?.as_str()?;
        let cached_at_secs: u64 = cached_at_str.parse().ok()?;
        let now_secs = std::time::UNIX_EPOCH
            .elapsed()
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let age_secs = now_secs.saturating_sub(cached_at_secs);
        if self.ttl.is_zero() || Duration::from_secs(age_secs) > self.ttl {
            return None;
        }
        let payload = parsed.get("payload")?.clone();
        let cached_instant = Instant::now()
            .checked_sub(Duration::from_secs(age_secs))
            .unwrap_or(Instant::now());
        Some((payload, cached_instant))
    }

    pub fn set(&self, api_key: &str, api_base: &str, payload: &Value) -> Result<(), String> {
        let key = cache_key(api_key, api_base);
        let now_secs = std::time::UNIX_EPOCH
            .elapsed()
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let entry = serde_json::json!({
            "cached_at": now_secs.to_string(),
            "payload": payload,
        });
        let bytes =
            serde_json::to_vec(&entry).map_err(|e| format!("cache serialize failed: {e}"))?;
        let tx = self
            .db
            .begin_write()
            .map_err(|e| format!("cache write tx failed: {e}"))?;
        {
            let mut table = tx
                .open_table(TABLE)
                .map_err(|e| format!("cache table open failed: {e}"))?;
            table
                .insert(key.as_bytes(), bytes.as_slice())
                .map_err(|e| format!("cache insert failed: {e}"))?;
        }
        tx.commit().map_err(|e| format!("cache commit failed: {e}"))
    }

    pub fn remove(&self, api_key: &str, api_base: &str) -> Result<(), String> {
        let key = cache_key(api_key, api_base);
        let tx = self
            .db
            .begin_write()
            .map_err(|e| format!("cache write tx failed: {e}"))?;
        {
            let mut table = tx
                .open_table(TABLE)
                .map_err(|e| format!("cache table open failed: {e}"))?;
            table
                .remove(key.as_bytes())
                .map_err(|e| format!("cache remove failed: {e}"))?;
        }
        tx.commit().map_err(|e| format!("cache commit failed: {e}"))
    }

    pub fn ttl(&self) -> Duration {
        self.ttl
    }

    pub fn entries(&self) -> Result<Vec<CacheEntry>, String> {
        let tx = self
            .db
            .begin_read()
            .map_err(|e| format!("cache read tx failed: {e}"))?;
        let table = tx
            .open_table(TABLE)
            .map_err(|e| format!("cache table open failed: {e}"))?;
        let now_secs = std::time::UNIX_EPOCH
            .elapsed()
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let mut entries = Vec::new();
        for item in table
            .iter()
            .map_err(|e| format!("cache iteration failed: {e}"))?
        {
            let (key, value) = item.map_err(|e| format!("cache row read failed: {e}"))?;
            let cache_key = std::str::from_utf8(key.value())
                .map_err(|e| format!("cache key is invalid UTF-8: {e}"))?
                .to_string();
            let parsed: Value = serde_json::from_slice(value.value())
                .map_err(|e| format!("cache value is invalid JSON: {e}"))?;
            let cached_at = parsed
                .get("cached_at")
                .and_then(Value::as_str)
                .unwrap_or("0")
                .to_string();
            let cached_at_secs = cached_at.parse::<u64>().unwrap_or(0);
            let payload = parsed.get("payload").cloned().unwrap_or(Value::Null);
            let api_base = cache_key
                .split_once('|')
                .map(|(_, api_base)| api_base.to_string())
                .unwrap_or_default();
            entries.push(CacheEntry {
                cache_key,
                api_base,
                cached_at,
                age_secs: now_secs.saturating_sub(cached_at_secs),
                payload,
            });
        }
        Ok(entries)
    }
}

fn cache_key(api_key: &str, api_base: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    api_key.hash(&mut hasher);
    let hash = hasher.finish();
    format!("{:016x}|{}", hash, api_base.trim_end_matches('/'))
}

fn default_cache_path() -> Option<PathBuf> {
    let base = if cfg!(target_os = "windows") {
        if let Ok(appdata) = std::env::var("APPDATA") {
            PathBuf::from(appdata).join("vimit")
        } else if let Ok(home) = std::env::var("USERPROFILE") {
            PathBuf::from(home).join("vimit")
        } else {
            return None;
        }
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".config").join("vimit")
    } else {
        return None;
    };
    Some(base.join("cache.redb"))
}
