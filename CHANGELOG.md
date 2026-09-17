# Changelog

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
