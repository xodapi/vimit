# xodapi/vimit + live-core — Итоговый план (v4)

**Статус:** актуальный source of truth на 5 июля 2026 после merge #165.
Заменяет `PROJECT_PLAN_FINAL_v3.ru.md` как рабочий план. Версии v1/v2/v3
остаются в `docs/` как история решений и не удаляются.

---

## 0. Что изменилось относительно v3

- #113 закрыт через PR #161: Android 13+ runtime `POST_NOTIFICATIONS`
  permission реализован.
- #103 закрыт через PR #162: Vimichi появился в Android dashboard.
- #164 закрыт через PR #165: для Android live token chart выбран
  `abtop --status-json` compatible telemetry contract.
- #105 больше не должен быть одной большой задачей для агента. Он остаётся
  umbrella/blocked parent, а реальная работа разложена на:
  - #166 — aggregate Android pulse + live token chart MVP;
  - #167 — future input/output token-rate split, пока blocked.
- live-core всё ещё не стартует параллельно Android. Следующий переход к
  live-core допускается после оценки результата #166.

---

## 1. Governance

`AGENTS.md` остаётся главным источником процесса. Любое изменение только через
GitHub Issue с label `agent`, отдельную ветку или worktree, проверки и PR.

Нельзя коммитить `.env`, реальные API keys, keystore, passwords, raw prompts,
session transcripts или приватные task titles.

Основной worktree может быть грязным. Новые задачи безопаснее делать через
отдельный `git worktree` вида `C:\project\vimit-issue-N`.

---

## 2. Уже сделано и не пересматривается

**Android infrastructure:**

- Android minimal APK entrypoint.
- Android API key settings для test APK.
- Android APK artifact/manual test workflow.
- Android manifest/library checks.
- `INTERNET`, `VIBRATE`, `POST_NOTIFICATIONS`.
- Runtime request for Android 13+ notification permission.
- `android-gui` feature with Android/JNI wiring.
- Android touch-friendly dashboard layout.
- Android notification/vibration bridge.
- Android foreground polling plan in docs.
- Android bridge moved out of `src/lib.rs` into `src/android.rs`.
- Vimichi Android dashboard card and alarm state.

**Core/shared behavior:**

- Shared dashboard refresh model for desktop and Android.
- Core pulse model.
- Stuck token burn detector.
- Cache TTL and `--no-cache` behavior.
- Stale cache/offline API semantics aligned across CLI/TUI/JSON.
- Lightweight CI annotations for threshold breaches.
- MCP stdio server for IDE integration.

**Closed/obsolete references:**

- #99, #101/#158, #150/#157, #151/#156, #152/#155, #154 are done.
- #100/#102/#104 are obsolete old Android stacked PRs. Do not revive them.
- v1/v2/v3 plan documents are historical, not active queue.

---

## 3. Active queue

### 3.1 #166 — Android aggregate pulse and live token chart MVP

**Take next.** This is the current Android implementation task.

Goal:

- show `Active` / `Idle` / `Unknown` pulse near Vimichi;
- show aggregate `tokens/sec` from `token_rate` + `interval_ms`;
- keep a short sample history and render a simple live chart;
- show idle/finished state after 30-60 seconds of zero rate;
- keep Android UI touch-friendly and avoid desktop behavior changes.

Allowed scope:

- `src/android.rs`
- `ui/app.slint`
- existing shared dashboard/pulse helpers if needed
- targeted tests for pulse/history/timeout behavior

Important boundary: #166 is aggregate-only. It must not claim strict
`input_tokens/sec` and `output_tokens/sec`.

### 3.2 #105 — umbrella parent for live Android telemetry

Keep #105 as blocked/umbrella until #166 lands and the result is reviewed.
Do not assign an agent directly to #105 for implementation.

After #166:

- close or update the MVP part of #105;
- decide whether #105 stays open only for the full input/output split;
- link #167 as the remaining blocker for strict split telemetry.

### 3.3 #167 — input/output token-rate split

**Blocked.** Do not take until there is a concrete telemetry producer to update:
`abtop`-compatible collector, remote collector endpoint, or future VibeMode/API
endpoint.

This issue is contract/producer work, not UI polish. Do not fake input/output
split from aggregate quota deltas.

### 3.4 #129 — Windows `vimit.pdb` filename collision

**Blocked.** Keep as separate build-track debt. Do not mix with Android work.

### 3.5 #163 — ExponentialSmoother/rolling_buffer docs tail

**Blocked, priority low.** This only records a future live-core v2 idea. Do not
start while Android #166 is active.

---

## 4. Work order

1. Finish #168 by publishing this v4 plan.
2. Take #166 or hand it to a focused Android agent.
3. After #166, retest manual Android APK and update #105 status.
4. Only after Android #166 is evaluated, decide whether to:
   - continue Android with #167 if a telemetry producer exists;
   - or switch to live-core bootstrap tasks.

Do not start Rig, jj/git-cliff, Bevy/3D, iOS, or new research tracks during
#166. Those ideas remain future work.

---

## 5. live-core status

live-core remains the next major track after Android stabilization, not a
parallel track right now.

The existing architecture documents stay valid:

- `LIVE_CORE_FULL_SPEC.ru.md`
- `LIVE_CORE_SECURITY_AUDIT.ru.md`
- `PROJECT_PLAN_FINAL_v2.ru.md` track B notes

When Android #166 is complete and reviewed, create focused live-core issues for
the first modules only: Clock, rolling buffer, threshold evaluation, and minimal
tests. Do not begin with research frameworks or UI experiments.

---

## 6. Future plan updates

The next plan version must:

- be added as `PROJECT_PLAN_FINAL_v5.ru.md`, not overwrite v4;
- start with "what changed since v4";
- verify real GitHub Issues/PRs before declaring active queue;
- keep historical plan files in `docs/`;
- avoid turning speculative ideas into active work unless the Android/live-core
  queue has room for them.
