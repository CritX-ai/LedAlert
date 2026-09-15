# LED alert! Panic optional.

![LedAlert logo: a mildly alarmed coral pixel diode with a cyan visor and one raised eyebrow](packaging/io.github.critx.LedAlert.svg)

**Spatial desktop notifications for WLED, built in Rust for Linux.** Draw your room, place your displays, trace the strip and give each application a place to glow. LedAlert observes existing desktop notifications without replacing your notification daemon; MPRIS media playback can add a steady, dim marker.

**0.1.0 is the initial private release.** The [repository](https://github.com/CritX-ai/LedAlert) and downloads currently require access. A public release around **0.2.0** is the goal after more polish, broader device and use-case work, and workflow-oriented documentation. There is no promised date or claim of support for additional devices yet.

[Release notes and requirements](RELEASE.md) · [Changelog](CHANGELOG.md) · [Technical reference](docs/reference.md)

## Get it running

The current target is **Linux x86_64, KDE Plasma on Wayland, and WLED RGB strips**. You need a graphical session, D-Bus and working OpenGL/EGL. The prebuilt binary requires **glibc 2.43 or newer** and desktop runtime libraries; it is **not a universal Linux binary**. See [binary compatibility](RELEASE.md#binary-compatibility) for the measured requirements and build metadata. Build from source if your system cannot run this bundle.

### Downloaded binary

With repository access, obtain these three files from the [release downloads](https://github.com/CritX-ai/LedAlert/releases):

- `ledalert-0.1.0-linux-x86_64.tar.gz` — executable and accompanying material.
- `ledalert-0.1.0-source.tar.gz` — source and accompanying material.
- `SHA256SUMS` — SHA-256 checksums for both archives.

Put them in the same directory, verify before extraction, then run:

```sh
sha256sum -c SHA256SUMS
tar -xzf ledalert-0.1.0-linux-x86_64.tar.gz
cd ledalert-0.1.0-linux-x86_64
./bin/ledalert
```

If either checksum fails, do not use the archive. Checksums detect byte changes; they are not a signature or proof of who supplied the files. If only one archive is present, the full checksum check also reports the missing archive.

The binary bundle contains:

```text
ledalert-0.1.0-linux-x86_64/
  bin/ledalert
  README.md, CHANGELOG.md, RELEASE.md
  docs/reference.md
  assets/                         native logo and font/license material
  packaging/                      desktop launcher and vector logo
  BUILD-INFO.json                 measured build and ABI metadata
  THIRD-PARTY-NOTICES.txt          dependency attribution
  licenses/                       third-party license texts
```

These relative paths are inside the extracted directory. No installation is required. Desktop-menu installation is [optional and explicit](docs/reference.md#build-dependencies-and-optional-desktop-launcher).

### From source

Extract the verified source archive and build with **Rust 1.95 or newer**:

```sh
tar -xzf ledalert-0.1.0-source.tar.gz
cd ledalert-0.1.0-source
cargo build --release --locked
./target/release/ledalert
```

The same build/run commands work from a source checkout. If `CARGO_TARGET_DIR` is set, use its `release/ledalert` instead. See [native dependencies](docs/reference.md#build-dependencies-and-optional-desktop-launcher); no browser, JavaScript toolchain, root access or notification-daemon replacement is needed.

## Connect, map, then choose whether to light

**There is no hardware lighting output on startup.** Lighting permission is session-only and always starts disabled, even with a saved setup. The initial target is `127.0.0.1`, not a discovered strip; existing saved addresses are preserved.

1. Follow **Room → Displays → Strip → Rules**. Shape the room, position your displays and trace the physical strip. KDE display discovery is optional; manual setup remains available.
2. In **Strip**, enter **your WLED strip's private LAN or link-local IPv4 address** and select **Connect**. This reads device information without sending pixels or changing WLED settings. Use the room view and LED rail to map the path and LED order.
3. In **Rules**, choose applications, positions, palettes and **One-off / Persistent** lifetimes. With lighting disabled, **Demo rule**, **Demo all** and toolbar **Preview** stay on-screen; **Try examples** is always local-only.
4. Select **Finish** to save and open the finished room view. Finishing does not enable lighting. For an older saved setup, use **Strip → Connect**, then **Finish** once to record the connection and completed-view state.
5. Only when ready, choose **Enable lighting** and start with low brightness. **Rule demos and Preview now also drive the real strip.** To identify physical LEDs while mapping, **Guide with real lights → Take control** is a separate explicit permission covering the entire strip for at most two minutes, capped at 25/255 per RGB channel.

Future launches restore the finished view and can reconnect read-only; failure opens **Strip** with the error. Neither reconnection nor finishing restores lighting permission. The guide can be replayed without clearing your setup.

## What stays running—and what stops

- **Minimize:** LedAlert keeps running. **Close:** it stops, with save/discard/cancel for unsaved edits. There is no hidden daemon, tray-only mode or automatic startup installation.
- **Session lock, unavailable lock state or quiet mode:** lighting is inhibited. New notifications received while inhibited are discarded, not replayed on unlock.
- **Persistent notifications:** popup timeout alone does not clear their lights. Desktop dismissal/closure or LedAlert's local **Dismiss** does; local dismissal does not dismiss the desktop notification.
- **Valid same-device edits and undo/redo:** preserve current enablement. Invalid drafts pause output; changing the device address or LED count disarms it. Notification history and permission are never saved.
- **Disable, stop or failure:** transmission stops and an acquired strip is asked to leave realtime mode. WLED determines what resumes. After a crash or network loss, recovery depends on the device's existing realtime timeout.

## Safety and privacy

Use LedAlert for ambient awareness, **not a safety alarm**. Use one controller on a trusted local network: WLED HTTP and DDP are unauthenticated and cannot guarantee exclusive ownership. Releasing control does not guarantee darkness or restoration of the previous effect. The brightness cap limits RGB data, not measured light output or electrical power. **Reduced motion** removes decorative movement and replaces fades/ripples with steady indications.

LedAlert does not retain or log notification bodies, summaries or song metadata, capture audio, or read screen content. Desktop messages necessarily pass transiently through the D-Bus library. Detailed behavior, limits, diagnostics, configuration paths and failure handling are in the [technical reference](docs/reference.md).

## License status

**No application license has been selected or granted.** This is not an open-source license grant; access to source or binaries does not change that. Cargo package publication is disabled. Third-party components retain their own licenses, supplied in the binary bundle's notices/license texts; the bundled font's [SIL Open Font License](assets/fonts/OFL.txt) applies to that font only.
