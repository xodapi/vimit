use serde_json::Value;
use std::fs;
use std::path::Path;

const MAX_FACTORY_SETTINGS_FILES: usize = 512;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ActivitySignal {
    pub energy: f32,
    pub burst: f32,
    pub active_sessions: u32,
    pub token_delta: f64,
    pub credit_delta: f64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FactoryActivitySnapshot {
    token_usage: f64,
    credits: f64,
    active_time_ms: f64,
    session_count: u32,
}

impl FactoryActivitySnapshot {
    pub fn from_settings_json(value: &Value) -> Option<Self> {
        let object = value.as_object()?;
        let token_usage = number_field(object, "inclusiveTokenUsage")
            .or_else(|| number_field(object, "tokenUsage"))?;
        Some(Self {
            token_usage,
            credits: number_field(object, "factoryCredits").unwrap_or(0.0),
            active_time_ms: number_field(object, "assistantActiveTimeMs").unwrap_or(0.0),
            session_count: 1,
        })
    }

    fn combine(&mut self, other: Self) {
        self.token_usage += other.token_usage;
        self.credits += other.credits;
        self.active_time_ms += other.active_time_ms;
        self.session_count += other.session_count;
    }
}

#[derive(Debug, Default)]
pub struct ActivityTracker {
    previous_factory: Option<FactoryActivitySnapshot>,
    energy: f32,
    burst: f32,
}

impl ActivityTracker {
    pub fn observe(
        &mut self,
        abtop_status: Option<&Value>,
        factory: Option<FactoryActivitySnapshot>,
    ) -> ActivitySignal {
        let (abtop_energy, active_sessions) = abtop_activity(abtop_status);
        let (token_delta, credit_delta, factory_active) = factory
            .map(|snapshot| self.factory_delta(snapshot))
            .unwrap_or_default();
        let factory_energy =
            ((token_delta / 2_000.0) as f32 + (credit_delta / 25.0) as f32).clamp(0.0, 1.0);
        let target = abtop_energy.max(factory_energy);
        let spike = (target - self.energy).max(0.0);
        self.energy = (self.energy * 0.62 + target * 0.38).clamp(0.0, 1.0);
        self.burst = (self.burst * 0.58 + spike * 0.9).clamp(0.0, 1.0);

        ActivitySignal {
            energy: self.energy,
            burst: self.burst,
            active_sessions: active_sessions.max(factory_active),
            token_delta,
            credit_delta,
        }
    }

    fn factory_delta(&mut self, current: FactoryActivitySnapshot) -> (f64, f64, u32) {
        let Some(previous) = self.previous_factory.replace(current) else {
            return (0.0, 0.0, 0);
        };
        let token_delta = (current.token_usage - previous.token_usage).max(0.0);
        let credit_delta = (current.credits - previous.credits).max(0.0);
        let active = if current.active_time_ms > previous.active_time_ms
            || token_delta > 0.0
            || credit_delta > 0.0
        {
            current.session_count
        } else {
            0
        };
        (token_delta, credit_delta, active)
    }
}

pub fn read_factory_activity_snapshot(root: &Path) -> Option<FactoryActivitySnapshot> {
    let mut remaining = MAX_FACTORY_SETTINGS_FILES;
    let mut total = FactoryActivitySnapshot::default();
    read_factory_directory(root, &mut remaining, &mut total);
    (total.session_count > 0).then_some(total)
}

pub fn default_factory_sessions_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(std::path::PathBuf::from)
        .map(|home| home.join(".factory").join("sessions"))
}

pub fn read_default_factory_activity_snapshot() -> Option<FactoryActivitySnapshot> {
    read_factory_activity_snapshot(&default_factory_sessions_dir()?)
}

fn read_factory_directory(
    directory: &Path,
    remaining: &mut usize,
    total: &mut FactoryActivitySnapshot,
) {
    if *remaining == 0 {
        return;
    }
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        if *remaining == 0 {
            break;
        }
        let path = entry.path();
        if path.is_dir() {
            read_factory_directory(&path, remaining, total);
        } else if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".settings.json"))
        {
            *remaining -= 1;
            let Ok(raw) = fs::read_to_string(&path) else {
                continue;
            };
            let Ok(value) = serde_json::from_str::<Value>(&raw) else {
                continue;
            };
            if let Some(snapshot) = FactoryActivitySnapshot::from_settings_json(&value) {
                total.combine(snapshot);
            }
        }
    }
}

fn abtop_activity(status: Option<&Value>) -> (f32, u32) {
    let Some(status) = status else {
        return (0.0, 0);
    };
    let active_sessions = status
        .get("sessions_active")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .min(u32::MAX as u64) as u32;
    let token_rate = status
        .get("token_rate")
        .and_then(number)
        .or_else(|| {
            status
                .get("agents")
                .and_then(Value::as_array)
                .map(|agents| {
                    agents
                        .iter()
                        .filter_map(|agent| agent.get("token_rate").and_then(number))
                        .sum()
                })
        })
        .unwrap_or(0.0);
    let interval_ms = status
        .get("interval_ms")
        .and_then(Value::as_u64)
        .filter(|value| *value > 0)
        .unwrap_or(1_000);
    let tokens_per_second = token_rate * 1_000.0 / interval_ms as f64;
    let session_energy = (active_sessions as f32 / 3.0).min(1.0) * 0.35;
    (
        ((tokens_per_second / 20.0) as f32).min(1.0) * 0.65 + session_energy,
        active_sessions,
    )
}

fn number_field(object: &serde_json::Map<String, Value>, key: &str) -> Option<f64> {
    object.get(key).and_then(number)
}

fn number(value: &Value) -> Option<f64> {
    value.as_f64().or_else(|| value.as_str()?.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn factory_snapshot_reads_only_aggregate_counters() {
        let snapshot = FactoryActivitySnapshot::from_settings_json(&json!({
            "inclusiveTokenUsage": 1200,
            "factoryCredits": "3.5",
            "assistantActiveTimeMs": 800,
            "prompt": "must not be retained"
        }))
        .unwrap();

        assert_eq!(snapshot.token_usage, 1200.0);
        assert_eq!(snapshot.credits, 3.5);
        assert_eq!(snapshot.active_time_ms, 800.0);
        assert_eq!(std::mem::size_of::<FactoryActivitySnapshot>(), 32);
    }

    #[test]
    fn tracker_detects_factory_delta_and_ignores_counter_reset() {
        let mut tracker = ActivityTracker::default();
        let first = FactoryActivitySnapshot {
            token_usage: 100.0,
            credits: 2.0,
            active_time_ms: 50.0,
            session_count: 1,
        };
        let next = FactoryActivitySnapshot {
            token_usage: 450.0,
            credits: 5.0,
            active_time_ms: 100.0,
            session_count: 1,
        };
        let reset = FactoryActivitySnapshot {
            token_usage: 20.0,
            credits: 1.0,
            active_time_ms: 10.0,
            session_count: 1,
        };

        assert_eq!(tracker.observe(None, Some(first)).token_delta, 0.0);
        let active = tracker.observe(None, Some(next));
        assert_eq!(active.token_delta, 350.0);
        assert_eq!(active.credit_delta, 3.0);
        assert_eq!(active.active_sessions, 1);
        assert!(active.energy > 0.0);
        let after_reset = tracker.observe(None, Some(reset));
        assert_eq!(after_reset.token_delta, 0.0);
        assert_eq!(after_reset.credit_delta, 0.0);
    }

    #[test]
    fn abtop_activity_creates_a_burst_then_decays() {
        let mut tracker = ActivityTracker::default();
        let active = json!({
            "interval_ms": 1000,
            "token_rate": 20.0,
            "sessions_active": 2
        });

        let first = tracker.observe(Some(&active), None);
        let idle = tracker.observe(None, None);

        assert!(first.energy > 0.0);
        assert!(first.burst > 0.0);
        assert!(idle.energy < first.energy);
        assert!(idle.burst < first.burst);
    }

    #[test]
    fn reader_skips_non_settings_files() {
        let root = std::env::temp_dir().join(format!(
            "vimit-activity-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("session.jsonl"), "{\"tokenUsage\": 9999}").unwrap();
        fs::write(
            root.join("safe.settings.json"),
            "{\"tokenUsage\": 12, \"factoryCredits\": 1}",
        )
        .unwrap();

        let snapshot = read_factory_activity_snapshot(&root).unwrap();
        assert_eq!(snapshot.token_usage, 12.0);
        assert_eq!(snapshot.credits, 1.0);
        fs::remove_dir_all(root).unwrap();
    }
}
