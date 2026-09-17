# Compatibility

LedAlert's home turf is **Linux x86_64, KDE Plasma on Wayland, and one WLED RGB strip**. Here's what fits.

## Desktop

<table class="support-matrix">
<thead><tr><th scope="col">Your setup</th><th scope="col">Status</th></tr></thead>
<tbody>
<tr><th scope="row">KDE Plasma on Wayland<small>Linux x86_64; desktop notifications, lock detection and media markers.</small></th><td><span class="support-status supported"><svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="9"/><path d="m7 12 3 3 7-7"/></svg>Supported</span></td></tr>
<tr><th scope="row">Automatic display placement<small>KDE's kscreen-doctor is optional. You can place displays manually.</small></th><td><span class="support-status supported"><svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="9"/><path d="m7 12 3 3 7-7"/></svg>Supported</span></td></tr>
<tr><th scope="row">Pinned-app suggestions and icons<small>Reads local Plasma launcher metadata. Manual rules and text-only tiles remain available.</small></th><td><span class="support-status supported"><svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="9"/><path d="m7 12 3 3 7-7"/></svg>Supported</span></td></tr>
<tr><th scope="row">Other Linux desktops and X11<small>Complete notification and lock integration is unverified.</small></th><td><span class="support-status unverified"><svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="9"/><path d="M9.5 9a2.5 2.5 0 0 1 5 0c0 2-2.5 2-2.5 4m0 3h.01"/></svg>Unverified</span></td></tr>
<tr><th scope="row">Windows and macOS<small>No desktop backends are implemented.</small></th><td><span class="support-status unavailable"><svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="9"/><path d="m9 9 6 6m0-6-6 6"/></svg>Unsupported</span></td></tr>
</tbody>
</table>

Notifications must use the freedesktop notification service. In-app-only messages and other notification transports are not observed. Media markers need an MPRIS-capable player. The session must allow notification monitoring and provide a working lock service.

## Lights

<table class="support-matrix">
<thead><tr><th scope="col">Your lights</th><th scope="col">Status</th></tr></thead>
<tbody>
<tr><th scope="row">One WLED RGB controller and strip<small>One continuous path, mapped in your room.</small></th><td><span class="support-status supported"><svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="9"/><path d="m7 12 3 3 7-7"/></svg>Supported</span></td></tr>
<tr><th scope="row">Up to 8,192 RGB LEDs<small>Configuration limit; actual performance depends on the device and network.</small></th><td><span class="support-status supported"><svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="9"/><path d="m7 12 3 3 7-7"/></svg>Supported</span></td></tr>
<tr><th scope="row">Multiple controllers, separate runs or matrices</th><td><span class="support-status unavailable"><svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="9"/><path d="m9 9 6 6m0-6-6 6"/></svg>Unsupported</span></td></tr>
<tr><th scope="row">RGBW/CCT-specific output and other controller protocols</th><td><span class="support-status unavailable"><svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="9"/><path d="m9 9 6 6m0-6-6 6"/></svg>Unsupported</span></td></tr>
</tbody>
</table>

Use a trusted local network and one realtime controller. WLED needs HTTP **80** for connection and UDP **4048** for RGB output. LedAlert does not change persistent WLED settings; the device's own limits and realtime timeout still apply.

## Before you install

The Linux binary needs **glibc 2.36 or newer**, a graphical session, session D-Bus and working OpenGL/EGL. Check the bundle's `BUILD-INFO.json` for its measured requirements. A matching CPU or glibc version alone does not guarantee desktop compatibility.

Ready? [Install and connect](guide/install.md). Something not responding? [Find a fix](guide/troubleshooting.md). For exact protocol behavior, see [Lighting and integrations](reference-runtime.md).
