use chrono::{DateTime, SecondsFormat, TimeZone, Utc};
use serde_json::{Map, Value, json};
use std::collections::HashMap;

use crate::{Metric, WINDOWS, WindowState, format_duration_secs, short_rate, to_number};

pub fn demo_payload() -> Value {
    let now = Utc::now();
    let five_start = now - chrono::Duration::minutes(28);
    let day_start = now - chrono::Duration::hours(5);
    let week_start = now - chrono::Duration::days(1);
    let month_start = now - chrono::Duration::days(9);
    json!({
        "usage": {
            "rows": [
                {
                    "model": "demo-model",
                    "creditLimit5Hours": 50,
                    "creditLimit24Hours": 180,
                    "creditLimit7Days": 600,
                    "creditLimit30Days": 2000,
                    "requestLimit5Hours": 1000,
                    "requestLimit24Hours": 4000,
                    "requestLimit7Days": 20000,
                    "requestLimit30Days": 80000,
                    "credits5Hours": 39,
                    "credits24Hours": 91,
                    "credits7Days": 214,
                    "credits30Days": 819,
                    "requests5Hours": 610,
                    "requests24Hours": 1510,
                    "requests7Days": 8300,
                    "requests30Days": 26000,
                    "window5HoursStartedAt": five_start.to_rfc3339_opts(SecondsFormat::Secs, true),
                    "window5HoursEndsAt": (now + chrono::Duration::hours(4)).to_rfc3339_opts(SecondsFormat::Secs, true),
                    "window24HoursStartedAt": day_start.to_rfc3339_opts(SecondsFormat::Secs, true),
                    "window24HoursEndsAt": (now + chrono::Duration::hours(19)).to_rfc3339_opts(SecondsFormat::Secs, true),
                    "window7DaysStartedAt": week_start.to_rfc3339_opts(SecondsFormat::Secs, true),
                    "window7DaysEndsAt": (now + chrono::Duration::days(6)).to_rfc3339_opts(SecondsFormat::Secs, true),
                    "window30DaysStartedAt": month_start.to_rfc3339_opts(SecondsFormat::Secs, true),
                    "window30DaysEndsAt": (now + chrono::Duration::days(21)).to_rfc3339_opts(SecondsFormat::Secs, true)
                }
            ]
        }
    })
}

pub fn summarize_me(
    payload: &Value,
    warning_threshold: f64,
    danger_threshold: f64,
) -> Vec<WindowState> {
    summarize_me_with_thresholds(
        payload,
        warning_threshold,
        danger_threshold,
        &HashMap::new(),
    )
}

pub fn summarize_me_with_thresholds(
    payload: &Value,
    warning_threshold: f64,
    danger_threshold: f64,
    window_thresholds: &HashMap<String, (f64, f64)>,
) -> Vec<WindowState> {
    let rows = extract_usage_rows(payload);
    let now = Utc::now();
    let mut summaries = Vec::new();

    for (key, suffix, _start_field, reset_field) in WINDOWS {
        let credits = summarize_metric(
            &rows,
            &format!("credits{suffix}"),
            &format!("creditLimit{suffix}"),
        );
        let requests = summarize_metric(
            &rows,
            &format!("requests{suffix}"),
            &format!("requestLimit{suffix}"),
        );
        let reset_value = first_value(&rows, reset_field);
        let (reset, reset_in_seconds) = parse_reset(reset_value, now);
        let percent = peak_percent(credits.as_ref(), requests.as_ref()).unwrap_or(0.0);

        if credits.is_none() && requests.is_none() && reset_in_seconds.is_none() {
            continue;
        }

        let (w, d) = window_thresholds
            .get(key)
            .copied()
            .unwrap_or((warning_threshold, danger_threshold));

        let level = window_level(credits.as_ref(), requests.as_ref(), w, d);

        summaries.push(WindowState {
            key,
            level: level.to_string(),
            reset,
            reset_in_seconds,
            credits,
            requests,
            percent,
        });
    }
    summaries
}

fn extract_usage_rows(payload: &Value) -> Vec<&Map<String, Value>> {
    if let Some(rows) = payload
        .get("usage")
        .and_then(Value::as_object)
        .and_then(|u| u.get("rows"))
        .and_then(Value::as_array)
    {
        let parsed = object_rows(rows);
        if !parsed.is_empty() {
            return parsed;
        }
    }

    if let Some(rows) = payload
        .get("data")
        .and_then(Value::as_object)
        .and_then(|d| d.get("usage"))
        .and_then(Value::as_object)
        .and_then(|u| u.get("rows"))
        .and_then(Value::as_array)
    {
        let parsed = object_rows(rows);
        if !parsed.is_empty() {
            return parsed;
        }
    }

    if let Some(rows) = payload.get("rows").and_then(Value::as_array) {
        let parsed = object_rows(rows);
        if !parsed.is_empty() {
            return parsed;
        }
    }

    if let Some(rows) = payload
        .get("data")
        .and_then(Value::as_object)
        .and_then(|d| d.get("rows"))
        .and_then(Value::as_array)
    {
        let parsed = object_rows(rows);
        if !parsed.is_empty() {
            return parsed;
        }
    }

    if let Some(rows) = payload.get("usage").and_then(Value::as_array) {
        let parsed = object_rows(rows);
        if !parsed.is_empty() {
            return parsed;
        }
    }

    if let Some(object) = payload.as_object() {
        for (_key, value) in object {
            if let Some(rows) = value.as_array() {
                let parsed = object_rows(rows);
                if parsed.iter().any(|row| has_usage_fields(row)) {
                    return parsed;
                }
            }
        }
        if let Some(data) = object.get("data").and_then(Value::as_object) {
            for (_key, value) in data {
                if let Some(rows) = value.as_array() {
                    let parsed = object_rows(rows);
                    if parsed.iter().any(|row| has_usage_fields(row)) {
                        return parsed;
                    }
                }
            }
        }
    }

    Vec::new()
}

fn has_usage_fields(row: &Map<String, Value>) -> bool {
    for key in row.keys() {
        let lower = key.to_lowercase();
        if (lower.contains("credit") || lower.contains("request") || lower.contains("limit"))
            && row.get(key.as_str()).and_then(to_number).is_some()
        {
            return true;
        }
    }
    false
}

fn object_rows(rows: &[Value]) -> Vec<&Map<String, Value>> {
    rows.iter().filter_map(Value::as_object).collect()
}

fn summarize_metric(
    rows: &[&Map<String, Value>],
    used_field: &str,
    limit_field: &str,
) -> Option<Metric> {
    let mut used_total = 0.0;
    let mut limits = Vec::<f64>::new();
    let mut seen = false;

    for row in rows {
        let used = row.get(used_field).and_then(to_number);
        let limit = row.get(limit_field).and_then(to_number);
        if used.is_none() && limit.is_none() {
            continue;
        }
        seen = true;
        used_total += used.unwrap_or(0.0);
        if let Some(limit) = limit.filter(|&l: &f64| {
            l > 0.0
                && !limits
                    .iter()
                    .any(|existing| (*existing - l).abs() < f64::EPSILON)
        }) {
            limits.push(limit);
        }
    }

    let limit_total: f64 = limits.iter().sum();
    if !seen || limit_total <= 0.0 {
        return None;
    }

    let remaining = (limit_total - used_total).max(0.0);
    let percent = ((used_total / limit_total) * 100.0).clamp(0.0, 999.0);
    Some(Metric {
        used: used_total,
        limit: limit_total,
        remaining,
        percent,
    })
}

fn first_value<'a>(rows: &[&'a Map<String, Value>], field: &str) -> Option<&'a Value> {
    rows.iter().find_map(|row| match row.get(field) {
        Some(Value::Null) | None => None,
        Some(Value::String(text)) if text.is_empty() => None,
        value => value,
    })
}

fn parse_datetime_value(value: &Value) -> Option<DateTime<Utc>> {
    match value {
        Value::Number(number) => number
            .as_i64()
            .and_then(|timestamp| Utc.timestamp_opt(timestamp, 0).single()),
        Value::String(text) => {
            let raw = text.trim();
            if raw.is_empty() {
                None
            } else if let Ok(timestamp) = raw.parse::<i64>() {
                Utc.timestamp_opt(timestamp, 0).single()
            } else {
                DateTime::parse_from_rfc3339(raw)
                    .map(|datetime| datetime.with_timezone(&Utc))
                    .ok()
            }
        }
        _ => None,
    }
}

fn parse_reset(value: Option<&Value>, now: DateTime<Utc>) -> (String, Option<i64>) {
    let Some(value) = value else {
        return ("сброс неизвестен".to_string(), None);
    };

    if let Some(datetime) = parse_datetime_value(value) {
        let seconds = (datetime - now).num_seconds().max(0);
        (
            format!("сброс {}", format_duration_secs(seconds)),
            Some(seconds),
        )
    } else {
        let text = value
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| value.to_string());
        (format!("сброс в {text}"), None)
    }
}

pub fn window_level(
    credits: Option<&Metric>,
    requests: Option<&Metric>,
    warning_threshold: f64,
    danger_threshold: f64,
) -> &'static str {
    let peak = peak_percent(credits, requests);
    match peak {
        Some(peak) if peak >= danger_threshold => "danger",
        Some(peak) if peak >= warning_threshold => "warning",
        Some(_) => "ok",
        None => "unknown",
    }
}

pub fn peak_percent(credits: Option<&Metric>, requests: Option<&Metric>) -> Option<f64> {
    [credits, requests]
        .into_iter()
        .flatten()
        .map(|metric| metric.percent)
        .fold(None, |peak: Option<f64>, percent| {
            Some(peak.map_or(percent, |peak| peak.max(percent)))
        })
}

pub fn peak_percent_all(windows: &[WindowState]) -> Option<f64> {
    windows
        .iter()
        .map(|w| w.percent)
        .fold(None, |peak: Option<f64>, value| {
            Some(peak.map_or(value, |peak| peak.max(value)))
        })
}

pub fn worst_level(windows: &[WindowState]) -> &str {
    windows
        .iter()
        .map(|window| window.level.as_str())
        .max_by_key(|level| level_rank(level))
        .unwrap_or("unknown")
}

fn level_rank(level: &str) -> u8 {
    match level {
        "danger" => 3,
        "warning" => 2,
        "ok" => 1,
        _ => 0,
    }
}

pub fn rate_text(
    credits: Option<&Metric>,
    requests: Option<&Metric>,
    start: Option<&Value>,
    now: DateTime<Utc>,
) -> String {
    let Some(start) = start.and_then(parse_datetime_value) else {
        return "темп окна: нет window*StartedAt".to_string();
    };
    let elapsed_minutes = ((now - start).num_seconds().max(60) as f64) / 60.0;
    let credit_rate = credits
        .map(|metric| format!("{}/мин", short_rate(metric.used / elapsed_minutes)))
        .unwrap_or_else(|| "н/д".to_string());
    let request_rate = requests
        .map(|metric| format!("{}/мин", short_rate(metric.used / elapsed_minutes)))
        .unwrap_or_else(|| "н/д".to_string());

    format!("темп окна: кред {credit_rate} | запр {request_rate}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarizes_credit_and_request_windows() {
        let windows = summarize_me(&demo_payload(), 75.0, 90.0);

        assert_eq!(
            windows.iter().map(|window| window.key).collect::<Vec<_>>(),
            ["5h", "24h", "7d", "30d"]
        );
        assert_eq!(windows[0].level, "warning");
        assert_eq!(windows[0].credits.as_ref().unwrap().percent, 78.0);
    }

    #[test]
    fn repeated_limit_rows_do_not_double_count_cap() {
        let payload = json!({
            "usage": {
                "rows": [
                    {"credits5Hours": 10, "creditLimit5Hours": 50},
                    {"credits5Hours": 20, "creditLimit5Hours": 50}
                ]
            }
        });

        let windows = summarize_me(&payload, 75.0, 90.0);

        let credits = windows[0].credits.as_ref().unwrap();
        assert_eq!(credits.used, 30.0);
        assert_eq!(credits.limit, 50.0);
        assert_eq!(credits.percent, 60.0);
    }

    #[test]
    fn custom_thresholds_change_window_level() {
        let windows = summarize_me(&demo_payload(), 80.0, 95.0);

        assert_eq!(windows[0].level, "ok");
    }

    #[test]
    fn window_level_changes_on_threshold_boundaries() {
        let mut credits = Metric {
            used: 74.9,
            limit: 100.0,
            remaining: 25.1,
            percent: 74.9,
        };

        assert_eq!(window_level(Some(&credits), None, 75.0, 90.0), "ok");

        credits.percent = 75.0;
        assert_eq!(window_level(Some(&credits), None, 75.0, 90.0), "warning");

        credits.percent = 89.9;
        assert_eq!(window_level(Some(&credits), None, 75.0, 90.0), "warning");

        credits.percent = 90.0;
        assert_eq!(window_level(Some(&credits), None, 75.0, 90.0), "danger");
    }

    #[test]
    fn summarize_metric_clamps_over_limit_percent() {
        let payload = json!({
            "usage": {
                "rows": [{"credits5Hours": 1500, "creditLimit5Hours": 100}]
            }
        });

        let windows = summarize_me(&payload, 75.0, 90.0);
        let credits = windows[0].credits.as_ref().unwrap();

        assert_eq!(credits.remaining, 0.0);
        assert_eq!(credits.percent, 999.0);
        assert_eq!(windows[0].level, "danger");
    }

    #[test]
    fn peak_percent_returns_max() {
        let windows = summarize_me(&demo_payload(), 75.0, 90.0);
        assert_eq!(
            peak_percent(windows[0].credits.as_ref(), windows[0].requests.as_ref()).unwrap(),
            78.0
        );
    }

    #[test]
    fn extract_usage_rows_handles_nested_data_usage() {
        let payload = json!({
            "data": {
                "usage": {
                    "rows": [{"credits5Hours": 10, "creditLimit5Hours": 100}]
                }
            }
        });
        let windows = summarize_me(&payload, 75.0, 90.0);
        assert!(!windows.is_empty());
        assert_eq!(windows[0].credits.as_ref().unwrap().used, 10.0);
    }

    #[test]
    fn extract_usage_rows_handles_flat_rows() {
        let payload = json!({
            "rows": [{"credits5Hours": 20, "creditLimit5Hours": 100}]
        });
        let windows = summarize_me(&payload, 75.0, 90.0);
        assert!(!windows.is_empty());
    }

    #[test]
    fn extract_usage_rows_handles_string_numbers() {
        let payload = json!({
            "usage": {"rows": [{"credits5Hours": "30", "creditLimit5Hours": "100"}]}
        });
        let windows = summarize_me(&payload, 75.0, 90.0);
        assert!(!windows.is_empty());
        assert_eq!(windows[0].credits.as_ref().unwrap().used, 30.0);
    }

    #[test]
    fn summarize_me_returns_empty_for_unknown_schema() {
        let payload = json!({"something": "unrelated"});
        let windows = summarize_me(&payload, 75.0, 90.0);
        assert!(windows.is_empty());
    }
}
