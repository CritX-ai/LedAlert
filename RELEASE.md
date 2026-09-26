# LedAlert releases

## 0.3.0-beta — macOS and Sidebar prerelease

**[v0.3.0-beta](https://github.com/CritX-ai/LedAlert/releases/tag/v0.3.0-beta)** adds native **macOS 27 / Apple Silicon** and **Sidebar.app 2.2.6** compatibility alongside Windows 11 x64 and Linux/KDE. This is a **GitHub-only development prerelease**, not the latest stable release and not a crates.io publication. Unversioned Cargo and cargo-binstall still select the stable published crate.

**Trust boundary:** Windows packages are unsigned; the MSIX is not a normal production installer. The macOS app is ad-hoc signed and unnotarized. Checksums and local signature integrity do not establish publisher trust. Notification access and physical-lighting consent remain separate, explicit choices. See the [step-by-step installation guide](docs/guide/install.md).

### Beta downloads

Download artifacts and checksums from this exact release. Do not mix versions or replace published bytes.

| File | Contents |
| :--- | :--- |
| [`ledalert-0.3.0-beta-macos-aarch64.tar.gz`](https://github.com/CritX-ai/LedAlert/releases/download/v0.3.0-beta/ledalert-0.3.0-beta-macos-aarch64.tar.gz) | Apple Silicon `LedAlert.app`, native icon, offline manual, notices and provenance; ad-hoc signed, not notarized. |
| [`MACOS-SHA256SUMS`](https://github.com/CritX-ai/LedAlert/releases/download/v0.3.0-beta/MACOS-SHA256SUMS) | macOS archive checksum. |
| [`ledalert-0.3.0-beta-windows-x86_64.zip`](https://github.com/CritX-ai/LedAlert/releases/download/v0.3.0-beta/ledalert-0.3.0-beta-windows-x86_64.zip) | Portable executable, registration manifest, offline manual and notices. |
| [`ledalert-0.3.0-beta-windows-x86_64.msix`](https://github.com/CritX-ai/LedAlert/releases/download/v0.3.0-beta/ledalert-0.3.0-beta-windows-x86_64.msix) | **Unsigned development artifact**, not a trusted installer. |
| [`WINDOWS-SHA256SUMS`](https://github.com/CritX-ai/LedAlert/releases/download/v0.3.0-beta/WINDOWS-SHA256SUMS) | Checksums for the Windows pair. |
| [`ledalert-0.3.0-beta-linux-x86_64.tar.gz`](https://github.com/CritX-ai/LedAlert/releases/download/v0.3.0-beta/ledalert-0.3.0-beta-linux-x86_64.tar.gz) | Linux executable, offline manual, launcher and notices; glibc 2.36 or newer. |
| [`ledalert-0.3.0-beta-source.tar.gz`](https://github.com/CritX-ai/LedAlert/releases/download/v0.3.0-beta/ledalert-0.3.0-beta-source.tar.gz) | Locked source, tests, native probes and packaging tools. |
| [`ledalert-0.3.0-beta.crate`](https://github.com/CritX-ai/LedAlert/releases/download/v0.3.0-beta/ledalert-0.3.0-beta.crate) | Cargo package source; downloadable, **not published to crates.io**. |
| [`SHA256SUMS`](https://github.com/CritX-ai/LedAlert/releases/download/v0.3.0-beta/SHA256SUMS) | Combined checksums for the release assets. |

### macOS highlights

- Native display UUIDs and global desktop-point geometry, native bundle identities/icons, and application storage under `~/Library/Application Support/LedAlert`.
- Actual Sidebar pins while it runs, respecting hidden entries and complete manual order; conventional Dock pins otherwise. This is compatibility with Sidebar, **not a new Sidebar-hosted plugin**. Unsupported active Sidebar metadata fails the scan without a Dock substitute.
- Read-only notification UUID membership, IDs, application identities and timestamps from the protected store; never notification payloads. Full Disk Access is a broad, separately chosen OS permission. Startup, recovery and permission restoration do not replay history; loss invalidates uncertain notification generations.
- Fail-closed console-lock observation and CoreAudio app output-activity markers. Audio activity includes calls and silent streams, **not exact player play/pause state**. No audio capture, track metadata, media helper executables or volume changes.
- Arm64 application packaging, portable Mach-O/inventory validation, native extraction/signature/GUI smoke and target-specific cargo-binstall metadata. Linux and Windows release/signature gates remain intact.

### macOS build and signing

From the [tagged source](https://github.com/CritX-ai/LedAlert/tree/v0.3.0-beta), on Apple Silicon with Xcode command-line tools/macOS SDK and Python 3.11+:

```sh
rustup toolchain install 1.95.0 --profile minimal --component rust-docs --target aarch64-apple-darwin
python3 -B packaging/macos.py --verify --gui-smoke --output dist/macos
python3 -B packaging/macos.py --inspect --output dist/macos
```

The package contains `LedAlert.app`, `BUILD-INFO.json` and `INSTALL.txt`; the executable is `LedAlert.app/Contents/MacOS/ledalert`. The manifest records all source and payload hashes, SDK/build-host versions, minimum deployment target and signing status. Existing release filenames are never overwritten. `--gui-smoke` explicitly launches only the extracted app with an isolated setup and lighting disabled; it neither installs the app nor grants OS access.

**Ad-hoc is not Developer ID signing, notarization or Apple trust.** Gatekeeper may block downloaded copies; do not disable it or strip quarantine. If you do not accept the beta's trust boundary, build the tagged source or wait for a trusted installer. The 14.2 deployment floor is not a compatibility claim for unexercised macOS versions. Intel/universal packages are not release targets.

The macOS CI jobs use the documented [`xcode-27` Apple Silicon runner](https://github.com/actions/runner-images/issues/14404). Hosted regressions/package checks do not establish GUI or permission acceptance. Release assembly compares the macOS source manifest against Linux/source/crate and Windows inputs, preserves exact native bytes for recovery, and records `macos_signing: ad-hoc` without weakening the [stable Windows signing gate](#windows-build-and-signing).

### Beta verification scope

Run the [development checks](#development-checks), native inventory/observation from [macOS verification](docs/support.md#macos-verification), and the packaging commands above. Current-workstation checks exercise actual Sidebar pin/icon resolution, display import, native GUI status, an unlocked console and idle CoreAudio metadata. The native notification reader is exercised against owned WAL fixtures through its 4,096-record bound, retained-history exclusion and payload-read rejection; lifecycle tests exercise baseline, raises/removals, discontinuities and fail-closed freshness.

**Live notification access is denied in this verification environment.** Grant/revocation, real notification delivery/dismissal, active audio, interactive lock/unlock and physical lighting have not been claimed as verified. Historical Windows evidence below is not a new Windows acceptance run, and this macOS work is not a new Linux interactive acceptance run. Private captures, device addresses and notification contents do not belong in public release assets.

The [production roadmap](docs/roadmap.md#release-boundary) separates this beta from trusted installers, clean-machine upgrades and the remaining live acceptance checks.

## 0.3.0-alpha — Windows 11 prerelease

LedAlert's [v0.3.0-alpha GitHub prerelease](https://github.com/CritX-ai/LedAlert/releases/tag/v0.3.0-alpha) adds **Windows 11 x64** alongside Linux x86_64 with KDE Plasma on Wayland. It is **not the latest stable release and is not published to crates.io**. Unversioned `cargo install ledalert --locked` and cargo-binstall continue to select the stable published crate and its available targets.

The historical alpha ships a Windows portable ZIP and an **unsigned, development-only MSIX**, not a production signed installer. macOS was not implemented in that alpha; current macOS support and its verification boundaries are described above.

### Highlights

- **Actual taskbar applications:** read Windows Explorer's pinned launchers, including packaged apps, with Shell-resolved names, exact application identities and icons. Start-menu inventory is not substituted for pins.
- **Native desktop integration:** Windows notification-center metadata drives notification rules and persistent dismissal; participating system-media sessions drive playback markers. Lock, disconnect and unavailable lock state inhibit output.
- **Windows display import and storage:** import active monitors with stable identities, portrait geometry and a single mixed-DPI coordinate space. Save Unicode paths and application IDs under `%APPDATA%\LedAlert`, with repeatable atomic replacement.
- **Cross-platform regression coverage:** captured Taskband records, notification lifecycle/timing, lock/media state, display topology, configuration and package-validation tests run on Linux. Native Windows CI also compiles the operating-system adapters and runs the regression suite.

Notification observation requires registered package identity and allowed Windows notification-listener access. The MSIX provides the required capabilities; an unregistered portable or Cargo installation cannot observe notifications. Access to notification metadata never grants physical lighting permission. Windows notifications use normal urgency, and notifications already present at startup or reconnection are not replayed.

### Release artifacts

| File | Contents |
| :--- | :--- |
| [`ledalert-0.3.0-alpha-windows-x86_64.zip`](https://github.com/CritX-ai/LedAlert/releases/download/v0.3.0-alpha/ledalert-0.3.0-alpha-windows-x86_64.zip) | Portable executable at `ledalert.exe`, registration manifest, offline manual, assets, build information and license notices. |
| [`ledalert-0.3.0-alpha-windows-x86_64.msix`](https://github.com/CritX-ai/LedAlert/releases/download/v0.3.0-alpha/ledalert-0.3.0-alpha-windows-x86_64.msix) | **Unsigned development artifact**, with notification, media and full-trust capabilities; not a production installer. |
| [`WINDOWS-SHA256SUMS`](https://github.com/CritX-ai/LedAlert/releases/download/v0.3.0-alpha/WINDOWS-SHA256SUMS) | Checksums for the Windows ZIP/MSIX pair. |
| [`ledalert-0.3.0-alpha-linux-x86_64.tar.gz`](https://github.com/CritX-ai/LedAlert/releases/download/v0.3.0-alpha/ledalert-0.3.0-alpha-linux-x86_64.tar.gz) | Linux executable, offline manual, assets, launcher and notices; glibc 2.36 or newer. |
| [`ledalert-0.3.0-alpha-source.tar.gz`](https://github.com/CritX-ai/LedAlert/releases/download/v0.3.0-alpha/ledalert-0.3.0-alpha-source.tar.gz) | Source, locked dependency manifest, portable fixtures, tests, native smoke and packaging tools. |
| [`ledalert-0.3.0-alpha.crate`](https://github.com/CritX-ai/LedAlert/releases/download/v0.3.0-alpha/ledalert-0.3.0-alpha.crate) | Cargo package source and target-specific cargo-binstall metadata; downloadable only, **not a crates.io publication**. |
| [`SHA256SUMS`](https://github.com/CritX-ai/LedAlert/releases/download/v0.3.0-alpha/SHA256SUMS) | Combined release checksums, including Windows artifacts. |

Use the [tagged source](https://github.com/CritX-ai/LedAlert/tree/v0.3.0-alpha) or source archive to compile the alpha. See [installation](docs/guide/install.md) and [compatibility](docs/support.md). The native Windows CI runner is Windows Server 2025, not a substitute for interactive Windows 11 acceptance.

### Windows package version ordering

Windows package versions are numeric, so packaging maps `0.3.0-alpha` to **`0.3.0.10000`**, beta to `0.3.0.20000`, release candidates to `0.3.0.30000`, and final `0.3.0` to `0.3.0.40000`. An optional `.N` after `alpha`, `beta` or `rc` adds `N` (1–9999) to that stage's fourth component; the bare stage uses zero offset. Thus `alpha.1` is `0.3.0.10001`, and every alpha sorts before every beta, then release candidates, then final.

Only these prerelease labels are accepted; `.0`, leading-zero counters, build metadata and components above 65535 are rejected. This conservative mapping is covered by packaging regressions. Upgrade ordering assumes the same package identity and publisher; switching a development identity to a production publisher may require explicit removal/reinstallation.

### Windows build and signing

On Windows x64 with MSVC build tools, Python 3.11+, the Windows SDK and Rust 1.95.0:

```powershell
rustup component add rust-docs --toolchain 1.95.0
python -B packaging/windows.py --verify --output dist/windows
python -B packaging/windows.py --inspect --output dist/windows
```

Local builds and unsigned prereleases, including this beta and the historical alpha, produce an **unsigned MSIX** and never install packages or modify certificate trust. Use the ZIP and [explicit Developer Mode registration procedure](docs/guide/install.md#windows-11) for notification testing. Do not double-click the unsigned MSIX expecting a production install, bypass signature checks or import certificates to trust it.

**Stable production releases remain signed-gated.** Pass `--publisher` matching the code-signing certificate's exact subject. After moving verified unsigned artifacts and the matching source checkout to an isolated signing environment, repackage the existing payload with that publisher and updated build metadata, then sign it without recompiling or executing the application:

```powershell
python -B packaging/windows.py --sign-existing dist/windows --output dist/windows-signed `
  --publisher "CN=YOUR-TRUSTED-PUBLISHER" --certificate-thumbprint YOUR_CERTIFICATE_THUMBPRINT `
  --timestamp-url https://YOUR-RFC3161-SERVICE --require-signed
```

The certificate must already be in `CurrentUser\My`; its trusted chain and private key are external release prerequisites. Signing uses SHA-256 Authenticode with an RFC3161 timestamp, followed by Windows signature verification. Package inspection checks inventory and provenance, not certificate trust, on either platform.

Stable release publication requires signed Windows artifacts and the combined release receipt. Its workflow requires protected-environment secrets `WINDOWS_SIGNING_PFX_BASE64` and `WINDOWS_SIGNING_PFX_PASSWORD`, and variables `WINDOWS_PUBLISHER` and `WINDOWS_TIMESTAMP_URL`. Windows build and signing run as separate steps, with signing credentials supplied only to signing. These external prerequisites are not satisfied by an unsigned development smoke.

Prereleases use the explicitly unsigned path, GitHub `prerelease=true` and `make_latest="false"`, and skip crates.io publication. Recovery reuses immutable release bytes rather than rebuilding or re-signing a partially published release. GitHub release notes include the matching version's [changelog](CHANGELOG.md) section.

### Development checks

Run `cargo test --all-targets --locked` and `python -B -m unittest discover -s packaging -p "test_*.py"` on the native target, plus `cargo clippy --all-targets --locked -- -D warnings`. Linux also needs `dbus-daemon` for isolated notification-service integration tests. [Windows verification](docs/windows-verification.md) and [macOS verification](docs/support.md#macos-verification) distinguish portable fixtures, native smoke and manual acceptance. Personal machine evidence belongs in a separately excluded private handoff, not public fixtures.

During alpha preparation on Windows 11 build 26200, the committed native probe returned **23 taskbar pins with icons** and **two monitors**, and its isolated packaged GUI passed all six synthetic notification lifecycle stages. Manual fixture delivery/removal and isolated registration/cleanup were also exercised. Windows already allowed notification access; a first-consent prompt was not verified. See the [scoped workstation evidence](docs/windows-verification.md#alpha-workstation-evidence), not a blanket platform-acceptance claim. Physical WLED output, active media playback, permission revocation and interactive lock/disconnect transitions remain unverified; portable state tests do not replace those manual cases.

Before stable production acceptance, repeat notification access in the registered Windows 11 GUI and in the macOS application under separately approved Full Disk Access; confirm lighting remains disabled without a separate output grant. Test media/audio with an appropriate source, permission changes and lock/disconnect transitions in a controlled session. Record skipped or blocked cases explicitly. The beta retains these unverified boundaries; neither regressions nor unsigned/ad-hoc smoke establish a trusted public signature or complete cross-platform acceptance.

## 0.2.1 — Documentation and packaging

The first follow-up release polishes how LedAlert reaches you. No native application behavior changed.

### Highlights

- **Install from anywhere:** `cargo install ledalert` now carries [cargo-binstall](https://github.com/cargo-bins/cargo-binstall) metadata, so `cargo binstall ledalert --strategies crate-meta-data` fetches the official binary asset without a compiler. Install instructions no longer pin a version.
- **A clearer manual:** release badges, platform and desktop-environment icons in the compatibility matrix, a rewritten [contribution policy](docs/roadmap.md#contribution-policy), and streamlined security reporting through GitHub's private vulnerability reporting.

### Downloads and installation

At publication, `cargo install ledalert --locked` installed 0.2.1 from [crates.io](https://crates.io/crates/ledalert); the unpinned command now selects the current release. To obtain the historical binary, download its archive and `SHA256SUMS` from the [0.2.1 release](https://github.com/CritX-ai/LedAlert/releases/tag/v0.2.1), then verify before extracting. Checksums detect changed bytes, not publisher identity; use a source you trust. The [installation guide](docs/guide/install.md) describes current installation routes.

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

In 0.2.0, other desktop/device setups were unverified and Windows and macOS backends were not implemented. [Cargo and source builds](docs/guide/install.md#compile-with-cargo-install) require **Rust 1.95 or newer**. See [Compatibility](docs/support.md) for current limits and [Everyday controls](docs/guide/operations.md) before enabling physical output.

## License status

LedAlert is dual-licensed under the [MIT License](LICENSE-MIT) or [Apache License 2.0](LICENSE-APACHE), **at your choice** (`MIT OR Apache-2.0`). Both license texts accompany the release packages. Third-party notices and license texts apply to their respective components, including the bundled font's SIL Open Font License in `assets/fonts/OFL.txt`.

[Repository and documentation](https://github.com/CritX-ai/LedAlert) · [Release downloads](https://github.com/CritX-ai/LedAlert/releases). Binary bundles and source archives include `README.md`, `CHANGELOG.md` and `docs/reference.md` for offline reading.
