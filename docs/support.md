# Compatibility

**0.3.0-alpha** adds Windows 11 x64 to LedAlert's Linux x86_64/KDE Plasma on Wayland support, with one mapped WLED RGB strip. Windows is an alpha feature with the [verification limits](windows-verification.md) below; macOS support is planned for beta, not implemented here.

## Desktop

| Your desktop | Status |
| :--- | :--- |
| <span class="platform-mark"><img src="site/assets/icons/windows.svg" alt="" width="20" height="20"></span> Windows 11 x64 | Alpha |
| <span class="platform-mark"><img src="site/assets/icons/kde.svg" alt="" width="20" height="20"></span> KDE Plasma / Wayland | Supported |
| <span class="platform-mark"><img src="site/assets/icons/kde.svg" alt="" width="20" height="20"></span> KDE Plasma / X11 | Unverified |
| <span class="platform-mark"><img src="site/assets/icons/gnome.svg" alt="" width="20" height="20"></span> GNOME | Unverified |
| <span class="platform-mark"><img src="site/assets/icons/xfce.svg" alt="" width="20" height="20"></span> Xfce | Unverified |
| <span class="platform-mark"><img src="site/assets/icons/cinnamon.svg" alt="" width="20" height="20"></span> Cinnamon | Unverified |
| <span class="platform-mark"><img src="site/assets/icons/linux.svg" alt="" width="20" height="20"></span> Other Linux desktops | Unverified |
| <span class="platform-mark"><img src="site/assets/icons/apple.svg" alt="" width="20" height="20"></span> macOS | Unsupported |

- **Windows 11:** native displays, actual taskbar pins, media playback and lock/disconnect detection. Notifications require registered package identity and permission; interactive acceptance remains limited.
- **KDE Plasma on Wayland:** Linux x86_64 display discovery, pinned-app suggestions, desktop notifications, lock detection and media markers.
- **KDE Plasma on X11:** runs as a normal X11 client; full integration is unverified.
- **GNOME:** standard notification and lock services; manual display placement replaces KDE discovery.
- **Other Linux sessions:** Xfce, Cinnamon, MATE, Budgie and LXQt remain unverified; the standard notification and lock services are used where available.
- **macOS:** no desktop backend is implemented in this alpha.

On Linux, notifications must use the freedesktop notification service, media markers need an MPRIS-capable player, and the session must allow monitoring and provide a working lock service.

On Windows, notifications use the Windows notification center and require registered package identity plus **Settings → Integration status → Allow Windows notifications**. This alpha has **no production signed installer**: use its portable ZIP and [explicit Developer Mode registration](guide/install.md#windows-11) for notification testing. Its MSIX is unsigned and development-only; do not bypass signature checks or add certificate trust. Unregistered portable/Cargo execution has no notification access. Media markers use participating Windows system-media sessions. Windows notifications have normal urgency; a critical-only rule will not match them. In-app-only messages are not observed on either platform.

Windows taskbar discovery reads Explorer's current **Taskband Favorites version 3**, including packaged pins that have no `.lnk` file. This undocumented format is covered by captured Windows 11 build 26200 fixtures; an unknown format reports an error rather than guessing pins from the Start menu. Uninstalled or unresolved pins can be omitted; unavailable icons leave text-only tiles.

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

Ready? [Install and connect](guide/install.md). Something not responding? [Find a fix](guide/troubleshooting.md). For exact protocol behavior, see [Lighting and integrations](reference-runtime.md).
