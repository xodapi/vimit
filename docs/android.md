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
cargo build --features android-gui --target aarch64-linux-android
```

The local Windows environment has Rust Android targets installed, but does not
currently have the Android SDK/NDK compiler tools, `cargo-apk`, `cargo-ndk`,
`adb`, or Gradle. A target check reaches native dependency compilation and then
fails because `aarch64-linux-android-clang` is missing, so APK validation must
be done after installing Android SDK/NDK tooling.

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

## Agent burn alerts

The Android build declares `INTERNET`, `VIBRATE`, and `POST_NOTIFICATIONS`.
`src/lib.rs` exposes an Android-gated notification/vibration bridge for
runaway token burn events:

- alert text is fixed and privacy-safe;
- prompt text, paths, session IDs, tool arguments, and API keys are never
  included in notifications;
- repeated runaway events are debounced by `AndroidAlertGate`;
- Android 8+ receives a `vimit-agent-alerts` notification channel;
- Android 13+ still requires the user to grant notification permission at
  runtime before notifications are visible.

Runtime permission UX for Android 13+ should be implemented as a separate
Android task before relying on notifications as the only alert channel.
