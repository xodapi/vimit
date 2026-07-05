# Slint UI testing research

**Статус:** исследование для #180.  
**Дата:** 5 июля 2026.

---

## 1. Короткий вывод

Для `vimit` сейчас не стоит сразу включать Slint `system-testing` или `mcp` в
основную Android/GUI ветку. Проект закреплён на Slint `1.16.1`, а эти
публичные feature flags появляются в Slint `1.17.x`.

Правильный следующий шаг: добавить маленький Slint GUI smoke-test на текущей
версии `1.16.1`, без изменения runtime APK. Он должен проверять, что
`AppWindow` создаётся, Android-mode properties выставляются, а callbacks
`refresh-requested`, `demo-requested`, `save-api-key` можно регистрировать и
вызывать.

Slint `system-testing` / `mcp` стоит исследовать только отдельным upgrade-spike
после этого.

---

## 2. Что есть в текущем проекте

`Cargo.toml`:

- `slint = 1.16.1`
- `slint-build = 1.16.1`
- `gui` включает `slint/std`, `backend-winit`, `renderer-software`,
  `compat-1-2`
- `android-gui` включает `slint/std`, `backend-android-activity-06`,
  `renderer-software`, `compat-1-2`

`ui/app.slint` уже хорошо подготовлен для smoke-tests:

- exported root component: `AppWindow`
- Android switch: `is-android`
- API-key state: `api-key-input`, `api-key-configured`
- telemetry text: `token-rate-text`, `token-rate-raw`
- source badge: `active-endpoint-label`
- Android-specific layout sections gated by `is-android`
- callbacks: `refresh-requested`, `demo-requested`, `save-api-key`

Это значит, что первый тест может быть не pixel-perfect, а property/callback
smoke-test. Для нас это ценнее: он ловит сломанный Slint compile/API contract
после изменений в `ui/app.slint`.

---

## 3. Что умеет Slint 1.16.1

В локальном crate metadata для Slint `1.16.1` нет публичных features:

- `system-testing`
- `mcp`

В исходниках Slint `1.16.1` есть внутренний testing backend, который сам Slint
использует в своих тестах через `i_slint_backend_testing`, например:

- `init_no_event_loop()`
- `init_integration_test_with_mock_time()`
- `mock_elapsed_time()`

Но для `vimit` это не лучший первый шаг:

- это internal crate/API, не часть обычного публичного `slint` surface;
- добавление internal dev-dependency может быть хрупким;
- оно не проверяет Android runtime напрямую;
- оно может затянуть задачу в dependency/toolchain upgrade вместо простого
  тестового покрытия.

---

## 4. Что появляется в Slint 1.17.x

В локальном crate metadata для Slint `1.17.0` появились публичные features:

- `mcp`
- `system-testing`

Также есть crate `i-slint-backend-testing-1.17.0`, где явно экспортируются:

- `TestingBackend`
- `TestingBackendOptions`
- `init_no_event_loop()`
- `init_integration_test_with_mock_time()`
- `init_integration_test_with_system_time()`
- `mock_elapsed_time()`

Это выглядит перспективно для будущего UI automation, но не должно попадать в
основную Android очередь без отдельного spike. Slint `1.17.0` также указывает
более новый Rust toolchain baseline, поэтому upgrade нужно проверять на CI,
Android build и `cargo apk`.

---

## 5. Рекомендуемая стратегия

### Шаг 1: текущая версия, минимальный smoke-test

Создать отдельный issue:

`test(gui): add Slint AppWindow property/callback smoke test`

Acceptance:

- test-only code;
- без Slint upgrade;
- без Android runtime изменений;
- test создаёт `AppWindow`;
- выставляет `is-android = true`;
- выставляет `api-key-configured`, `api-key-input`, `token-rate-text`,
  `active-endpoint-label`;
- регистрирует callbacks `refresh-requested`, `demo-requested`,
  `save-api-key`;
- вызывает callbacks через generated `invoke_*` методы;
- проверяет, что callbacks сработали;
- не использует реальный API key.

Проверки:

```bash
cargo test --locked --features gui
cargo clippy --all-targets --features gui -- -D warnings
cargo fmt --check
cargo build --features gui --locked
```

### Шаг 2: отдельный Slint 1.17 testing spike

Создать отдельный issue:

`test(gui): spike Slint 1.17 system-testing`

Acceptance:

- отдельная ветка;
- попробовать Slint `1.17.x`;
- попробовать `slint/system-testing` или `slint/mcp` только для test/dev path;
- проверить CI на Windows/Linux/macOS;
- проверить `cargo apk build --features android-gui --target
  aarch64-linux-android --lib`;
- по итогам: либо upgrade issue, либо documented defer.

---

## 6. Что не делать сейчас

- Не добавлять `i-slint-backend-testing` в production dependencies.
- Не менять Slint версию вместе с Android APK smoke-test.
- Не превращать UI testing spike в рефактор `ui/app.slint`.
- Не проверять реальные API keys.
- Не считать Slint system-testing заменой ручного Android теста на телефоне.

---

## 7. Как это связано с #179

#179 остаётся Android APK smoke-test и ручная проверка на телефоне.

Slint UI automation может уменьшить риск регрессий в desktop/Android Slint
contract, но она не заменяет проверку:

- установки APK;
- Android runtime permissions;
- Android private app storage;
- notification/vibration;
- реального touch layout на телефоне.

Поэтому #180 должен закончиться рекомендацией и отдельной задачей на первый
smoke-test, а #179 продолжает жить как ручной APK gate.
