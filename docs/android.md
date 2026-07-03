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

The local Windows environment has Rust Android targets, Android SDK/NDK,
`cargo-apk`, `cargo-ndk`, and `adb` installed. A debug APK can be built with:

```bash
cargo apk build --features android-gui --target aarch64-linux-android --lib
```

Release APK signing must stay local. Do not commit keystores or passwords; pass
them through environment variables when creating a release artifact:

```bash
CARGO_APK_RELEASE_KEYSTORE=/path/to/release.keystore \
CARGO_APK_RELEASE_KEYSTORE_PASSWORD=... \
cargo apk build --release --features android-gui --target aarch64-linux-android --lib
```

## API key entry and storage

Do not bake `VIBEMODE_API_KEY` into the APK, Android manifest, release
keystore, or repository files. Install the APK, open vimit on the phone, paste
the key into the `VIBEMODE_API_KEY` field, tap `Save key`, then tap `Check`.

The Android UI clears the input after a successful save and keeps the key in the
app's private data directory as `vimit-api-key`. Other normal Android apps
cannot read that directory, and vimit must not print the key to logs.

This is acceptable for test APKs, but it is not full encryption-at-rest. A rooted
device, device backup with app data access, debug tooling with the app UID, or a
compromised app process could still expose the key. Production hardening should
move the saved key to Android Keystore-backed encrypted storage before treating
the Android build as release-ready.

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
