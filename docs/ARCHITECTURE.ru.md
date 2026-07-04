# Архитектурные границы vimit

Этот документ фиксирует целевое устройство репозитория. Он нужен, чтобы новые
фичи, Android-работа и агентские PR не размывали ядро проекта.

## Цель

vimit должен оставаться локальным Rust-инструментом с понятными слоями:

- core: чистые модели, расчеты, парсинг `/v1/me`, pulse/burn logic;
- API: HTTP-клиент, failover, endpoint routing;
- CLI: аргументы, config, textual/JSON output, init, doctor, update;
- TUI: ratatui monitor, layout, rendering, snapshots;
- desktop GUI: Slint window, tray, overlay, dashboard wiring;
- Android bridge: Android lifecycle, key storage bridge, notification/vibration glue;
- CI/release: checks, artifacts, package/release automation.

Каждый слой может зависеть только на слой ниже или на явно выделенный shared
модуль. UI-код не должен становиться местом, где живет бизнес-логика.

## Границы модулей

### Core

Core-код должен быть платформенно нейтральным и тестируемым без GUI/Android:

- `Metric`, `WindowState`, `AgentPulse`, `AgentBurnDetector`;
- парсинг usage payloads;
- threshold and level decisions;
- форматирование чисел и времени, если оно используется несколькими UI.

Core не должен:

- читать API keys напрямую из UI;
- запускать Slint, tray, terminal или Android APIs;
- знать о конкретном экране, карточке, кнопке или notification channel.

### API

API-слой отвечает за сеть и failover:

- `GET /v1/me`;
- router/fallback selection;
- таймауты и ошибки;
- user-agent.

API-слой не должен форматировать TUI/GUI output и не должен логировать secrets.

### CLI и TUI

CLI отвечает за команды, аргументы, config merge, JSON/text output and exit codes.
TUI отвечает за интерактивный монитор.

Целевая структура TUI:

- state collection;
- event loop;
- layout calculation;
- widgets/render helpers;
- snapshot tests.

Snapshot tests должны покрывать визуально важные состояния: waiting, error,
warning, danger, agent pulse, reset text.

### Desktop GUI

Desktop GUI должен быть тонким orchestration layer поверх core:

- window bootstrap;
- tray/menu integration;
- floating overlay;
- dashboard refresh and apply;
- handlers for UI actions.

GUI не должен дублировать threshold/burn logic. Если логика нужна и Android, и
desktop GUI, она должна жить в core/shared module.

### Android

Android-specific код должен быть изолирован за:

```rust
#[cfg(all(target_os = "android", feature = "android-gui"))]
```

Android bridge может знать о Slint Android backend, key storage path,
notification/vibration bridge and Android-only UI state. Он не должен менять
формат core models или CLI JSON.

## Бюджет размера файлов

Размер файла не является абсолютным правилом, но это хороший сигнал для ревью.

- 0-300 строк: нормальный размер для focused module.
- 300-500 строк: допустимо, если файл имеет одну понятную ответственность.
- 500-800 строк: нужен комментарий в PR, почему пока не делим.
- 800+ строк: почти всегда нужен отдельный Issue на split.

Исключения:

- `Cargo.lock`;
- generated snapshots/assets;
- файлы, которые технически удобнее держать вместе, с явным обоснованием.

Текущие кандидаты на split:

- `src/bin/vimit-gui.rs`;
- `src/cli/monitor.rs`;
- `src/lib.rs`;
- `ui/app.slint`.

## Slint UI

`ui/app.slint` должен постепенно дробиться на компоненты:

- dashboard cards;
- setup/key entry;
- Vimichi/alert state;
- settings/update controls;
- reusable buttons, badges, gauges.

Компоненты должны получать уже подготовленное состояние, а не вычислять
business rules внутри разметки.

## Secrets

Нельзя коммитить `.env` и реальные API keys.

Запрещено логировать:

- `VIBEMODE_API_KEY`;
- `NEUROGATE_API_KEY`;
- значения из локальных secret files.

Документация может показывать только placeholder values, например
`YOUR_VIBEMODE_API_KEY`.

## Feature flags и platform cfg

Правило: feature открывает capability, `cfg(target_os)` ограничивает платформу.

- `gui`: desktop Slint GUI dependencies;
- `android-gui`: Android Slint backend and JNI bridge;
- platform APIs должны быть спрятаны за `cfg`;
- core tests должны проходить без GUI features.

Нельзя добавлять зависимость в `Cargo.toml` без отдельного обоснования в Issue.

## Тестовая стратегия

Минимум для каждого PR:

```bash
cargo test --locked
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

Для GUI/Android-adjacent изменений:

```bash
cargo clippy --all-targets --features gui -- -D warnings
cargo build --features gui --locked
```

Что тестировать в первую очередь:

- чистую core-логику unit tests;
- boundary values для thresholds and timeouts;
- CLI output/exit behavior integration tests;
- TUI snapshots для layout regressions;
- Android bridge через compile checks and manual APK artifact.

## PR checklist

Перед PR проверь:

- есть GitHub Issue с label `agent`;
- изменения строго в рамках Issue;
- нет `.env`, keys, release tags, version bumps;
- файл не разросся выше 500 строк без причины;
- UI не содержит новой business logic, если ее можно вынести в core;
- Android-only код закрыт `cfg`;
- обязательные проверки зелёные;
- PR body содержит `Closes #N`.

## Когда создавать отдельный Issue

Создай отдельный Issue, если во время работы обнаружено:

- need to rename public target/crate/binary;
- need to change JSON output format;
- need to add dependency;
- need to touch CI/release workflow outside current task;
- file split larger than current Issue;
- security concern around API keys or logs.

Маленькие PR важнее героических PR. Репозиторий выглядит зрелым, когда каждое
изменение можно быстро понять, проверить и откатить.
