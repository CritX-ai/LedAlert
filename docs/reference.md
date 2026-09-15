# LedAlert 0.1.0 reference

[Quickstart](../README.md) · [Release notes and binary requirements](../RELEASE.md) · [Changelog](../CHANGELOG.md)

LedAlert is a native Linux room workbench for WLED RGB strips, targeting KDE Plasma on Wayland. This reference describes the initial private 0.1.0 release, not support for other controllers or certified compatibility with every Linux desktop.

## Setup and room editing

The first launch opens **Room → Displays → Strip → Rules**. The guide remembers the current step, can be skipped, and can be replayed through **Settings → Replay setup guide** without clearing the setup. The lower-right **Finish** button saves the configuration and hides the sidebar; it does not enable lighting. Select a top tab to edit again.

- **Room:** drag floor edges to shape the footprint and upper edges to adjust **Height**. Middle-button drag orbits; reset view restores the camera without changing the setup. **Square / Wide / Long** provide starting shapes. Measurements are optional.
- **Displays:** detected monitors stand upright with their proportions and relative desktop arrangement. Drag along the floor, lift vertically, resize with the corner handle and rotate with the floor ring. Numbers are mirrored on the rear face. Rotation snaps to **45°**, including typed angles; hold **Shift** when committing a fine adjustment or clear **Snap to 45°**. **Use desktop layout** explicitly rearranges detected displays; undo restores the previous placement.
- **Strip:** replace the initial loopback address, `127.0.0.1`, with your strip's private LAN or link-local IPv4 address, then **Connect**. Loopback is also accepted for local fixtures. Connection reads the LED count, version and realtime state; it sends no pixels and changes no settings. Use a starting placement or trace the path directly. **Along walls** selects adjacent walls in order; **Around room** creates a perimeter. Placement controls disappear after applying a route or editing the path. **Starting placements** reopens them without changing the path until you choose a replacement.
- **Rules:** examples appear first and can be collapsed. Select an application tile to edit it; its separate power icon enables or disables the rule. Drag the round marker along the strip in either the room or bottom LED view. Choose **One-off / Persistent**, a preset, **Glow / Ripple**, **Room / LEDs** range and **Solid / Gradient** color. **Triggers & timing** exposes notification/media filters, urgency, critical emphasis, duration, intensity and fading.

Distances use **meters (`m`)**, displayed to at most two decimal places. Both `3.75` and `3,75` mean 3.75 m. Fields commit on Enter or focus loss; viewing a rounded value does not round stored geometry. Mixed separators, nonfinite values and thousands grouping are not supported.

Select a strip point or span for contextual tools above the room. **+** or a span double-click inserts a bend; the trash icon or **Delete** removes a selected point when the remaining path is valid. Position, lift, mapped LED index, direction reversal and proportional reset are in this scene toolbar. The graphical lift handle and contextual **Lift** field move one point; sidebar **Whole-strip height** moves every point while preserving relative heights.

Drag a numbered handle on the bottom LED rail to adjust allocation directly. The first actual drag converts proportional positions into explicit point indices without moving the path. Endpoints stay fixed and interior indices stay ordered. Proportional reset restores uniform physical spacing; reversed layouts display actual device indices. Review allocation after changing path topology or connecting a different LED count.

Fresh rooms import KDE displays automatically. Saved names, placements, sizes, angles and rule destinations stay unchanged unless explicitly rearranged. Known connectors refresh their aspect ratios; new displays are offered for import. **Refresh** rescans. Discovery reads geometry, not screen content, physical distances or application-window positions.

Room coordinates describe your drawing, not automatic calibration. Each rule initially follows its assigned display. Dragging its marker sets an explicit device LED position shared by both views; contextual **Position** selects an exact LED. The display icon restores following the assigned display, and clicking a room display changes that assignment. Explicit positions survive geometry, allocation and direction changes and scale proportionally when LED count changes. **Room distance** uses three-dimensional distance from the anchor LED; **LED steps** uses device-index distance, independent of drawing scale. Range extends outward on both sides. Proportional allocation samples the polyline uniformly; by-point allocation interpolates within each assigned LED interval.

## Lighting permission and lifecycle

**Startup never sends lighting output.** Lighting is disabled on every launch, including restored setups. Read-only connection or automatic reconnection does not grant permission. **Enable lighting** is a separate, session-only action; start with low brightness.

Valid same-device room, strip and rule edits, selection and undo/redo preserve enablement. Invalid drafts and lock/quiet inhibition pause output while preserving that choice; missed notifications are discarded. Changing the address or LED count disarms output with an explanation. An output failure remains visible.

Finishing opens the room with all application markers at their actual LED anchors in both views. Overlapping markers have a count and inspectable application list. Slow scene rotation runs only while connected and lighting is enabled; reduced motion or manual orbit pauses it.

Later launches restore the finished view with lighting off and probe the saved target read-only when eligible. Older saved setups without connection history also receive a read-only check. Failure opens **Strip** with an error and retry action. If an older setup still opens the editor, use **Strip → Connect**, then **Finish** once to save its connection and completed-view state. Finishing or reconnecting never restores hardware permission, demos or notification history.

The application remains a foreground desktop program. Minimizing keeps it running; closing stops it, with save/discard/cancel offered for unsaved changes. There is no hidden daemon, tray-only mode or automatic startup installation.

## Rules, effects and palettes

Application matching is exact and ASCII case-insensitive. **All other apps** is the `*` fallback. A disabled or trigger-filtered explicit rule suppresses fallback for that application. Removing a display redirects its rules to the first remaining display.

**One-off** gives a finite indication and ends. Selecting it enables fading. **Glow** fades in and out; **Ripple** adds one outward band from the chosen strip position. Duration controls the indication; a coalesced one-off notification extends movement without restarting the band. No effect repeats or strobes.

**Persistent** stays until the desktop explicitly dismisses or closes the notification, or you dismiss its light locally. Popup timeout alone does not clear it. Scene **Dismiss** clears persistent lights for the selected rule; the toolbar dismissal counter clears all, without dismissing desktop notifications. Persistent ripple plays its entrance once over a steady marker. Independent desktop notification IDs remain independently dismissible.

Overlapping notifications, gradients and media markers add RGB contributions before a shared per-LED brightness limit. The limit preserves hue ratios instead of clipping individual channels. Red and blue mix toward magenta; removing one leaves the other rather than blanking their shared position.

**Gradient** supports two to eight stops from the center to the outer range. Endpoints remain at 0% and 100%; intermediate stops can move or be removed. Glow and ripple share the palette. Optional critical emphasis is applied after interpolation.

For a steady finite notification, choose **One-off → Glow** and disable **Fade in and out**, or enable **Reduced motion**. Reduced motion suppresses fades and substitutes steady glow for ripples without changing lifetime. Media markers remain steady. There are at most 32 active notification records; if all are persistent, further notifications are ignored until one is dismissed rather than evicting an existing persistent light.

Persistent state is session-local: it is neither saved nor reconstructed from history. Only notifications observed with a matching daemon reply can be tracked. Existing persistent lights can resume after lock/quiet inhibition; new notifications received while inhibited are discarded. Lost lifecycle observation or a notification-daemon restart clears tracked lights. Output failures, device changes, guidance and changes to lighting enablement also clear active records.

## Demos and preview

**Rule demos and toolbar Preview reach the real strip when lighting is enabled.** With lighting disabled they remain on-screen. **Try examples** is always local-only. On-screen color is lifted for low-brightness visibility; it is not a photometric prediction.

- **Demo rule** simulates the selected rule's position, palette, intensity, range, duration and lifetime. Persistent demos toggle off with the same button; selecting another rule stops them. One-off demos expire normally.
- **Demo all** starts eligible notification rules together, including an explicit fallback. Persistent demos stop when toggled off, when a single rule is selected or when the Rules sidebar is left or hidden. Disabled and notification-filtered rules do not play.
- Toolbar **Preview**, available after finishing a valid saved setup, cycles through eligible rules in shuffled order, reshuffling each cycle with a random **1–5 second** delay between notifications. Lifetimes are preserved. Click again, press **Esc** or select a rule to stop.
- Synthetic records are separate from desktop notifications; stopping demos never dismisses a real notification. Demos, desktop notifications and media share the renderer and brightness limit, with at most one synthetic record per configured rule.
- Configuration edits, lock/quiet inhibition, output faults and guidance stop demos. Recovery does not restart them. Changing lighting enablement clears demos; start a fresh demo after enabling the strip.

## Find the real LEDs

In **Strip**, connect and choose **Guide with real lights**. The consent dialog identifies the address and LED count before **Take control**.

- Permission temporarily covers the **entire strip**, replacing its lighting with steady white on the selected area and darkness elsewhere.
- A point highlights its mapped LED and up to two neighbours on either side. A span highlights the interval between its endpoints. By-point allocation edits immediately move the highlight.
- Each session lasts at most **two minutes**, with every RGB channel capped at **25/255**. There is no flashing, automatic renewal, saved permission or persistent WLED settings change.
- **Stop**, **Stop guidance** or **Esc** ends it. Leaving Strip, undo/redo, target changes, invalid selection, quiet mode, unavailable/locked session, producer stalls and transport failures revoke it. Recovery never silently rearms it; **Extend** requests consent again.
- Ordinary alerts are suspended and are not automatically re-enabled afterward. Geometry and allocation may be edited during guidance; history navigation ends it.

Release asks WLED to leave realtime mode. WLED decides what resumes: LedAlert cannot guarantee restoration of the previous effect or exclusive ownership against another controller. Failed release stays visible and blocks acquisition until a read-only check sees the old device no longer live. Crash recovery depends on the device's existing realtime timeout.

## Taskbar examples and application icons

**Rules → Examples** reads Plasma's pinned launchers and desktop-entry metadata. Communication and productivity apps are initially selected; other pins can be included. These are suggested routes, not claims that an app emits desktop notifications.

- **Try examples** cycles selected applications locally, identifies the current app in the status bar, and neither adds rules nor sends pixels.
- **Add selected** adds only missing explicit rules as one undoable edit; existing rules and fallback stay intact.
- Select a display for new examples' initial destination, then refine each marker. New rules use the resolved icon's dominant color, or restrained category colors when no icon resolves. Saved colors are not replaced. Examples are finite three-second indications without media or critical emphasis.

Suggestions launch no applications, execute no launcher command lines, scan no history and read no notification content. Unsupported launchers are skipped. If notifications use another identity, choose the observed source through **Add app**.

Tiles resolve the shortcut's **Icon** through the local icon theme and inheritance. **Add app** uses the same metadata. Supported artwork is local PNG or self-contained SVG; unavailable or rejected artwork leaves a text-only tile, not an invented icon. Loading runs off the GUI thread. New-rule color extraction ignores transparent padding and never overwrites existing chosen colors.

**Settings → Hide demo content** hides examples and stops synthetic example playback. **Hide tooltips** separately disables hover explanations; icon buttons retain accessible names. Both preferences survive restart.

## Identity, motion and keyboard controls

The original pixel-alert mascot and coral/cyan wordmark belong to LedAlert's visual identity. WLED is an integration target and inspiration, not the author or endorser of these assets. The [vector mark](../packaging/io.github.critx.LedAlert.svg) has a bundled PNG companion for the native window. The Silkscreen font has its own [SIL Open Font License](../assets/fonts/OFL.txt); this does not license LedAlert itself.

**Settings → Reduced motion** freezes decorative movement and takes precedence over notification fading and ripples.

- **Undo / Redo**, **Ctrl+Z / Ctrl+Shift+Z** cover room, strip, rules, settings and the address draft, retaining up to 64 steps. Drags and continuous edits are grouped. No-op edits retain redo; a new edit after undo creates a branch.
- Text fields keep native editing shortcuts. Leave the field or use the toolbar for whole-editor undo.
- Undo never grants permission or restores quiet/lock state or notification history. Same-device history preserves current enablement and connection; a target change disarms output. Guidance always ends on history navigation.
- Middle-button orbit changes only the camera, not configuration or history. **Shift** temporarily bypasses display rotation snapping.
- **Ctrl+S** saves. **Delete** removes the selected display or strip point when allowed.

## Desktop integration and privacy

- **Notifications:** passive `org.freedesktop.DBus.Monitoring.BecomeMonitor` observes freedesktop `Notify` calls, correlated replies from the current daemon owner and `NotificationClosed` signals. KDE retains notification ownership and delivery. Caller identity and message serial bind replies to calls; replacement IDs come from the actual reply.
- **Routing:** the canonical `desktop-entry` prefix takes precedence over application name. Legitimate suffixes are preserved; a suffix is not automatically stripped as though the ID were a filename. Observed source IDs are offered by **Add app** and saved only when a rule is created. See the [notification hint specification](https://specifications.freedesktop.org/notification/latest/hints.html) and [MPRIS DesktopEntry specification](https://specifications.freedesktop.org/mpris/latest/Media_Player.html#Property:DesktopEntry).
- Only bounded application metadata, urgency, notification ID and in-memory receipt time enter the renderer. Summaries, bodies, actions, notification images and song metadata are not retained or logged. Wire messages necessarily exist transiently in the D-Bus library before decoding. Shortcut icons are separate local files.
- Notifications over 64 KiB or with invalid application metadata are rejected. Consumer and pending-call queues are each bounded at 64, with a five-second reply deadline. Lost, malformed or overflowing lifecycle observations invalidate tracked IDs and clear lights. Raised events older than one second at consumption are discarded; stale events are not replayed.
- **Media:** up to 32 MPRIS players; reads only `PlaybackStatus`, `DesktopEntry` and, when necessary, `Identity`. No audio capture or track metadata reads.
- **Lock:** KDE's `org.freedesktop.ScreenSaver.GetActive` at `/ScreenSaver`, polled every 250 ms after each call. Calls have a 500 ms deadline. Unknown, unavailable or stale lock state inhibits lighting. Notifications received while inhibited are discarded rather than replayed on unlock.
- Services reconnect with bounded backoff. **Settings → Integration status** shows detailed errors; lock/output problems stay in the status bar. Denied passive-monitoring policy is not modified. Display discovery runs off the GUI thread with a three-second deadline and bounded response.

Application identities are routing hints, not authenticated security identities. LedAlert provides ambient awareness, not a safety alarm or confidentiality boundary. Quieting releases its control; it cannot guarantee darkness if WLED or another controller supplies an effect.

## WLED transport and failure behavior

[WLED DDP](https://kno.wled.ge/interfaces/ddp/) carries RGB frames over UDP **4048**. The [WLED JSON API](https://kno.wled.ge/interfaces/json-api/) uses HTTP **80** for discovery and transient realtime release. Only private LAN, link-local or loopback IPv4 targets are accepted.

- Full frames, at most 30 per second. Each datagram carries at most 480 RGB LEDs, byte offsets, a 1–15 frame sequence and PUSH on the final datagram. A 600-LED frame uses two datagrams below ordinary Ethernet MTU.
- A single latest-frame slot replaces obsolete frames instead of queuing playback. A 500 ms producer stall stops transmission. An outage does not replay queued notifications.
- Before output, the worker checks LED count and refuses an already-active realtime controller. Run one controller: DDP offers no authenticated ownership or atomic acquisition.
- HTTP has short deadlines, no proxy or redirects, and a 64 KiB response cap. Health checks run off the GUI thread. UDP submission is not acknowledgment of delivery.
- Disable, quieting, inactivity, target change, shutdown or failure stops packets. Only a device actually sent pixels receives `{"live":false}`. There are no persistent configuration, preset, segment, brightness or power-setting writes.
- Uncertain release stays visible. LedAlert does not repeatedly release a device that might now have another owner. Recovery requires a read-only observation that the prior device is no longer live.
- Crashes, network loss and forced termination depend on the **device-configured WLED realtime timeout**. LedAlert does not change it or promise a fixed recovery interval.

WLED current limits, realtime settings, LED mapping and brightness affect output. The GUI caps RGB data, not measured lumens or electrical power. These HTTP/UDP interfaces are unauthenticated: use a trusted local network. Physical appearance, brightness, power safety and restoration of prior device effects are not guaranteed by software checks.

## Configuration and diagnostics

Configuration lives at `$XDG_CONFIG_HOME/ledalert/config.json`, falling back to `~/.config/ledalert/config.json`. It contains room, routing and device settings, never runtime permission or notification history. Saves use a same-directory temporary file with mode 0600, file synchronization, atomic replacement and directory synchronization. Invalid settings do not overwrite the saved file. A directory-sync error after replacement means persistence could not be confirmed.

A `.ui.json` sibling stores guide progress, completed setup/strip placement, the last successfully connected saved target, and tooltip/demo visibility. It stores no lighting authority, guidance permission, demo state or undo history.

From an extracted binary bundle:

```sh
./bin/ledalert --config /path/to/config.json check-config
./bin/ledalert --config /path/to/config.json probe
./bin/ledalert --help
```

From a source checkout after building, replace `./bin/ledalert` with `./target/release/ledalert` (or the corresponding path under `CARGO_TARGET_DIR`). If explicitly installed on PATH, use `ledalert`.

`check-config` has no desktop or network access. `probe` is read-only but contacts the configured device. An unreadable saved file remains untouched until you explicitly begin and save a new setup. A save interrupted by a crash may leave a named `.tmp` sibling; remove it only after confirming no LedAlert instance is saving.

Version-1 configurations remain readable: missing display metadata, LED allocation and advanced rule options receive compatible defaults. Existing saved targets are not replaced by the initial loopback default. If an older rule depended on stripping a legitimate `.desktop` suffix, update it to the canonical observed prefix; no routing aliases or silent rewrites are introduced.

Limits: **8,192 RGB LEDs**, **16 displays**, **64 strip vertices**, **128 rules**, a **256 KiB** configuration file, adjacent vertices at least **1 cm** apart, and room dimensions of **0.5–50 m**.

## Build dependencies and optional desktop launcher

Source builds require Rust **1.95 or newer**, a Linux graphical session, D-Bus and working OpenGL/EGL. On Arch/CachyOS, native build/runtime libraries come from `base-devel`, `wayland`, `libxkbcommon`, `libx11`, `libxcursor`, `libxi`, `libxrandr` and `mesa`. KDE discovery uses `kscreen-doctor` from `libkscreen`; without it, configure displays manually. Development regression fixtures additionally require `dbus-daemon`. Other distributions use different package names. A matching CPU architecture alone does not establish binary compatibility; see [release requirements](../RELEASE.md#binary-compatibility).

No browser, HTTP server, JavaScript toolchain, root access, notification-daemon replacement or autostart installation is needed. The [README](../README.md) gives separate source and binary launch instructions.

Optional user-local installation, **from an extracted binary bundle**:

```sh
install -Dm755 bin/ledalert ~/.local/bin/ledalert
install -Dm644 packaging/io.github.critx.LedAlert.desktop ~/.local/share/applications/io.github.critx.LedAlert.desktop
install -Dm644 packaging/io.github.critx.LedAlert.svg ~/.local/share/icons/hicolor/scalable/apps/io.github.critx.LedAlert.svg
```

For a source build, replace `bin/ledalert` in the first command with `target/release/ledalert`, adjusted for `CARGO_TARGET_DIR` if set. Ensure `~/.local/bin` is on the desktop session's PATH. Installation is optional and never performed automatically.

## Preparing local release archives

From a source checkout, use **Python 3.11+**, Cargo/Rust, GNU `readelf`, and the matching Rust documentation component (for standard-library copyright notices):

```sh
python3 packaging/release.py
```

Run the checksum check from `dist/` to verify the archives themselves:

```sh
cd dist
sha256sum -c SHA256SUMS
```

The helper performs a locked native Linux x86_64 release build with baseline `x86-64` CPU instructions, packages an explicit source allowlist, includes version-bound dependency/font/runtime notices, and records the binary ABI and source digests in `BUILD-INFO.json`. Its target-specific executable is built under `target/x86_64-unknown-linux-gnu/release/`. It never commits, tags, uploads, installs or contacts WLED.

Outputs default to ignored `dist/`. Existing release filenames are not overwritten; use `--output /path/to/empty-directory` for a separate preparation. Archive ordering, owner fields and timestamps are normalized (`SOURCE_DATE_EPOCH`, default 0); this makes archive assembly deterministic for identical inputs, not a claim of reproducible compiler output across machines or toolchains.

`packaging/licenses/index.json` pins upstream texts missing from registry packages, using repository commits and SHA-256 hashes. Dependency updates may require new notice inputs. Keep copyright and font-license material with both archives. The inventory conservatively includes resolved target build/development dependencies; not every listed crate is necessarily linked into the executable.

## Release scope and licensing

See [RELEASE.md](../RELEASE.md) for artifacts, compatibility and bounded verification evidence. Initial 0.1.0 distribution and repository access are private. A public release around 0.2.0 is a goal after further polish, broader device/use-case work and workflow-oriented documentation; it is not a dated promise or current support claim.

**No application license has been selected or granted.** Source availability or possession of a bundle does not grant an open-source license. Cargo package publication is disabled. Bundled third-party notices, dependency license texts and the font license apply to their respective components only.
