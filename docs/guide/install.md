# Install and connect

**[0.3.0-beta](https://github.com/CritX-ai/LedAlert/releases/tag/v0.3.0-beta)** adds **macOS 27 / Apple Silicon and Sidebar.app 2.2.6** to **Windows 11 x64** and **Linux x86_64 with KDE Plasma on Wayland**. This GitHub-only development prerelease is **not the latest stable release and is not published to crates.io**. A graphical session and working graphics driver are required; Linux also uses session D-Bus and EGL. Other desktop/OS versions have the [compatibility limits](../support.md) listed in the manual.

You do not need a strip to draw a room and try local rule demos. Lighting starts disabled on every launch; enabling output is an explicit, separate choice.

## macOS

Download the [Apple Silicon archive](https://github.com/CritX-ai/LedAlert/releases/download/v0.3.0-beta/ledalert-0.3.0-beta-macos-aarch64.tar.gz) and [MACOS-SHA256SUMS](https://github.com/CritX-ai/LedAlert/releases/download/v0.3.0-beta/MACOS-SHA256SUMS) into a new directory. Verify before extracting:

```sh
shasum -a 256 -c MACOS-SHA256SUMS &&
tar -xzf ledalert-0.3.0-beta-macos-aarch64.tar.gz
```

Expect `ledalert-0.3.0-beta-macos-aarch64.tar.gz: OK`. Stop on any failure. Checksums detect changed bytes, not publisher identity.

Quit any existing LedAlert instance. Keep the `.app` intact and install it in a stable location before choosing notification permissions. For a first user-local installation:

```sh
mkdir -p "$HOME/Applications" &&
test ! -e "$HOME/Applications/LedAlert.app" &&
ditto ledalert-0.3.0-beta-macos-aarch64/LedAlert.app "$HOME/Applications/LedAlert.app" &&
open "$HOME/Applications/LedAlert.app"
```

This chain intentionally stops if that app already exists. For an update, first back up your saved setup, quit LedAlert and move the previous app aside in Finder; then install the new complete bundle. Do not merge bundle contents or delete your configuration to update.

**The beta is ad-hoc signed and unnotarized.** There is no Developer ID or Apple trust claim. Gatekeeper may block a downloaded copy. If you choose to trust an app reported as from an unidentified developer, review [Apple's per-app **Privacy & Security → Open Anyway** decision](https://support.apple.com/en-us/102445) yourself; never override a malware warning, disable Gatekeeper or strip quarantine. Otherwise [build the tagged source](#build-from-source) or wait for a trusted installer. No installer or check grants OS access.

To build the bundle from the tagged source on Apple Silicon, install Xcode command-line tools/macOS SDK, Python **3.11+** and the pinned Rust toolchain:

```sh
rustup toolchain install 1.95.0 --profile minimal --component rust-docs --target aarch64-apple-darwin
python3 -B packaging/macos.py --verify --output dist/macos
python3 -B packaging/macos.py --inspect --output dist/macos
```

The output directory must not contain artifacts with the same names; release bytes are not overwritten. A separately requested `--gui-smoke` launches the extracted app with an isolated empty setup, verifies its window and leaves lighting disabled. Local builds are not byte-identical replacements for published downloads.

Sidebar pins, bundle icons, native displays, app audio-output activity and lock monitoring are available without notification permission. **Settings → Integration status → Open Full Disk Access settings** opens Apple's permission page; the permission itself is your decision. It is a broad OS grant, although LedAlert selects only notification identity/lifecycle metadata. Relaunch after a grant; rebuilding or moving an ad-hoc app can require a new approval. Present notifications become a silent baseline, not replayed alerts. OS access never enables physical lighting.

The binary deployment target is **macOS 14.2**, but desktop integration is exercised only on **macOS 27 arm64**. Intel/universal builds and other Sidebar versions are unverified. See [macOS verification](../support.md#macos-verification) for privacy, app-audio semantics and live checks not performed during local preparation.

## Windows 11

The [0.3.0-beta release](https://github.com/CritX-ai/LedAlert/releases/tag/v0.3.0-beta) provides a [portable ZIP](https://github.com/CritX-ai/LedAlert/releases/download/v0.3.0-beta/ledalert-0.3.0-beta-windows-x86_64.zip), an [unsigned development MSIX](https://github.com/CritX-ai/LedAlert/releases/download/v0.3.0-beta/ledalert-0.3.0-beta-windows-x86_64.msix) and [SHA256SUMS](https://github.com/CritX-ai/LedAlert/releases/download/v0.3.0-beta/SHA256SUMS). **There is no production signed installer yet.** The MSIX is an inspectable development artifact, not a normal double-click installation route. Use the ZIP for portable execution or explicit development registration below.

Download the ZIP and checksums from that same release. Compare this PowerShell result with the ZIP's entry in `SHA256SUMS` before extraction:

```powershell
(Get-FileHash .\ledalert-0.3.0-beta-windows-x86_64.zip -Algorithm SHA256).Hash
```

Checksums detect changed bytes; they do not establish publisher identity. Stop if the hash differs. Do not bypass Windows signature checks or import certificates to trust this unsigned beta. Future production MSIX releases require the [external signing prerequisites](../../RELEASE.md#windows-build-and-signing) and a verified publisher/signature.

After verifying, extract to a new directory and run:

```powershell
Expand-Archive .\ledalert-0.3.0-beta-windows-x86_64.zip -DestinationPath .\LedAlert-0.3.0-beta
.\LedAlert-0.3.0-beta\ledalert.exe
```

The bundle includes the offline manual. **Unregistered portable/Cargo execution cannot read Windows notifications**; taskbar suggestions, display import, media, lock detection and the editor remain available. Keep lighting off while checking the installation.

For local development, `python -B packaging/windows.py --output dist/windows` produces the ZIP and an **unsigned** MSIX using Rust 1.95.0, MSVC tools, Python 3.11+, the Windows SDK and `rustup component add rust-docs --toolchain 1.95.0`. It never installs packages or changes certificate trust.

If you explicitly enable Developer Mode, extract the ZIP to a permanent directory and register its manifest in PowerShell, substituting the actual absolute path:

```powershell
Add-AppxPackage -Register "C:\path\to\LedAlert\AppxManifest.xml"
```

Launch **LedAlert from Start**, not the loose executable, to use its registered package identity. Open **Settings → Integration status → Allow Windows notifications** and accept any Windows permission prompt. Windows may already report access as allowed. Denied or revoked access must be restored through Windows notification-access settings. Previously present notifications are not replayed when access becomes available.

The manifest declares `userNotificationListener`, `globalMediaControl` and `runFullTrust`; notification consent and lighting permission remain separate. To remove this development registration, run `Get-AppxPackage CritX.LedAlert | Remove-AppxPackage`. Nothing in packaging or verification silently registers the app or changes certificate trust. For repeatable native smoke tools, regression fixtures and manual acceptance steps, see [Windows verification](../windows-verification.md).

## Install with Cargo

**These unversioned Cargo commands select the stable published release, not 0.3.0-beta.** The beta `.crate` is downloadable source, not a registry publication. Use the versioned GitHub packages or tagged source below for the beta.

### Download a binary with cargo-binstall

[cargo-binstall](https://github.com/cargo-bins/cargo-binstall#installation) downloads the official release binary instead of compiling. It reads LedAlert's release metadata and fetches only GitHub release assets — no third-party mirrors, no surprise compilation:

```sh
cargo binstall ledalert --strategies crate-meta-data
ledalert --version
```

The beta's metadata includes Linux x86_64, Windows x64 MSVC and macOS arm64 assets, but it is not published to crates.io. Unversioned cargo-binstall uses the selected stable crate's available targets. On macOS a bare binary lacks the complete `.app` installation/permission workflow; use the native bundle. A portable Windows binary has neither registered package identity nor notification permission until explicitly registered.

### Compile with cargo install

Install [Rust](https://rustup.rs/) (**1.95 or newer**) and the [native dependencies](#native-dependencies), then:

```sh
cargo install ledalert --locked
ledalert
```

If `ledalert` is not found, add `~/.cargo/bin` to your `PATH`. To install from a local checkout instead, run `cargo install --path . --locked` from its root.

## Download a release bundle

Use the **[v0.3.0-beta release](https://github.com/CritX-ai/LedAlert/releases/tag/v0.3.0-beta)** and take each bundle and its checksums from that same version:

- **[`ledalert-0.3.0-beta-linux-x86_64.tar.gz`](https://github.com/CritX-ai/LedAlert/releases/download/v0.3.0-beta/ledalert-0.3.0-beta-linux-x86_64.tar.gz)** — prebuilt Linux application.
- **[`ledalert-0.3.0-beta-windows-x86_64.zip`](https://github.com/CritX-ai/LedAlert/releases/download/v0.3.0-beta/ledalert-0.3.0-beta-windows-x86_64.zip)** — Windows portable executable and offline manual.
- **[`ledalert-0.3.0-beta-windows-x86_64.msix`](https://github.com/CritX-ai/LedAlert/releases/download/v0.3.0-beta/ledalert-0.3.0-beta-windows-x86_64.msix)** — unsigned development artifact, not a production installer.
- **[`ledalert-0.3.0-beta-macos-aarch64.tar.gz`](https://github.com/CritX-ai/LedAlert/releases/download/v0.3.0-beta/ledalert-0.3.0-beta-macos-aarch64.tar.gz)** — ad-hoc signed, unnotarized Apple Silicon app.
- **[`SHA256SUMS`](https://github.com/CritX-ai/LedAlert/releases/download/v0.3.0-beta/SHA256SUMS)** — combined release checksums.

The Linux binary needs **glibc 2.36 or newer**; Windows packages need **Windows 11 x64**; macOS support is scoped to **macOS 27 / Apple Silicon**. Check `BUILD-INFO.json` inside the bundle and use the platform-specific instructions above.

### Verify, extract and run on Linux

In the directory containing both downloads, run this chain. It checks the archive and launches only if verification and extraction succeed:

```sh
grep '  ledalert-0\.3\.0-beta-linux-x86_64\.tar\.gz$' SHA256SUMS | sha256sum --check --strict - &&
tar -xzf ledalert-0.3.0-beta-linux-x86_64.tar.gz &&
cd ledalert-0.3.0-beta-linux-x86_64 &&
./bin/ledalert
```

**Expected result:** `ledalert-<version>-linux-x86_64.tar.gz: OK`, followed by the native room setup with lighting off.

**If verification fails**, download both files again from the same release and retry; do not bypass the check. Checksums detect changed bytes, not publisher identity — use a source you trust.

## First launch: establish your base

1. Follow **Room → Displays → Strip → Rules**. Start with [the room guide](room.md); measurements and advanced outlines are optional.
2. Try rules with [on-screen demos](rules.md#preview-on-screen) while arranging the scene; no hardware needed. **Try examples** is always local-only.
3. To reach hardware, set the WLED address and [connect](strip.md#connect-read-only). The initial `127.0.0.1` is a loopback target, not a discovered device.
4. Select **Finish** when the setup is valid. It saves and opens the finished room view. A top tab returns to editing.

The setup guide remembers its step; skip it or replay it under **Settings → Replay setup guide**. [Everyday controls](operations.md#choose-whether-to-enable-real-lighting) covers the lighting decision when you get there.

## If launch fails

Compare the bundle's `BUILD-INFO.json` with your CPU and runtime, then see [the application will not start](troubleshooting.md#the-application-will-not-start). If the binary is incompatible, [build from source](#build-from-source). A source build does not add unsupported desktop integrations.

## Optional Linux desktop-menu installation

Running from the extracted directory needs no installation. For a user-local launcher, run from the extracted bundle:

```sh
install -Dm755 bin/ledalert ~/.local/bin/ledalert
install -Dm644 packaging/io.github.critx.LedAlert.desktop ~/.local/share/applications/io.github.critx.LedAlert.desktop
install -Dm644 packaging/io.github.critx.LedAlert.svg ~/.local/share/icons/hicolor/scalable/apps/io.github.critx.LedAlert.svg
```

For a source build, replace `bin/ledalert` with `target/release/ledalert` (adjust for `CARGO_TARGET_DIR` if set). Ensure `~/.local/bin` is on the desktop session's PATH. Nothing installs a launcher or enables autostart automatically.

## Build from source

Install **Rust 1.95 or newer** and the [native dependencies](#native-dependencies). To build this exact beta rather than a moving branch:

```sh
git clone --branch v0.3.0-beta --depth 1 https://github.com/CritX-ai/LedAlert.git LedAlert-0.3.0-beta
cd LedAlert-0.3.0-beta
cargo build --release --locked
./target/release/ledalert
```

If you need a source archive, download **`ledalert-<version>-source.tar.gz`** and **`SHA256SUMS`** from the same release. Verify before extracting and building — expect `ledalert-<version>-source.tar.gz: OK`:

```sh
grep '  ledalert-.*-source\.tar\.gz$' SHA256SUMS | sha256sum --check --strict - &&
tar -xzf ledalert-*-source.tar.gz &&
cd ledalert-*-source &&
cargo build --release --locked &&
./target/release/ledalert
```

If `CARGO_TARGET_DIR` is set, the executable is in its `release/` directory instead. Initial builds need Cargo dependency access unless cached.

On Windows use the MSVC Rust toolchain and **Visual Studio Build Tools → Desktop development with C++**, including a Windows SDK. The executable is `.\target\release\ledalert.exe`; a source build is still unpackaged until explicitly registered or installed through MSIX.

On macOS, install Xcode command-line tools and use the Apple Silicon Rust target. `cargo run --release --locked` starts the editor directly; [native packaging](#macos) adds the application identity, icon and bundled resources. No D-Bus service or Linux desktop libraries are required.

For cross-platform development, run `cargo test --all-targets --locked` and `python -B -m unittest discover -s packaging -p "test_*.py"`. Linux needs `dbus-daemon` for the private-bus integration suite. Portable Windows/macOS pin parsers, notification reducers, timing/lock/media transitions, display topology and package inspection run without those native services:

```sh
cargo test --locked --test windows_taskbar --test windows_displays --test macos_taskbar --test macos_displays
cargo test --locked desktop::windows_state::tests
cargo test --locked desktop::macos_state::tests
python -B -m unittest discover -s packaging -p "test_*.py"
```

These focused commands supplement, rather than replace, the full suite. Native Windows/macOS jobs compile their adapters; hosted CI is not interactive desktop acceptance. Follow [Windows verification](../windows-verification.md), [macOS verification](../support.md#macos-verification) and the [release verification scope](../../RELEASE.md#development-checks) for observed coverage and untested transitions.

### Native dependencies

Compilation and runtime libraries for the Cargo and source-build routes. On Arch/CachyOS:

- `base-devel`
- `wayland`, `libxkbcommon`
- `libx11`, `libxcursor`, `libxi`, `libxrandr`
- `mesa`

KDE display discovery uses `kscreen-doctor` from `libkscreen`; without it, use **Displays → Add manually**. Other distributions use different package names. No root access or JavaScript toolchain is involved.

## Release contents

| File | Purpose |
| --- | --- |
| `ledalert-<version>-linux-x86_64.tar.gz` | Native executable, offline manual and notices. |
| `ledalert-<version>-windows-x86_64.zip` | Portable Windows executable, offline manual and notices; no notification access without registration. |
| `ledalert-<version>-windows-x86_64.msix` | Package with required capabilities; **0.3.0-beta is unsigned and development-only**. Stable production releases require trusted signing. |
| `WINDOWS-SHA256SUMS` | Checksums for the Windows pair, also included in the combined release receipt. |
| `ledalert-<version>-macos-aarch64.tar.gz` | Apple Silicon `LedAlert.app`, offline manual and notices; ad-hoc signed, not notarized. |
| `MACOS-SHA256SUMS` | macOS archive checksum, included in the combined release receipt. |
| `ledalert-<version>-source.tar.gz` | Source, locked dependency manifest, tests, assets and packaging instructions. |
| `ledalert-<version>.crate` | Cargo package source; distribution and registry publication are separate steps. |
| `SHA256SUMS` | SHA-256 checksums for all release assets. |

The Linux bundle places the executable at `bin/ledalert` and includes an optional desktop launcher. The Windows bundle places it at `ledalert.exe` and includes `AppxManifest.xml` for explicit development registration. The macOS bundle uses `LedAlert.app/Contents/MacOS/ledalert` with resources under `Contents/Resources`. All include the field manual, release notes, `SECURITY.md`, logo/font assets, `BUILD-INFO.json`, `LICENSE-MIT`, `LICENSE-APACHE`, `THIRD-PARTY-NOTICES.txt` and `licenses/`.

LedAlert is [MIT OR Apache-2.0](../../README.md#license-status), at your choice. Keep the license texts and third-party notices with redistributed bundles.

Next: [Draw the room](room.md).

