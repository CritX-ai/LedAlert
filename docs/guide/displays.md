# Place displays

Displays give rules a destination in the room. They are landmarks—not screen capture or window tracking. Start after [drawing the room](room.md).

## Import or add a display

A fresh room imports macOS, Windows or KDE displays automatically when discovery is available. Imported monitors stand upright with their aspect ratios and relative desktop arrangement. macOS uses native display UUIDs and global point geometry; imports are a starting layout, not physical measurements.

In **Displays**:

- Select a display in the list or in the room to edit it.
- Use **Refresh** to read the desktop's current setup again. It does not reconfigure the desktop or rearrange saved placements.
- Use **Add manually** when discovery is unavailable or you want to place a display yourself.
- Choose **Use desktop layout** only when you want to rearrange detected displays to match the desktop. This replaces their placement; **Undo** restores it.

**Refresh** preserves saved names, placements, sizes, angles and rule destinations. Known connectors can refresh aspect ratios; new displays are offered for import, up to **16 displays**. Discovery reads KDE logical geometry or Windows desktop pixels, not physical distances: adjust the result to suit your room. Windows monitor device paths retain placements across enumeration changes; mixed-DPI and portrait displays are not scaled or rotated twice.

## Place and orient each monitor

1. Drag the display along the floor to set its position.
2. Use the vertical handle or the sidebar **Height** control to lift it.
3. Drag its corner handle, or use **Width**, to resize it.
4. Drag the floor ring, or enter **Rotation**, to face it in the right direction.
5. Expand **Position & dimensions** for exact coordinates. Measurements use meters; `1.25` and `1,25` are both accepted.

Numbers appear on both faces of each display. Middle-drag to orbit without moving it.

Rotation snaps to **45°**, including typed angles. For a fine adjustment, clear **Snap to 45°**, or hold **Shift** while dragging or committing the angle.

## Decide what a rule follows

- **Follow a display:** click a room display in **Rules** to assign it. The marker follows that display's mapping to the strip.
- **Pin to an LED:** drag the rule marker in either strip view, or enter a contextual **Position**. It stays at that device LED when the display moves.
- Use the display icon to return to following the assigned display.

Removing a display redirects its rules to the first remaining display. Review those destinations or use **Undo**. Removal is available only when the resulting setup allows it.

## If discovery is unavailable

Check the **Detection unavailable** error, then use **Add manually** if needed. Discovery needs `kscreen-doctor` from `libkscreen`; see [native dependencies](install.md#native-dependencies).

Use **Refresh**, not **Use desktop layout**, to rescan without rearranging. If an anchor falls outside a custom outline, move it inside or adjust the outline.

Save a valid arrangement with **Ctrl+S**. See [keyboard controls](../reference-editing.md#identity-motion-and-keyboard-controls) for more shortcuts.

Next: [Map the strip](strip.md), then [assign application rules](rules.md).
