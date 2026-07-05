# xodapi/vimit + live-core — Итоговый план (v5)

**Статус:** актуальный source of truth на 5 июля 2026 после завершения
Android #166 и bootstrap-трека `xodapi/live-core`.
Заменяет `PROJECT_PLAN_FINAL_v4.ru.md` как рабочий план. Версии v1-v4
остаются в `docs/` как история решений и не удаляются.

---

## 0. Что изменилось относительно v4

- #166 закрыт через PR #170: Android получил aggregate live token chart MVP.
- #105 остаётся umbrella/blocked parent: MVP-часть реализована через #166,
  строгий `input_tokens/sec` и `output_tokens/sec` split остаётся в #167.
- #167 остаётся blocked: нужен реальный telemetry producer, нельзя
  подделывать split из aggregate quota deltas.
- `xodapi/live-core` создан и больше не является идеей в плане.
- `xodapi/live-core` bootstrap завершён:
  - scaffold merged in `xodapi/live-core#1`;
  - `Clock` + `Instant` merged in `xodapi/live-core#5`;
  - `Threshold` classifier merged in `xodapi/live-core#6`;
  - `RollingBuffer` v1 merged in `xodapi/live-core#7`.
- Старые planning issues #174/#175/#176 в `xodapi/vimit` закрыты как
  перенесённые в `xodapi/live-core`.

---

## 1. Governance

`AGENTS.md` остаётся главным источником процесса. Любое изменение только через
GitHub Issue с label `agent`, отдельную ветку или worktree, проверки и PR.

Нельзя коммитить env-файлы с секретами, реальные API keys, keystore,
passwords, raw prompts, session transcripts или приватные task titles.

Основной worktree может быть грязным. Новые задачи безопаснее делать через
отдельный `git worktree` под конкретный issue.

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
- Aggregate Android live token chart MVP.

**Core/shared behavior:**

- Shared dashboard refresh model for desktop and Android.
- Core pulse model.
- Stuck token burn detector.
- Cache TTL and `--no-cache` behavior.
- Stale cache/offline API semantics aligned across CLI/TUI/JSON.
- Lightweight CI annotations for threshold breaches.
- MCP stdio server for IDE integration.

**live-core v1 primitives:**

- `Clock` trait and deterministic `Instant` newtype.
- `StdClock` behind the `std` feature.
- no-allocation `RollingBuffer<T, N>` with `f64` mean.
- `Threshold` classifier with explicit NaN/inf behavior.
- CI for Ubuntu, Windows, macOS, and `no_std` check.

---

## 3. Active queue

### 3.1 Android APK manual retest after #166

**Take next if the goal is Android release confidence.**

Goal:

- build/download the latest Android test APK artifact;
- install it on a real phone;
- verify API key entry, dashboard refresh, Vimichi card, pulse states,
  live chart, notification/vibration behavior, and idle timeout;
- record results in the relevant issue or a new manual test note.

This is not a code task unless testing finds a reproducible bug.

### 3.2 #105 — umbrella parent for live Android telemetry

Keep #105 as blocked/umbrella until manual Android review decides whether it
should close as MVP complete or remain open only for strict split telemetry.

Do not assign an agent directly to #105 for implementation.

### 3.3 #167 — input/output token-rate split

**Blocked.** Do not take until there is a concrete telemetry producer to update:
`abtop`-compatible collector, remote collector endpoint, or future VibeMode/API
endpoint.

This issue is contract/producer work, not UI polish. Do not fake input/output
split from aggregate quota deltas.

### 3.4 live-core next layer

Bootstrap is complete. The next live-core work should be new focused issues in
`xodapi/live-core`, not more planning issues in `xodapi/vimit`.

Recommended order:

1. restore or write the full live-core spec and security audit;
2. define a small activity-state module using `Clock`, `RollingBuffer`, and
   `Threshold`;
3. only then consider idle/offline tracker and stuck-burn extraction from
   `vimit`.

Do not start Rig, jj/git-cliff, Bevy/3D, iOS, or new research tracks as part of
this queue.

### 3.5 #129 — Windows `vimit.pdb` filename collision

**Blocked.** Keep as separate build-track debt. Do not mix with Android or
live-core work.

### 3.6 #163 — ExponentialSmoother/rolling_buffer docs tail

**Blocked, priority low.** This remains a future live-core v2 idea. It should
not be implemented until the live-core v1 activity layer has a concrete need.

---

## 4. Work order

1. Finish #177 by publishing this v5 plan.
2. Do Android APK manual retest from the latest artifact.
3. Update #105 based on manual test result:
   - close if MVP is accepted and #167 fully represents the remaining work;
   - keep blocked if user-facing acceptance is still incomplete.
4. Create the next `xodapi/live-core` issues for spec/security and activity
   state.
5. Continue code only from those focused issues.

---

## 5. Future plan updates

The next plan version must:

- be added as `PROJECT_PLAN_FINAL_v6.ru.md`, not overwrite v5;
- start with "what changed since v5";
- verify real GitHub Issues/PRs before declaring active queue;
- keep historical plan files in `docs/`;
- avoid turning speculative ideas into active work unless the Android/live-core
  queue has room for them.
