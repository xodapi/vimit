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

## Acceptance path

1. Install Android SDK/NDK, `cargo-apk` or `cargo-ndk`, and
   `aarch64-linux-android` Rust target.
2. Add a minimal Android entrypoint that calls the shared quota/creature model
   and renders a compact Slint view.
3. Validate a demo APK on a device or emulator.
4. Add notification/widget mode.
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
