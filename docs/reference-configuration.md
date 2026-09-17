# Saved setups and diagnostics

[Technical reference](reference.md) · [Compatibility](support.md)

Keep your room map safe. For a device or desktop problem, start with [Troubleshooting](guide/troubleshooting.md).

## Configuration and diagnostics

Your setup lives at `$XDG_CONFIG_HOME/ledalert/config.json`, or `~/.config/ledalert/config.json` when `XDG_CONFIG_HOME` is unset. Use `--config /path/to/config.json` to choose another location.

- **`config.json`** stores room, routing and device settings—not runtime lighting permission or notification history.
- **`config.json.ui.json`** stores guide progress, completed setup/strip placement, the last successfully connected saved target, and tooltip/demo visibility. It stores no lighting permission, guidance session, demo state or undo history. For a custom path, append `.ui.json` to the filename.
- **Saving** uses a same-directory temporary file with mode 0600, file synchronization, atomic replacement and directory synchronization. Invalid settings cannot overwrite the saved file. A directory-sync error after replacement means persistence could not be confirmed.

From an extracted binary bundle:

```sh
./bin/ledalert --config /path/to/config.json check-config
./bin/ledalert --config /path/to/config.json probe
./bin/ledalert --help
```

`check-config` validates the file with **no desktop or network access**. `probe` is read-only but contacts the configured device; it sends no lighting output. For a source build, use `./target/release/ledalert`, adjusted for `CARGO_TARGET_DIR` if set. If installed on PATH, use `ledalert`.

### Validation limits

| Setting | Limit |
| --- | --- |
| RGB LEDs | 8,192 |
| Displays | 16 |
| Strip vertices | 2–64 |
| Rules | 128 |
| Configuration file | 256 KiB |
| Room width, depth and height | 0.5–50 m |
| Custom outline | 3–12 normalized counterclockwise corners; absent or empty means rectangle |
| Adjacent outline or strip vertices | At least 1 cm apart, allowing coordinate-rounding error |

Outlines cannot cross themselves, overlap or contain holes. Whole strip segments must fit inside the footprint, not just their endpoints. See [room outlines](reference-editing.md#optional-room-outlines) for editing behavior.

## Back up and recover

1. **Close LedAlert** before copying or restoring files. Minimizing leaves it running.
2. Copy the configuration and optional `.ui.json` sibling to a separate backup location. Keep a known-good copy rather than overwriting it during recovery.
3. Run `check-config` against the file you want to use. If validation fails, preserve it and read the error before changing anything. Do not alter a version field to bypass validation.
4. To restore a backup, copy it to the intended configuration path while LedAlert is closed, then launch with that path. Review the room, allocation and rules with lighting disabled; hardware permission is always a separate action.

**An unreadable setup remains untouched until you choose Begin new setup and save.** That action replaces the file; it is not a diagnostic reset.

For a save failure, check destination access and the reported cause. A crash may leave a named `.tmp` sibling. Remove it only after keeping a backup and confirming no LedAlert instance is saving. Do not run two instances against the same setup or controller.
