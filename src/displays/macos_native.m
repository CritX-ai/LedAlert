// Read-only CoreGraphics display adapter for LedAlert.
//
// Bounds are reported in the common global desktop coordinate space (points,
// top-left origin of the main display). CoreGraphics scales each display by
// its own backing scale, so mixed-DPI arrangements need no per-display
// division. Identities are the stable CoreGraphics display UUIDs from
// ColorSync's public CGDisplayCreateUUIDFromDisplayID (declared in
// ColorSyncDevice.h); the Rust side derives the labels ("Built-in display"
// or "Display N") because this OS exposes no worker-safe display-name query.
// No display mode, arrangement or preference is ever changed, and the
// enumeration is safe on a worker thread.

#import <ColorSync/ColorSyncDevice.h>
#import <CoreFoundation/CoreFoundation.h>
#import <CoreGraphics/CoreGraphics.h>

#import <stdint.h>
#import <string.h>

#define LED_DISPLAY_LIMIT 128
#define LED_UUID_CAP 64

// Mirrors the #[repr(C)] DisplayBoundsRecord in displays/macos.rs.
typedef struct {
    char uuid[LED_UUID_CAP];
    double x;
    double y;
    double width;
    double height;
    uint8_t main;
    uint8_t builtin;
} LedDisplayBounds;

int32_t ledalert_displays_bounds(LedDisplayBounds *out, uintptr_t capacity, uintptr_t *out_count) {
    if (!out || !out_count || capacity == 0) {
        return -1;
    }
    CGDirectDisplayID displays[LED_DISPLAY_LIMIT + 1];
    uint32_t count = 0;
    if (CGGetActiveDisplayList(LED_DISPLAY_LIMIT + 1, displays, &count) != kCGErrorSuccess) {
        return -1;
    }
    if (count > LED_DISPLAY_LIMIT || count > capacity) {
        return -1;
    }
    for (uint32_t index = 0; index < count; index++) {
        LedDisplayBounds *slot = &out[index];
        memset(slot, 0, sizeof *slot);
        CGRect bounds = CGDisplayBounds(displays[index]);
        slot->x = bounds.origin.x;
        slot->y = bounds.origin.y;
        slot->width = bounds.size.width;
        slot->height = bounds.size.height;
        slot->main = CGDisplayIsMain(displays[index]) ? 1 : 0;
        slot->builtin = CGDisplayIsBuiltin(displays[index]) ? 1 : 0;
        CFUUIDRef uuid = CGDisplayCreateUUIDFromDisplayID(displays[index]);
        if (uuid) {
            CFStringRef text = CFUUIDCreateString(kCFAllocatorDefault, uuid);
            if (text) {
                if (!CFStringGetCString(text, slot->uuid, LED_UUID_CAP, kCFStringEncodingASCII)) {
                    slot->uuid[0] = 0;
                }
                CFRelease(text);
            }
            CFRelease(uuid);
        }
        if (!slot->uuid[0]) {
            // Without a stable identity the snapshot fails; no invented connector.
            return -1;
        }
    }
    *out_count = count;
    return 0;
}
