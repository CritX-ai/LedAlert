# Roadmap

LedAlert's job is simple: give desktop events a useful place in your room. Future work should make that easier without turning the app into an RGB entertainment dashboard.

## Current status

**[0.3.0-beta](https://github.com/CritX-ai/LedAlert/releases/tag/v0.3.0-beta) adds native macOS 27 / Apple Silicon and Sidebar.app compatibility** alongside Windows 11 x64 and Linux/KDE. Display import, native application icons, notification lifecycle, lock inhibition and application media/activity markers are implemented. macOS notification access is permission-gated and uses a private metadata schema; app audio activity is not exact player play/pause state. See [release notes](../RELEASE.md), [Compatibility](support.md) and [Windows verification](windows-verification.md) for the exercised boundaries.

## Release boundary

- **Development beta:** GitHub-only, not the latest stable release and not published to crates.io. Cargo's unversioned installation route still selects the stable published crate.
- **macOS trust:** the beta is ad-hoc signed, not Developer ID signed or notarized. A Developer ID–signed, notarized and stapled DMG is production work, not a beta feature. No Gatekeeper or permission changes are automated.
- **Windows publication:** the beta MSIX is unsigned and development-only. Trusted signing, stable package/publisher identity and clean-machine installation/upgrade checks are required for the production route; the existing stable signing gate is not satisfied by this beta.
- **Interactive acceptance:** live notification permission changes, active media/audio, lock transitions and physical lighting require controlled, separately authorized checks. Fixture success and CI do not imply complete desktop or device acceptance.
- **Before stable 0.3.0:** complete trusted installers and final-byte verification, exercise the remaining live acceptance matrix, and document installation and upgrade without development-only setup. Homebrew Cask, WinGet and Microsoft Store distribution are optional later channels, not available beta installers.

## Future directions

These are possibilities, **not implemented features or delivery promises**:

- **More Linux desktops:** explore notification and lock-state integration beyond KDE Plasma, while keeping manual setup useful.
- **More lighting layouts:** consider separate physical runs on one controller, then multiple independent WLED controllers. These need distinct mapping and control behavior.
- **Matrices:** explore grid mapping and orientation as a later addition; current strip mapping is not first-class matrix support.

Current [compatibility limits](support.md) still apply.

## Contribution policy

LedAlert accepts **feature additions that are demonstrably verified** in the environment they target. We do not merge partial implementations, and routine fixes are not a contribution path — reproducible bug reports through [issues](https://github.com/CritX-ai/LedAlert/issues) are the way to flag defects.

**Unsupported environment? You are the right person to change that.** If your desktop, display server, or WLED setup is not covered yet, implement the support, test it on your own setup, and send a pull request with evidence. This applies to the unverified desktops in [Compatibility](support.md), additional lighting layouts, and platform ports.

Requirements for a merged contribution:

1. **Complete, end-to-end work.** A change must be usable and testable by a reader of the manual. Stubs, scaffolds, and half-wired behavior are declined.
2. **Evidence from the target environment.** Show real execution on the hardware or desktop the feature claims to support: session and WLED behavior, before/after captures, and the checks you ran. Source-level reasoning alone does not verify a feature.
3. **Agreed scope.** Open an issue describing the user problem, your environment and your approach before starting a platform port or lighting-layout work; large unrequested refactors are declined.
4. **Boundaries respected.** Real-lighting work needs the device owner's consent. Keep personal notification content, device addresses and private logs out of public reports.

For a suspected vulnerability, use [security reporting](../SECURITY.md) rather than a public bug report containing sensitive details.

LedAlert is [MIT OR Apache-2.0](../README.md#license-status), at the recipient's choice. Preserve required attribution and identify any third-party material in a contribution.
