# agent-device evaluation for Android APK smoke checks

This note records the first practical evaluation of `callstack/agent-device`
against the current `vimit` Android APK flow.

## Result

`agent-device` is a viable external Android smoke-test tool for `vimit`, but
the current recommendation is:

- use it first as a local QA/device verification tool;
- move it into CI only after the emulator boot path and app startup stability
  are reliable.

The tool itself worked well enough to prove value:

- installation succeeded on Windows;
- Android inventory and doctor checks succeeded;
- `agent-device` installed the current `vimit.apk` on an emulator;
- `agent-device open` launched the app;
- `agent-device snapshot -i` produced actionable evidence when the app failed.

## Installed version

- `agent-device 0.18.3`
- `adb 36.0.2`

## Local setup used

Prerequisites:

- Node.js and npm available on PATH
- Android SDK with `adb`
- an Android emulator or physical Android device

Install:

```bash
npm install -g agent-device@latest
agent-device --version
adb version
```

Inventory and readiness checks:

```bash
agent-device devices --platform android --json
agent-device doctor --platform android
adb devices -l
```

In this environment, `agent-device doctor --platform android` reported:

- `agent-device` ready
- Android toolchain detected
- one Android emulator available

## APK used

Tested APK path:

```text
C:\Users\d88u5\Downloads\vimit-android-release-test-apk\vimit.apk
```

## Minimal smoke flow

Install and launch:

```bash
agent-device install "C:\Users\d88u5\Downloads\vimit-android-release-test-apk\vimit.apk" --platform android --json
agent-device open "Vimit" --platform android --json
agent-device snapshot -i --platform android --json
```

Useful supporting checks:

```bash
agent-device appstate --platform android --json
adb -s emulator-5554 shell getprop sys.boot_completed
adb -s emulator-5554 shell getprop dev.bootcomplete
adb -s emulator-5554 shell getprop init.svc.bootanim
adb -s emulator-5554 logcat -d
```

## Observed behavior on current APK

The APK installed successfully and `agent-device open` launched
`pro.vibemod.vimit`.

However, the first interactive snapshot did not show the expected dashboard
content. Instead it captured the Android system error surface with controls
equivalent to:

- `App info`
- `Close app`

`agent-device` also reported:

- Android snapshot helper fallback to stock UIAutomator dump
- slow snapshot runtime in this failure path

Direct `adb logcat` evidence showed repeated native crashes / ANR-style failure
loops in `pro.vibemod.vimit`, with `android.app.NativeActivity` restarting and
`libvimit.so` present in the native backtrace.

That means `agent-device` already proved useful: it caught a real startup
failure automatically and surfaced evidence without manual phone inspection.

## Important caveat: emulator boot readiness

One practical caveat appeared during the run:

- `agent-device boot` timed out at least once;
- `agent-device install` / `appstate` initially failed with
  `Android device did not finish booting in time`;
- direct `adb` checks later showed the emulator was actually ready:
  `sys.boot_completed=1`, `dev.bootcomplete=1`, `bootanim=stopped`.

For now, the safest local sequence is:

1. boot or start the Android emulator;
2. wait until `adb shell getprop sys.boot_completed` returns `1`;
3. only then run `agent-device install/open/snapshot`.

This should be treated as an operational readiness caveat for CI as well.

## Recommended first regression scenario

Once the APK starts cleanly, the first Android smoke scenario should verify:

1. app launches successfully;
2. dashboard becomes visible;
3. vertical scroll works on a narrow screen;
4. settings can expand without covering content;
5. API key form remains reachable;
6. lower limit cards remain reachable.

Suggested commands:

```bash
agent-device open "Vimit" --platform android --json
agent-device snapshot -i --platform android --json
agent-device scroll down --platform android --json
agent-device snapshot -i --platform android --json
agent-device screenshot C:\tmp\vimit-agent-device-after-scroll.png --platform android --json
```

## CI recommendation

Current recommendation:

- keep `agent-device` as a local QA / pre-merge Android verification tool first;
- do not make it a required CI gate yet.

Reasons:

- emulator readiness needs explicit stabilization;
- the current APK still shows startup instability on the tested emulator;
- the first value is immediate debugging evidence, even before full CI
  hardening.

Once the startup path is stable, the next step is to connect this flow to the
existing APK artifact track and issue `#179`.
