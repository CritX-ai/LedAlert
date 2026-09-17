# LedAlert releases

## 0.2.1 — Documentation and packaging

The first follow-up release polishes how LedAlert reaches you. No native application behavior changed.

### Highlights

- **Install from anywhere:** `cargo install ledalert` now carries [cargo-binstall](https://github.com/cargo-bins/cargo-binstall) metadata, so `cargo binstall ledalert --strategies crate-meta-data` fetches the official binary asset without a compiler. Install instructions no longer pin a version.
- **A clearer manual:** release badges, platform and desktop-environment icons in the compatibility matrix, a rewritten [contribution policy](docs/roadmap.md#contribution-policy), and streamlined security reporting through GitHub's private vulnerability reporting.

### Downloads and installation

`cargo install ledalert --locked` installs this release from [crates.io](https://crates.io/crates/ledalert). Without a toolchain, download the binary archive and `SHA256SUMS` from the [0.2.1 release](https://github.com/CritX-ai/LedAlert/releases/tag/v0.2.1), then verify before extracting. Checksums detect changed bytes, not publisher identity; use a source you trust. The [installation guide](docs/guide/install.md) has both routes.

| File | Contents |
| --- | --- |
| `ledalert-0.2.1-linux-x86_64.tar.gz` | Native app, offline documentation, assets, optional desktop launcher, build information and license notices. |
| `ledalert-0.2.1-source.tar.gz` | Source, locked dependency manifest, tests, assets and documentation. |
| `ledalert-0.2.1.crate` | Cargo package source, with cargo-binstall binary metadata. |
| `SHA256SUMS` | SHA-256 checksums for the release assets. |

Binary compatibility is unchanged from 0.2.0: **Linux x86_64, glibc 2.36 or newer**; see [Compatibility](docs/support.md).

## 0.2.0 — First public release

Your notifications, mapped to your room. LedAlert brings a native Linux workbench for turning application notifications and participating media playback into spatial light on a WLED RGB strip.

### Highlights

- **A room of your own:** draw a rectangle or an optional custom outline, place displays and trace the strip's route, elevation and LED order.
- **Light where it matters:** place application rules along the strip, choose glow or ripple effects and build gradients. Persistent notifications can keep their spot lit until dismissed.
- **A quiet now-playing marker:** participating MPRIS players add steady, dim light without audio capture or retained song metadata.
- **Try before you light:** preview rules in the native room view, then grant real output when you're ready. Each permission is an explicit, separate choice.
- **Everyday control:** save a finished room, undo edits, minimize while notifications continue, or use **Quiet mode** and **Reduced motion**.

### Downloads and installation

`cargo install ledalert --locked` installs the package from [crates.io](https://crates.io/crates/ledalert) (Rust **1.95+** and [native dependencies](docs/guide/install.md#native-dependencies) required). Without a toolchain, download the binary archive and `SHA256SUMS` from [GitHub Releases](https://github.com/CritX-ai/LedAlert/releases), then verify before extracting. Checksums detect changed bytes, not publisher identity; use a source you trust. The [installation guide](docs/guide/install.md) has both routes.

| File | Contents |
| --- | --- |
| `ledalert-0.2.0-linux-x86_64.tar.gz` | Native app, offline documentation, assets, optional desktop launcher, build information and license notices. |
| `ledalert-0.2.0-source.tar.gz` | Source, locked dependency manifest, tests, assets and documentation. |
| `ledalert-0.2.0.crate` | Cargo package source — what `cargo install ledalert` fetches. |
| `SHA256SUMS` | SHA-256 checksums for the release assets. |

### Binary compatibility

The supported setup is **Linux x86_64, KDE Plasma on Wayland, and one WLED RGB controller with its mapped strip**. The binary targets **glibc 2.36 or newer** and needs a graphical session, session D-Bus and Wayland/X11, xkbcommon and EGL/OpenGL runtime libraries. `BUILD-INFO.json` records the bundle's requirements.

Other desktop/device setups are unverified. Windows and macOS backends are not implemented. [Cargo and source builds](docs/guide/install.md#install-with-cargo) require **Rust 1.95 or newer**. See [Compatibility](docs/support.md) for current limits and [Everyday controls](docs/guide/operations.md) before enabling physical output.

## License status

LedAlert is dual-licensed under the [MIT License](LICENSE-MIT) or [Apache License 2.0](LICENSE-APACHE), **at your choice** (`MIT OR Apache-2.0`). Both license texts accompany the release packages. Third-party notices and license texts apply to their respective components, including the bundled font's SIL Open Font License in `assets/fonts/OFL.txt`.

[Repository and documentation](https://github.com/CritX-ai/LedAlert) · [Release downloads](https://github.com/CritX-ai/LedAlert/releases). Both archives include `README.md`, `CHANGELOG.md` and `docs/reference.md` for offline reading.
