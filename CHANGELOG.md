# Changelog

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
- Planned progression: beta adds macOS support and verification; final 0.3.0 follows cross-platform polishing. macOS is not implemented in this alpha.

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
