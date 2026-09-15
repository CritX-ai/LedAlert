# Changelog

## 0.1.0 — Initial private release

- Native Rust desktop workbench for Linux, targeting KDE Plasma on Wayland and WLED RGB strips.
- Guided **Room → Displays → Strip → Rules** setup with a draggable three-dimensional room, upright displays, strip paths, elevation, explicit LED allocation and reversible device ordering.
- Optional KDE display discovery, pinned-taskbar rule suggestions, local application icons and icon-derived colors that preserve saved palettes.
- Per-application notification routing with one-off and persistent lifetimes, glow/ripple effects, two-to-eight-stop gradients, notification/media filters and independently dismissible persistent lights.
- MPRIS playback markers and additive color mixing with hue-preserving per-LED brightness limiting. Reduced motion provides steady alternatives.
- Rule demos, all-rule demos and shuffled finished-view preview; demos reach hardware only with explicit lighting enablement. Taskbar examples remain local-only.
- Finished-scene restoration and read-only saved-target reconnection. Startup always leaves lighting disabled; new setups begin with a loopback target rather than a preconfigured LAN device.
- Separate, explicit strip-guidance permission: at most two minutes of steady selected-area output, capped at 25/255 per RGB channel, with revocation on inhibition, navigation, target changes or failure.
- Passive freedesktop notification observation with daemon-correlated lifecycle tracking, bounded queues, fail-closed lock handling and no retained notification content or song metadata.
- Full-frame WLED DDP output, bounded read-only HTTP discovery, stale-producer watchdog, takeover refusal and acquired-device-only realtime release without persistent WLED configuration changes.
- Bounded undo/redo, validated versioned configuration, atomic saves, resumable setup, integration diagnostics, command-line configuration checking and read-only probing.
- Original pixel-alert logo, licensed bundled font, optional desktop launcher and private Linux/source release bundles with checksums, build metadata and third-party notices.

See the [release notes](RELEASE.md) for artifacts, support and verification limits, the [quickstart](README.md) to begin, and the [technical reference](docs/reference.md) for operational details. No application license is granted by this release.
