# Workflow агентов: jj, orchestration tools и практический процесс

Дата: 2026-07-04

## Решение

Основной workflow проекта менять не нужно. Для Vimit остаётся обязательным
правило из `AGENTS.md`: GitHub Issue -> отдельная ветка/worktree -> проверки ->
PR -> merge-gate.

Новые инструменты можно использовать только как вспомогательный слой вокруг
этого процесса:

- `jj` — пилотировать локально у одного агента, не делать обязательным.
- Vibe-Kanban — взять как UX-паттерн доски задач, не делать зависимостью.
- goose — можно пробовать как отдельного внешнего агента для docs/audit/refactor.
- `pi_agent_rust` — не использовать в боевом workflow до отдельного sandbox-аудита.

## Текущий рабочий процесс Vimit

Проект уже имеет сильный процесс:

1. Человек или координатор создаёт GitHub Issue с label `agent`.
2. Агент берёт только незаблокированный Issue без чужого assignee.
3. Агент назначает себя и оставляет стартовый комментарий.
4. Работа идёт в отдельной ветке `issue-N-slug`.
5. Для параллельных задач используется отдельный `git worktree`.
6. Перед PR проходят проверки из `AGENTS.md`.
7. Merge-gate проверяет CI, scope diff и порядок merge.

Этот процесс уже выдержал параллельную работу нескольких агентов. Главная
практическая проблема сейчас не в Git, а в координации: какие задачи заняты,
какие worktree активны, какие PR зелёные, где агент завис.

## Телефонный/удалённый режим

Для работы с телефона через удалённый рабочий стол или SSH не нужен новый
фреймворк. Достаточно:

```text
SSH или RDP
+ tmux
+ gh CLI
+ git worktree
+ AGENTS.md / MULTI-AGENT.md
+ один merge-gate
```

Рекомендуемый layout:

```text
tmux session: vimit

windows:
- gate
- agent-099
- agent-113
- agent-114
- agent-115
- agent-117
- audit
```

Быстрые команды статуса:

```bash
gh issue list --state open --label agent
gh pr list --state open
git worktree list
git status --short --branch
```

WezTerm или Zellij могут улучшить UX, но не являются блокером и не должны
попадать в production-зависимости проекта.

## Jujutsu (`jj`)

Jujutsu — Git-compatible VCS, написанная на Rust. Она может работать поверх
Git-репозитория и экспортировать изменения обратно в Git. Это делает её
интересной для AI-агентов: проще undo, меньше ручной возни с amend/rebase,
working copy воспринимается как commit.

Полезные свойства для агентов:

- `jj undo` снижает цену ошибочного действия.
- Работа с изменениями как с графом удобнее, чем последовательность
  интерактивных Git-операций.
- `jj workspace` концептуально похож на `git worktree`.
- GitHub PR остаётся возможным через Git-compatible backend.

Риски для Vimit:

- `jj` автоматически snapshot-ит рабочую копию; агент может случайно включить
  лишний файл.
- В проекте есть untracked docs, build logs, `.env` и потенциальные секреты.
- `AGENTS.md` сейчас описывает Git-команды, не `jj`.
- Не все агенты одинаково хорошо понимают `jj`.
- Merge-gate и GitHub Actions всё равно остаются Git/PR-ориентированными.

Рекомендация: не внедрять `jj` как обязательный инструмент. Разрешить
эксперимент одному опытному агенту только при условиях:

- отдельный worktree;
- отдельный Issue;
- никаких `.env`, keystore, ключей и временных логов в изменениях;
- итоговый PR создаётся обычным GitHub способом;
- перед PR проверяется `git status --short`, а не только `jj status`.

## Vibe-Kanban

Vibe-Kanban полезен как модель: доска задач, назначение агентов, изолированные
worktree, видимые статусы, быстрый review. Это ровно та проблема, которая
возникает при 4-6 параллельных агентах.

Но продукт не стоит делать фундаментом процесса: официальный репозиторий
помечен как sunsetting. Поэтому брать нужно не зависимость, а принципы:

- Issue = карточка.
- Assignee = агент.
- Worktree = изолированная среда.
- PR = результат карточки.
- Merge-gate = отдельная роль.
- Status = started / tests / PR / merged / blocked.

Для Vimit это уже почти реализовано через GitHub Issues, `gh CLI`,
`MULTI-AGENT.md` и ручной merge-gate.

## goose

goose — open-source AI agent от Block/Square с CLI/Desktop/API и сильной
MCP-ориентацией. Он выглядит полезным кандидатом для отдельных задач:

- docs-аудит;
- исследовательские задачи;
- безопасные refactor-задачи;
- проверка MCP-интеграций;
- headless-запуск на сервере.

Ограничение: goose не должен обходить `AGENTS.md`. Если используется, он
должен работать как обычный агент проекта:

- берёт GitHub Issue;
- работает в отдельном worktree;
- не трогает чужие зоны;
- запускает проверки;
- открывает PR;
- не получает доступ к секретам без необходимости.

## pi_agent_rust

`pi_agent_rust` интересен как молодой Rust-порт coding agent, но для Vimit
его рано использовать в боевом workflow.

Причины:

- проект молодой;
- качество и устойчивость требуют отдельного аудита;
- нет доказанной совместимости с нашим `AGENTS.md`;
- риск дать агенту слишком широкие filesystem/tool права выше ожидаемой пользы.

Рекомендация: только sandbox-эксперимент вне основного репозитория. Не давать
ему задачи с push/PR до отдельного результата аудита.

## Что не начинать сейчас

Не нужно открывать новый архитектурный трек только из-за красивого инструмента.
В частности:

- не добавлять `jj`-обязательность в `AGENTS.md`;
- не добавлять новые Rust-зависимости для orchestration;
- не встраивать Rig/LLM-framework в Vimit в рамках этой задачи;
- не смешивать project-management workflow и live-core runtime design.

Для live-core действует отдельный план: сначала v1 по уже зафиксированной
спецификации, потом новые research-треки.

## Рекомендуемый следующий шаг

Добавить в процесс координатора короткий ручной чеклист:

```text
Перед выдачей новой задачи:
1. gh issue list --state open --label agent
2. gh pr list --state open
3. git worktree list
4. Проверить, нет ли занятой зоны из MULTI-AGENT.md
5. Назначить задачу агенту
6. После PR: проверить diff scope, CI и merge order
```

Если этого станет мало, следующий docs-Issue может описать
`tmux + gh + git worktree` как phone-friendly runbook для координатора.

## Источники

- Jujutsu docs: https://docs.jj-vcs.dev/latest/git-compatibility/
- Jujutsu working copy/workspaces: https://docs.jj-vcs.dev/latest/working-copy/
- Vibe-Kanban repository: https://github.com/BloopAI/vibe-kanban
- goose documentation: https://goose-docs.ai/
- goose repository: https://github.com/aaif-goose/goose
- pi_agent_rust repository: https://github.com/Dicklesworthstone/pi_agent_rust
