// Read-only macOS pinned-launcher adapter for LedAlert.
//
// CFPreferences values are copied out without any write, cache flush or
// preference mutation. Application bundles are only inspected for identity,
// display name and icon; nothing is launched, activated or registered.
// Only launcher identity, visibility and order influence the Rust projection.
// Functions are safe to call from a discovery worker thread: every AppKit
// entry point used here is a documented read-only query (NSWorkspace
// runningApplications, URLForApplicationWithBundleIdentifier, iconForFile)
// and drawing happens into a local bitmap only.

#import <AppKit/AppKit.h>
#import <CoreFoundation/CoreFoundation.h>

#import <stdint.h>
#import <string.h>

static const uintptr_t kLedConfigLimit = 1024 * 1024;
static const size_t kLedIconSide = 64;

// Status convention shared by every entry point:
//   0 = data/metadata returned, 1 = absent or omitted (not an error),
//  -1 = malformed source or adapter failure (fails the whole snapshot).

typedef int32_t (*LedConfigSink)(const uint8_t *, uintptr_t, void *);

static int32_t ledalert_copy_bytes(const void *bytes, uintptr_t length,
                                   LedConfigSink sink, void *context) {
    if (!sink || !bytes || length > kLedConfigLimit) return -1;
    if (length == 0) return 1;
    return sink(bytes, length, context) ? 0 : -1;
}

int32_t ledalert_taskbar_sidebar_active(void) {
    @autoreleasepool {
        // Read-only enumeration of running applications; Sidebar is never
        // launched to answer this question.
        for (NSRunningApplication *application in NSWorkspace.sharedWorkspace.runningApplications) {
            if ([application.bundleIdentifier isEqualToString:@"at.sidebar.Sidebar"]) {
                return 1;
            }
        }
        return 0;
    }
}

int32_t ledalert_taskbar_sidebar_config(LedConfigSink sink, void *context) {
    CFTypeRef value = CFPreferencesCopyAppValue(CFSTR("applicationConfigurations"),
                                                CFSTR("at.sidebar.Sidebar"));
    if (!value) return 1;
    if (CFGetTypeID(value) != CFDataGetTypeID()) {
        CFRelease(value);
        return -1;
    }
    CFDataRef data = (CFDataRef)value;
    int32_t status = ledalert_copy_bytes(CFDataGetBytePtr(data), (uintptr_t)CFDataGetLength(data),
                                         sink, context);
    CFRelease(value);
    return status;
}

int32_t ledalert_taskbar_dock_config(LedConfigSink sink, void *context) {
    @autoreleasepool {
        id value = CFBridgingRelease(CFPreferencesCopyAppValue(CFSTR("persistent-apps"),
                                                              CFSTR("com.apple.dock")));
        if (!value) return 1;
        if (![value isKindOfClass:NSArray.class] || [value count] > 512) return -1;
        NSMutableArray *surface = [NSMutableArray arrayWithCapacity:[value count]];
        for (id tile in value) {
            if (![tile isKindOfClass:NSDictionary.class]) return -1;
            id type = tile[@"tile-type"];
            if (![type isKindOfClass:NSString.class] || [type length] > 64) return -1;
            NSMutableDictionary *copy = [NSMutableDictionary dictionaryWithObject:type forKey:@"tile-type"];
            if ([type isEqualToString:@"file-tile"]) {
                id data = tile[@"tile-data"];
                if (![data isKindOfClass:NSDictionary.class]) return -1;
                id file = data[@"file-data"];
                if (![file isKindOfClass:NSDictionary.class]) return -1;
                id url = file[@"_CFURLString"];
                if (![url isKindOfClass:NSString.class] || [url length] > 4096) return -1;
                NSMutableDictionary *fields = [NSMutableDictionary dictionaryWithObject:
                    @{@"_CFURLString": url} forKey:@"file-data"];
                id identifier = data[@"bundle-identifier"];
                if (identifier) {
                    if (![identifier isKindOfClass:NSString.class] || [identifier length] > 512) return -1;
                    fields[@"bundle-identifier"] = identifier;
                }
                copy[@"tile-data"] = fields;
            }
            [surface addObject:copy];
        }
        // Bookmarks and unrelated preference values never enter the JSON projection.
        NSData *json = [NSJSONSerialization dataWithJSONObject:surface options:0 error:NULL];
        if (!json) return -1;
        return ledalert_copy_bytes(json.bytes, (uintptr_t)json.length, sink, context);
    }
}

int32_t ledalert_taskbar_application_lookup(const char *identifier, char *out_path,
                                            uintptr_t path_cap) {
    if (!identifier || !out_path || path_cap == 0) {
        return -1;
    }
    out_path[0] = 0;
    @autoreleasepool {
        NSString *bundle = [NSString stringWithUTF8String:identifier];
        if (!bundle.length) {
            return -1;
        }
        // Read-only LaunchServices query; it never launches or registers the
        // target and it is documented thread-safe.
        NSURL *url = [NSWorkspace.sharedWorkspace URLForApplicationWithBundleIdentifier:bundle];
        if (!url) {
            return 1;
        }
        NSString *path = url.path;
        if (!path.length) {
            return 1;
        }
        const char *utf8 = path.fileSystemRepresentation;
        if (!utf8 || strlen(utf8) >= path_cap) {
            return -1;
        }
        strcpy(out_path, utf8);
        return 0;
    }
}

static BOOL ledalert_write_c_string(NSString *value, char *out, uintptr_t cap) {
    if (![value isKindOfClass:NSString.class] || cap == 0 || value.length >= cap ||
        [value rangeOfCharacterFromSet:NSCharacterSet.controlCharacterSet].location != NSNotFound) {
        return NO;
    }
    return [value getCString:out maxLength:cap encoding:NSUTF8StringEncoding];
}

static void ledalert_render_icon(NSImage *icon, uint8_t *out_rgba, uintptr_t rgba_cap,
                                 uintptr_t *out_rgba_len) {
    *out_rgba_len = 0;
    if (!icon || rgba_cap < kLedIconSide * kLedIconSide * 4) {
        return;
    }
    NSRect proposed = NSMakeRect(0, 0, kLedIconSide, kLedIconSide);
    CGImageRef image = [icon CGImageForProposedRect:&proposed context:nil hints:nil];
    if (!image || CGImageGetWidth(image) == 0 || CGImageGetHeight(image) == 0) {
        return;
    }
    CGColorSpaceRef space = CGColorSpaceCreateWithName(kCGColorSpaceSRGB);
    if (!space) {
        return;
    }
    size_t side = (size_t)kLedIconSide;
    // Premultiplied RGBA rows, top-left origin, exactly what the Rust side
    // unpremultiplies into the shared AppIcon shape.
    CGContextRef context = CGBitmapContextCreate(out_rgba, side, side, 8, side * 4, space,
                                                 kCGImageAlphaPremultipliedLast |
                                                     kCGBitmapByteOrder32Big);
    CGColorSpaceRelease(space);
    if (!context) {
        return;
    }
    CGContextClearRect(context, CGRectMake(0, 0, side, side));
    CGContextSetInterpolationQuality(context, kCGInterpolationHigh);
    CGFloat scale = MIN((CGFloat)side / (CGFloat)CGImageGetWidth(image),
                        (CGFloat)side / (CGFloat)CGImageGetHeight(image));
    CGFloat width = (CGFloat)CGImageGetWidth(image) * scale;
    CGFloat height = (CGFloat)CGImageGetHeight(image) * scale;
    // Bitmap rows start at the top; flip so the icon is not drawn upside down.
    CGContextTranslateCTM(context, 0.0, (CGFloat)side);
    CGContextScaleCTM(context, 1.0, -1.0);
    CGContextDrawImage(context,
                       CGRectMake(((CGFloat)side - width) / 2.0, ((CGFloat)side - height) / 2.0,
                                  width, height),
                       image);
    CFRelease(context);
    *out_rgba_len = side * side * 4;
}

int32_t ledalert_taskbar_resolve_pin(const char *path_c, const char *declared_c, char *out_id,
                                     uintptr_t id_cap, char *out_name, uintptr_t name_cap,
                                     uint8_t *out_rgba, uintptr_t rgba_cap,
                                     uintptr_t *out_rgba_len) {
    if (!path_c || !out_id || !out_name || !out_rgba || !out_rgba_len || !id_cap || !name_cap) {
        return -1;
    }
    *out_rgba_len = 0;
    out_id[0] = 0;
    out_name[0] = 0;
    @autoreleasepool {
        NSString *path = [NSString stringWithUTF8String:path_c];
        if (!path.length) {
            return -1;
        }
        NSBundle *bundle = [NSBundle bundleWithPath:path];
        NSString *identifier = bundle.bundleIdentifier;
        if (![identifier isKindOfClass:NSString.class] || !identifier.length) {
            return 1;  // Missing or not an application bundle: omit, never invent.
        }
        if (declared_c) {
            NSString *declared = [NSString stringWithUTF8String:declared_c];
            if (!declared || ![identifier isEqualToString:declared]) {
                return 1;  // Stale or misidentified record: omit.
            }
        }
        NSString *name = [bundle objectForInfoDictionaryKey:@"CFBundleDisplayName"];
        if (![name isKindOfClass:NSString.class] || name.length == 0) {
            name = [bundle objectForInfoDictionaryKey:@"CFBundleName"];
        }
        if (![name isKindOfClass:NSString.class] || name.length == 0) {
            name = path.lastPathComponent;
        }
        if (!ledalert_write_c_string(identifier, out_id, id_cap) ||
            !ledalert_write_c_string(name, out_name, name_cap)) {
            return 1;
        }
        ledalert_render_icon([NSWorkspace.sharedWorkspace iconForFile:path], out_rgba, rgba_cap,
                             out_rgba_len);
        return 0;
    }
}
