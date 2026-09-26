# Technical reference

The details behind your setup. New here? Follow [Room](guide/room.md) → [Displays](guide/displays.md) → [Strip](guide/strip.md) → [Rules](guide/rules.md). Something stuck? [Troubleshoot by symptom](guide/troubleshooting.md).

LedAlert supports **macOS 27 on Apple silicon**, **Windows 11 x64**, and **Linux x86_64 with KDE Plasma on Wayland**, with one WLED RGB controller and its mapped strip. macOS uses native Dock or Sidebar.app pins and permission-gated desktop adapters. See [Compatibility](support.md) for requirements and limits.

## Editing and rules

[Editing and rule behavior](reference-editing.md) explains room geometry, LED allocation, application matching, effects and keyboard controls.

- [Room and strip mapping](reference-editing.md#setup-and-room-editing)
- [Custom room outlines](reference-editing.md#optional-room-outlines)
- [Rules, effects and palettes](reference-editing.md#rules-effects-and-palettes)
- [Taskbar examples and icons](reference-editing.md#taskbar-examples-and-application-icons)
- [Motion and keyboard controls](reference-editing.md#identity-motion-and-keyboard-controls)

## Lighting and integrations

[Lighting and integrations](reference-runtime.md) covers permission to send pixels, desktop privacy and device behavior.

- [Lighting permission](reference-runtime.md#lighting-permission-and-lifecycle)
- [Demos and preview](reference-runtime.md#demos-and-preview)
- [Physical LED guidance](reference-runtime.md#find-the-real-leds)
- [Desktop integration and privacy](reference-runtime.md#desktop-integration-and-privacy)
- [WLED transport and failures](reference-runtime.md#wled-transport-and-failure-behavior)

## Saved setups and diagnostics

[Saved setups and diagnostics](reference-configuration.md) covers file locations, validation limits and recovery without losing your map.

- [Files and diagnostic commands](reference-configuration.md#configuration-and-diagnostics)
- [Back up and recover](reference-configuration.md#back-up-and-recover)
