# Compatibility

**[0.3.0-beta](https://github.com/CritX-ai/LedAlert/releases/tag/v0.3.0-beta)** adds native **macOS 27 on Apple Silicon**, including **Sidebar.app 2.2.6**, alongside Windows 11 x64 and Linux x86_64/KDE Plasma on Wayland. The output target remains one mapped WLED RGB strip. This development prerelease provides unsigned Windows and ad-hoc, unnotarized macOS packages; publication does not establish trusted installation or complete interactive acceptance.

## Desktop

| Your desktop | Status |
| :--- | :--- |
| <span class="platform-mark"><img src="site/assets/icons/windows.svg" alt="" width="20" height="20"></span> Windows 11 x64 | Native; interactive verification limits |
| <span class="platform-mark"><img src="site/assets/icons/kde.svg" alt="" width="20" height="20"></span> KDE Plasma / Wayland | Supported |
| <span class="platform-mark"><img src="site/assets/icons/kde.svg" alt="" width="20" height="20"></span> KDE Plasma / X11 | Unverified |
| <span class="platform-mark"><img src="site/assets/icons/gnome.svg" alt="" width="20" height="20"></span> GNOME | Unverified |
| <span class="platform-mark"><img src="site/assets/icons/xfce.svg" alt="" width="20" height="20"></span> Xfce | Unverified |
| <span class="platform-mark"><img src="site/assets/icons/cinnamon.svg" alt="" width="20" height="20"></span> Cinnamon | Unverified |
| <span class="platform-mark"><img src="site/assets/icons/linux.svg" alt="" width="20" height="20"></span> Other Linux desktops | Unverified |
| <span class="platform-mark"><img src="site/assets/icons/apple.svg" alt="" width="20" height="20"></span> macOS 27 / Apple Silicon | Native; permission-gated |

- **Windows 11:** native displays, actual taskbar pins, media playback and lock/disconnect detection. Notifications require registered package identity and permission; interactive acceptance remains limited.
- **KDE Plasma on Wayland:** Linux x86_64 display discovery, pinned-app suggestions, desktop notifications, lock detection and media markers.
- **KDE Plasma on X11:** runs as a normal X11 client; full integration is unverified.
- **GNOME:** standard notification and lock services; manual display placement replaces KDE discovery.
- **Other Linux sessions:** Xfce, Cinnamon, MATE, Budgie and LXQt remain unverified; the standard notification and lock services are used where available.
- **macOS:** native displays, Sidebar/Dock pins, bundle identities and icons, app audio-output activity and console-lock monitoring. Cross-app notification metadata requires Full Disk Access and the recognized private schema.

On Linux, notifications must use the freedesktop notification service, media markers need an MPRIS-capable player, and the session must allow monitoring and provide a working lock service.

On Windows, notifications use the Windows notification center and require registered package identity plus **Settings → Integration status → Allow Windows notifications**. The beta and historical alpha use portable execution and [explicit Developer Mode registration](guide/install.md#windows-11), not a trusted installer. Stable publication remains signed-gated; do not bypass signature checks or add certificate trust. Unregistered portable/Cargo execution has no notification access. Media markers use participating Windows system-media sessions. Windows and macOS notifications have normal urgency; a critical-only rule will not match them. In-app-only messages are not observed.

Windows taskbar discovery reads Explorer's current **Taskband Favorites version 3**, including packaged pins that have no `.lnk` file. This undocumented format is covered by captured Windows 11 build 26200 fixtures; an unknown format reports an error rather than guessing pins from the Start menu. Uninstalled or unresolved pins can be omitted; unavailable icons leave text-only tiles.

## macOS verification

The exercised environment is **macOS 27.0 arm64 with Sidebar 2.2.6**. The native probe resolves Sidebar pins with decoded icons, exact bundle lookup and CoreGraphics display geometry. The GUI displays the permission boundary and native status with lighting off. The macOS deployment target of **14.2** is a binary floor, **not a support claim for unexercised OS versions**; Intel and universal macOS packages are not provided.

- **Sidebar compatibility, not a new plugin:** while Sidebar runs, its bounded, read-only `applicationConfigurations` pin metadata is used. Hidden entries are excluded; configured manual order is respected when complete. Names and icons come from the verified application bundle, not Sidebar window titles or custom commands. No preferences are written and no source application is launched. An unknown Sidebar format fails the scan rather than silently substituting Dock pins.
- **Notification privacy and access:** Apple does not provide a public cross-app notification-listener API. LedAlert reads only active delivery UUIDs, record IDs, bundle identities and timestamps from the protected notification database. **Full Disk Access is a broad OS permission**, not a grant limited to these fields. The Settings button opens Apple's permission page; it never grants access. Approve access only if that boundary is acceptable, then relaunch the app. Lighting consent is separate.
- **Audio activity is not exact player state:** CoreAudio reports applications running output streams, including calls and silent streams. A player can keep a stream open while paused. No microphone/system-audio capture, samples, volume changes or track metadata are used.
- **Scoped evidence:** live observation exercised the unlocked console, idle CoreAudio metadata and denied notification access. Owned SQLite/WAL fixtures exercise the actual native notification reader, including UUID membership, retained-history exclusion and metadata-only authorization. Deterministic lifecycle tests cover baseline, raises/removals, recovery, stale lock state and queue loss. Live Full Disk Access grant/revocation, real notification delivery/dismissal, active audio, interactive lock/unlock and physical WLED behavior remain unverified.

Repeat the read-only probe from a source checkout:

```sh
cargo run --locked --example macos_probe -- inventory
cargo run --locked --example macos_probe -- observe 5
```

The probe reports counts and status, not notification contents or an application inventory dump. Keep lighting disabled. For manual acceptance, use your own application and separately approved permissions; do not post notification contents or unrelated desktop captures. Package verification and extracted-app launch are described under [macOS installation](guide/install.md#macos).

## Lights

| Your lights | Status |
| :--- | :--- |
| One WLED RGB strip | Supported |
| Up to 8,192 RGB LEDs | Supported |
| Multiple controllers or runs | Unsupported |
| Matrices | Unsupported |
| RGBW/CCT output | Unsupported |
| Other controller protocols | Unsupported |

Map one continuous strip path in your room. The 8,192-LED configuration limit is not a performance guarantee; actual performance depends on the device and network.

Use a trusted local network and one realtime controller. WLED needs HTTP **80** for connection and UDP **4048** for RGB output. LedAlert does not change persistent WLED settings; the device's own limits and realtime timeout still apply.

Your setup is not listed? See the [roadmap](roadmap.md#future-directions) for where support is headed and the [contribution policy](roadmap.md#contribution-policy) for how to land it — verified additions from real environments are exactly what we want.

## Before you install

The Linux binary needs **glibc 2.36 or newer**, a graphical session, session D-Bus and working OpenGL/EGL. Check the bundle's `BUILD-INFO.json` for its measured requirements. A matching CPU or glibc version alone does not guarantee desktop compatibility.

Windows requires **Windows 11 x64 (build 22000 or newer)** and a working desktop/OpenGL driver. ARM64-native Windows and Windows 10 are not release targets. Native CI uses Windows Server 2025 for compilation/tests/package checks; it does not establish interactive Windows 11 acceptance. [Windows verification](windows-verification.md) separates portable fixture coverage, native smoke tools, and manual notification-permission, media, lock/disconnect and physical-lighting checks.

macOS requires Apple Silicon and a graphical session. The `.app` is ad-hoc signed for local integrity only, without a Developer ID, notarization or an Apple trust claim. Gatekeeper may block downloaded copies; do not disable it or strip quarantine. Packaging and verification do not modify system permissions.

Ready? [Install and connect](guide/install.md). Something not responding? [Find a fix](guide/troubleshooting.md). For exact protocol behavior, see [Lighting and integrations](reference-runtime.md).
