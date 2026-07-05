# xodapi/vimit + live-core — Итоговый план (v3)

**Статус:** Актуальный source of truth на 5 июля 2026.
Заменяет `PROJECT_PLAN_FINAL.ru.md` (v1) и `PROJECT_PLAN_FINAL_v2.ru.md`
(v2) — оба сохраняются в `docs/` как историческая основа, не как
текущий план. v2 успел устареть быстрее, чем предполагалось на
момент написания — проект продвинулся значительно дальше между
2 и 5 июля.

---

## 0. Что изменилось относительно v2 (коротко)

- Расследование #28/#29 через claude-tap — **больше не нужно**.
  Оба issue закрыты 28 июня 2026, расхождение было временным
  артефактом, не системной проблемой. Раздел удалён из плана.
- Android APK — прошёл значительно дальше состояния "API key
  settings в работе". Ниже — полный актуальный список сделанного.
- live-core — остаётся отдельным треком, но явно **после**
  стабилизации Android, не параллельно. В v2 трек B шёл сразу
  вторым приоритетом — это скорректировано.

---

## 1. Governance — без изменений

`AGENTS.md` — главный источник правды по процессу. Любое изменение
только через Issue с label `agent`, отдельная ветка `issue-N-slug`,
проверки, PR. Не коммитить `.env`, ключи, keystore, пароли. Работать
в отдельном `git worktree` под issue, если основной worktree грязный.

---

## 2. Android/core — что уже сделано (не пересматривать, не мержить заново)

**Инфраструктура:**
- Android minimal APK entrypoint
- Android API key settings для test APK
- Android APK artifact/manual test workflow
- Android manifest/library checks
- `INTERNET` permission
- `POST_NOTIFICATIONS` в manifest metadata
- `android-gui` feature с нужной Android/JNI связкой
- Android touch-friendly layout
- Android notification/vibration bridge
- Android foreground polling plan (в docs)
- Android bridge вынесен из `src/lib.rs` в `src/android.rs`

**Общая логика:**
- Shared dashboard refresh model для desktop/Android
- Core pulse model
- Stuck token burn detector
- Android Vimichi alarm state для runaway burn
- Cache TTL и `--no-cache` поведение
- Stale cache/offline API семантика выровнена в CLI/TUI/JSON
- Lightweight CI annotations для threshold breaches
- MCP stdio server для IDE integration
- GUI/TUI/core refactor задачи закрыты

**Закрытые PR/Issues (для справки, не трогать):**
- #150/#157 — configurable cache TTL, explicit cache bypass
- #151/#156 — stale/offline semantics
- #152/#155 — CI annotation mode
- #154 — already resolved in main
- #101/#158 — docs про VIBEMODE_API_KEY на Android и безопасность
- #99 — already resolved
- #100/#102/#104 — старый Android stacked PR stack, закрыты как
  obsolete. **Не пытаться оживлять или мержить.**

---

## 3. Актуальный next queue — единственный источник приоритета

### 3.1 #113 — feat(android): request notification permission at runtime
**Главная текущая задача.** Android 13+ требует runtime-запрос
разрешения `POST_NOTIFICATIONS` — сейчас оно только в manifest,
без runtime-диалога.

Не трогать: API key handling, `.env`, desktop notifications (это
zone других задач).

Проверки перед PR:
```bash
cargo test --locked
cargo clippy --all-targets -- -D warnings
cargo fmt --check
cargo apk build --features android-gui --target aarch64-linux-android --lib
```

### 3.2 #103 — feat(android): show Vimichi mascot on dashboard
Старый PR (был поверх #100) закрыт как obsolete, но сам issue
актуален — нужен **чистый redo от свежего main**, не восстановление
старой ветки.

- Использовать существующий `PulseOrganism` (уже реализован для
  desktop overlay — переиспользовать, не писать заново)
- Работать с текущими `src/android.rs` и `ui/app.slint` — **не** со
  старым `src/lib.rs`, откуда Android-логика уже вынесена
- **Осторожно:** может конфликтовать с #113, если оба трогают
  `ui/app.slint` — проверить, кто идёт первым, второй агент делает
  rebase после merge первого

### 3.3 #105 — feat(android): agent activity pulse and live token chart
**Blocked.** Не брать сейчас. Пересмотреть после того, как #113 и
#103 смержены — вероятно оба зависят от структуры, которую они
меняют в `ui/app.slint`/`src/android.rs`.

### 3.4 #129 — chore(build): resolve Windows vimit.pdb filename collision
**Blocked.** Не брать.

---

## 4. Порядок работы прямо сейчас

1. **#113** (notification permission) — брать первым, наименее
   рискованная зона конфликта.
2. **#103** (Vimichi mascot) — брать после #113 смержен, или
   параллельно если явно подтверждено что `ui/app.slint`-изменения
   не пересекаются построчно.
3. **#105** — переоценить только после обоих выше, снять `blocked`
   вручную, не агентом.
4. **#129** — не трогать, ждёт отдельного решения по Windows build.

---

## 5. live-core — статус трека

Полная спецификация готова (`LIVE_CORE_FULL_SPEC.ru.md`,
`LIVE_CORE_SECURITY_AUDIT.ru.md`) — архитектура не пересматривается.

**Явное решение:** live-core — **следующий трек после стабилизации
Android** (после закрытия #113, #103, разблокировки и оценки #105),
не параллельный трек прямо сейчас. Причина — ограниченное внимание
координатора лучше сфокусировать на одном активном треке за раз,
даже если технически репозитории независимы.

Когда Android-очередь (раздел 3) закрыта — вернуться к разделу
"Трек B" из `PROJECT_PLAN_FINAL_v2.ru.md` (создание `xodapi/live-core`,
первые Issues по модулям Clock/rolling_buffer/threshold) без
изменений в самой архитектуре.

---

## 6. Явно отложено — не открывать заново

Без изменений относительно v2: Rig, jj+git-cliff, 3D/Bevy-визуализация,
iOS-порт, percentile/median в rolling_buffer, derive-макрос
StateMachine, serde-сериализация буферов. Все — future work,
не текущая работа.

**Дополнительно к списку:** не открывать новых research-направлений
до закрытия текущей Android-очереди — правило зафиксировано отдельно
по итогам ревью этого плана.

---

## 7. Правило для будущих обновлений плана

Каждое следующее обновление плана (v4 и далее) должно:
- явно указывать, что изменилось относительно предыдущей версии
  (раздел 0 по образцу этого документа), не переписывать всё заново
- сверяться с реальным состоянием GitHub Issues перед фиксацией
  "next queue", не полагаться на память предыдущей версии плана
- предыдущие версии не удалять, хранить в `docs/` с пометкой
  superseded — история решений имеет ценность сама по себе
