# Android Slint Spike

This spike records the Android path for vimit quota monitoring.

## Goal

Bring VibeMode spend and limit control to Android in two stages:

1. Android-friendly monitor: native app or notification/widget that shows quota
   percent, status, reset time, and the living creature state.
2. Native overlay: optional always-visible layer only if Android permissions and
   UX are acceptable on a real device.

## Feasibility

Slint supports Android through the `backend-android-activity-06` backend. The
project now has a separate Cargo feature:

```bash
cargo apk build --features android-gui --target aarch64-linux-android --lib
```

The local Windows environment has Android SDK/NDK tooling and `cargo-apk`
available when `ANDROID_HOME` / `ANDROID_NDK_HOME` are configured. If the build
fails before Rust compilation with missing `aarch64-linux-android-clang`,
install the Android NDK and point `ANDROID_NDK_HOME` to it.

## Proposed architecture

- Reuse existing quota parsing, API failover, thresholds, and creature state
  logic.
- Split desktop-only GUI parts from portable UI state:
  - keep tray, Windows sounds, and window positioning desktop-only;
  - expose a small Android model: status, active endpoint, four quota windows,
    reset text, creature state, and rate text.
- First Android UI should be a normal Slint Activity, not an overlay.
- Android overlay should be a second step.

## Overlay constraints

Android overlays require special "draw over other apps" permission and can be
blocked or hidden by sensitive apps. A stable production design likely also
needs a foreground service or notification entry point so polling is transparent
to the user and compliant with Android background execution limits.

## Foreground polling plan

Current Android support is an in-app Slint Activity that refreshes quota data
only while the screen is open. That path is useful for manual checks, but it is
not a compliant background polling solution for Android alerts.

The recommended background design is:

1. Keep quota fetch and parsing in shared Rust code.
2. Add a small Android-native foreground service entrypoint that owns the
   polling timer and lifecycle.
3. Let that service call a Rust helper that returns a privacy-safe quota
   summary for alerts.
4. Route alert delivery through a notification/vibration bridge owned by the
   Android layer.
5. Re-open the Slint Activity only for richer drill-down UI, not for the
   background timer itself.

This split keeps Android lifecycle responsibilities where Android expects them:
service startup, foreground notification, wake policy, retry scheduling, and
notification channels should stay in the Android integration layer, while Rust
continues to own `/v1/me` fetch, threshold evaluation, failover, and creature
state calculation.

## Service contract

The planned foreground service should exchange only minimal, privacy-safe data
with Rust:

- input: API key from existing secure local storage, poll interval, threshold
  configuration, demo/live flag
- output: worst level, affected quota window, percent used, reset countdown,
  endpoint label, and whether notification/vibration should fire

The service should not log API keys, raw `/v1/me` payloads, task titles, agent
session names, or other private workflow content. Foreground notification text
should stay generic, for example "VibeMode warning in 5h window", and vibration
should be keyed off alert level only.

## Notification and vibration bridge

The existing desktop notification code is not an Android implementation. The
Android path should expose a dedicated bridge that can be invoked from the
foreground polling service with a compact alert payload:

- `level`: `warning`, `danger`, or `recovery`
- `window_key`: `5h`, `24h`, `7d`, or `30d`
- `percent_used`: rounded numeric summary
- `reset_text`: already formatted countdown string

That bridge can then decide:

- which Android notification channel to use
- whether vibration is allowed and appropriate
- how to debounce repeated notifications
- whether tapping the notification opens the Slint Activity

## Manifest and Android constraints

The eventual implementation will likely require Android manifest metadata beyond
the current `INTERNET` permission, including foreground-service support and a
user-visible persistent notification while polling is active. Exact manifest
entries depend on the chosen Android service wrapper and should be added only
when the native integration layer is implemented.

Important Android limitations to preserve:

- background polling must not depend on the Slint Activity staying open
- foreground polling must always show a visible ongoing notification
- retries should tolerate process death and Activity recreation
- the service must avoid aggressive wakeups that look like stealth telemetry
- alert content must remain privacy-safe on the lock screen

## Current blocker

The repository currently has Android-gated Rust UI code in `src/lib.rs`, but it
does not yet contain the Android-native service/lifecycle layer needed to host a
true foreground polling service. Implementing that safely likely requires a
dedicated Android integration step around `android-activity`/manifest/service
wiring rather than a Rust-only patch inside the existing Slint Activity.

Because of that, the current issue is treated as a concrete implementation
plan and blocker record:

- shared Rust quota logic is reusable as-is
- foreground polling should be hosted by an Android-native service layer
- notification/vibration should be triggered through an Android bridge
- overlay work should wait until foreground polling is proven on device

## Acceptance path

1. Install Android SDK/NDK, `cargo-apk` or `cargo-ndk`, and
   `aarch64-linux-android` Rust target.
2. Keep the current Slint Activity for manual/live inspection and settings.
3. Add a foreground service entrypoint that polls through shared Rust quota
   logic and posts privacy-safe Android notifications.
4. Validate service lifecycle, notification delivery, and vibration behavior on
   a device or emulator.
5. Only then test optional native overlay permission flow.

## Manual test APK

Use the GitHub Actions workflow for a safe manual-test APK without creating a
production release:

1. Open GitHub Actions.
2. Select `Android Test APK`.
3. Click `Run workflow` on the branch you want to test.
4. Download the `vimit-android-test-apk` artifact.
5. Install `vimit.apk` on a device or emulator.

The artifact also contains `permissions.txt`, produced from the APK manifest.
It must include:

- `android.permission.INTERNET`
- `android.permission.VIBRATE`
- `android.permission.POST_NOTIFICATIONS`

Local build and manifest verification:

```bash
rustup target add aarch64-linux-android
cargo install cargo-apk --locked
rustc tools/guarded-run.rs -O -o target/guarded-run
./target/guarded-run --timeout-secs 1800 --heartbeat-secs 30 -- \
  cargo apk build --features android-gui --target aarch64-linux-android --lib
aapt dump permissions target/debug/apk/vimit.apk
```

`tools/guarded-run.rs` is a tiny Rust wrapper for long-running commands. It
prints heartbeat messages and exits with code `124` if the command exceeds the
timeout, which helps distinguish a real build hang from normal Android build
work.

This is intentionally not a GitHub Release. Release tags and production
release assets stay under the existing release workflow.

## Android API key entry and storage

For the current test APK, enter the API key directly in the Android UI:

1. Paste the key into the `VIBEMODE_API_KEY` field.
2. Tap `Сохранить ключ`.
3. Tap `Проверить` to refresh live quota data.

Do not bake `VIBEMODE_API_KEY` into the APK. Do not commit `.env` files,
keystores, signing passwords, or any real credentials to the repository.

Current storage model:

- the key is stored in the app's private Android app storage;
- the value is intended for local app use only and is not logged by the app;
- this is better than shipping a key inside the APK, but it is not yet
  encrypted at rest with Android Keystore-backed protection.

Future hardening should move this secret to Android Keystore or another
encrypted app-storage layer so the device keeps the same simple UI flow with a
stronger storage guarantee.

## CI Android library check

The main CI workflow installs the `aarch64-linux-android` Rust target and runs:

```bash
cargo check --locked --features android-gui --target aarch64-linux-android --lib
```

This is a fast compile guard for Android-gated Rust code. It does not install
or run the APK, does not require API keys, and does not replace the manual APK
artifact/device test above. Use the `Android Test APK` workflow when a change
needs manifest, packaging, or real-device validation.

## Agent burn alerts

The Android build declares `INTERNET`, `VIBRATE`, and `POST_NOTIFICATIONS`.
`src/lib.rs` exposes an Android-gated notification/vibration bridge for
runaway token burn events:

- alert text is fixed and privacy-safe;
- prompt text, paths, session IDs, tool arguments, and API keys are never
  included in notifications;
- repeated runaway events are debounced by `AndroidAlertGate`;
- runaway events set Vimichi into a visible alarm state in the Android UI;
- idle/recovery events clear the alarm state;
- Android 8+ receives a `vimit-agent-alerts` notification channel;
- Android 13+ still requires the user to grant notification permission at
  runtime before notifications are visible.

Runtime permission UX for Android 13+ should be implemented as a separate
Android task before relying on notifications as the only alert channel.

The Android alert bridge consumes `AgentBurnEvent` from the shared core
detector. The foreground/background polling source for those events is tracked
as a separate Android task.
