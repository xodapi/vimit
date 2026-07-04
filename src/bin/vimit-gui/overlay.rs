use std::collections::VecDeque;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use slint::{ComponentHandle, PhysicalSize};

use crate::AppWindow;
use crate::ng;

static CREATURE_SOUND_LEVEL: OnceLock<Mutex<Option<String>>> = OnceLock::new();

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    #[link_name = "Beep"]
    fn windows_beep(dw_freq: u32, dw_duration: u32) -> i32;
}

const OVERLAY_RATE_WINDOW: Duration = Duration::from_secs(5 * 60);
pub(crate) const OVERLAY_HISTORY_RETENTION: Duration = Duration::from_secs(30 * 60);
const OVERLAY_SPARK_SAMPLES: usize = 15;
const OVERLAY_COMPACT_SIZE: (f32, f32) = (260.0, 52.0);
const OVERLAY_FULL_SIZE: (f32, f32) = (340.0, 380.0);
const CREATURE_MIN_POINTS: usize = 8;
const CREATURE_MAX_POINTS: usize = 128;
const CREATURE_ORGANIC_MAX_POINTS: usize = 64;

pub(crate) fn overlay_logical_size(compact: bool) -> (f32, f32) {
    if compact {
        OVERLAY_COMPACT_SIZE
    } else {
        OVERLAY_FULL_SIZE
    }
}

pub(crate) fn set_overlay_window_size(app: &AppWindow, compact: bool) {
    let (width, height) = overlay_logical_size(compact);
    let scale = app.window().scale_factor();
    app.window().set_size(PhysicalSize::new(
        (width * scale).round() as u32,
        (height * scale).round() as u32,
    ));
}

pub(crate) fn creature_node_count(percent: f32) -> usize {
    let normalized = (percent.clamp(0.0, 100.0) / 100.0) as f64;
    (CREATURE_MIN_POINTS + (normalized * 120.0).floor() as usize)
        .clamp(CREATURE_MIN_POINTS, CREATURE_MAX_POINTS)
}

pub(crate) fn organic_creature_node_count(percent: f32) -> usize {
    let normalized = percent.clamp(0.0, 100.0) / 100.0;
    (CREATURE_MIN_POINTS
        + (normalized * (CREATURE_ORGANIC_MAX_POINTS - CREATURE_MIN_POINTS) as f32).floor()
            as usize)
        .clamp(CREATURE_MIN_POINTS, CREATURE_ORGANIC_MAX_POINTS)
}

pub(crate) fn creature_node_count_for_skin(percent: f32, skin: i32) -> usize {
    if skin == 1 {
        organic_creature_node_count(percent)
    } else {
        creature_node_count(percent)
    }
}

pub(crate) fn creature_path_commands_for_skin(percent: f32, phase: f32, skin: i32) -> String {
    if skin == 1 {
        organic_creature_path_commands(percent, phase)
    } else {
        creature_path_commands(percent, phase)
    }
}

pub(crate) fn creature_path_commands(percent: f32, phase: f32) -> String {
    let count = creature_node_count(percent);
    let center_x = 52.0f32;
    let center_y = 44.0f32;
    let base_radius = 26.0 + percent.clamp(0.0, 100.0) * 0.10;
    let mut points = Vec::with_capacity(count);

    for idx in 0..count {
        let angle = (idx as f32 / count as f32) * std::f32::consts::TAU;
        let jitter = (phase.to_radians() * 1.7 + idx as f32 * 1.618).sin() * 3.4
            + (phase.to_radians() * 0.7 + idx as f32 * 2.413).cos() * 1.8;
        let radius = (base_radius + jitter).clamp(22.0, 39.0);
        points.push((
            center_x + radius * angle.cos(),
            center_y + radius * angle.sin(),
        ));
    }

    smooth_closed_path(&points)
}

fn organic_creature_path_commands(percent: f32, phase: f32) -> String {
    let count = organic_creature_node_count(percent);
    let normalized = percent.clamp(0.0, 100.0) / 100.0;
    let center_x = 52.0f32;
    let center_y = 44.0f32;
    let base_radius = 24.0 + normalized * 10.0;
    let amplitude = 3.0 + normalized * 5.0;
    let waves = 3.0 + (normalized * 5.0).round();
    let speed = if percent >= 90.0 {
        3.0
    } else if percent >= 75.0 {
        1.7
    } else {
        0.7
    };
    let time = phase.to_radians();
    let mut points = Vec::with_capacity(count);

    for idx in 0..count {
        let theta = (idx as f32 / count as f32) * std::f32::consts::TAU;
        let primary = (waves * theta + speed * time).sin();
        let secondary = ((waves * 0.5 + 1.0) * theta - speed * time * 0.65).sin() * 0.35;
        let breath = (time * 0.4).sin() * (1.4 + normalized * 1.8);
        let radius = (base_radius + amplitude * (primary + secondary) + breath).clamp(20.0, 39.0);
        points.push((
            center_x + radius * theta.cos(),
            center_y + radius * theta.sin(),
        ));
    }

    smooth_closed_path(&points)
}

fn smooth_closed_path(points: &[(f32, f32)]) -> String {
    let count = points.len();
    let mut commands = String::with_capacity(count * 34);
    let (start_x, start_y) = points[0];
    commands.push_str(&format!("M {:.2} {:.2} ", start_x, start_y));
    for idx in 0..count {
        let p0 = points[(idx + count - 1) % count];
        let p1 = points[idx];
        let p2 = points[(idx + 1) % count];
        let p3 = points[(idx + 2) % count];
        let c1 = (p1.0 + (p2.0 - p0.0) / 6.0, p1.1 + (p2.1 - p0.1) / 6.0);
        let c2 = (p2.0 - (p3.0 - p1.0) / 6.0, p2.1 - (p3.1 - p1.1) / 6.0);
        commands.push_str(&format!(
            "C {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} ",
            c1.0, c1.1, c2.0, c2.1, p2.0, p2.1
        ));
    }
    commands.push('Z');
    commands
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CreatureSound {
    Warning,
    Danger,
    Recovery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CreatureState {
    Sleeping,
    Awake,
    Alert,
    Critical,
    Recovery,
}

impl CreatureState {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Sleeping => "sleeping",
            Self::Awake => "awake",
            Self::Alert => "alert",
            Self::Critical => "critical",
            Self::Recovery => "recovery",
        }
    }
}

pub(crate) fn creature_state_for(
    percent: f32,
    credit_rate: f32,
    reset_detected: bool,
    samples_len: usize,
) -> CreatureState {
    if reset_detected {
        return CreatureState::Recovery;
    }
    if samples_len > 1 && credit_rate <= 0.05 {
        return CreatureState::Sleeping;
    }
    if percent >= 90.0 {
        CreatureState::Critical
    } else if percent >= 75.0 {
        CreatureState::Alert
    } else {
        CreatureState::Awake
    }
}

pub(crate) fn overlay_phase_step(state: CreatureState) -> f32 {
    match state {
        CreatureState::Sleeping => 2.0,
        CreatureState::Awake => 7.0,
        CreatureState::Alert => 9.0,
        CreatureState::Critical => 13.0,
        CreatureState::Recovery => 16.0,
    }
}

pub(crate) fn creature_state_from_str(value: &str) -> CreatureState {
    match value {
        "sleeping" => CreatureState::Sleeping,
        "alert" => CreatureState::Alert,
        "critical" => CreatureState::Critical,
        "recovery" => CreatureState::Recovery,
        _ => CreatureState::Awake,
    }
}

fn level_rank(level: &str) -> u8 {
    match level {
        "danger" => 2,
        "warning" => 1,
        _ => 0,
    }
}

pub(crate) fn creature_sound_for_transition(
    previous: Option<&str>,
    current: &str,
) -> Option<CreatureSound> {
    let previous = previous?;
    let previous_rank = level_rank(previous);
    let current_rank = level_rank(current);

    if current_rank == previous_rank {
        None
    } else if previous_rank < current_rank {
        Some(match current_rank {
            1 => CreatureSound::Warning,
            2 => CreatureSound::Danger,
            _ => return None,
        })
    } else if previous_rank == 2 && current_rank == 0 {
        Some(CreatureSound::Recovery)
    } else {
        None
    }
}

pub(crate) fn maybe_play_creature_sound(current_level: &str) {
    let store = CREATURE_SOUND_LEVEL.get_or_init(|| Mutex::new(None));
    let mut previous = store.lock().unwrap();
    let sound = creature_sound_for_transition(previous.as_deref(), current_level);
    *previous = Some(current_level.to_string());
    drop(previous);

    if let Some(sound) = sound {
        thread::spawn(move || play_creature_sound_blocking(sound));
    }
}

fn play_creature_sound_blocking(sound: CreatureSound) {
    #[cfg(windows)]
    unsafe {
        match sound {
            CreatureSound::Warning => {
                let _ = windows_beep(440, 200);
            }
            CreatureSound::Danger => {
                let _ = windows_beep(440, 150);
                let _ = windows_beep(220, 150);
            }
            CreatureSound::Recovery => {
                let _ = windows_beep(880, 150);
            }
        }
    }

    #[cfg(not(windows))]
    {
        let _ = sound;
    }
}

#[derive(Default)]
pub(crate) struct OverlayHistory {
    pub(crate) observations: VecDeque<OverlayUsagePoint>,
    samples: VecDeque<f32>,
}

pub(crate) struct OverlayUsagePoint {
    at: Instant,
    used: f64,
}

impl OverlayHistory {
    pub(crate) fn record_usage(&mut self, at: Instant, used: f64) -> (f32, bool) {
        let reset_detected = self
            .observations
            .back()
            .is_some_and(|latest| used < latest.used);
        self.observations.push_back(OverlayUsagePoint { at, used });
        self.trim_observations(at);

        let rate = self.average_credit_rate(at);
        if self.samples.len() == OVERLAY_SPARK_SAMPLES {
            self.samples.pop_front();
        }
        self.samples.push_back(rate);
        (rate, reset_detected)
    }

    pub(crate) fn average_credit_rate(&self, now: Instant) -> f32 {
        let Some(latest) = self.observations.back() else {
            return 0.0;
        };

        let baseline = self
            .observations
            .iter()
            .find(|point| within_duration(now, point.at, OVERLAY_RATE_WINDOW))
            .unwrap_or(latest);

        let elapsed_min = OVERLAY_RATE_WINDOW.as_secs_f64() / 60.0;
        ((latest.used - baseline.used).max(0.0) / elapsed_min) as f32
    }

    fn trim_observations(&mut self, now: Instant) {
        while self
            .observations
            .front()
            .is_some_and(|point| !within_duration(now, point.at, OVERLAY_HISTORY_RETENTION))
        {
            self.observations.pop_front();
        }
    }
}

fn within_duration(now: Instant, earlier: Instant, duration: Duration) -> bool {
    now.checked_duration_since(earlier)
        .is_some_and(|elapsed| elapsed <= duration)
}

pub(crate) struct OverlayState {
    pub(crate) credit_rate_text: String,
    pub(crate) token_rate_text: String,
    pub(crate) percent_hour_text: String,
    pub(crate) spark_data: Vec<f32>,
    pub(crate) creature_percent: f32,
    pub(crate) creature_state: CreatureState,
    pub(crate) delta_text: String,
    pub(crate) delta_level: String,
    pub(crate) reset_label: String,
    pub(crate) reset_seconds: i32,
}

pub(crate) fn build_overlay_state(
    windows: &[ng::WindowState],
    token_rate_text: &str,
    history: &Arc<Mutex<OverlayHistory>>,
) -> OverlayState {
    let now = Instant::now();
    let five = windows.iter().find(|window| window.key == "5h");
    let current_used = five.and_then(|window| window.credits.as_ref().map(|metric| metric.used));
    let credit_limit = five
        .and_then(|window| window.credits.as_ref().map(|metric| metric.limit))
        .unwrap_or(0.0);

    let mut history = history.lock().unwrap();
    let (credit_rate, reset_detected) = current_used
        .map(|used| history.record_usage(now, used))
        .unwrap_or_default();

    let samples: Vec<f32> = history.samples.iter().copied().collect();
    let spark_data = scale_samples(&samples);
    let token_rate = parse_rate_value(token_rate_text).unwrap_or(credit_rate * 750.0);
    let creature_percent = five.map(|window| window.percent as f32).unwrap_or(0.0);
    let creature_state = creature_state_for(
        creature_percent,
        credit_rate,
        reset_detected,
        history.samples.len(),
    );
    let percent_hour = if credit_limit > 0.0 {
        (credit_rate as f64 / credit_limit * 60.0 * 100.0) as f32
    } else {
        0.0
    };
    let (delta_text, delta_level) = overlay_delta(&samples);
    let (reset_label, reset_seconds) = nearest_reset(windows);

    OverlayState {
        credit_rate_text: format!("{} кред/мин", one_decimal_local(credit_rate)),
        token_rate_text: format!("{} токены/мин", one_decimal_local(token_rate)),
        percent_hour_text: format!("{}%/час", one_decimal_local(percent_hour)),
        spark_data,
        creature_percent,
        creature_state,
        delta_text,
        delta_level,
        reset_label,
        reset_seconds,
    }
}

fn scale_samples(samples: &[f32]) -> Vec<f32> {
    let max = samples.iter().copied().fold(0.0f32, f32::max).max(1.0);
    samples
        .iter()
        .map(|value| ((*value / max) * 100.0).clamp(2.0, 100.0))
        .collect()
}

fn overlay_delta(samples: &[f32]) -> (String, String) {
    let Some((&latest, previous)) = samples.split_last() else {
        return ("стабильно".to_string(), "ok".to_string());
    };
    if previous.is_empty() {
        return ("старт".to_string(), "ok".to_string());
    }
    let avg = previous.iter().sum::<f32>() / previous.len() as f32;
    if avg <= 0.05 {
        if latest <= 0.05 {
            return ("стабильно".to_string(), "ok".to_string());
        }
        return ("новый расход".to_string(), "warning".to_string());
    }
    let delta = ((latest - avg) / avg) * 100.0;
    let level = if delta >= 50.0 {
        "danger"
    } else if delta >= 15.0 {
        "warning"
    } else {
        "ok"
    };
    let sign = if delta >= 0.0 { "+" } else { "" };
    (
        format!("{sign}{}% к норме", one_decimal_local(delta)),
        level.to_string(),
    )
}

fn nearest_reset(windows: &[ng::WindowState]) -> (String, i32) {
    let nearest = windows
        .iter()
        .filter_map(|window| {
            window
                .reset_in_seconds
                .map(|seconds| (window.key, seconds.max(0)))
        })
        .min_by_key(|(_, seconds)| *seconds);

    match nearest {
        Some((key, seconds)) => (
            format!("сброс {key} окна"),
            seconds.min(i32::MAX as i64) as i32,
        ),
        None => ("сброс окна".to_string(), -1),
    }
}

pub(crate) fn format_overlay_countdown(seconds: i32) -> String {
    if seconds < 0 {
        "unknown".to_string()
    } else {
        ng::format_duration_secs(seconds as i64)
    }
}

pub(crate) fn parse_rate_value(text: &str) -> Option<f32> {
    text.split_whitespace()
        .next()
        .and_then(|value| value.replace(',', ".").parse::<f32>().ok())
}

pub(crate) fn one_decimal_local(value: f32) -> String {
    format!("{value:.1}").replace('.', ",")
}
