# Roadmap

LedAlert's job is simple: give desktop events a useful place in your room. Future work should make that easier without turning the app into an RGB entertainment dashboard.

## Current status

**0.2.0 is the first public release**, built for Linux x86_64, KDE Plasma on Wayland and one mapped WLED RGB strip. Room outlines, spatial rules, local previews and explicit lighting controls are available now. See [release notes](../RELEASE.md) and [Compatibility](support.md).

## Future directions

These are possibilities, **not implemented features or delivery promises**:

- **More Linux desktops:** explore notification and lock-state integration beyond KDE Plasma, while keeping manual setup useful.
- **Windows:** investigate a notification-focused port with clear user consent before committing to a backend.
- **More lighting layouts:** consider separate physical runs on one controller, then multiple independent WLED controllers. These need distinct mapping and control behavior.
- **Matrices:** explore grid mapping and orientation as a later addition; current strip mapping is not first-class matrix support.
- **macOS:** investigate a supported event source before considering a port. An editor that opens is not equivalent to notification support.

No dates are promised. Current [compatibility limits](support.md) still apply.

## Taking a task

Want to help a notification find its place?

1. [Open or join an issue](https://github.com/CritX-ai/LedAlert/issues). Describe the user problem, your proposed change and the environment you can use.
2. Agree on the scope before starting a large feature or platform port. Small fixes, clearer instructions and reproducible bug reports are welcome too.
3. Keep pull requests focused. Explain what users will notice and what you exercised; distinguish real hardware observations from simulations.
4. Use synthetic notifications in screenshots and examples. Leave personal configurations, device addresses and private logs out of public reports. Hardware work needs the device owner's permission.

For a suspected vulnerability, use [security reporting](../SECURITY.md) rather than a public bug report containing sensitive details.

LedAlert is [MIT OR Apache-2.0](../README.md#license-status), at the recipient's choice. Preserve required attribution and identify any third-party material in a contribution.
