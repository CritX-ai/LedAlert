# Windows verification

This is a repeatable verification procedure, **not a record that every case has passed**. Record the exact commit, Windows edition/build, architecture, session type, commands and observed results for each run. `0.3.0-alpha` supports Windows 11 x64; Windows Server CI is not Windows 11 desktop acceptance. macOS support and broader verification are planned for beta, not provided by these tools.

The committed [native probe](https://github.com/CritX-ai/LedAlert/blob/main/examples/windows_probe.rs) uses LedAlert's production taskbar, display and desktop-monitor adapters. The [PowerShell helper](https://github.com/CritX-ai/LedAlert/blob/main/tools/windows-verify.ps1) supplies an **explicitly opt-in, isolated** development package and synthetic toast fixture. Both are in the source archive; the Rust example also ships in the Cargo source package. They do not depend on an earlier maintainer's temporary scripts, machine paths, screenshots, certificates or captured notifications.

## What each verification level proves

| Level | Runs where | Proves / does not prove |
| --- | --- | --- |
| Portable fixtures and state tests | Linux or Windows source checkout | Public/synthetic Taskband parsing fixtures, display geometry, notification generations/baselines, timing, lock/media and config-path contracts. No live Windows shell or consent. |
| Headless CI | `windows-2025` (Windows Server) | Compiles Windows APIs and the probe, runs Rust/Python regressions, parses the helper without executing its actions, builds/inspects unsigned packages. Does not claim usable Explorer pins, notification permission, media playback, lock UI or Windows 11 acceptance. |
| Native inventory | Interactive Windows 11 user session | Real read-only Taskband/icon and active display queries succeed and return reviewable counts/geometry. Not a visual icon-match or notification claim. |
| Isolated synthetic lifecycle | Interactive Windows 11 with foreground consent | A real notification enters history, initial baseline is silent, a new toast raises, removal closes the same event, a recreated worker silently baselines retained history, and new add/remove still works. No WLED transport. |
| Manual acceptance | Windows 11 desktop, chosen applications and optional second session | Visual pin/icon correctness, mixed-DPI/portrait import, permission denial/revocation, application restart, playback and lock/disconnect behavior. Record unexercised cases as **not verified**. |

## Alpha workstation evidence

The `0.3.0-alpha` preparation exercised these committed tools on Windows 11 build 26200:

- Native inventory returned **23 pins with decoded icons** and **two active displays**.
- A bounded unregistered observation reported the session unlocked, an attached media monitor with no active playback, and notification access unavailable without package identity.
- The isolated packaged GUI completed all six synthetic lifecycle stages and recorded final **PASS**. The foreground access action was invoked; Windows already allowed access, so this did not verify a first-consent prompt.
- The manual synthetic toast helper was checked against actual notification history: one fixture after creation, none after removal.
- Explicit registration and cleanup succeeded, including removal of Windows-created hidden package metadata. The verification package was removed.

These are scoped workstation observations, not signed-installer acceptance. Permission denial/revocation, full application restart, active playback transitions, interactive lock/disconnect and physical WLED output were **not verified** in this run. Repeat the appropriate manual cases before promotion; do not infer them from the synthetic worker-restart check.

## Prerequisites and safety

- Use a Windows 11 **x64** interactive account with Explorer and a working GUI/OpenGL environment. Do not treat an SSH/service/Server runner session as equivalent.
- Install the repository's Rust toolchain (currently 1.95.0), the MSVC target and Visual Studio Build Tools **Desktop development with C++**, including the Windows SDK. Use `--locked`.
- The helper is intended for **Windows PowerShell 5.1** (`powershell.exe`); the manual toast actions require its WinRT projection. CI uses PowerShell 7 only to parse the file. Follow your organization's script execution policy; the helper does not weaken it.
- For the isolated packaged listener only, an authorized user must already have enabled **Developer Mode**. Registration is not needed for inventory or media/lock observation. If policy prohibits registration or consent, stop: record the listener scenario as blocked rather than changing trust/policy or claiming a pass.
- The alpha MSIX is unsigned, for development only. This procedure does not make it signed or production-trusted. The helper uses a loose development manifest under a separate identity; it never imports a certificate, changes certificate trust, enables Developer Mode, elevates itself, or installs/removes the real `CritX.LedAlert` package.
- The Rust probe never constructs a WLED transport. Manual application cases must keep **Enable lighting**, **Guide with real lights / Take control**, and any hardware control off. Use a separate test configuration; never overwrite a personal room. No physical LED/output claim follows from these checks.

## Portable regressions after leaving Windows

On a supported source-build host, install the [native build prerequisites](guide/install.md#native-dependencies), then run:

```sh
cargo test --locked --test windows_taskbar --test windows_displays --test desktop --test config_paths
cargo test --locked desktop::windows_state::tests
python -B -m unittest discover -s packaging -p "test_windows.py"
```

On Linux, use `python3` when that is the Python 3.11+ command, and provide `dbus-daemon` for private-bus desktop tests. These focused checks supplement the full release gates; they do not replace them. The checked-in `tests/fixtures/windows-taskband-v3.hex` is a selected public Windows 11 capture, not generated synthetic bytes: its header records the build and selected apps, with no user paths or notification data. Tests also construct synthetic edge cases. Do not replace this fixture with a raw personal registry export, shortcut, full taskbar dump or notification capture.

## Native inventory and bounded observation

From the source root, in PowerShell:

```powershell
cargo build --locked --example windows_probe
.\target\debug\examples\windows_probe.exe inventory
.\target\debug\examples\windows_probe.exe observe 30
```

If `CARGO_TARGET_DIR` or an explicit `--target` is used, adjust the executable path. `inventory` runs the blocking production discoveries on a worker with a **30-second** deadline. It prints one JSON object with indexed pins, coarse app kind, decoded icon dimensions or `null`, and indexed active display pixel rectangles/primary flags. It omits app names/IDs, shortcut targets, connector paths and icon pixels. Exit 0 / `PASS` means only that both discovery calls succeeded. An empty list, missing icon, unexpected topology, failure or timeout needs investigation; a missing icon must not be reported as a successful visual match.

`observe SECONDS` accepts **1–300 seconds**, samples every 250 ms, then exits 0 with `OBSERVED`. It prints notification/media/lock status, generation, aggregate event/playback counts and `locked` (`false`, `true` or `null`), never application IDs or message/track text. **OBSERVED is not PASS**: compare transitions against the cases below. An unregistered executable must report that Windows notifications require the installed edition; media/lock remain available. This is an expected limitation, not permission to bypass package identity. Invalid arguments/failed startup exit nonzero; the non-Windows example exits 2 with an unsupported-platform message.

## Real synthetic notification lifecycle

This changes only explicitly selected verification state. Close any previous probe GUI first. Review the helper before running it:

```powershell
powershell.exe -NoProfile -File .\tools\windows-verify.ps1 -Action Register
powershell.exe -NoProfile -File .\tools\windows-verify.ps1 -Action Launch
```

`Register` copies the already-built example, generates non-personal logos and a manifest, and explicitly calls `Add-AppxPackage -Register`. Its identity is **LedAlert.Verification**, publisher **CN=LedAlertVerification**, application **Probe**, version **1.0.0.0**. Files live under `%LOCALAPPDATA%\LedAlertVerification\package`; an ownership marker sits in the parent directory. The helper refuses to adopt an existing package/directory, follow a reparse-point root, replace a running probe, or modify a matching identity at another location. It grants only the same full-trust, notification-listener and media capabilities needed by the adapters. Use `-Executable PATH` only when your build output differs.

Launch activates the package through Windows AppsFolder. **Do not launch the copied EXE directly** for this case: it needs package identity. In the foreground GUI:

1. Click **1. Request Windows notification access**. Accept the Windows prompt if offered. Already granted access may not prompt. No consent request happens automatically at startup.
2. Wait for **Monitoring Windows notifications (metadata only)**. Denied access is a real outcome: use the manual denial case below, do not force it.
3. Click **2. Run synthetic lifecycle**. Do not run manual `Toast` actions concurrently, dismiss the test toasts, revoke permission or lock the session during this positive scenario.
4. Observe six `PASS` stages: **initial baseline without replay**, **new toast raised**, **removed toast closed**, **worker restart without replay**, **new add and remove after restart**, and **synthetic toast cleanup**. The overall result must be `PASS`.
5. Read the machine-checkable result, then close the GUI:

```powershell
powershell.exe -NoProfile -File .\tools\windows-verify.ps1 -Action Report
```

Each polling wait has a 15-second timeout; the GUI records failure if the overall 180-second observation deadline expires (including time spent waiting for user consent). It remains visible for inspection until closed. `Report` exits 0 only for schema 1 with final `PASS`; absent/aborted/failed runs return nonzero. Relaunch clears the previous result before activation, so an old pass cannot stand in for the new run. An early window close has no completed report and is **not verified**.

The fixture text is always “LedAlert verification” / “Synthetic fixture; no personal content.” The worker confirms the seed really entered its own toast history before checking baseline behavior. Event matching uses only the verification AUMID, event generation and ID. Unrelated notifications are ignored by the assertions and never logged. This proves adapter worker recreation, **not** restart of Windows' notification service or the full application. The six-stage result is not a general permission/media/lock or rendered-lighting certification.

### Failure handling

Check Developer Mode, Windows notification settings for **LedAlert Verification**, notification history availability, foreground consent, the interactive session and whether another probe is running. Focus assist/Do not disturb or enterprise policy may affect delivery; record the actual settings instead of disabling policy automatically. A timeout or absent toast is a failed/blocked observation, not a pass based on API invocation alone.

Normal probe reports deliberately suppress OS payloads and paths. For a private local diagnosis only, rerun the failed helper action with `-PrivateDiagnostics`; deployment exceptions may contain local paths. Do not attach that raw output publicly. If registration partially succeeds, use the owned cleanup action; do not broaden removal to all Appx packages or delete unrelated application data.

## Manual acceptance matrix

Use a fresh, separate application configuration with loopback/default device address and lighting disabled. For notification rendering/permission cases, the actual LedAlert GUI must be installed or explicitly development-registered as described in [Windows installation](guide/install.md#windows-11); the verification package does not register the real app or grant its permission. Launch the real app from Start, and use **Settings → Integration status → Allow Windows notifications**. Do not confuse the real app's access switch with **LedAlert Verification**.

For a controlled notification source, after registering the isolated verification package:

```powershell
powershell.exe -NoProfile -File .\tools\windows-verify.ps1 -Action Toast
powershell.exe -NoProfile -File .\tools\windows-verify.ps1 -Action RemoveToast
```

These actions affect only tag `fixture`, group `manual`, under the isolated verification AUMID. They print **REQUESTED**, not a delivery claim; confirm the fixture in Windows notification center. Repeating `Toast` replaces the same manual fixture. Use `RemoveToast` between distinct-add cases. This is separate from the automatic scenario's per-process group. Do not clear all Windows notification history.

| Case | Procedure | Acceptance / evidence |
| --- | --- | --- |
| Taskbar pins and icons | Run `inventory`; open the real GUI's **Examples**. Compare locally with the actual pinned taskbar, including one packaged app and one Win32 shortcut when available. Do not launch pins from the probe. | Correct current pins/order and matching available artwork; no substitute Start-menu inventory. Missing/unsupported icons remain missing, not fabricated. Record only counts and pass/fail publicly. Taskbar layout or app-name screenshots stay private. |
| Display geometry | Compare inventory with Windows Display settings. If available, use mixed DPI, a portrait screen, a negative desktop origin and an external display. In a disposable setup use **Displays → Refresh**, then explicitly **Use desktop layout**, then **Undo**. | Active pixel extents and orientation match; no second DPI scaling/rotation. Refresh preserves edited placements/names; explicit arrangement updates them; Undo restores them. Disconnect/reconnect one display and confirm stable saved associations; record absent hardware combinations as not verified. |
| Portable access limitation | Run unregistered `observe 10` and launch unregistered LedAlert. | Clear packaged-edition requirement, no automatic prompt; media/lock still observed where supported. |
| First consent / denial | With a newly registered identity or access reset in Windows Settings, open the relevant GUI without clicking consent. Then explicitly request and deny. | No startup prompt; denied status remains visible; no toast capture or silent access workaround. Record whether Windows actually presented a fresh prompt; previously granted access is not a fresh-consent test. |
| Permission revocation/restoration | In registered LedAlert, establish a persistent synthetic alert, then revoke **LedAlert** notification access in Windows Settings while it remains open. Post another manual fixture while denied; restore access explicitly. | Unavailable/denied status and clearing of uncertain tracked alerts; restoration silently baselines existing history. Only a new fixture after restoration creates a new alert. Do not infer this from a portable process or from the separate probe's access state. |
| Add/remove/replacement | With real GUI permission, enable desktop notifications and create a persistent rule from the observed verification source in **Add app** (or a fallback in the disposable setup). Use `Toast`, repeat once, then `RemoveToast`. | New notification routes to the intended rule; replacement does not leave duplicate/stuck persistent records; removal clears tracking. Popup timeout alone is not dismissal. Keep physical lighting disabled. |
| Application restart | Leave the manual fixture in notification center. Close and restart the registered real GUI. After status returns to monitoring, remove/recreate the fixture. | Retained history is not replayed on restart; fresh add/removal works; lighting remains disabled, and guidance/demos are not restored. This complements the automatic worker-restart case. |
| Active media / pause / stop | Run `observe 60` while starting, pausing, resuming and closing a known Windows system-media-session player. In the real GUI enable **Media playback** and a matching rule's **Media** trigger, with lighting still off. | `playing_count` follows Playing rather than merely an open application, returns to zero when all sessions stop, and on-screen playback marker follows the source. Allow the 750-ms query interval and up to 3-second stale-state window. No audio/track metadata is captured. A player without system-media integration is not a valid positive test. |
| Lock/unlock | Start `observe 60`, wait for `locked:false`, press **Win+L**, wait at least five seconds, then unlock before the run ends. Repeat with the real GUI and a local demo active. | During lock the trace changes to `true` or unavailable `null`, never continuing stale unlocked state beyond its one-second freshness bound. Unlock returns to `false`; stopped demos do not restart. Record the elapsed trace transitions, not a lock-screen screenshot. |
| Disconnection / unavailable session | Only on a machine with a safe recovery path, start `observe 120`, disconnect an RDP session (do not log off), wait at least five seconds, then reconnect. Separately check a session where WTS access is unavailable if available. | Disconnected/unavailable session becomes inhibited (`true` or `null`); stale unlock is not accepted. Reconnection is observed afresh. Console lock is not proof of RDP disconnect; unsupported setups are not verified. |
| Notification while inhibited | With registered real GUI and a persistent rule, lock the session; have a second authorized session/operator invoke the fixed toast helper for the same test account, then unlock. | Notifications received during inhibition are not replayed as new alerts. Existing persistent alerts from before the lock may resume; permission/lifecycle loss instead clears uncertain tracking. Without a second safe producer/session, record this case as not verified. |

These are **acceptance instructions**, not automated desktop tests. The probe's lock trace establishes the production adapter's observed input; portable runtime tests establish fail-closed consumption. Neither proves real physical LEDs stopped. Actual WLED/network failure and hardware output require a separate, explicitly authorized low-brightness hardware procedure and must never be inferred from this no-output probe.

## Reports, privacy and cleanup

A useful public report contains commit/version, toolchain, Windows edition/build and session type, inventory pin/icon/display counts, synthetic stage results, transition timing, and a per-case **pass / fail / blocked / not verified** table. Even redacted geometry and timing can describe a workstation: review them before posting. Do not publish raw Taskband bytes, AUMIDs that embed paths, shortcut targets, usernames, private configuration, toast/track text, notification-center screenshots or unrelated desktop captures. Synthetic fixture text is safe; personal content appearing beside it is not.

The isolated machine-readable report is `%LOCALAPPDATA%\LedAlertVerification\report.json`, outside the repository. Copy only a reviewed sanitized result into a public issue. Keep any private evidence in a separately ignored/private location; never add machine dumps to test fixtures. Cleanup deletes the report, so save the sanitized result first:

```powershell
# Close the probe window first; do not forcibly kill an unrelated application.
powershell.exe -NoProfile -File .\tools\windows-verify.ps1 -Action RemoveToast
powershell.exe -NoProfile -File .\tools\windows-verify.ps1 -Action Cleanup
```

The automatic scenario removes only its own per-process toast group, including best-effort cleanup on failure. The manual fixture is removed explicitly. `Cleanup` validates ownership, refuses a running copied probe, unregisters only the exact isolated package at the owned path, confirms removal and deletes its directory. It never removes the real LedAlert package, certificates or other applications. If the ownership marker is missing or package location differs, it refuses; investigate privately instead of deleting broadly. Re-running Cleanup with no owned state is harmless. After rebuilding the example, close, clean up, and register again rather than replacing a registered running binary.

### Hooks and CI boundaries

The checked-in hooks are opt-in; nothing here installs them. They locate Python 3.11+ as `python3` or `python`, and accept available Podman or Docker for the pinned documentation build. Missing prerequisites fail the gate rather than skipping it. Pandoc is also required. The full pre-push controlled-builder gate still needs Linux x86_64 and its working container runtime (for example a properly provisioned Linux environment); a Windows-only native smoke is not a substitute and the hook does not bypass the baseline. GitHub's Windows Server job compiles the native probe and syntax-checks the helper without granting consent, registration or trust. Use a future interactive Windows 11 machine to repeat the remaining matrix, even after this workstation is no longer available.
