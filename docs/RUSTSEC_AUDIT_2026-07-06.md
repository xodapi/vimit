# RustSec audit 2026-07-06

Issue: #189

Command:

```powershell
cargo audit --no-fetch --stale
```

Result summary:

- Vulnerabilities: 0
- Informational warnings: 13
- Advisory DB: local stale/offline mode, 900 advisories loaded
- Cargo.lock dependencies scanned: 758

`cargo audit` reports these as allowed informational warnings because the
current audit settings include `unmaintained`, `unsound`, and `notice`.

## Active warnings

| Advisory | Crate | Path | Reachability | Action |
| --- | --- | --- | --- | --- |
| RUSTSEC-2024-0413 | `atk 0.18.2` | `tray-icon -> libappindicator/gtk -> atk` | Linux desktop tray only, via optional `gui` feature. | No patch release exists in GTK3 bindings. Requires tray backend replacement or Linux tray feature split. |
| RUSTSEC-2024-0416 | `atk-sys 0.18.2` | `tray-icon -> libappindicator/gtk-sys -> atk-sys` | Linux desktop tray only, via optional `gui` feature. | Same as GTK3 tray migration. |
| RUSTSEC-2024-0412 | `gdk 0.18.2` | `tray-icon -> libappindicator/gtk -> gdk` | Linux desktop tray only, via optional `gui` feature. | Same as GTK3 tray migration. |
| RUSTSEC-2024-0418 | `gdk-sys 0.18.2` | `tray-icon -> libappindicator/gtk-sys -> gdk-sys` | Linux desktop tray only, via optional `gui` feature. | Same as GTK3 tray migration. |
| RUSTSEC-2024-0415 | `gtk 0.18.2` | `tray-icon -> libappindicator/gtk` | Linux desktop tray only, via optional `gui` feature. | Same as GTK3 tray migration. |
| RUSTSEC-2024-0420 | `gtk-sys 0.18.2` | `tray-icon -> libappindicator/gtk-sys` | Linux desktop tray only, via optional `gui` feature. | Same as GTK3 tray migration. |
| RUSTSEC-2024-0419 | `gtk3-macros 0.18.2` | `tray-icon -> gtk -> gtk3-macros` | Linux desktop tray only, via optional `gui` feature. | Same as GTK3 tray migration. |
| RUSTSEC-2024-0429 | `glib 0.18.5` | `tray-icon -> gtk/libappindicator -> glib` | Linux desktop tray only, via optional `gui` feature. The project does not call `glib::VariantStrIter` directly. | Patched in `glib >=0.20`, but current `tray-icon 0.24.1` still resolves GTK3/glib 0.18. Requires upstream tray dependency migration. |
| RUSTSEC-2026-0002 | `lru 0.12.5` | `ratatui 0.28.1 -> lru` | TUI dependency. Project does not call `lru::IterMut` directly; exposure is through ratatui internals. | Patched in `lru >=0.16.3`. Requires `ratatui` upgrade beyond current `0.28` line and TUI compatibility verification. |
| RUSTSEC-2024-0436 | `paste 1.0.15` | `ratatui 0.28.1 -> paste`; also `slint-build/slint -> image -> ravif -> rav1e -> paste` | Build/runtime transitive dependency. Project does not use `paste` directly. | Requires upstream dependency updates: ratatui upgrade for TUI path and Slint/image/ravif updates for Slint path. |
| RUSTSEC-2025-0141 | `bincode 2.0.1` | `slint-build/slint -> i-slint-compiler -> typed-index-collections -> bincode` | Build-time Slint compiler path. Not used by vimit runtime code directly. | Requires Slint compiler dependency change upstream or a Slint upgrade that removes it. |
| RUSTSEC-2025-0119 | `number_prefix 0.4.0` | `self_update 0.41.0 -> indicatif 0.17.11 -> number_prefix` | CLI/GUI self-update progress dependency. Project does not use `number_prefix` directly. | `self_update 1.0.0-rc.2` exists but is pre-release and likely API-impacting; defer to dedicated migration. |
| RUSTSEC-2024-0370 | `proc-macro-error 1.0.4` | `tray-icon -> gtk3-macros/glib-macros -> proc-macro-error` | Linux desktop tray compile-time macro path. | Same as GTK3 tray migration. |

## Dependency posture

- `tray-icon 0.24.1` is the latest published `tray-icon` release, but its
  default Linux backend still pulls GTK3/libappindicator crates with RustSec
  unmaintained warnings.
- `ratatui 0.28.1` is behind latest `0.30.2`; moving to `0.30.x` is a TUI
  migration rather than a lockfile-only update.
- `slint 1.16.1` is behind latest `1.17.0`; the project has active Android
  Slint risk tracking, so this should be handled as a GUI/Android migration
  with APK validation rather than folded into this audit-only issue.
- `self_update 0.41.0` latest is `1.0.0-rc.2`; this is not a low-risk stable
  patch update.

## Low-risk updates

No low-risk patch updates were available inside the current version
constraints:

```powershell
cargo update -p ratatui --dry-run
cargo update -p self_update --dry-run
cargo update -p tray-icon --dry-run
cargo update -p slint --dry-run
cargo update -p slint-build --dry-run
```

Each dry run locked 0 packages. The remaining fixes require explicit
migration work, not opportunistic lockfile churn.

## Follow-up work

- #200: Migrate TUI from `ratatui 0.28` to `0.30.x` and verify snapshots/rendering.
  This is the path to remove `lru 0.12.5` and the ratatui-side `paste`.
- #201: Evaluate the desktop tray backend: either replace `tray-icon` on Linux,
  split tray support behind a narrower feature, or accept/document GTK3
  warnings until `tray-icon` moves off GTK3.
- #202: Evaluate `slint 1.17.x` after Android smoke checks stabilize, because the
  Slint compiler/build graph is responsible for `bincode` and one `paste`
  path.
- #203: Evaluate `self_update 1.0.0-rc` only after release/update behavior is covered,
  because it is a pre-release major API change.
