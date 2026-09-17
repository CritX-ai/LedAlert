# Editing and rule behavior

[Technical reference](reference.md) · [Compatibility](support.md)

For setup steps, follow [Room](guide/room.md) → [Displays](guide/displays.md) → [Strip](guide/strip.md) → [Rules](guide/rules.md).

## Setup and room editing

The setup guide remembers your step. Skip it or replay it through **Settings → Replay setup guide** without clearing your work. **Finish** saves a valid setup and hides the sidebar; it does not enable lighting. Select a top tab to edit again.

### Measurements and displays

- Distances use **meters (`m`)**. Both `3.75` and `3,75` work; mixed separators, nonfinite values and thousands grouping do not. Fields commit on **Enter** or focus loss. Display rounding to two decimal places does not round saved geometry.
- Fresh rooms import KDE displays with their aspect ratios and relative desktop arrangement. Discovery reads logical geometry—not screen content, physical distances or window positions.
- **Refresh** preserves saved names, placements, sizes, angles and rule destinations. Known connectors can refresh aspect ratios; new displays are offered for import. **Use desktop layout** explicitly rearranges detected displays; **Undo** restores placement.
- Display rotation snaps to **45°**, including typed angles. Clear **Snap to 45°** or hold **Shift** while dragging or committing a fine adjustment.

### Strip allocation and rule anchors

- **Connect** reads LED count, WLED version and realtime state without sending pixels or changing settings. Use a private LAN or link-local IPv4 address; the initial `127.0.0.1` is loopback for local fixtures.
- **Around room / Along walls** replace the route and reset allocation and direction. **Starting placements** only reopens the choices. Review allocation after replacing a route, adding/removing points or connecting a different LED count.
- Proportional allocation samples the path at uniform physical spacing. Dragging a numbered LED-rail handle converts it to explicit point indices without moving geometry. Endpoints stay fixed, interior indices stay ordered, and each interval is interpolated separately. Proportional reset restores uniform spacing; reversed layouts show actual device indices.
- Contextual **Lift** moves one point. Sidebar **Whole-strip height** moves every point while preserving relative heights. Inserting a bend in by-point allocation needs a free LED index between neighbouring anchors.
- Rules initially follow their assigned display. Drag the marker in either strip view or enter **Position** to pin it to a device LED; the display icon restores display-following. Clicking a room display changes the assignment.
- Explicit rule positions survive geometry, allocation and direction changes, and scale proportionally when LED count changes. Removing a display redirects its rules to the first remaining display.

### Optional room outlines

Use **Room → Room shape…** for **Rectangle / L-shaped** starting outlines, corner/wall dragging, **Across / Along** measurements and **Snap · 5 cm**. **Add corner** inserts on the selected corner's outgoing edge. **Done** returns to the 3D view without saving or enabling lighting.

- Custom outlines have **3–12 corners**, uniform height and at least **1 cm** between adjacent corners. No self-intersections, overlapping edges, holes, internal partitions, multiple rooms or curved walls.
- Width/depth/height resizing scales placement proportionally and preserves LED allocation indices.
- Shape-only changes, including **Rotate outline**, preserve display anchors, strip points, allocation and rule anchors. Rotation stays within the existing width/depth box. Nothing automatically fits to new walls.
- Display containment checks its floor anchor. Every complete strip span must stay inside the footprint. Correct red conflicts by moving objects, adding a bend, reshaping the outline or using **Undo**.
- Invalid drafts pause output and cannot overwrite a valid save. A valid same-device correction can resume already-enabled output; missed notifications are discarded. Disable lighting for local-only editing.

The drawing is a routing map, not physical calibration or a simulation of wall occlusion and light propagation. See [validation limits](reference-configuration.md#validation-limits) for sizes and counts.

## Rules, effects and palettes

### Match an application

Matching is exact and ASCII case-insensitive. The canonical `desktop-entry` prefix takes precedence over the app name; legitimate suffixes remain significant. Choose an observed source in **Add app** when a launcher name differs. **All other apps** is the `*` fallback; an explicit disabled or trigger-filtered rule suppresses fallback for that app.

### Choose an effect

- **One-off:** finite duration; selecting it enables fading. **Glow** gives a centered signal and **Ripple** adds one outward band. A coalesced one-off notification extends movement without restarting the band. Effects never repeat or strobe.
- **Persistent:** remains until the desktop explicitly dismisses/closes it or you dismiss its light. Popup timeout is not dismissal. Persistent ripple enters once over a steady marker; independent notification IDs remain independently dismissible.
- **Dismiss:** scene control clears the selected rule's persistent lights; the toolbar counter clears all. Neither dismisses desktop notifications.
- **Room** range uses 3D distance from the anchor LED; **LEDs** uses device-index steps. Both extend on either side of the anchor.
- **Gradient:** two to eight stops from center to outer range. Endpoints stay at 0% and 100%; intermediate stops move or can be removed. Glow and ripple share the palette; critical emphasis applies after interpolation.
- **Steady light:** choose **One-off → Glow** and clear **Fade in and out**, or enable **Reduced motion**. Reduced motion replaces fades/ripples with steady indications without changing lifetime. Media markers stay steady.

Overlapping notifications, gradients and media add RGB contributions before a shared per-LED brightness limit that preserves hue ratios. Red and blue can mix toward magenta; removing one leaves the other.

### Notification state

At most **32 notification records** are active. If all are persistent, new notifications are ignored until one is dismissed.

Persistent lights are session-local and require a matching notification-daemon reply. They can resume after temporary lock/quiet inhibition; events received while inhibited are discarded. Lost lifecycle observation, daemon restart, output failure, device changes, guidance or changes to lighting enablement clear active records. Restarting LedAlert does not restore them.

## Taskbar examples and application icons

**Rules → Examples** reads Plasma's pinned launchers and desktop-entry metadata. Communication/productivity apps start selected; other pins can be included. Suggestions do not guarantee that an app emits desktop notifications.

- **Try examples** cycles locally, names the current app in the status bar, and neither adds rules nor sends pixels.
- **Add selected** adds only missing explicit rules in one undoable edit, preserving existing rules and fallback. Choose their initial display, then refine each marker.
- Examples last three seconds without media or critical emphasis. New rules use the icon's dominant color, ignoring transparent padding, or a category color. Saved colors are never replaced.
- Suggestions do not launch apps, execute launcher commands, scan history or read notification content. Unsupported launchers are skipped.

Artwork resolves through local icon themes and inheritance, using local PNG or self-contained SVG. Loading runs off the GUI thread. Missing or rejected artwork leaves a text-only tile; **Add app** uses the same metadata.

**Settings → Hide demo content** hides examples and stops their playback. **Hide tooltips** disables hover explanations, not accessible button names. Both preferences survive restart.

## Identity, motion and keyboard controls

The original pixel-alert mascot and coral/cyan wordmark belong to LedAlert's visual identity. WLED is an integration target and inspiration, not the author or endorser of these assets. The [vector mark](../packaging/io.github.critx.LedAlert.svg) has a bundled PNG companion for the native window. The Silkscreen font has its own [SIL Open Font License](../assets/fonts/OFL.txt); this does not license LedAlert itself.

**Settings → Reduced motion** freezes decorative movement and takes precedence over notification fading and ripples.

| Control | Behavior |
| --- | --- |
| **Undo / Redo**, **Ctrl+Z / Ctrl+Shift+Z** | Up to 64 steps for room, strip, rules, settings and address draft. Drags/continuous edits group together. No-op edits retain redo; editing after undo starts a new branch. |
| Text-field shortcuts | Edit text locally. Leave the field or use toolbar buttons for whole-editor undo. |
| Middle-drag / reset view | Change only the camera, not configuration or undo history. |
| **Shift** | Temporarily bypass display rotation snapping. |
| **Ctrl+S** | Save a valid setup. |
| **Delete** | Remove the selected display or strip point when allowed. |

Undo never grants hardware permission or restores quiet/lock state or notification history. Same-device undo/redo preserves current enablement and connection; changing the target disarms output. Undo/redo always ends guidance.
