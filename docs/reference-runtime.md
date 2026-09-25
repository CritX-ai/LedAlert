# Lighting and integrations

[Technical reference](reference.md) · [Compatibility](support.md)

For daily use, see [Everyday controls](guide/operations.md). Connecting, previewing and lighting the room are separate choices.

## Lighting permission and lifecycle

**Lighting starts disabled on every launch.** **Connect**, automatic reconnection and **Finish** send no pixels. [Enable lighting](guide/operations.md#choose-whether-to-enable-real-lighting) grants session-only permission; start with low brightness.

| Action or state | Output behavior |
| --- | --- |
| Valid same-device edits, selection or undo/redo | Preserve current enablement. |
| Invalid draft, quiet mode or locked/unavailable lock state | Pause output without changing that choice. Missed notifications are discarded. |
| Address or LED-count change | Disarms output; review the target and allocation. |
| Output failure | Shows an error; inspect the current state before restarting. |
| Minimize | Keeps LedAlert and notification/lock processing running, independently of window redraws. |
| Close | Stops the app, with save/discard/cancel for unsaved edits. No hidden daemon, tray-only mode or installed autostart. |

**Finish** saves and opens the room with application markers at their actual LED anchors. Overlapping markers have a count and inspectable app list. Slow scene rotation runs only while connected with lighting enabled; reduced motion or manual orbit pauses it.

Later launches restore the finished view and probe the saved target read-only when eligible. A connection failure opens **Strip** with an error and retry action. Neither reconnecting nor finishing restores lighting permission, demos or notification state.

## Demos and preview

**Demo rule**, **Demo all** and toolbar **Preview** play on screen; they also drive the real strip whenever lighting is enabled. **Try examples** is always local-only. Preview colors are lifted for visibility, not a prediction of room brightness.

- **Demo rule:** plays the selected rule. One-off demos expire; persistent demos toggle off with the same button. Selecting another rule stops them.
- **Demo all:** starts eligible notification rules together, including an explicit fallback. Disabled/notification-filtered rules do not play. Toggle off, select a single rule or leave/hide the Rules sidebar to stop persistent demos.
- **Preview:** after finishing a valid saved setup, cycles eligible rules in shuffled order with **1–5 second** gaps and their configured lifetimes. Click again, press **Esc** or select a rule to stop.

Demos are separate from desktop notifications: stopping one never dismisses a real notification. They share the renderer and brightness limit, with at most one demo record per rule. Configuration edits, lock/quiet inhibition, output faults, guidance or changing lighting enablement stop demos. Recovery never restarts them.

## Find the real LEDs

In **Strip**, connect, select a point/span and choose **Guide with real lights**. Check the address and count before choosing **Take control**: this grants temporary control of the **entire strip**, not an on-screen preview.

- Selected LEDs glow steady white; the rest go dark. A point highlights its mapped LED and up to two neighbours on each side; a span highlights its endpoint interval. Allocation edits move the highlight immediately.
- Sessions last at most **two minutes**, capped at **25/255 per RGB channel**. No flashing, automatic renewal, saved permission or persistent WLED settings changes. **Extend** asks for consent again.
- **Stop**, **Stop guidance** or **Esc** ends the session. Leaving Strip, undo/redo, target changes, invalid selection, quiet/lock inhibition, producer stalls or transport failures also end it.
- Ordinary alerts are suspended and are **not automatically re-enabled** afterward. Geometry/allocation remain editable; recovery never restarts guidance.

Release asks WLED to leave realtime mode; its own effect may resume. Failed release stays visible and blocks acquisition until a read-only check finds the prior device no longer live. Crashes rely on WLED's configured realtime timeout. See [Map the strip](guide/strip.md#optional-identify-physical-leds) for the steps.

## Desktop integration and privacy

### Linux metadata

- **Notifications:** passive `org.freedesktop.DBus.Monitoring.BecomeMonitor` observes `Notify`, replies from the current daemon and `NotificationClosed`. KDE keeps notification ownership and delivery. Caller identity and message serial correlate replies; notification replacement IDs come from the actual reply.
- **Routing:** canonical `desktop-entry` prefix before app name, preserving legitimate suffixes. Observed IDs appear in **Add app** and are saved only when you create a rule. See the [notification hints](https://specifications.freedesktop.org/notification/latest/hints.html) and [MPRIS DesktopEntry](https://specifications.freedesktop.org/mpris/latest/Media_Player.html#Property:DesktopEntry) specifications.
- **Media:** up to **32 MPRIS players**, reading only `PlaybackStatus`, `DesktopEntry` and, when needed, `Identity`. No audio capture or track metadata.
- **Lock:** KDE's `org.freedesktop.ScreenSaver.GetActive` at `/ScreenSaver`, polled 250 ms after each call with a 500 ms deadline. Unknown, unavailable or stale state inhibits lighting; missed notifications are not replayed on unlock.

Only bounded app metadata, urgency, notification ID and in-memory receipt time enter the renderer. Notification summaries, bodies, actions, images and song metadata are not retained or logged. Wire messages necessarily exist transiently in the D-Bus library before decoding; shortcut icons are separate local files.

### Windows metadata

- **Notifications:** the [Windows notification listener](https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/notification-listener) requires package identity, the MSIX `userNotificationListener` capability and explicit foreground user consent. LedAlert reads only notification ID, creation time and application AUMID—not `Notification`, visual/text fields, images or actions. Creation time is mapped to monotonic receipt time so delayed snapshots cannot replay events from before an unlock or Quiet-mode boundary.
- **Lifecycle:** current notifications are reconciled every **250 ms**, with optional event wakeups. Startup/reconnect/permission restoration establishes a silent baseline. Replacements use ID plus creation time; removal from notification center dismisses a tracked light. Urgency is **normal (1)**; Windows supplies no freedesktop urgency or close reason. A toast that disappears entirely between snapshots cannot be reconstructed.
- **Media:** up to **32** Windows system-media sessions; only playback status and source AUMID are read. Failures and stale observations clear playback markers.
- **Lock:** the current WTS session's connection/lock state is polled every **250 ms**. Disconnected sessions inhibit lighting; unknown or older-than-one-second state fails closed.
- **Taskbar:** bounded, read-only Explorer Taskband v3 records identify actual pins. Shell metadata resolves their current names, exact IDs and icons without invoking shortcuts or applications. Start inventory is never substituted for pins.
- **Displays:** active Windows display paths provide stable monitor identities and already-oriented pixel geometry. Mixed-DPI displays remain in a single coordinate space; imported geometry is a visual starting point, not a physical measurement.

Windows notification snapshots are bounded to **4,096** metadata records and the consumer queue to **64** events. Queue loss, ambiguous IDs, unavailable snapshots or permission loss invalidate the notification generation and clear uncertain lights. Native async queries have a **two-second** deadline. Media observations expire after **three seconds**.

### Limits and failures

On Linux, notifications over **64 KiB** or with invalid app metadata are rejected. Consumer and pending-call queues hold at most **64** each, with a **five-second** reply deadline. On both platforms, lost, malformed or overflowing lifecycle observations clear tracked lights. Events older than **one second** at consumption are discarded.

Services reconnect with bounded delays. **Settings → Integration status** shows errors; lock/output problems remain in the status bar. LedAlert does not bypass denied access. Discovery runs off the GUI thread; KDE's external display query has a three-second deadline and bounded response, and Windows native discovery has bounded topology buffers.

Application IDs are routing hints, not authenticated identities. LedAlert is ambient awareness—not a safety alarm or confidentiality boundary. Quieting releases control; it cannot guarantee darkness when WLED or another controller supplies an effect.

## WLED transport and failure behavior

[DDP](https://kno.wled.ge/interfaces/ddp/) carries RGB frames over UDP **4048**; the [JSON API](https://kno.wled.ge/interfaces/json-api/) uses HTTP **80** for discovery and realtime release. Only private LAN, link-local or loopback **IPv4** targets are accepted. Use a trusted local network: these interfaces are unauthenticated.

- **Frames:** up to **30/second**, at most **480 RGB LEDs/datagram**, byte offsets, a 1–15 sequence and PUSH on the final datagram. A 600-LED frame uses two datagrams below ordinary Ethernet MTU.
- **Freshness:** one latest-frame slot replaces obsolete frames. A **500 ms** producer stall stops transmission; outages do not replay queued notifications.
- **Ownership:** LED count is checked before output; an active realtime controller is refused. Run one controller—DDP has no authenticated ownership or atomic acquisition.
- **HTTP:** short deadlines, no proxy/redirects, **64 KiB** response cap; health checks run off the GUI thread. UDP submission does not acknowledge delivery.
- **Stopping:** disable, quieting, inactivity, target change, shutdown or failure stops packets. Only a device actually sent pixels receives `{"live":false}`. LedAlert never writes persistent configuration, presets, segments, brightness or power settings.
- **Uncertain release:** remains visible; LedAlert does not repeatedly release a device another app might now own. Recovery needs a read-only observation that the prior device is no longer live.
- **Crash/network loss:** relies on the device-configured realtime timeout. LedAlert does not change it or promise a fixed recovery interval.

WLED current limits, realtime settings, mapping and brightness affect output. LedAlert caps RGB values—not lumens or electrical power—and cannot guarantee physical brightness, power safety or restoration of an earlier device effect.
