# Install and connect

**0.3.0-alpha** is a GitHub prerelease for **Windows 11 x64** and **Linux x86_64 with KDE Plasma on Wayland**. A graphical session and working OpenGL are required; Linux also uses session D-Bus and EGL. Other Linux desktops are unverified; macOS has no desktop backend in this alpha. See [Compatibility](../support.md).

You do not need a strip to draw a room and try local rule demos. Lighting starts disabled on every launch; enabling output is an explicit, separate choice.

## Windows 11

The [0.3.0-alpha release](https://github.com/CritX-ai/LedAlert/releases/tag/v0.3.0-alpha) provides a [portable ZIP](https://github.com/CritX-ai/LedAlert/releases/download/v0.3.0-alpha/ledalert-0.3.0-alpha-windows-x86_64.zip), an [unsigned development MSIX](https://github.com/CritX-ai/LedAlert/releases/download/v0.3.0-alpha/ledalert-0.3.0-alpha-windows-x86_64.msix) and [SHA256SUMS](https://github.com/CritX-ai/LedAlert/releases/download/v0.3.0-alpha/SHA256SUMS). **There is no production signed installer yet.** The MSIX is an inspectable development artifact, not a normal double-click installation route. Use the ZIP for portable execution or explicit development registration below.

Download the ZIP and checksums from that same release. Compare this PowerShell result with the ZIP's entry in `SHA256SUMS` before extraction:

```powershell
(Get-FileHash .\ledalert-0.3.0-alpha-windows-x86_64.zip -Algorithm SHA256).Hash
```

Checksums detect changed bytes; they do not establish publisher identity. Do not bypass Windows signature checks or import certificates to trust this unsigned alpha. Future production MSIX releases require the [external signing prerequisites](../../RELEASE.md#windows-build-and-signing) and a verified publisher/signature.

The `ledalert-<version>-windows-x86_64.zip` bundle runs with `.\ledalert.exe` after checksum verification and extraction. It includes the offline manual. **Unregistered portable/Cargo execution cannot read Windows notifications**; taskbar suggestions, display import, media, lock detection and the editor remain available.

For local development, `python -B packaging/windows.py --output dist/windows` produces the ZIP and an **unsigned** MSIX using Rust 1.95.0, MSVC tools, Python 3.11+, the Windows SDK and `rustup component add rust-docs --toolchain 1.95.0`. It never installs packages or changes certificate trust.

If you explicitly enable Developer Mode, extract the ZIP to a permanent directory and register its manifest in PowerShell, substituting the actual absolute path:

```powershell
Add-AppxPackage -Register "C:\path\to\LedAlert\AppxManifest.xml"
```

Launch **LedAlert from Start**, not the loose executable, to use its registered package identity. Open **Settings → Integration status → Allow Windows notifications** and accept any Windows permission prompt. Windows may already report access as allowed. Denied or revoked access must be restored through Windows notification-access settings. Previously present notifications are not replayed when access becomes available.

The manifest declares `userNotificationListener`, `globalMediaControl` and `runFullTrust`; notification consent and lighting permission remain separate. To remove this development registration, run `Get-AppxPackage CritX.LedAlert | Remove-AppxPackage`. Nothing in packaging or verification silently registers the app or changes certificate trust. For repeatable native smoke tools, regression fixtures and manual acceptance steps, see [Windows verification](../windows-verification.md).

## Install with Cargo

**These unversioned Cargo commands select the stable published release, not 0.3.0-alpha.** The alpha is not published to crates.io. Its `.crate` GitHub asset is downloadable package source, not a registry release. Use the alpha downloads or [build its tagged source](#build-from-source) to try Windows support.

### Download a binary with cargo-binstall

[cargo-binstall](https://github.com/cargo-bins/cargo-binstall#installation) downloads the official release binary instead of compiling. It reads LedAlert's release metadata and fetches only GitHub release assets — no third-party mirrors, no surprise compilation:

```sh
cargo binstall ledalert --strategies crate-meta-data
ledalert --version
```

The alpha's GitHub binaries target **Linux x86_64** and **Windows x64 MSVC**, but unversioned cargo-binstall uses the stable crate's available targets; it does not select this alpha or add Windows support to an older stable release. A portable Windows binary has neither MSIX identity nor notification permission until explicitly registered.

### Compile with cargo install

Install [Rust](https://rustup.rs/) (**1.95 or newer**) and the [native dependencies](#native-dependencies), then:

```sh
cargo install ledalert --locked
ledalert
```

If `ledalert` is not found, add `~/.cargo/bin` to your `PATH`. To install from a local checkout instead, run `cargo install --path . --locked` from its root.

## Download a release bundle

No compiler needed. For this prerelease use the [v0.3.0-alpha download page](https://github.com/CritX-ai/LedAlert/releases/tag/v0.3.0-alpha), not the latest-stable shortcut. Download the matching bundle and checksums into a new directory:

- **[`ledalert-0.3.0-alpha-linux-x86_64.tar.gz`](https://github.com/CritX-ai/LedAlert/releases/download/v0.3.0-alpha/ledalert-0.3.0-alpha-linux-x86_64.tar.gz)** — the prebuilt Linux application.
- **[`ledalert-0.3.0-alpha-windows-x86_64.zip`](https://github.com/CritX-ai/LedAlert/releases/download/v0.3.0-alpha/ledalert-0.3.0-alpha-windows-x86_64.zip)** — Windows portable executable and offline manual.
- **[`ledalert-0.3.0-alpha-windows-x86_64.msix`](https://github.com/CritX-ai/LedAlert/releases/download/v0.3.0-alpha/ledalert-0.3.0-alpha-windows-x86_64.msix)** — unsigned, development only; not a production installer.
- **[`SHA256SUMS`](https://github.com/CritX-ai/LedAlert/releases/download/v0.3.0-alpha/SHA256SUMS)** — the release checksums.

The Linux binary needs **glibc 2.36 or newer**; Windows packages need **Windows 11 x64**. Check `BUILD-INFO.json` inside the bundle. For Windows verification and installation use the [Windows steps](#windows-11) above.

### Verify, extract and run on Linux

In the directory containing both downloads, run this chain. It checks the archive and launches only if verification and extraction succeed:

```sh
grep '  ledalert-.*-linux-x86_64\.tar\.gz$' SHA256SUMS | sha256sum --check --strict - &&
tar -xzf ledalert-*-linux-x86_64.tar.gz &&
cd ledalert-*-linux-x86_64 &&
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

Install **Rust 1.95 or newer** and the [native dependencies](#native-dependencies). Use the [`v0.3.0-alpha` tag](https://github.com/CritX-ai/LedAlert/tree/v0.3.0-alpha) or the [alpha source archive](https://github.com/CritX-ai/LedAlert/releases/download/v0.3.0-alpha/ledalert-0.3.0-alpha-source.tar.gz) for this prerelease. In that source checkout:

```sh
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

For cross-platform development, run `cargo test --all-targets --locked` and `python -B -m unittest discover -s packaging -p "test_*.py"`. Linux needs `dbus-daemon` for the private-bus integration suite. Windows taskbar fixtures, notification reducers, timing/lock/media transitions, display topology and package inspection run on Linux without Windows services:

```sh
cargo test --locked --test windows_taskbar --test windows_displays
cargo test --locked desktop::windows_state::tests
python -B -m unittest discover -s packaging -p "test_windows.py"
```

These focused commands supplement, rather than replace, the full cross-platform suite. Native Windows CI also compiles the API adapters and runs the regression suite; desktop permission and real toasts require an interactive Windows 11 smoke. Follow [Windows verification](../windows-verification.md) for repeatable native tools, portable fixtures and manual cases, and the [release verification scope](../../RELEASE.md#development-checks) for observed coverage and untested transitions.

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
| `ledalert-<version>-windows-x86_64.msix` | Package with required capabilities; **0.3.0-alpha is unsigned and development-only**. Stable production releases require trusted signing. |
| `WINDOWS-SHA256SUMS` | Checksums for the Windows pair, also included in the combined release receipt. |
| `ledalert-<version>-source.tar.gz` | Source, locked dependency manifest, tests, assets and packaging instructions. |
| `ledalert-<version>.crate` | Cargo package source; the alpha asset is not published to crates.io. |
| `SHA256SUMS` | SHA-256 checksums for all release assets. |

The Linux bundle places the executable at `bin/ledalert` and includes an optional desktop launcher. The Windows bundle places it at `ledalert.exe` and includes `AppxManifest.xml` for explicit development registration. Both include the field manual, release notes, `SECURITY.md`, logo/font assets, `BUILD-INFO.json`, `LICENSE-MIT`, `LICENSE-APACHE`, `THIRD-PARTY-NOTICES.txt` and `licenses/`.

LedAlert is [MIT OR Apache-2.0](../../README.md#license-status), at your choice. Keep the license texts and third-party notices with redistributed bundles.

Next: [Draw the room](room.md).

