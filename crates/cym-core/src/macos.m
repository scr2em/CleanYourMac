// Native API adaptation only. Discovery, classification and action policy live in Rust.
#import <AppKit/AppKit.h>
#include <libproc.h>
#include <mach/mach_time.h>
#include <sys/sysctl.h>
#include <stdint.h>
#include <string.h>

typedef struct {
    int32_t pid; uint32_t uid; int32_t parent;
    uint64_t started_sec, started_usec, cpu_nanos, memory;
    uint8_t has_usage;
    char path[4096], cwd[1024], command[16384];
} CYMProcess;

size_t cym_process_struct_size(void) { return sizeof(CYMProcess); }
int cym_processes(int32_t *buffer, int capacity) { return proc_listallpids(buffer, capacity * sizeof(int32_t)); }
int cym_inspect_process(int32_t pid, CYMProcess *out) {
    memset(out, 0, sizeof(*out));
    struct proc_bsdinfo info;
    if (proc_pidinfo(pid, PROC_PIDTBSDINFO, 0, &info, sizeof(info)) != sizeof(info)) return 0;
    if (proc_pidpath(pid, out->path, sizeof(out->path)) <= 0) return 0;
    out->pid = pid; out->uid = info.pbi_uid; out->parent = info.pbi_ppid;
    out->started_sec = info.pbi_start_tvsec; out->started_usec = info.pbi_start_tvusec;
    struct rusage_info_v4 usage;
    if (proc_pid_rusage(pid, RUSAGE_INFO_V4, (rusage_info_t *)&usage) == 0) {
        mach_timebase_info_data_t base; mach_timebase_info(&base);
        uint64_t ticks = usage.ri_user_time + usage.ri_system_time;
        out->cpu_nanos = ticks / base.denom * base.numer + ticks % base.denom * base.numer / base.denom;
        out->memory = usage.ri_phys_footprint; out->has_usage = 1;
    }
    struct proc_vnodepathinfo vnode;
    if (proc_pidinfo(pid, PROC_PIDVNODEPATHINFO, 0, &vnode, sizeof(vnode)) == sizeof(vnode)) strlcpy(out->cwd, vnode.pvi_cdir.vip_path, sizeof(out->cwd));
    int mib[3] = {CTL_KERN, KERN_ARGMAX, 0}, maximum = 0; size_t length = sizeof(maximum);
    if (sysctl(mib, 2, &maximum, &length, NULL, 0) == 0 && maximum > 0 && maximum < 4194304) {
        char *args = malloc(maximum); length = maximum; mib[1] = KERN_PROCARGS2; mib[2] = pid;
        if (args && sysctl(mib, 3, args, &length, NULL, 0) == 0 && length > sizeof(int)) {
            int argc; memcpy(&argc, args, sizeof(argc)); size_t pos = sizeof(argc);
            while (pos < length && args[pos]) pos++;
            while (pos < length && !args[pos]) pos++;
            for (int i = 0; i < argc && pos < length; i++) {
                size_t end = pos; while (end < length && args[end]) end++;
                if (end == length) break;
                if (i) strlcat(out->command, " ", sizeof(out->command));
                strlcat(out->command, args + pos, sizeof(out->command)); pos = end + 1;
            }
        }
        free(args);
    }
    return 1;
}
char *cym_native_apps(void) {
    @autoreleasepool {
        NSMutableArray *rows = [NSMutableArray array];
        for (NSRunningApplication *app in NSWorkspace.sharedWorkspace.runningApplications) {
            [rows addObject:@{@"pid": @(app.processIdentifier), @"bundleID": app.bundleIdentifier ?: @"", @"path": app.bundleURL.path ?: @""}];
        }
        NSData *data = [NSJSONSerialization dataWithJSONObject:rows options:0 error:nil];
        if (!data) return NULL;
        return strndup(data.bytes, data.length);
    }
}
char *cym_native_trash(const char *path, char **failure) {
    @autoreleasepool {
        NSString *string = [[NSString alloc] initWithUTF8String:path];
        if (!string) { *failure = strdup("Invalid UTF-8 path"); return NULL; }
        NSURL *result = nil; NSError *error = nil;
        BOOL success = [NSFileManager.defaultManager trashItemAtURL:[NSURL fileURLWithPath:string] resultingItemURL:&result error:&error];
        if (!success || !result.path) { *failure = strdup((error.localizedDescription ?: @"Trash operation did not return a destination").UTF8String); return NULL; }
        return strdup(result.path.UTF8String);
    }
}
