# Troubleshooting

Start with the status bar and **Settings → Integration status**: desktop observation, lock state and WLED output are separate. Keep lighting disabled for checks that do not need real light. Do not bypass the lock gate, force another controller out of realtime mode or replace a setup just to clear an error.

## The application will not start

Read the error and compare the bundle's `BUILD-INFO.json` with your system:

- The Linux x86_64 bundle needs **glibc 2.36 or newer**.
- Wayland/X11, xkbcommon and EGL/OpenGL runtime libraries may be needed too.
- Windows needs Windows 11 x64 and a working OpenGL driver. **0.3.0-alpha has no production signed installer**; its MSIX is unsigned and development-only. Use the portable ZIP; unregistered execution has no notification access. For notification testing use [explicit Developer Mode registration](install.md#windows-11), never a signature bypass or certificate trust change.
- The GUI needs a graphical session (and session D-Bus on Linux); `check-config` can run headlessly.

If the bundle does not fit, [build from source](install.md#build-from-source). A launch on another desktop does not establish notification or lock support; see [Compatibility](../support.md).

## Connected, but the strip stays dark

**Connect**, automatic reconnection and **Finish** send no pixels. For real output, follow [Enable lighting](operations.md#choose-whether-to-enable-real-lighting), then check **Alert brightness**, **Quiet mode**, lock state and configuration errors. Enabling also makes rule demos and toolbar **Preview** drive the device.

Connection checks HTTP, not UDP delivery. Check WLED's current limits, realtime settings and LED mapping too.

### Enable lighting is unavailable

Connect the intended device and correct invalid drafts. The session must be observed as unlocked: unknown, unavailable or stale lock state inhibits output. Read the integration error; reconnecting WLED cannot fix the lock service.

## Connection fails or times out

Check the device's private LAN/link-local IPv4 address, network reachability and HTTP **80**. The initial `127.0.0.1` is local loopback, not a discovered strip. Only private LAN, link-local and loopback IPv4 are accepted.

**Connect** retries a read-only probe. Physical output separately needs UDP **4048**. Keep private addresses out of public reports.

### Another realtime source is active

Stop that controller through its own controls, then reconnect. LedAlert refuses takeover. WLED HTTP/DDP has no authenticated exclusive ownership; use one controller on a trusted LAN.

### Release is uncertain, or an earlier effect returns

Inspect WLED and other controllers rather than repeating takeover attempts. Failed release blocks acquisition until a read-only check finds the previous device no longer live.

Disable/exit asks WLED to leave realtime mode; WLED may resume its own effect. Crash or network-loss recovery relies on its existing realtime timeout. Neither darkness nor restoration of a previous effect is guaranteed.

## The LED order or count looks wrong

Review the connected count, endpoint indices, allocation and reversal in [Map the strip](strip.md#match-the-led-order).

- A count change disarms output and invalidates the previous explicit allocation.
- Dragging a rail handle switches to explicit point indices without moving geometry.
- **Around room / Along walls** resets allocation and direction; reopening **Starting placements** does not.
- Changing only the room outline preserves placements; it does not fit the strip to new walls.

Cannot add a bend? Check the **64-point** limit, **1 cm** spacing and available LED indices between anchors. A red span must fit entirely inside the room; add a bend around a concave corner.

## A demo works but desktop notifications do not

Check in order:

1. **Integration status:** observation must be available. On Windows launch LedAlert from Start with installed or explicitly registered package identity and use **Allow Windows notifications**; denied/revoked access needs Windows Settings. On Linux LedAlert does not replace the daemon or change a denied session-bus policy.
2. **Desktop notifications:** enable it in Settings.
3. **Rules:** enable the app's rule and its **Notifications** trigger; check minimum **Urgency**. Windows notifications use normal urgency, not critical.
4. **Application identity:** choose the observed source in **Add app**. Matching is exact, ignoring ASCII case; launcher names can differ.
5. **Fallback:** a disabled or filtered explicit rule suppresses **All other apps** for that app.
6. **Source:** the app must emit a supported desktop notification, not an in-app-only message.

Taskbar suggestions and demos are on-screen previews, not desktop integration checks. Persistent tracking requires a matching Linux daemon reply or an observed Windows notification-center lifecycle; lost observation clears tracked lights. Windows history is baselined rather than replayed after startup, permission changes or reconnection.

## No media marker appears

Check **Settings → Media playback**, the rule's **Media** trigger and the player's Windows system-media or Linux MPRIS participation. Notification support does not imply playback support. LedAlert reads playback state and identity—not audio or track metadata.

## A persistent light outlives its popup

Popup timeout is not dismissal. Close/dismiss the notification through the desktop, or use the selected-rule **Dismiss** or toolbar counter. Local dismissal clears lights, not desktop notifications.

There are **32 active notification slots**. If all are persistent, dismiss one to make room for new notifications.

## Lighting or demos stopped after lock, quiet mode or an error

Read the current state before restarting. Events received while inhibited are discarded; stopped demos do not restart. Tracked persistent lights can resume after temporary inhibition, but lost lifecycle observation clears them. Device changes and output faults need attention to the error and current lighting permission.

## Displays or application artwork are missing

Use **Displays → Add manually** when discovery is unavailable. Windows queries active monitor topology; KDE needs `kscreen-doctor` from `libkscreen`. **Refresh** rescans; **Use desktop layout** deliberately rearranges displays.

Use **Add app** for an exact or observed ID. Missing launchers/icons can leave text-only tiles; you do not need to replace the room. An unsupported Windows Taskband format is reported explicitly; LedAlert does not invent taskbar suggestions from the Start menu.

## A setup will not load or save

**Keep the original file before recovery.** From an extracted bundle:

```sh
./bin/ledalert --config /path/to/config.json check-config
```

For a source build, use `./target/release/ledalert`, adjusted for `CARGO_TARGET_DIR`. This checks the file without desktop or network access. Read validation errors rather than changing a version field to bypass them.

**Begin new setup followed by saving replaces an unreadable setup.** Opening it alone leaves it untouched. For save failures, check destination access. Invalid settings are not saved; a directory-sync error means persistence could not be confirmed. Remove crash-left `.tmp` files only after keeping a backup and confirming no instance is saving. See [backup and recovery](../reference-configuration.md#back-up-and-recover).

## Prepare a useful, public-safe report

Include the app/bundle version, OS and desktop/session, WLED firmware, the exact error and steps to reproduce. Say whether the problem was on-screen or on real hardware.

**Do not post** notification text, raw bus captures, personal configuration, private addresses, song metadata or unrelated desktop screenshots. For a security concern, follow [security reporting](../../SECURITY.md).

More detail: [Technical reference](../reference.md) · [Compatibility](../support.md) · [Roadmap](../roadmap.md).
