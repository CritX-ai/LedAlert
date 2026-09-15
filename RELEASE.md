# LedAlert 0.1.0 — LED alert! Panic optional.

The initial **private** release of LedAlert: spatial desktop notifications for WLED, built in Rust for Linux. Repository and release access remain restricted. A public release around **0.2.0** is the goal after further polish, broader device/use-case work and workflow-oriented documentation—not a promised date or current support commitment for more hardware.

## Highlights

- Draw a room, arrange displays and map a real strip, including elevation, LED allocation and direction.
- Route application notifications to positions with one-off or persistent glow/ripple effects, gradients and additive color mixing. MPRIS playback gets steady markers; reduced motion provides steady alternatives.
- Start with optional Plasma taskbar examples and local application icons. Refine rules with on-screen demos, then save a finished room view with shuffled preview.
- Keep control explicit: read-only connection, lighting disabled on every launch, lock/quiet inhibition, bounded real-light guidance and no persistent WLED settings writes.
- Use bounded undo/redo, resumable setup, validated atomic configuration saves and integration diagnostics.

## Artifacts

| File | Contents |
| --- | --- |
| `ledalert-0.1.0-linux-x86_64.tar.gz` | Native executable in `bin/ledalert`, documentation, logo/font assets, optional desktop launcher, `BUILD-INFO.json`, `THIRD-PARTY-NOTICES.txt` and `licenses/`. |
| `ledalert-0.1.0-source.tar.gz` | Explicitly selected source, locked dependency manifest, tests, assets, packaging and documentation; no prebuilt executable. |
| `SHA256SUMS` | SHA-256 checksums for both archives. |

The archives extract into `ledalert-0.1.0-linux-x86_64/` and `ledalert-0.1.0-source/`, respectively. Verify with `sha256sum -c SHA256SUMS` before extracting; keep both archives beside the checksum file for the complete check. A checksum detects byte changes, not publisher identity.

From the extracted **binary** directory, run `./bin/ledalert`. From the extracted **source** directory, use `cargo build --release --locked`, then `./target/release/ledalert` (adjust for `CARGO_TARGET_DIR` if set). The accompanying `README.md` gives complete steps; `docs/reference.md` covers native dependencies and optional desktop-menu installation. Nothing installs or enables autostart automatically.

## Binary compatibility

The supplied binary is **Linux x86_64**, dynamically linked, and requires **glibc 2.43 or newer**. Its native dependencies include libc, libm, libgcc and the ELF loader; the graphical runtime also loads Wayland/X11, xkbcommon and EGL/OpenGL libraries as needed. A working graphical session and session D-Bus are required. This is **not** a statically linked or universally portable Linux package.

`BUILD-INFO.json` in the binary bundle records the measured ABI requirements, linked dependencies, compiler/toolchain and checksums for the packaged build. These are measured requirements, not a cross-distribution compatibility certification. If the binary cannot run on your distribution, build from source with **Rust 1.95 or newer** and the native dependencies listed in the accompanying `docs/reference.md`.

## Connection and lighting permission

A new setup starts at **`127.0.0.1`**; enter your own strip's private LAN or link-local IPv4 address under **Strip → Connect**. Existing saved addresses are preserved. Connection and automatic reconnection are read-only: **there is no hardware lighting output on startup**.

For an older saved setup, use **Strip → Connect**, then **Finish** once to record the connection and completed-view state. Finishing saves and closes the editor; it never enables lighting. Future launches can restore the finished scene and probe the saved target read-only. A failed probe opens Strip with a retry action.

**Enable lighting** grants session-only permission. With it enabled, rule demos and toolbar Preview also reach the real strip; **Try examples** remains local-only. **Guide with real lights → Take control** is a separate consent action covering the entire strip for up to two minutes at a 25/255 per-channel cap. Guidance suspends normal alerts and does not automatically re-enable them afterward.

## Known support and limitations

- Targeted desktop: **KDE Plasma on Wayland**. Optional display detection requires `kscreen-doctor`; manual setup works without it. Other desktops and distribution combinations are not certified.
- Targeted device interface: **WLED RGB**, JSON API over HTTP 80 and DDP over UDP 4048, using private LAN, link-local or loopback IPv4. No additional controller/device support is claimed. The configured limit is 8,192 RGB LEDs, not a measured performance guarantee at that size.
- WLED HTTP/DDP is unauthenticated and has no atomic exclusive-control protocol. Use a trusted LAN and one controller. Output submission is not delivery acknowledgment.
- Lock, unknown lock state and quiet mode inhibit output. Notifications received while inhibited are discarded. Persistent lights require desktop closure/dismissal or local dismissal; popup timeout alone is insufficient.
- Releasing control does not guarantee darkness or restore a previous WLED effect. Crash/network-loss recovery depends on the device-configured realtime timeout, which LedAlert does not change.
- Brightness limits cap RGB data, not measured lumens or electrical power. This is ambient awareness software, not a safety alarm or confidentiality boundary.
- LedAlert remains a foreground application: minimizing keeps it running; closing stops it. There is no hidden daemon or tray-only mode.

## Verification scope

Local development verification completed **134 top-level tests plus seven private-D-Bus child executions**, formatting and lint checks, and debug/release builds. The actual executable was exercised. Automated desktop/network checks use private session buses and loopback WLED fixtures; they do not establish real-strip appearance, timing, electrical safety, broad device support or universal Linux compatibility.

No real-hardware actions were performed for this release preparation. The latest independent review did not produce a verdict before timing out; this release does not claim a completed independent review. The measured native requirements above and bundled build metadata are not a substitute for testing your own environment.

## License status

**No application license has been selected or granted.** Repository access, source availability and these artifacts do not grant an open-source license. Cargo package publication remains disabled. Third-party notices and license texts apply only to their respective components, including the bundled font's SIL Open Font License in `assets/fonts/OFL.txt`.

[Repository and documentation](https://github.com/CritX-ai/LedAlert) · [Release downloads](https://github.com/CritX-ai/LedAlert/releases). Both archives include `README.md`, `CHANGELOG.md` and `docs/reference.md` for offline reading.
