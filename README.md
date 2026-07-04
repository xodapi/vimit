# vimit

[English](README.md) | [Русский](README.ru.md)

[![CI](https://github.com/xodapi/vimit/actions/workflows/ci.yml/badge.svg)](https://github.com/xodapi/vimit/actions/workflows/ci.yml)
[![Stars](https://img.shields.io/github/stars/xodapi/vimit.svg?style=flat-square)](https://github.com/xodapi/vimit/stargazers)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg?style=flat-square)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-2024-orange.svg?style=flat-square)](https://www.rust-lang.org)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20Linux%20%7C%20macOS-lightgrey?style=flat-square)](https://github.com/xodapi/vimit)
[![Tests](https://img.shields.io/badge/tests-CI%20passing-brightgreen?style=flat-square)](https://github.com/xodapi/vimit/actions/workflows/ci.yml)

**vimit** gives your VibeMode quota a visual presence: one native Rust binary with CLI, TUI, GUI, and a floating overlay where remaining credits pulse as a living creature.

Polls `GET /v1/me`, summarizes credit/request windows (5h / 24h / 7d / 30d),
renders a live TUI dashboard (or JSON / compact text), sends desktop
notifications on threshold breach, and supports multiple VibeMode accounts.

No Python, Node, or SDK dependencies — just one executable.

## Demo

![vimit floating overlay demo](docs/vmit_demo_scren.gif)

## Quick Start

```bash
# Download from releases, then:
vimit --demo                # try without an API key
vimit --demo --monitor      # full-screen dashboard
vimit --init                # interactive setup wizard
vimit                       # real VibeMode limits
vimit --doctor              # system diagnostics
```

## Why

Vibe coders need to know whether they can safely keep a Codex/Droid/Claude
session running or whether they are about to hit VibeMode limits. The tool is
small, local-first, and intentionally avoids storing API keys or logging
private prompts.

## Features

- **Self-updates** (`vimit update`): check and update the CLI & GUI natively using GitHub Releases. The GUI includes a dedicated Updates section with a Check Now button and an Auto-check toggle.
- **Active endpoint failover**: automatically failover between API routers, displaying an active connection badge (`api`/`r-api`).
- **Trend sparklines**: 15-day usage graphs rendered inside GUI quota cards.
- **Stealth Mode**: mask sensitive numeric values with `***` in the GUI.
- **Living overlay creature**: a floating, color-shifting quota indicator with compact/full modes, skin switching, pulse animation, Windows sounds on level changes, and calm sleep/recovery behavior instead of punishment mechanics.
- **Multiple output modes**: human, JSON (`--json`), compact one-line (`--compact`).
- **Live TUI monitor** (`--monitor`): ratatui dashboard with gauges, sparklines, color themes.
- **Monitor presets**: `full` (2-column grid), `compact` (single-column), `mini` (one-liner).
- **12 color themes**: btop, dracula, catppuccin, tokyo-night, gruvbox, nord, high-contrast, protanopia, deuteranopia, tritanopia, solarized, monokai.
- **Multi-account**: `accounts.toml` profiles, Tab switching in TUI, dropdown in GUI.
- **Desktop notifications** (`--notify`): alert on warning/danger, no-repeat logic.
- **Custom thresholds** (`--warning`, `--danger`, `--threshold`): per-window warning/danger levels.
- **CI integration** (`--fail-on`): exit non-zero when threshold is breached.
- **Watch mode** (`--watch N`): periodic polling every N seconds.
- **abtop integration** (`--with-abtop`): merge local Codex/Claude agent status.
- **Diagnostics** (`--doctor`): validate config, accounts, env, API connectivity.
- **Setup wizard** (`--init`): interactive config, .env, and API key setup.
- **GUI** (`--features gui`): Slint-based desktop window with stealth toggle, sparklines, account dropdown, informative system tray tooltip with exact usage percentages on hover, proper taskbar application icon on Windows, updates configuration panel, and floating creature overlay.
- **Safe by design**: API key from env only, never logged, no telemetry.

## Download

Release binaries:

https://github.com/xodapi/vimit/releases

Pick the archive for your platform, unpack it, then run:

```bash
vimit --version
vimit --demo
vimit --demo --monitor
```

Windows PowerShell:

```powershell
.\vimit.exe --version
.\vimit.exe --demo
.\vimit.exe --demo --monitor
```

If you double-click `vimit.exe` in Explorer, the Windows console will stay
open after the command finishes. The Windows archive also includes
`vimit-open.cmd`, a double-click helper that always pauses at the end, and
`vimit-monitor.cmd` for launching the live monitor directly.

## .env Next To The Binary

You can keep the VibeMode API key in a local `.env` file next to `vimit`
or in the directory where you run it. The release archive includes
`.env.example`.

```bash
cp .env.example .env
```

Edit `.env`:

```dotenv
VIBEMODE_API_KEY=YOUR_VIBEMODE_API_KEY
VIBEMODE_API_BASE=https://r-api.vibemod.pro
```

Then run:

```bash
vimit
vimit --compact
vimit --json
```

Windows PowerShell:

```powershell
Copy-Item .env.example .env
notepad .env
.\vimit.exe --compact
```

Double-click option on Windows:

```text
vimit-open.cmd
vimit-monitor.cmd
```

Lookup order:

1. `--env-file <PATH>`
2. `.env` in the current directory
3. `.env` next to the `vimit` executable
4. `~/.vimit/.env` (or `%USERPROFILE%\.vimit\.env` on Windows)

Real environment variables have priority over `.env` values. `.env` is ignored
by git and should not be committed.

## Build From Source

Requirements: Rust stable.

```bash
git clone https://github.com/xodapi/vimit.git
cd vimit
cargo build --release --locked
```

Binary location:

- Windows: `target/release/vimit.exe`
- Linux/macOS: `target/release/vimit`

## Diagnostics

Check system health:

```bash
vimit --doctor
```

Example output:

```
vimit doctor — system diagnostics

  [✓] config.toml found: /home/user/.config/vimit/config.toml
  [✓] config.toml is valid TOML
  [✓] accounts.toml: 2 account(s): dev, prod

  environment:
       HOME: /home/user
       VIBEMODE_API_BASE: (not set, will use default)
       VIBEMODE_API_KEY: (set)

  testing API connection to https://r-api.vibemod.pro... OK (4 window(s))

  status: all checks passed
```

## Interactive Setup

```bash
vimit --init
```

Creates config directory, config.toml with defaults, optionally sets up
`.env` with your API key, and tests the connection.

## Self-Updates

`vimit` supports native self-updating via GitHub Releases:

```bash
vimit update --check   # Check if a new version is available
vimit update           # Automatically download and install the latest release
```

Background checking is performed automatically in the background on TUI and GUI startup (if enabled). To avoid GitHub API rate limits, checks are cached for 24 hours in `~/.config/vimit/state.json`.

In the GUI, a dedicated Updates section has been added to the settings panel:
- **Check Now** button to trigger an immediate check.
- **Auto-check** toggle (On/Off) to enable or disable automatic startup checks (setting is persisted in `state.json`).
- Orange **"Update Available"** badge next to the router status in the window header when a newer version is found.

## Usage

Try without a key or network:

```bash
vimit --demo
vimit --demo --json
```

Use a real VibeMode key:

```bash
export VIBEMODE_API_KEY="YOUR_VIBEMODE_API_KEY"
vimit
vimit --json
vimit --with-abtop
```

Windows PowerShell:

```powershell
$env:VIBEMODE_API_KEY = "YOUR_VIBEMODE_API_KEY"
.\vimit.exe
.\vimit.exe --json
```

Mock a saved `/v1/me` payload:

```bash
vimit --mock tests/fixtures/me.json
vimit --mock tests/fixtures/me.json --json
```

Watch mode:

```bash
vimit --watch 60 --with-abtop
vimit --watch 60 --notify
```

Live monitor:

```bash
vimit --monitor
vimit --monitor --watch 10
vimit --monitor --with-abtop
vimit --monitor --notify
```

Monitor presets for different terminal sizes:

```bash
vimit --monitor --preset full      # 2-column grid, sparklines (default)
vimit --monitor --preset compact   # single-column, gauge + metrics
vimit --monitor --preset mini      # one line per window, minimal
```

Per-window thresholds:

```bash
vimit --monitor --threshold 5h=80:95,7d=90
vimit --fail-on warning --threshold 24h=85:98
```

Format: `KEY=WARNING[:DANGER]` where KEY is one of `5h`, `24h`, `7d`, `30d`.
Per-window thresholds override `--warning`/`--danger` for those windows.

In monitor mode, press `r` to refresh immediately and `q` or `Esc` to quit.
It renders an abtop-style dashboard with VibeMode quota windows, warning
alerts, reset timers, remaining credits/requests, and optional local
Codex/Claude agent context from `abtop --status-json`.

API cache behavior:

- Successful live `/v1/me` responses are cached for 30 seconds by default.
- Set `cache_ttl_secs = 120` in `config.toml` to change the cache TTL.
- `--no-cache` always performs a live fetch and does not read or write the API
  cache, including stale fallback.
- In monitor mode, pressing `r` clears the current account/endpoint cache entry
  before forcing the next refresh.

Desktop notifications:

```bash
vimit --notify
vimit --watch 60 --notify
vimit --monitor --notify
```

`--notify` sends a local desktop alert when a quota window escalates into
`warning` or `danger`. It keeps one in-process state map, so polling and
monitor loops do not spam the same alert on every refresh. De-escalation after a
window reset is silent.

CI/automation threshold:

```bash
vimit --fail-on warning
vimit --fail-on danger --json
vimit --warning 80 --danger 95 --fail-on warning
vimit --fail-on warning --ci-annotate
```

`--ci-annotate` keeps normal stdout output intact and emits GitHub Actions-style
`::warning` / `::error` lines to stderr for breached windows, so it can be used
in CI logs without adding hook orchestration or request mutation.

Compact one-line output for widgets/status bars:

```bash
vimit --compact
vimit --compact --with-abtop
```

## Output

Human output:

```text
VibeMode limits
  5h   warning reset in 2h 30m
       credits  39/50 (78.0%, left 11)
       requests 610/1000 (61.0%, left 390)
```

JSON output:

```json
{
  "source": "vibemode",
  "windows": [
    {
      "window": "5h",
      "level": "warning",
      "credits": { "used": 39.0, "limit": 50.0, "remaining": 11.0, "percent": 78.0 }
    }
  ],
  "abtop": null
}
```

## Safety

- API key is read from the environment variable `VIBEMODE_API_KEY` (with `NEUROGATE_API_KEY` supported as a fallback).
- The key is never written to disk.
- Errors do not print the key.
- JSON output intentionally omits account identity fields.
- `--with-abtop` uses `abtop --status-json`, which is the compact,
  privacy-preserving abtop payload without local paths, prompts, chat text, or
  session IDs.
- `--monitor` uses the same privacy-safe summaries and keeps the API key out of
  the terminal output.
- `--notify` only passes quota summary text to a local OS notification helper:
  Windows PowerShell toast/fallback popup, macOS `osascript`, or Linux/BSD
  `notify-send`.
- No telemetry, no external network calls except VibeMode `/v1/me`.

## Configuration

Environment variables:

- `VIBEMODE_API_KEY`: VibeMode API key (fallback: `NEUROGATE_API_KEY`).
- `VIBEMODE_API_BASE`: API base URL, default `https://r-api.vibemod.pro` (fallback: `NEUROGATE_API_BASE`).
- `ABTOP_BIN`: abtop binary path, default `abtop`.

CLI options:

```bash
vimit --help
```

## Discussions

Ideas, feature requests, and VibeMode/Codex/Droid workflow notes are welcome
in GitHub Discussions:

https://github.com/xodapi/vimit/discussions

See [ROADMAP.md](ROADMAP.md) for the current improvement backlog.

## Supported OS

- Windows (x86_64)
- macOS (aarch64)
- Linux (x86_64, aarch64)
- Android/Termux — see [docs/termux.md](docs/termux.md); native Slint Android spike is tracked in [docs/android.md](docs/android.md)

## Optional Jujutsu Workflow

`jj` is reasonable here only as an optional local workflow on top of the
existing Git repository. It can improve local iteration with operation log,
conflict recovery, and cleaner in-progress history, but it should not replace
the project's canonical GitHub flow.

Recommended stance:

- Use `jj` only if a developer or agent already prefers it locally.
- Keep GitHub Issues, branches, PRs, and remote state managed through `git` +
  `gh`.
- Do not require `jj` in CI, build scripts, onboarding, or AGENTS workflow.
- Do not add `vcs-jj` or `vcs-core` unless a future automation issue proves
  they solve a concrete project problem.

Minimal issue workflow with `jj`:

```bash
jj git clone https://github.com/xodapi/vimit.git
cd vimit
gh issue view 138
git checkout -b issue-138-evaluate-optional-jujutsu-workflow
jj bookmark create issue-138-evaluate-optional-jujutsu-workflow -r @

# edit files, then verify the required check
cargo fmt --check

# when ready, sync the current jj commit to the Git branch and push normally
jj git push --bookmark issue-138-evaluate-optional-jujutsu-workflow
gh pr create --base main --title "docs(dev): evaluate optional Jujutsu workflow" --body "Closes #138"
```

If you use `jj`, treat it as a personal productivity layer. The repository
still expects Git-compatible branch names, standard commits, and the same issue
assignment / PR review flow documented in `AGENTS.md`.

## Tests

```bash
cargo test --locked
cargo clippy --all-targets -- -D warnings
cargo fmt --check
cargo run --locked -- --demo --json
```

## What works

- Native Rust CLI and GUI (build with `--features gui`).
- VibeMode `/v1/me` polling with robust schema tolerance.
- 5h / 24h / 7d / 30d credit and request windows.
- Human, JSON, and compact output modes with ANSI color coding.
- Full-screen ratatui monitor with gauges, sparklines, color coding.
- Monitor presets: `full`, `compact`, `mini` for different terminal sizes.
- 12 color themes including accessibility-optimized (protanopia, deuteranopia, tritanopia, high-contrast).
- Per-window thresholds: `--threshold 5h=80:95,7d=90`.
- `.env` file next to the binary or working directory.
- Custom warning/danger thresholds.
- Desktop notifications with escalation tracking.
- Multi-account support via `accounts.toml` (Tab switching in TUI, dropdown in GUI).
- Demo/mock mode without a key.
- Optional local abtop integration.
- `--doctor` system diagnostics.
- `--init` interactive setup wizard.
- Actionable error messages with suggestions.
- CI and release workflow for native binaries (Windows, Linux, macOS, ARM).
- PowerShell install/uninstall scripts.
- Termux/Android install guide.

---
*Note: Sign up for VibeMode using [this referral link](https://portal.vibemod.pro/?ref=cbvBMDP06DSwPL9u) to receive a $5 bonus on your account.*
