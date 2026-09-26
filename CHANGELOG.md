# Changelog

## 0.3.0-beta — macOS and Sidebar prerelease

- Native macOS support scoped to macOS 27 on Apple Silicon: CoreGraphics display UUIDs/global geometry, NSBundle application identities and NSWorkspace icons, and configuration under `~/Library/Application Support/LedAlert`.
- Actual Sidebar.app 2.2.6 pins while Sidebar is running; Dock pins otherwise. Hidden entries are excluded and complete manual ordering is respected. Unknown active Sidebar formats fail visibly rather than substituting Dock inventory; discovery never launches source applications or writes preferences.
- Bounded, read-only notification metadata from the protected macOS store, with explicit Full Disk Access guidance, UUID delivery membership, silent startup/recovery baselines, replacement/removal tracking and generation invalidation on loss. No notification payload column is read.
- Independent console-lock monitoring fails closed on unknown/stale state. CoreAudio app output activity supplies media markers, including calls and silent streams rather than exact player play/pause state; no samples or track metadata are captured.
- Native Apple Silicon `.app` archive with icon, offline manual, notices, source/payload manifests, checksum, portable Mach-O inspection and extracted-app smoke. Signing is ad-hoc only: no Developer ID, notarization, permission changes or Gatekeeper bypass.
- macOS native CI, cargo-binstall target metadata and three-platform release assembly/recovery. macOS provenance must match Linux/Windows source bytes, and stable Windows publication still requires trusted signing.
- Preserve unstripped build-time procedural macros for the pinned Rust 1.95 toolchain on macOS 27; the shipped executable retains its release stripping policy. macOS dependency notices include exact upstream licensing declarations and retained MIT terms.
- Portable macOS pin/display/lifecycle regressions and real native SQLite/WAL projection checks. Current-workstation inventory, denied-access status and GUI behavior are exercised; live permission changes, active audio, interactive lock transitions and physical WLED output remain separate acceptance work.
- GitHub-only development prerelease at `v0.3.0-beta`, not the latest stable release and not published to crates.io. Windows MSIX remains unsigned; macOS is ad-hoc signed and unnotarized. Versioned installation instructions separate checksum verification, OS access and lighting consent.

## 0.3.0-alpha — Windows 11 prerelease

- Native Windows notification monitoring with registered package identity and explicit permission, participating media playback, and fail-closed session lock/disconnect detection.
- Default application suggestions from actual Windows taskbar pins, including packaged apps; canonical application identities and local Shell icons.
- Windows display discovery with stable monitor identities, portrait layouts and mixed-DPI desktop coordinates.
- Windows configuration under `%APPDATA%\LedAlert`, Unicode paths, and repeatable atomic configuration/preferences saves.
- Windows x64 portable ZIP and **unsigned development-only MSIX**, external signing support, target-specific cargo-binstall metadata, and native Windows CI alongside Linux verification. No production signed installer is available yet.
- Release packaging preserves pinned upstream license bytes across Windows CRLF checkouts and includes Windows-only dependency notices.
- Linux-runnable Windows taskbar fixtures, notification lifecycle/timing/permission reducers, lock/media state tests, display geometry tests and package-security regressions. Existing Linux integrations remain supported.
- Cargo source inventory verification includes the portable Taskband fixture and rejects missing or changed fixture bytes, while keeping private captures outside the package.
- GitHub-only prerelease at `v0.3.0-alpha`, not the latest stable release and not published to crates.io; unversioned Cargo installation still selects the stable published release.
- Windows installation and release documentation distinguishes explicit Developer Mode registration from trusted production signing; no signature bypass or automatic certificate trust changes.
- Repeatable [Windows verification](docs/windows-verification.md) provides production-adapter inventory/observation, an isolated synthetic notification lifecycle GUI, opt-in registration/toast/cleanup helpers, public regression fixtures and a manual acceptance matrix. The alpha workstation passed the synthetic lifecycle; interactive media, lock/disconnect and physical WLED acceptance remain unverified.
- Left-aligned, concise README and compatibility matrices with feature notes outside narrow table columns; linked preview screenshot preserves video access on GitHub and crates.io.
- macOS was not implemented in this historical alpha; the 0.3.0-beta changes are listed above.

## 0.2.1 — Documentation and packaging

- cargo-binstall metadata in the Cargo package; official binary assets install via `cargo binstall ledalert --strategies crate-meta-data`.
- Version-agnostic install instructions; release badges and platform/desktop icons in the compatibility documentation.
- Contribution policy requiring demonstrably verified feature additions, and simplified private security reporting through GitHub.
- No native application behavior changes; binary compatibility is unchanged from 0.2.0.

## 0.2.0 — First public release

- Native Linux room editor with rectangular and optional custom outlines, display placement and spatial WLED strip mapping.
- Per-application notification rules with glow/ripple effects, gradients, one-off or persistent lights and local dismissal.
- MPRIS playback markers, additive color mixing and per-LED brightness limiting.
- Optional KDE display discovery, taskbar suggestions and local application icons.
- On-screen rule demos and a finished-room preview; real lighting starts disabled and needs session-only consent.
- Separate, time-limited physical LED guidance for mapping; lock-state and **Quiet mode** inhibition for everyday output.
- Notification processing while minimized, with no replay of notifications received during inhibition.
- Undo/redo, resumable setup, validated saves, integration status and command-line diagnostics.
- Linux x86_64 binary for glibc 2.36 or newer, source and Cargo installation options, and an offline field manual.

See [release notes](RELEASE.md) for downloads, the [quickstart](README.md) to begin, and the [roadmap](docs/roadmap.md) for future directions. LedAlert is [MIT OR Apache-2.0](README.md#license-status), at your choice.
