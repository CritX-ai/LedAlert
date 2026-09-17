# Roadmap

LedAlert's job is simple: give desktop events a useful place in your room. Future work should make that easier without turning the app into an RGB entertainment dashboard.

## Current status

**Version 0.2 is the first public release**, built for Linux x86_64, KDE Plasma on Wayland and one mapped WLED RGB strip. Room outlines, spatial rules, local previews and explicit lighting controls are available now. See [release notes](../RELEASE.md) and [Compatibility](support.md).

## Future directions

These are possibilities, **not implemented features or delivery promises**:

- **More Linux desktops:** explore notification and lock-state integration beyond KDE Plasma, while keeping manual setup useful.
- **Windows:** investigate a notification-focused port with clear user consent before committing to a backend.
- **More lighting layouts:** consider separate physical runs on one controller, then multiple independent WLED controllers. These need distinct mapping and control behavior.
- **Matrices:** explore grid mapping and orientation as a later addition; current strip mapping is not first-class matrix support.
- **macOS:** investigate a supported event source before considering a port. An editor that opens is not equivalent to notification support.

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
