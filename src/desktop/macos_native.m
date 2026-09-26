// Native metadata only. No notification payload column, audio samples or track metadata.
#import <AppKit/AppKit.h>
#import <CoreAudio/CoreAudio.h>
#import <CoreGraphics/CoreGraphics.h>
#import <sqlite3.h>
#import <math.h>
#import <string.h>
#import <stdbool.h>
#import <stdlib.h>
#import <time.h>

#define LED_NOTIFICATION_LIMIT 4096
#define LED_AUDIO_PROCESS_LIMIT 256
#define LED_APP_BYTES 512

typedef int (*LedNotificationSink)(int64_t, const uint8_t *, uintptr_t, double, void *);
typedef int (*LedApplicationSink)(const uint8_t *, uintptr_t, void *);

static double led_monotonic(void) {
    struct timespec now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    return (double)now.tv_sec + (double)now.tv_nsec / 1000000000.0;
}

static int led_query_expired(void *context) {
    return led_monotonic() >= *(const double *)context;
}

// A malformed/private schema cannot redirect the fixed projection to payloads or functions.
static int led_metadata_authorizer(void *unused, int action, const char *table,
                                   const char *column, const char *database, const char *trigger) {
    (void)unused;
    (void)database;
    (void)trigger;
    if (action == SQLITE_SELECT) return SQLITE_OK;
    if (action != SQLITE_READ || !table || !column) return SQLITE_DENY;
    if (!strcmp(table, "record") && (!strcmp(column, "rec_id") || !strcmp(column, "app_id") ||
                                    !strcmp(column, "uuid") || !strcmp(column, "delivered_date")))
        return SQLITE_OK;
    if (!strcmp(table, "app") && (!strcmp(column, "app_id") || !strcmp(column, "identifier")))
        return SQLITE_OK;
    // SQLite also authorizes row membership with an empty column name.
    if (!strcmp(table, "delivered") && (!strcmp(column, "app_id") || !strcmp(column, "list") ||
                                       !column[0])) return SQLITE_OK;
    return SQLITE_DENY;
}

typedef struct {
    int64_t application;
    uint8_t uuid[16];
    bool resolved;
} LedDeliveredUuid;

static int led_compare_delivery(const void *left, const void *right) {
    const LedDeliveredUuid *a = left, *b = right;
    if (a->application != b->application) return a->application < b->application ? -1 : 1;
    return memcmp(a->uuid, b->uuid, sizeof(a->uuid));
}

// Status: 0 complete, 1 access unavailable, 2 schema unavailable, 3 incomplete/invalid snapshot.
// Production passes NULL for fixture_path. Only isolated native tests supply their own database.
int ledalert_macos_notifications(const char *fixture_path, LedNotificationSink sink, void *context) {
    @autoreleasepool {
        NSString *path = fixture_path ? [NSString stringWithUTF8String:fixture_path] :
            [NSHomeDirectory() stringByAppendingPathComponent:
                @"Library/Group Containers/group.com.apple.usernoted/db2/db"];
        if (!path || !sink) return 1;
        sqlite3 *database = NULL;
        int flags = SQLITE_OPEN_READONLY | SQLITE_OPEN_NOMUTEX | SQLITE_OPEN_NOFOLLOW;
        if (sqlite3_open_v2(path.fileSystemRepresentation, &database, flags, NULL) != SQLITE_OK) {
            if (database) sqlite3_close(database);
            return 1;
        }
        sqlite3_db_config(database, SQLITE_DBCONFIG_DEFENSIVE, 1, NULL);
        sqlite3_db_config(database, SQLITE_DBCONFIG_TRUSTED_SCHEMA, 0, NULL);
        sqlite3_busy_timeout(database, 100);
        sqlite3_limit(database, SQLITE_LIMIT_LENGTH, 1024 * 1024);
        sqlite3_limit(database, SQLITE_LIMIT_SQL_LENGTH, 16384);
        sqlite3_limit(database, SQLITE_LIMIT_VARIABLE_NUMBER, LED_NOTIFICATION_LIMIT);
        double deadline = led_monotonic() + 0.2;
        sqlite3_progress_handler(database, 1000, led_query_expired, &deadline);
        // Both membership and record metadata belong to one read transaction, including WAL rows.
        if (sqlite3_exec(database, "BEGIN", NULL, NULL, NULL) != SQLITE_OK) {
            sqlite3_close(database);
            return 3;
        }
        sqlite3_set_authorizer(database, led_metadata_authorizer, NULL);
        // macOS 27 usernoted stores delivered.list as packed 16-byte UUIDs. These are
        // membership metadata, not record IDs, a property list, or notification payloads.
        LedDeliveredUuid members[LED_NOTIFICATION_LIMIT];
        unsigned member_count = 0, applications = 0, resolved = 0, rows = 0;
        sqlite3_stmt *membership = NULL, *statement = NULL;
        int step, result = 3;
        if (sqlite3_prepare_v2(database,
                "SELECT app_id,list FROM delivered ORDER BY app_id LIMIT 4097",
                -1, &membership, NULL) != SQLITE_OK) {
            result = 2;
            goto finished;
        }
        while ((step = sqlite3_step(membership)) == SQLITE_ROW) {
            if (++applications > LED_NOTIFICATION_LIMIT || led_query_expired(&deadline) ||
                sqlite3_column_type(membership, 0) != SQLITE_INTEGER) goto finished;
            int64_t application = sqlite3_column_int64(membership, 0);
            int type = sqlite3_column_type(membership, 1);
            if (type != SQLITE_BLOB && type != SQLITE_NULL) goto finished;
            int bytes = sqlite3_column_bytes(membership, 1);
            if (bytes < 0 || bytes % 16 || (unsigned)bytes / 16 > LED_NOTIFICATION_LIMIT - member_count)
                goto finished;
            const uint8_t *uuids = sqlite3_column_blob(membership, 1);
            if (bytes && !uuids) goto finished;
            for (int offset = 0; offset < bytes; offset += 16) {
                LedDeliveredUuid *member = &members[member_count++];
                member->application = application;
                memcpy(member->uuid, uuids + offset, sizeof(member->uuid));
                member->resolved = false;
            }
        }
        if (step != SQLITE_DONE) goto finished;
        qsort(members, member_count, sizeof(members[0]), led_compare_delivery);
        for (unsigned i = 1; i < member_count; ++i)
            if (!led_compare_delivery(&members[i - 1], &members[i])) goto finished;

        // One bounded IN query avoids one full record-history scan per UUID on schemas
        // without an index. All values are bound blobs; no persisted bytes become SQL.
        static const char prefix[] =
            "SELECT r.rec_id,a.identifier,r.delivered_date,r.app_id,r.uuid FROM record AS r "
            "LEFT JOIN app AS a ON a.app_id=r.app_id WHERE r.uuid IN(";
        static const char suffix[] = " ORDER BY r.rec_id LIMIT 4097";
        char sql[sizeof(prefix) + sizeof(suffix) + 2 * LED_NOTIFICATION_LIMIT];
        size_t position = sizeof(prefix) - 1;
        memcpy(sql, prefix, position);
        if (!member_count) {
            memcpy(sql + position, "NULL)", 5);
            position += 5;
        } else {
            for (unsigned i = 0; i < member_count; ++i) {
                sql[position++] = '?';
                sql[position++] = ',';
            }
            sql[position - 1] = ')';
        }
        memcpy(sql + position, suffix, sizeof(suffix));
        if (sqlite3_prepare_v2(database, sql, -1, &statement, NULL) != SQLITE_OK) {
            result = 2;
            goto finished;
        }
        for (unsigned i = 0; i < member_count; ++i)
            if (sqlite3_bind_blob(statement, (int)i + 1, members[i].uuid, 16, SQLITE_STATIC) != SQLITE_OK)
                goto finished;
        while ((step = sqlite3_step(statement)) == SQLITE_ROW) {
            if (++rows > LED_NOTIFICATION_LIMIT || led_query_expired(&deadline) ||
                sqlite3_column_type(statement, 0) != SQLITE_INTEGER ||
                sqlite3_column_type(statement, 1) != SQLITE_TEXT ||
                (sqlite3_column_type(statement, 2) != SQLITE_FLOAT &&
                 sqlite3_column_type(statement, 2) != SQLITE_INTEGER) ||
                sqlite3_column_type(statement, 3) != SQLITE_INTEGER ||
                sqlite3_column_type(statement, 4) != SQLITE_BLOB ||
                sqlite3_column_bytes(statement, 4) != 16) goto finished;
            const void *uuid = sqlite3_column_blob(statement, 4);
            if (!uuid) goto finished;
            LedDeliveredUuid key = {.application = sqlite3_column_int64(statement, 3)};
            memcpy(key.uuid, uuid, sizeof(key.uuid));
            LedDeliveredUuid *member = bsearch(&key, members, member_count, sizeof(members[0]),
                                               led_compare_delivery);
            if (!member) continue; // A matching UUID in another app is not this app's delivery.
            if (member->resolved) goto finished; // Ambiguous UUID-to-record mapping.
            member->resolved = true;
            ++resolved;
            int bytes = sqlite3_column_bytes(statement, 1);
            const unsigned char *application = sqlite3_column_text(statement, 1);
            double delivered = sqlite3_column_double(statement, 2);
            if (!application || bytes < 1 || bytes > LED_APP_BYTES || !isfinite(delivered) ||
                !sink(sqlite3_column_int64(statement, 0), application, (uintptr_t)bytes,
                      delivered, context)) goto finished;
        }
        if (step == SQLITE_DONE && resolved == member_count && !led_query_expired(&deadline))
            result = 0;
    finished:
        sqlite3_finalize(membership);
        sqlite3_finalize(statement);
        sqlite3_close(database);
        return result;
    }
}

// Unknown keys or session absence fail closed. No distributed notification registration required.
int ledalert_macos_lock(void) {
    @autoreleasepool {
        NSDictionary *session = CFBridgingRelease(CGSessionCopyCurrentDictionary());
        if (![session isKindOfClass:NSDictionary.class]) return -1;
        id console = session[(__bridge NSString *)kCGSessionOnConsoleKey];
        id complete = session[(__bridge NSString *)kCGSessionLoginDoneKey];
        id locked = session[@"CGSSessionScreenIsLocked"];
        if (![console isKindOfClass:NSNumber.class] || ![complete isKindOfClass:NSNumber.class])
            return -1;
        if (![console boolValue] || ![complete boolValue]) return 1;
        // Unlocked sessions omit CGSSessionScreenIsLocked; a present value must be boolean-like.
        if (!locked) return 0;
        if (![locked isKindOfClass:NSNumber.class]) return -1;
        return [locked boolValue] ? 1 : 0;
    }
}

static OSStatus led_audio_property(AudioObjectID object, AudioObjectPropertySelector selector,
                                   UInt32 *size, void *output) {
    AudioObjectPropertyAddress address = { selector, kAudioObjectPropertyScopeGlobal,
                                           kAudioObjectPropertyElementMain };
    return AudioObjectGetPropertyData(object, &address, 0, NULL, size, output);
}

// Audio activity, not a private MediaRemote entitlement or a claim about silent video playback.
int ledalert_macos_audio(LedApplicationSink sink, void *context) {
    @autoreleasepool {
        AudioObjectPropertyAddress list = { kAudioHardwarePropertyProcessObjectList,
            kAudioObjectPropertyScopeGlobal, kAudioObjectPropertyElementMain };
        UInt32 bytes = 0;
        if (AudioObjectGetPropertyDataSize(kAudioObjectSystemObject, &list, 0, NULL, &bytes) != noErr ||
            bytes > LED_AUDIO_PROCESS_LIMIT * sizeof(AudioObjectID) || bytes % sizeof(AudioObjectID))
            return -1;
        AudioObjectID processes[LED_AUDIO_PROCESS_LIMIT];
        if (bytes && AudioObjectGetPropertyData(kAudioObjectSystemObject, &list, 0, NULL,
                                               &bytes, processes) != noErr) return -1;
        if (bytes > sizeof(processes) || bytes % sizeof(AudioObjectID)) return -1;
        for (unsigned index = 0; index < bytes / sizeof(AudioObjectID); index++) {
            UInt32 running = 0;
            UInt32 size = sizeof(running);
            if (led_audio_property(processes[index], kAudioProcessPropertyIsRunningOutput,
                                   &size, &running) != noErr || size != sizeof(running)) return -1;
            if (!running) continue;
            CFStringRef identifier = NULL;
            size = sizeof(identifier);
            if (led_audio_property(processes[index], kAudioProcessPropertyBundleID,
                                   &size, &identifier) != noErr) return -1;
            NSString *application = CFBridgingRelease(identifier);
            if (![application isKindOfClass:NSString.class]) return -1;
            if (application.length == 0) continue; // Unidentified CLI/system audio is not an app.
            if (application.length > LED_APP_BYTES) return -1;
            char utf8[LED_APP_BYTES + 1];
            if (![application getCString:utf8 maxLength:sizeof(utf8) encoding:NSUTF8StringEncoding] ||
                [application rangeOfCharacterFromSet:NSCharacterSet.controlCharacterSet].location != NSNotFound)
                return -1;
            if (!sink((const uint8_t *)utf8, strlen(utf8), context)) return -1;
        }
        return 0;
    }
}

int ledalert_macos_open_notification_settings(void) {
    @autoreleasepool {
        if (![NSThread isMainThread]) return -1;
        // This opens a pane. It neither grants permission nor alters a preference.
        return [NSWorkspace.sharedWorkspace openURL:
            [NSURL URLWithString:@"x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles"]]
            ? 0 : -1;
    }
}
