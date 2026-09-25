# Roadmap

LedAlert's job is simple: give desktop events a useful place in your room. Future work should make that easier without turning the app into an RGB entertainment dashboard.

## Current status

**0.3.0-alpha is a GitHub prerelease adding Windows 11 x64** alongside Linux x86_64 and KDE Plasma on Wayland. Native notifications, media and lock detection, taskbar suggestions, display import and one mapped WLED RGB strip are implemented. Windows notification access requires package identity and consent; the alpha provides a portable ZIP and an unsigned development MSIX, not a production signed installer. It is not published to crates.io. See [release notes](../RELEASE.md), [Compatibility](support.md) and [Windows verification](windows-verification.md) for the remaining interactive acceptance work.

## Planned 0.3.0 progression

- **Alpha now:** Windows and Linux prerelease with repeatable portable regressions and explicit native Windows smoke tools. Trusted production Windows signing remains an external prerequisite.
- **Beta next:** add macOS support and verify it on macOS, while continuing Linux and Windows regression checks. macOS support is planned, not available in the alpha; an editor that opens is not sufficient evidence of desktop integration.
- **Final 0.3.0:** cross-platform polishing after beta, with verification evidence and production signing requirements satisfied.

These are planned release stages, not claims that future platform support or acceptance is complete.

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
