# vimit

[English](README.md) | [Русский](README.ru.md)

[![CI](https://github.com/xodapi/vimit/actions/workflows/ci.yml/badge.svg)](https://github.com/xodapi/vimit/actions/workflows/ci.yml)
[![Stars](https://img.shields.io/github/stars/xodapi/vimit.svg?style=flat-square)](https://github.com/xodapi/vimit/stargazers)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg?style=flat-square)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-2024-orange.svg?style=flat-square)](https://www.rust-lang.org)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20Linux%20%7C%20macOS-lightgrey?style=flat-square)](https://github.com/xodapi/vimit)
[![Tests](https://img.shields.io/badge/tests-CI%20passing-brightgreen?style=flat-square)](https://github.com/xodapi/vimit/actions/workflows/ci.yml)

**vimit** делает лимиты VibeMode наглядными: один нативный Rust-бинарник с CLI, TUI, GUI и floating overlay, где остаток кредитов пульсирует как живое существо.

Опрашивает `GET /v1/me`, показывает расход credit/request по окнам (5ч / 24ч / 7д / 30д),
рисует live TUI-дашборд (или JSON / compact), отправляет desktop-уведомления при
достижении порогов, поддерживает несколько аккаунтов VibeMode.

Не нужны Python, Node или SDK — только один исполняемый файл.

## Демо

![vimit floating overlay demo](docs/vmit_demo_scren.gif)

## Быстрый старт

```bash
# Скачайте из релизов, затем:
vimit --demo                # попробовать без API-ключа
vimit --demo --monitor      # полноэкранный дашборд
vimit --init                # интерактивный мастер настройки
vimit                       # реальные лимиты VibeMode
vimit --doctor              # диагностика системы
```

## Зачем

Пользователю VibeMode полезно заранее понимать, можно ли спокойно продолжать
сессию Codex/Droid/Claude/Cursor или лимиты уже близко. Проект маленький,
локальный и не хранит API-ключи, промпты или приватные сообщения.

## Возможности

- **Автообновления** (`vimit update`): нативная проверка и установка обновлений CLI & GUI напрямую из GitHub Releases. В GUI добавлена секция настроек с кнопкой ручной проверки и переключателем авто-проверки.
- **Отказоустойчивый роутер**: автоматический ретрай и failover между роутерами API с отображением статуса активного подключения (`api`/`r-api`).
- **Спарклайны трендов**: визуализация 15-дневной истории использования лимитов прямо внутри GUI.
- **Скрытый режим (Stealth Mode)**: скрытие точных числовых значений расходов с заменой на `***` в GUI.
- **Живое overlay-существо**: floating-индикатор лимита с компактным/полным режимом, сменой скина, пульсацией, изменением цвета, Windows-звуками при смене уровня и спокойным сном/выдохом вместо механик наказания.
- **Несколько режимов вывода**: human, JSON (`--json`), compact одной строкой (`--compact`).
- **Live TUI-монитор** (`--monitor`): ratatui-дашборд с индикаторами, sparkline, цветовыми темами.
- **Пресеты монитора**: `full` (сетка 2 колонки), `compact` (одна колонка), `mini` (одна строка).
- **12 цветовых тем**: btop, dracula, catppuccin, tokyo-night, gruvbox, nord, high-contrast, protanopia, deuteranopia, tritanopia, solarized, monokai.
- **Несколько аккаунтов**: `accounts.toml`, переключение по Tab в TUI, выпадающий список в GUI.
- **Desktop-уведомления** (`--notify`): оповещение при warning/danger, без повторов.
- **Свои пороги** (`--warning`, `--danger`, `--threshold`): для каждого окна отдельно.
- **CI-интеграция** (`--fail-on`): ненулевой exit code при превышении порога.
- **Watch mode** (`--watch N`): периодический опрос каждые N секунд.
- **abtop-интеграция** (`--with-abtop`): статус локальных Codex/Claude-агентов.
- **Диагностика** (`--doctor`): проверка конфига, аккаунтов, env, API.
- **Мастер установки** (`--init`): интерактивное создание конфига, .env и API-ключа.
- **30-дневные тренды** (`--trend`): история использования в redb, sparklines в TUI.
- **GUI** (`--features gui`): desktop-приложение на Slint с графиками трендов, выпадающим списком аккаунтов, информативным тултипом в системном трее (с точным расходом квоты в процентах на mouse hover), полноценной иконкой приложения на панели задач Windows, разделом управления обновлениями и floating overlay с живым существом.
- **Безопасно**: API-ключ только из env, не логируется, нет телеметрии.

## Диагностика

Проверка здоровья системы:

```bash
vimit --doctor
```

Пример вывода:

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

## Интерактивная настройка

```bash
vimit --init
```

Создаёт директорию конфига, `config.toml` с умолчаниями, опционально `.env`
с API-ключом, тестирует соединение.

## Автообновления

`vimit` поддерживает нативное обновление исполняемого файла через GitHub Releases:

```bash
vimit update --check   # Проверить наличие новой версии
vimit update           # Автоматически скачать и установить последнюю версию
```

Фоновая проверка выполняется автоматически при запуске TUI и GUI (если включена). Результаты кэшируются на 24 часа в файле `~/.config/vimit/state.json`, чтобы избежать блокировок со стороны лимитов GitHub API.

В GUI-версии добавлена панель управления обновлениями в настройках:
- Кнопка **"Проверить сейчас"** для мгновенного поиска релизов.
- Чекбокс **"Авто-проверка"** для включения/выключения фонового поиска при старте (настройка сохраняется в `state.json`).
- Оранжевый бадж **"Update Available"** в шапке окна, если доступна новая версия.

## Скачать

Бинарные релизы:

https://github.com/xodapi/vimit/releases

Скачайте архив под свою платформу, распакуйте и запустите:

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

Если запустить `vimit.exe` двойным кликом из Explorer, Windows-консоль
останется открытой после завершения команды. В Windows-архив также входит
`vimit-open.cmd` (helper для двойного клика с паузой) и
`vimit-monitor.cmd` для прямого запуска live-monitor.

## .env рядом с бинарником

Ключ VibeMode можно держать в локальном `.env` рядом с `vimit` или в
директории, из которой вы запускаете команду. В релизный архив входит
`.env.example`.

```bash
cp .env.example .env
```

Отредактируйте `.env`:

```dotenv
VIBEMODE_API_KEY=YOUR_VIBEMODE_API_KEY
VIBEMODE_API_BASE=https://r-api.vibemod.pro
```

После этого:

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

Порядок поиска:

1. `--env-file <PATH>`
2. `.env` в текущей директории
3. `.env` рядом с исполняемым файлом `vimit`
4. `~/.vimit/.env` (или `%USERPROFILE%\.vimit\.env` на Windows)

Настоящие переменные окружения имеют приоритет над значениями из `.env`.

## Сборка из исходников

Требуется Rust stable.

```bash
git clone https://github.com/xodapi/vimit.git
cd vimit
cargo build --release --locked
```

Где будет бинарник:

- Windows: `target/release/vimit.exe`
- Linux/macOS: `target/release/vimit`

## Использование

Проверить без ключа и без сети:

```bash
vimit --demo
vimit --demo --json
```

С реальным ключом VibeMode:

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

Mock режим (сохранённый payload):

```bash
vimit --mock tests/fixtures/me.json
vimit --mock tests/fixtures/me.json --json
```

Watch mode:

```bash
vimit --watch 60 --with-abtop
vimit --watch 60 --notify
```

Live-monitor:

```bash
vimit --monitor
vimit --monitor --watch 10
vimit --monitor --with-abtop
vimit --monitor --notify
```

Пресеты монитора:

```bash
vimit --monitor --preset full      # 2 колонки, sparklines (по умолчанию)
vimit --monitor --preset compact   # 1 колонка, индикатор + метрики
vimit --monitor --preset mini      # одна строка на окно
```

Управление в TUI: `q`/`Esc` выход, `r` обновить, `?` помощь, `1`-`6` панели,
`Tab` переключение аккаунтов (если несколько).

Пер-оконные пороги:

```bash
vimit --monitor --threshold 5h=80:95,7d=90
vimit --fail-on warning --threshold 24h=85:98
```

Desktop-уведомления:

```bash
vimit --notify
vimit --watch 60 --notify
vimit --monitor --notify
```

CI-интеграция:

```bash
vimit --fail-on warning
vimit --fail-on danger --json
vimit --warning 80 --danger 95 --fail-on warning
```

30-дневные тренды:

```bash
vimit --trend
vimit --trend --json
vimit --trend --days 7
vimit --monitor          # нажмите 5 для sparklines трендов
```

## Вывод

Человекочитаемый вывод с ANSI-цветами:

```text
VibeMode limits
  5h   warning reset in 2h 30m  peak 78%
       credits  39/50 (78.0%, left 11)
       requests 610/1000 (61.0%, left 390)
```

JSON:

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

## Тренды

```bash
vimit --trend
```

Показывает ежедневную статистику по окнам за последние 30 дней:
пик max/avg, средний расход credits/requests. Данные хранятся в
`~/.config/vimit/trends.redb` (redb — встраиваемая ACID-БД, pure Rust).

## Безопасность

- API-ключ читается из переменной окружения `VIBEMODE_API_KEY` (поддержка `NEUROGATE_API_KEY` сохранена для обратной совместимости).
- Ключ не записывается на диск.
- Ошибки не печатают ключ.
- JSON-вывод не содержит identity-поля аккаунта.
- `--with-abtop` использует `abtop --status-json`: компактный privacy-safe
  payload без локальных путей, промптов, текста чатов и session ID.
- `--monitor` не выводит API-ключ в терминал.
- `--notify` передает только краткую сводку лимита локальному helper-у ОС.
- Нет телеметрии и внешних сетевых запросов, кроме VibeMode `/v1/me`.

## Настройка

Переменные окружения:

- `VIBEMODE_API_KEY`: API-ключ VibeMode (фолбек: `NEUROGATE_API_KEY`).
- `VIBEMODE_API_BASE`: API base URL, по умолчанию `https://r-api.vibemod.pro` (фолбек: `NEUROGATE_API_BASE`).
- `ABTOP_BIN`: путь к бинарнику abtop, по умолчанию `abtop`.

CLI:

```bash
vimit --help
```

## Обсуждения

Идеи, предложения и заметки по VibeMode/Codex/Droid workflow можно оставлять
в GitHub Discussions:

https://github.com/xodapi/vimit/discussions

Текущий список улучшений: [ROADMAP.md](ROADMAP.md).

## Поддерживаемые ОС

- Windows (x86_64)
- macOS (aarch64)
- Linux (x86_64, aarch64)
- Android/Termux — см. [docs/termux.md](docs/termux.md); native Slint Android spike описан в [docs/android.md](docs/android.md)

## Optional Jujutsu Workflow

`jj` здесь уместен только как optional локальный workflow поверх уже
существующего Git-репозитория. Он может сделать локальные итерации удобнее за
счёт operation log, более мягкого восстановления после конфликтов и аккуратной
истории промежуточных изменений, но не должен заменять canonical GitHub flow
проекта.

Рекомендация по проекту:

- Использовать `jj` только если разработчику или агенту он уже удобен локально.
- GitHub Issues, ветки, PR и remote state оставлять на `git` + `gh`.
- Не делать `jj` обязательным для CI, build scripts, onboarding или AGENTS workflow.
- Не добавлять `vcs-jj` или `vcs-core`, пока отдельный automation issue не
  докажет, что это решает конкретную проблему проекта.

Минимальный issue workflow с `jj`:

```bash
jj git clone https://github.com/xodapi/vimit.git
cd vimit
gh issue view 138
git checkout -b issue-138-evaluate-optional-jujutsu-workflow
jj bookmark create issue-138-evaluate-optional-jujutsu-workflow -r @

# правки, затем обязательная проверка
cargo fmt --check

# когда всё готово, синхронизировать текущий jj commit с Git-веткой и пушить как обычно
jj git push --bookmark issue-138-evaluate-optional-jujutsu-workflow
gh pr create --base main --title "docs(dev): evaluate optional Jujutsu workflow" --body "Closes #138"
```

Если использовать `jj`, то только как личный слой удобства поверх Git. Для
репозитория по-прежнему обязательны Git-совместимые имена веток, обычные
коммиты и тот же issue/PR workflow, который описан в `AGENTS.md`.

## Тесты

```bash
cargo test --locked
cargo clippy --all-targets -- -D warnings
cargo fmt --check
cargo run --locked -- --demo --json
```

## Что работает

- Нативный Rust CLI и GUI (`--features gui`).
- VibeMode `/v1/me` с устойчивостью к разным схемам ответа.
- Окна 5h / 24h / 7d / 30d credit и request.
- Human, JSON, compact вывод с ANSI-цветами.
- Full-screen ratatui monitor с индикаторами, sparklines, цветовыми темами.
- Пресеты `full`, `compact`, `mini`.
- 12 цветовых тем, включая для accessibility.
- Пер-оконные пороги `--threshold`.
- `.env` файл рядом с бинарником.
- Desktop-уведомления с отслеживанием эскалации.
- Несколько аккаунтов (`accounts.toml`, Tab в TUI, dropdown в GUI).
- Demo/mock без ключа.
- `--doctor` диагностика.
- `--init` мастер установки.
- `--trend` 30-дневные тренды (redb).
- Интеграция с abtop.
- CI и release workflow для Windows, Linux, macOS, ARM.
- PowerShell скрипты установки/удаления.
- Инструкция для Termux/Android.

---
*Примечание: Зарегистрируйтесь в VibeMode по [этой реферальной ссылке](https://portal.vibemod.pro/?ref=cbvBMDP06DSwPL9u), чтобы получить бонус $5 на ваш аккаунт.*
