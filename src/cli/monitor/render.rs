use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, Paragraph, Sparkline};
use std::collections::HashMap;

use crate::{self as ng, VERSION};

use super::super::args::Preset;
use super::super::theme::{Palette, Theme};
use super::super::trends::TrendDay;
use super::helpers::{monitor_alerts, render_alert_lines};
use super::state::{PanelState, StatusSnapshot, WindowHistory};

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_frame(
    frame: &mut ratatui::Frame,
    snapshot: Option<&StatusSnapshot>,
    error: Option<&str>,
    interval_secs: u64,
    next_refresh_secs: u64,
    with_abtop: bool,
    warning_threshold: f64,
    window_history: &HashMap<&str, WindowHistory>,
    preset: Preset,
    theme: Theme,
    panels: &PanelState,
    account_names: &[String],
    cur_account: usize,
    trend_days: &[TrendDay],
) {
    if panels.show_help {
        draw_help_overlay(frame, theme);
        return;
    }

    let area = frame.area();
    let pal = theme.palette();
    let (header_len, footer_len) = match preset {
        Preset::Mini => (1, 1),
        Preset::Compact => (2, 1),
        Preset::Full => (3, 3),
    };

    let mut constraints = Vec::new();
    if panels.show_header {
        constraints.push(Constraint::Length(header_len));
    }
    constraints.push(Constraint::Min(4));
    if panels.show_trends {
        constraints.push(Constraint::Length(8));
    }
    if panels.show_footer {
        constraints.push(Constraint::Length(footer_len));
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    let mut idx = 0;
    if panels.show_header {
        draw_header(
            frame,
            chunks[idx],
            snapshot,
            &pal,
            account_names,
            cur_account,
            panels,
        );
        idx += 1;
    }
    if panels.show_quota {
        draw_body(
            frame,
            chunks[idx],
            snapshot,
            error,
            with_abtop,
            warning_threshold,
            window_history,
            preset,
            &pal,
            panels,
        );
        idx += 1;
    }
    if panels.show_trends {
        draw_trend_panel(frame, chunks[idx], trend_days, &pal, preset);
        idx += 1;
    }
    if panels.show_footer {
        let current_account_name = if account_names.len() > 1 {
            Some(account_names[cur_account].as_str())
        } else if account_names.len() == 1 {
            Some(account_names[0].as_str())
        } else {
            None
        };
        draw_footer(
            frame,
            chunks[idx],
            interval_secs,
            next_refresh_secs,
            &pal,
            current_account_name,
        );
    }
}

fn draw_header(
    frame: &mut ratatui::Frame,
    area: Rect,
    snapshot: Option<&StatusSnapshot>,
    pal: &Palette,
    account_names: &[String],
    cur_account: usize,
    panels: &PanelState,
) {
    let account_prefix = if account_names.len() > 1 {
        let mut list = String::new();
        for (i, name) in account_names.iter().enumerate() {
            if i > 0 {
                list.push_str(" | ");
            }
            if i == cur_account {
                list.push('*');
                list.push_str(name);
                list.push('*');
            } else {
                list.push_str(name);
            }
        }
        format!(" [ {list} ] (Tab to switch)")
    } else if account_names.len() == 1 {
        format!(" [{}]", account_names[0])
    } else {
        String::new()
    };
    let vpn_label = if panels.vpn_mode { " VPN" } else { " Dir" };
    let (title, style) = match snapshot {
        Some(s) => {
            let level = ng::worst_level(&s.windows);
            let peak = ng::peak_percent_all(&s.windows)
                .map(|v| format!("{v:.0}%"))
                .unwrap_or_else(|| "n/a".into());
            let label = if level == "danger" {
                "DANGER"
            } else if level == "warning" {
                "WARNING"
            } else {
                "OK"
            };
            let stale_tag = if s.stale { " STALE-CACHE" } else { "" };
            let latency_tag = if s.latency_ms > 0 {
                format!(" {}ms", s.latency_ms)
            } else {
                String::new()
            };
            let endpoint_tag = if s.api_endpoint != "r-api" && !s.api_endpoint.is_empty() {
                format!(" /{}", s.api_endpoint)
            } else {
                String::new()
            };
            let mut title = format!(
                " VibeMode v{VERSION}{account_prefix}{vpn_label}{stale_tag}{latency_tag}{endpoint_tag} | {label} | peak {peak} "
            );
            let mut final_style = pal.bold_level_style(level);

            if let Some(min) = s.offline_duration_min {
                title.push_str(&format!("| API offline {min}m "));
                final_style = pal.danger.into();
            }

            (title, final_style)
        }
        None => (
            format!(" VibeMode v{VERSION}{account_prefix}{vpn_label} | waiting for data... "),
            Style::default().fg(pal.muted),
        ),
    };

    let header = Paragraph::new(Line::from(Span::styled(title, style))).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(pal.border_style()),
    );
    frame.render_widget(header, area);
}

#[allow(clippy::too_many_arguments)]
fn draw_body(
    frame: &mut ratatui::Frame,
    area: Rect,
    snapshot: Option<&StatusSnapshot>,
    error: Option<&str>,
    _with_abtop: bool,
    warning_threshold: f64,
    window_history: &HashMap<&str, WindowHistory>,
    preset: Preset,
    pal: &Palette,
    panels: &PanelState,
) {
    let (alerts_height, cols) = match preset {
        Preset::Mini => (1, 1),
        Preset::Compact => (2, 1),
        Preset::Full => (5, 2),
    };

    let mut body_constraints = Vec::new();
    body_constraints.push(Constraint::Min(4));
    if panels.show_alerts {
        body_constraints.push(Constraint::Length(alerts_height));
    }
    if panels.show_agents {
        body_constraints.push(Constraint::Length(alerts_height));
    }

    let body_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(body_constraints)
        .split(area);

    let windows_area = body_chunks[0];
    let mut idx = 1;

    if let Some(snapshot) = snapshot {
        let rows = (snapshot.windows.len() as u16).div_ceil(cols).max(1);
        let mut row_constraints = Vec::new();
        for _ in 0..rows {
            row_constraints.push(Constraint::Percentage(100 / rows));
        }

        let grid = Layout::default()
            .direction(Direction::Vertical)
            .constraints(row_constraints)
            .split(windows_area);

        for (i, window) in snapshot.windows.iter().enumerate() {
            let row = (i as u16) / cols;
            let col = (i as u16) % cols;
            let cell = if cols == 1 {
                grid[row as usize]
            } else {
                let col_chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(50); 2])
                    .split(grid[row as usize]);
                col_chunks[col as usize]
            };
            draw_window_card(
                frame,
                cell,
                window,
                window_history.get(window.key),
                warning_threshold,
                preset,
                pal,
            );
        }
    } else {
        let waiting = Paragraph::new(vec![
            Line::from(Span::styled(
                "Collecting VibeMode status...",
                pal.muted_style(),
            )),
            Line::from(Span::styled(
                "Press r to refresh, ? for help",
                pal.muted_style(),
            )),
        ])
        .block(
            Block::default()
                .title("Limits")
                .borders(Borders::ALL)
                .border_style(pal.border_style()),
        );
        frame.render_widget(waiting, windows_area);
    }

    if panels.show_alerts {
        let alerts_area = body_chunks[idx];
        idx += 1;
        draw_alerts(frame, alerts_area, snapshot, warning_threshold, pal);
    }

    if panels.show_agents {
        let agents_area = body_chunks[idx];
        draw_agents_panel(frame, agents_area, snapshot, pal);
    }

    if let Some(error) = error {
        let error_area = Rect {
            y: area.y + area.height.saturating_sub(1),
            height: 1,
            ..area
        };
        let err_widget = Paragraph::new(Span::styled(
            format!(" ! {error}"),
            Style::default().fg(pal.danger),
        ));
        frame.render_widget(err_widget, error_area);
    }
}

fn draw_window_card(
    frame: &mut ratatui::Frame,
    area: Rect,
    window: &ng::WindowState,
    history: Option<&WindowHistory>,
    _warning_threshold: f64,
    preset: Preset,
    pal: &Palette,
) {
    let peak = ng::peak_percent(window.credits.as_ref(), window.requests.as_ref()).unwrap_or(0.0);
    let title_style = pal.bold_level_style(&window.level);
    let reset_text = ng::format_duration_opt(window.reset_in_seconds);

    match preset {
        Preset::Mini => {
            let line = Line::from(vec![
                Span::styled(format!("{} ", window.key), title_style),
                Span::styled(format!("{:.0}%", peak), pal.level_style(&window.level)),
                Span::styled(format!(" reset {}", reset_text), pal.muted_style()),
            ]);
            let widget = Paragraph::new(line).block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(pal.border_style()),
            );
            frame.render_widget(widget, area);
        }
        Preset::Compact => {
            let title = format!(" {} | {} ", window.key, window.level);
            let inner = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Min(1)])
                .margin(1)
                .split(area);

            let gauge = Gauge::default()
                .block(
                    Block::default()
                        .title(title)
                        .title_style(title_style)
                        .borders(Borders::ALL)
                        .border_style(pal.border_style()),
                )
                .gauge_style(pal.gauge_style(&window.level))
                .ratio(peak / 100.0);
            frame.render_widget(gauge, inner[0]);

            let credit_line = match &window.credits {
                Some(m) => Line::from(vec![
                    Span::styled("cr ", pal.muted_style()),
                    Span::raw(format!(
                        "{}/{} ({:.0}%)",
                        ng::short_number(m.used),
                        ng::short_number(m.limit),
                        m.percent
                    )),
                ]),
                None => Line::from(Span::styled("cr n/a", pal.muted_style())),
            };
            let request_line = match &window.requests {
                Some(m) => Line::from(vec![
                    Span::styled("rq ", pal.muted_style()),
                    Span::raw(format!(
                        "{}/{} ({:.0}%)",
                        ng::short_number(m.used),
                        ng::short_number(m.limit),
                        m.percent
                    )),
                ]),
                None => Line::from(Span::styled("rq n/a", pal.muted_style())),
            };
            let metrics = Paragraph::new(vec![credit_line, request_line]);
            frame.render_widget(metrics, inner[1]);
        }
        Preset::Full => {
            let title = format!(" {} | {} | reset {} ", window.key, window.level, reset_text);
            let inner = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Length(2),
                    Constraint::Min(2),
                ])
                .margin(1)
                .split(area);

            let gauge = Gauge::default()
                .block(
                    Block::default()
                        .title(title)
                        .title_style(title_style)
                        .borders(Borders::ALL)
                        .border_style(pal.border_style()),
                )
                .gauge_style(pal.gauge_style(&window.level))
                .ratio(peak / 100.0);
            frame.render_widget(gauge, inner[0]);

            let credit_line = match &window.credits {
                Some(m) => Line::from(vec![
                    Span::styled("cr ", pal.muted_style()),
                    Span::raw(format!(
                        "{}/{} ({:.0}%)",
                        ng::short_number(m.used),
                        ng::short_number(m.limit),
                        m.percent
                    )),
                ]),
                None => Line::from(Span::styled("cr n/a", pal.muted_style())),
            };
            let request_line = match &window.requests {
                Some(m) => Line::from(vec![
                    Span::styled("rq ", pal.muted_style()),
                    Span::raw(format!(
                        "{}/{} ({:.0}%)",
                        ng::short_number(m.used),
                        ng::short_number(m.limit),
                        m.percent
                    )),
                ]),
                None => Line::from(Span::styled("rq n/a", pal.muted_style())),
            };
            let metrics = Paragraph::new(vec![credit_line, request_line]);
            frame.render_widget(metrics, inner[1]);

            if let Some(hist) = history {
                let values = hist.sparkline_values();
                if !values.is_empty() {
                    let spark = Sparkline::default()
                        .block(
                            Block::default()
                                .title("history")
                                .border_style(pal.border_style()),
                        )
                        .data(&values)
                        .style(pal.sparkline_style());
                    frame.render_widget(spark, inner[2]);
                }
            }
        }
    }
}

fn draw_alerts(
    frame: &mut ratatui::Frame,
    area: Rect,
    snapshot: Option<&StatusSnapshot>,
    warning_threshold: f64,
    pal: &Palette,
) {
    let alerts = match snapshot {
        Some(s) => monitor_alerts(&s.windows, warning_threshold),
        None => vec!["waiting for data".into()],
    };

    let block = Block::default()
        .title(" alerts ")
        .borders(Borders::ALL)
        .border_style(pal.border_style());

    if alerts.is_empty() {
        let ok = Paragraph::new(Span::styled(
            "all windows below threshold",
            Style::default().fg(pal.ok),
        ))
        .block(block);
        frame.render_widget(ok, area);
    } else {
        let widget = Paragraph::new(render_alert_lines(&alerts, pal)).block(block);
        frame.render_widget(widget, area);
    }
}

fn draw_footer(
    frame: &mut ratatui::Frame,
    area: Rect,
    interval_secs: u64,
    next_refresh_secs: u64,
    pal: &Palette,
    current_account: Option<&str>,
) {
    let has_multi_account = current_account.is_some();
    let height = area.height;
    let footer = if height <= 1 {
        let mut text = "? help | 5 trends".to_string();
        if has_multi_account {
            text = format!("{text} | Tab account");
        }
        text = format!("{text} | q quit | r refresh | {interval_secs}s/{next_refresh_secs}s");
        if let Some(latest) = super::super::update::latest_checked_version() {
            text = format!("{text} | ⚠ Update v{latest}!");
        }
        Paragraph::new(Line::from(vec![Span::styled(text, pal.muted_style())]))
    } else {
        let mut spans = vec![
            Span::styled(" ? ", pal.key_binding_style()),
            Span::raw("help  "),
            Span::styled(" 5 ", pal.key_binding_style()),
            Span::raw("trends  "),
        ];
        if has_multi_account {
            spans.push(Span::styled(" Tab ", pal.key_binding_style()));
            spans.push(Span::raw("account  "));
        }
        spans.push(Span::styled(" q ", pal.key_binding_style()));
        spans.push(Span::raw("quit  "));
        spans.push(Span::styled(" r ", pal.key_binding_style()));
        spans.push(Span::raw(format!(
            "refresh  auto {interval_secs}s  next {next_refresh_secs}s"
        )));
        if let Some(latest) = super::super::update::latest_checked_version() {
            spans.push(Span::raw("  "));
            spans.push(Span::styled(
                format!("⚠ Update v{latest}"),
                pal.bold_level_style("warning"),
            ));
        }
        Paragraph::new(Line::from(spans)).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(pal.border_style()),
        )
    };
    frame.render_widget(footer, area);
}

fn draw_agents_panel(
    frame: &mut ratatui::Frame,
    area: Rect,
    snapshot: Option<&StatusSnapshot>,
    pal: &Palette,
) {
    let block = Block::default()
        .title(" agents ")
        .borders(Borders::ALL)
        .border_style(pal.border_style());

    let content = match snapshot {
        Some(s) => {
            if let Some(abtop) = &s.abtop {
                let token_rate = abtop
                    .get("token_rate")
                    .and_then(ng::to_number)
                    .map(|v| format!("{v:.1}/min"))
                    .unwrap_or_else(|| "n/a".into());
                let sessions =
                    ng::value_string(abtop.get("sessions_total")).unwrap_or_else(|| "?".into());
                let active =
                    ng::value_string(abtop.get("sessions_active")).unwrap_or_else(|| "?".into());
                Line::from(Span::styled(
                    format!("token rate {token_rate} | sessions {sessions} active {active}"),
                    pal.accent_style(),
                ))
            } else {
                Line::from(Span::styled(
                    "run with --with-abtop for agent data",
                    pal.muted_style(),
                ))
            }
        }
        None => Line::from(Span::styled("waiting for data", pal.muted_style())),
    };

    let widget = Paragraph::new(content).block(block);
    frame.render_widget(widget, area);
}

fn draw_trend_panel(
    frame: &mut ratatui::Frame,
    area: Rect,
    trend_days: &[TrendDay],
    pal: &Palette,
    preset: Preset,
) {
    let block = Block::default()
        .title(" 30-day trends ")
        .borders(Borders::ALL)
        .border_style(pal.border_style());

    if trend_days.is_empty() {
        let msg = Paragraph::new(Span::styled(
            "no trend data yet — run vimit to collect snapshots",
            pal.muted_style(),
        ))
        .block(block);
        frame.render_widget(msg, area);
        return;
    }

    let max_bars = match preset {
        Preset::Mini => 14,
        Preset::Compact => 21,
        Preset::Full => 30,
    };

    let days: Vec<&TrendDay> = trend_days.iter().rev().take(max_bars).collect::<Vec<_>>();
    let days = days.into_iter().rev().collect::<Vec<_>>();

    let mut lines: Vec<Line> = Vec::new();
    for w_key in ["5h", "24h", "7d", "30d"] {
        let peaks: Vec<f64> = days
            .iter()
            .map(|d| {
                d.windows
                    .iter()
                    .find(|w| w.key == w_key)
                    .map(|w| w.peak_max)
                    .unwrap_or(0.0)
            })
            .collect();
        let max_p = peaks.iter().cloned().fold(0.0_f64, f64::max).max(1.0);
        let bar_chars: String = peaks
            .iter()
            .map(|p| {
                let bars = ((p / max_p) * 8.0).round() as usize;
                match bars.min(8) {
                    0 => ' ',
                    1 => '▁',
                    2 => '▂',
                    3 => '▃',
                    4 => '▄',
                    5 => '▅',
                    6 => '▆',
                    7 => '▇',
                    _ => '█',
                }
            })
            .collect();
        let max_peak = peaks.iter().cloned().fold(0.0_f64, f64::max);
        lines.push(Line::from(vec![
            Span::styled(format!(" {w_key} "), pal.bold_level_style("ok")),
            Span::raw(format!(" {bar_chars}  max {max_peak:.0}%")),
        ]));
    }

    let widget = Paragraph::new(lines).block(block);
    frame.render_widget(widget, area);
}

fn draw_help_overlay(frame: &mut ratatui::Frame, theme: Theme) {
    let pal = theme.palette();
    let area = frame.area();

    let help_text = vec![
        Line::from(Span::styled(
            "  VibeMode Monitor - Keybindings",
            pal.bold_level_style("ok"),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("  q / Esc", pal.key_binding_style()),
            Span::raw("    Quit"),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+C", pal.key_binding_style()),
            Span::raw("      Force quit"),
        ]),
        Line::from(vec![
            Span::styled("  r", pal.key_binding_style()),
            Span::raw("            Force refresh"),
        ]),
        Line::from(vec![
            Span::styled("  ?", pal.key_binding_style()),
            Span::raw("            Toggle this help"),
        ]),
        Line::from(vec![
            Span::styled("  p", pal.key_binding_style()),
            Span::raw("            Toggle VPN/Direct endpoint"),
        ]),
        Line::from(""),
        Line::from(Span::styled("  Panel Toggles:", pal.accent_style())),
        Line::from(vec![
            Span::styled("  1", pal.key_binding_style()),
            Span::raw("            Toggle header"),
        ]),
        Line::from(vec![
            Span::styled("  2", pal.key_binding_style()),
            Span::raw("            Toggle quota cards"),
        ]),
        Line::from(vec![
            Span::styled("  3", pal.key_binding_style()),
            Span::raw("            Toggle alerts"),
        ]),
        Line::from(vec![
            Span::styled("  4", pal.key_binding_style()),
            Span::raw("            Toggle agents"),
        ]),
        Line::from(vec![
            Span::styled("  5", pal.key_binding_style()),
            Span::raw("            Toggle 30-day trends"),
        ]),
        Line::from(vec![
            Span::styled("  6", pal.key_binding_style()),
            Span::raw("            Toggle footer"),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Press ?", pal.muted_style()),
            Span::raw(" to close this overlay"),
        ]),
    ];

    let help_widget = Paragraph::new(help_text)
        .block(
            Block::default()
                .title(" help ")
                .borders(Borders::ALL)
                .border_style(pal.border_style()),
        )
        .style(pal.header_style());

    let popup_area = Rect {
        x: area
            .width
            .saturating_sub(super::super::constants::HELP_WIDTH)
            / 2,
        y: area
            .height
            .saturating_sub(super::super::constants::HELP_HEIGHT)
            / 2,
        width: super::super::constants::HELP_WIDTH.min(area.width),
        height: super::super::constants::HELP_HEIGHT.min(area.height),
    };

    frame.render_widget(help_widget, popup_area);
}
