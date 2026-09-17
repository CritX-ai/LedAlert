# Install and connect

LedAlert needs **Linux x86_64, a graphical session with session D-Bus, and working OpenGL/EGL**. The supported desktop is **KDE Plasma on Wayland**; other Linux desktops are unverified, and Windows/macOS are not implemented. See [Compatibility](../support.md) for current limits.

You do not need a strip to draw a room and try local rule demos. Lighting starts disabled on every launch; enabling output is an explicit, separate choice.

## Install with Cargo

### Download a binary with cargo-binstall

[cargo-binstall](https://github.com/cargo-bins/cargo-binstall#installation) downloads the official release binary instead of compiling. It reads LedAlert's release metadata and fetches only GitHub release assets — no third-party mirrors, no surprise compilation:

```sh
cargo binstall ledalert --strategies crate-meta-data
ledalert --version
```

Only the **Linux x86_64** binary is published; other targets report a download error. Binary metadata ships with the crate starting with 0.2.1 and cannot serve older versions.

### Compile with cargo install

Install [Rust](https://rustup.rs/) (**1.95 or newer**) and the [native dependencies](#native-dependencies), then:

```sh
cargo install ledalert --locked
ledalert
```

If `ledalert` is not found, add `~/.cargo/bin` to your `PATH`. To install from a local checkout instead, run `cargo install --path . --locked` from its root.

## Download a release bundle

No compiler needed. From the latest release on [GitHub Releases](https://github.com/CritX-ai/LedAlert/releases), download into a new directory:

- **`ledalert-<version>-linux-x86_64.tar.gz`** — the prebuilt native application.
- **`SHA256SUMS`** — the release checksums.

The binary needs **glibc 2.36 or newer**. Check `BUILD-INFO.json` inside the bundle against your system if unsure.

### Verify, extract and run

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

## Optional desktop-menu installation

Running from the extracted directory needs no installation. For a user-local launcher, run from the extracted bundle:

```sh
install -Dm755 bin/ledalert ~/.local/bin/ledalert
install -Dm644 packaging/io.github.critx.LedAlert.desktop ~/.local/share/applications/io.github.critx.LedAlert.desktop
install -Dm644 packaging/io.github.critx.LedAlert.svg ~/.local/share/icons/hicolor/scalable/apps/io.github.critx.LedAlert.svg
```

For a source build, replace `bin/ledalert` with `target/release/ledalert` (adjust for `CARGO_TARGET_DIR` if set). Ensure `~/.local/bin` is on the desktop session's PATH. Nothing installs a launcher or enables autostart automatically.

## Build from source

Install **Rust 1.95 or newer** and the [native dependencies](#native-dependencies). In an existing source checkout:

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
| `ledalert-<version>-source.tar.gz` | Source, locked dependency manifest, tests, assets and packaging instructions. |
| `ledalert-<version>.crate` | Cargo package source — what `cargo install ledalert` fetches. |
| `SHA256SUMS` | SHA-256 checksums for all three assets. |

The binary bundle includes `bin/ledalert`, the field manual, release notes, `SECURITY.md`, logo/font assets, an optional desktop launcher, `BUILD-INFO.json`, `LICENSE-MIT`, `LICENSE-APACHE`, `THIRD-PARTY-NOTICES.txt` and `licenses/`.

LedAlert is [MIT OR Apache-2.0](../../README.md#license-status), at your choice. Keep the license texts and third-party notices with redistributed bundles.

Next: [Draw the room](room.md).

