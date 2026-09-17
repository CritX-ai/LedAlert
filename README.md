# LedAlert

**Your notifications, in the room.** LedAlert is a native Linux workbench for routing desktop notifications and media playback to spots of light on a WLED RGB strip. Draw the room, place your displays, map the strip — then watch each application claim its place.

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
cargo install ledalert --version 0.2.0 --locked
```

Rather skip the compiler? Download `ledalert-0.2.0-linux-x86_64.tar.gz` and `SHA256SUMS` from [GitHub Releases](https://github.com/CritX-ai/LedAlert/releases), then [verify and run](docs/guide/install.md#download-a-release-bundle). Prefer [building from source](docs/guide/install.md#build-from-source)? That works too.

Both routes end at the same native setup with lighting off. Lighting always starts disabled — enabling it is a deliberate, later choice.

## Quickstart

1. [Draw the room](docs/guide/room.md). Start with a rectangle; **Room shape…** handles L-shapes later.
2. [Place your displays](docs/guide/displays.md). Import KDE geometry or arrange them by hand.
3. [Map the strip](docs/guide/strip.md). Trace the route and LED order, then set the WLED address.
4. [Add rules](docs/guide/rules.md). Try examples and on-screen demos first — no hardware needed.
5. **Finish** saves the setup. [Enable lighting](docs/guide/operations.md#choose-whether-to-enable-real-lighting) whenever you're ready.

## Lighting consent

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

