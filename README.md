# <img src="assets/ledalert.png" alt="LedAlert" width="88" align="right"> LedAlert

**Your notifications, in the room.** LedAlert is a native Linux workbench for routing desktop notifications and media playback to spots of light on a WLED RGB strip. Draw the room, place your displays, map the strip — then watch each application claim its place.

[![GitHub release](https://img.shields.io/github/v/release/CritX-ai/LedAlert?label=GitHub&style=flat-square)](https://github.com/CritX-ai/LedAlert/releases/latest) [![crates.io release](https://img.shields.io/crates/v/ledalert?label=crates.io&style=flat-square)](https://crates.io/crates/ledalert) [![Verification workflow](https://github.com/CritX-ai/LedAlert/actions/workflows/verify.yml/badge.svg?branch=main&label=verify)](https://github.com/CritX-ai/LedAlert/actions/workflows/verify.yml) [![License](https://img.shields.io/crates/l/ledalert?style=flat-square)](#license-status)

---

<figure>
  <video controls muted loop playsinline preload="none" poster="docs/site/assets/screenshots/notifications.png" aria-label="Application notifications spreading along the mapped strip in LedAlert's configured room">
    <source src="docs/site/assets/screenshots/notifications.webm" type="video/webm">
    <source src="docs/site/assets/screenshots/notifications.mp4" type="video/mp4">
    <a href="docs/site/assets/screenshots/notifications.mp4">Watch the notification preview</a>.
  </video>
  <figcaption>The native app: colors spread from each application's assigned LEDs.</figcaption>
</figure>

## Install

With [Rust](https://rustup.rs/) and the [native dependencies](docs/guide/install.md#native-dependencies) installed:

```sh
cargo install ledalert --locked
```

Prefer a prebuilt binary? [cargo-binstall](https://github.com/cargo-bins/cargo-binstall#installation) fetches the official release asset:

```sh
cargo binstall ledalert --strategies crate-meta-data
```

You can also [download and verify](docs/guide/install.md#download-a-release-bundle) the `ledalert-*-linux-x86_64.tar.gz` archive and `SHA256SUMS` from the latest [GitHub Releases](https://github.com/CritX-ai/LedAlert/releases). Real nerds of course [build from source](docs/guide/install.md#build-from-source)?

## Quickstart

1. [Draw the room](docs/guide/room.md). Start with a rectangle; **Room shape…** handles L-shapes later.
2. [Place your displays](docs/guide/displays.md). Import KDE geometry or arrange them by hand.
3. [Map the strip](docs/guide/strip.md). Trace the route and LED order, then set the WLED address.
4. [Add rules](docs/guide/rules.md). Try examples and on-screen demos first — no hardware needed.
5. **Finish** saves the setup. [Enable lighting](docs/guide/operations.md#choose-whether-to-enable-real-lighting) whenever you're ready.

## Compatibility

Your desktop and your lights decide whether LedAlert fits. The short version:

<table class="support-matrix">
<thead><tr><th scope="col">Your desktop</th><th scope="col">Status</th></tr></thead>
<tbody>
<tr><th scope="row"><span class="platform-mark"><img src="docs/site/assets/icons/kde.svg" alt="" width="20" height="20"></span>KDE Plasma on Wayland<small>Linux x86_64 — the primary target: display discovery, pinned-app suggestions, notifications, lock detection and media markers.</small></th><td><span class="support-status supported"><svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="square" aria-hidden="true"><circle cx="12" cy="12" r="9"/><path d="m7 12 3 3 7-7"/></svg> Supported</span></td></tr>
<tr><th scope="row"><span class="platform-mark"><img src="docs/site/assets/icons/kde.svg" alt="" width="20" height="20"></span>KDE Plasma on X11<small>LedAlert runs as a normal X11 client; the full integration is unverified.</small></th><td><span class="support-status unverified"><svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="square" aria-hidden="true"><circle cx="12" cy="12" r="9"/><path d="M9.5 9a2.5 2.5 0 0 1 5 0c0 2-2.5 2-2.5 4m0 3h.01"/></svg> Unverified</span></td></tr>
<tr><th scope="row"><span class="platform-mark"><img src="docs/site/assets/icons/gnome.svg" alt="" width="20" height="20"></span>GNOME<small>Standard notification and lock services; manual display placement replaces KDE discovery.</small></th><td><span class="support-status unverified"><svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="square" aria-hidden="true"><circle cx="12" cy="12" r="9"/><path d="M9.5 9a2.5 2.5 0 0 1 5 0c0 2-2.5 2-2.5 4m0 3h.01"/></svg> Unverified</span></td></tr>
<tr><th scope="row"><span class="platform-mark"><img src="docs/site/assets/icons/xfce.svg" alt="" width="20" height="20"></span>Xfce</th><td><span class="support-status unverified"><svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="square" aria-hidden="true"><circle cx="12" cy="12" r="9"/><path d="M9.5 9a2.5 2.5 0 0 1 5 0c0 2-2.5 2-2.5 4m0 3h.01"/></svg> Unverified</span></td></tr>
<tr><th scope="row"><span class="platform-mark"><img src="docs/site/assets/icons/cinnamon.svg" alt="" width="20" height="20"></span>Cinnamon</th><td><span class="support-status unverified"><svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="square" aria-hidden="true"><circle cx="12" cy="12" r="9"/><path d="M9.5 9a2.5 2.5 0 0 1 5 0c0 2-2.5 2-2.5 4m0 3h.01"/></svg> Unverified</span></td></tr>
<tr><th scope="row"><span class="platform-mark"><img src="docs/site/assets/icons/linux.svg" alt="" width="20" height="20"></span>Other Linux desktops<small>MATE, Budgie, LXQt and others use the same standard notification and lock services; they are unverified.</small></th><td><span class="support-status unverified"><svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="square" aria-hidden="true"><circle cx="12" cy="12" r="9"/><path d="M9.5 9a2.5 2.5 0 0 1 5 0c0 2-2.5 2-2.5 4m0 3h.01"/></svg> Unverified</span></td></tr>
<tr><th scope="row"><span class="platform-mark"><img src="docs/site/assets/icons/windows.svg" alt="" width="20" height="20"></span>Windows<small>No desktop backend is implemented.</small></th><td><span class="support-status unavailable"><svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="square" aria-hidden="true"><circle cx="12" cy="12" r="9"/><path d="m9 9 6 6m0-6-6 6"/></svg> Unsupported</span></td></tr>
<tr><th scope="row"><span class="platform-mark"><img src="docs/site/assets/icons/apple.svg" alt="" width="20" height="20"></span>macOS<small>No desktop backend is implemented.</small></th><td><span class="support-status unavailable"><svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="square" aria-hidden="true"><circle cx="12" cy="12" r="9"/><path d="m9 9 6 6m0-6-6 6"/></svg> Unsupported</span></td></tr>
</tbody>
</table>

<table class="support-matrix">
<thead><tr><th scope="col">Your lights</th><th scope="col">Status</th></tr></thead>
<tbody>
<tr><th scope="row">One WLED RGB controller and strip<small>One continuous path, mapped in your room; up to 8,192 RGB LEDs.</small></th><td><span class="support-status supported"><svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="square" aria-hidden="true"><circle cx="12" cy="12" r="9"/><path d="m7 12 3 3 7-7"/></svg> Supported</span></td></tr>
<tr><th scope="row">Multiple controllers, separate runs or matrices</th><td><span class="support-status unavailable"><svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="square" aria-hidden="true"><circle cx="12" cy="12" r="9"/><path d="m9 9 6 6m0-6-6 6"/></svg> Unsupported</span></td></tr>
<tr><th scope="row">RGBW/CCT-specific output and other controller protocols</th><td><span class="support-status unavailable"><svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="square" aria-hidden="true"><circle cx="12" cy="12" r="9"/><path d="m9 9 6 6m0-6-6 6"/></svg> Unsupported</span></td></tr>
</tbody>
</table>

The Linux binary needs **glibc 2.36 or newer**, a graphical session, session D-Bus and working OpenGL/EGL. Per-feature details and the tested baseline: [Compatibility](docs/support.md). Your setup is not listed? See the [roadmap](docs/roadmap.md#future-directions) for where support is headed and the [contribution policy](docs/roadmap.md#contribution-policy) for how to land it — verified additions from real environments are exactly what we want.

## Lighting permissions

**Lighting starts disabled on every launch.** Connecting to WLED is read-only, but contacts the device. [Enable lighting](docs/guide/operations.md#choose-whether-to-enable-real-lighting) grants session-only permission; demos and **Preview** then reach the strip too. **Guide with real lights → Take control** is separate consent for whole-strip mapping, capped at two minutes and 25/255 per RGB channel. Neither permission is saved.

- **Minimize** keeps LedAlert running; **Close** stops it. No hidden daemon, no tray-only mode, no automatic startup.
- Lock and **Quiet mode** inhibit output. Notifications received while inhibited are discarded, not replayed.
- WLED HTTP/DDP is unauthenticated — use one controller on a trusted LAN. Releasing control does not guarantee darkness; recovery from a crash or network loss depends on WLED's realtime timeout.
- Brightness limits cap RGB data, not electrical power. LedAlert is ambient awareness, not a safety alarm. **Reduced motion** gives steady indications instead of fades and ripples.

LedAlert does not retain notification text or song metadata, capture audio, or read screen content. See [privacy details](docs/reference-runtime.md#desktop-integration-and-privacy).

## Field manual

[Install](docs/guide/install.md) · [Room](docs/guide/room.md) · [Displays](docs/guide/displays.md) · [Strip](docs/guide/strip.md) · [Rules](docs/guide/rules.md) · [Everyday controls](docs/guide/operations.md) · [Troubleshooting](docs/guide/troubleshooting.md)

[Compatibility](docs/support.md) · [Technical reference](docs/reference.md) · [Roadmap and contributing](docs/roadmap.md) · [Release notes](RELEASE.md) · [Changelog](CHANGELOG.md) · [Security reporting](SECURITY.md)

The field manual is built with [ReGen](https://regen.critx.ai/).

## License status

LedAlert is dual-licensed under the [MIT License](LICENSE-MIT) or [Apache License 2.0](LICENSE-APACHE), **at your choice** (`MIT OR Apache-2.0`). Third-party components retain their own licenses, supplied in bundle notices and license texts. The bundled font's [SIL Open Font License](assets/fonts/OFL.txt) applies to that font only.

