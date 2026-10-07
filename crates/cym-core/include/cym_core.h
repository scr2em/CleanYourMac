// CleanYourMac core C ABI. All strings are UTF-8 JSON and NUL-terminated.
//
// Ownership: every `char *` returned by this library must be released exactly once with
// cym_string_free. Strings passed to callbacks are borrowed and valid only during the call.
#ifndef CYM_CORE_H
#define CYM_CORE_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define CYM_ABI_VERSION 1

typedef struct CymEngine CymEngine;
typedef struct CymScan CymScan;
typedef void (*CymScanCallback)(void *context, const char *event_json);

uint32_t cym_abi_version(void);

// config_json: NULL or {"journalPath": "/optional/history.json"}. Returns NULL on invalid config.
CymEngine *cym_engine_new(const char *config_json);
// Running scans keep their own reference and finish normally.
void cym_engine_free(CymEngine *engine);

// Blocking request {"method": "...", "params": {...}}. Never returns NULL. Response is
// {"ok": true, "result": ...} or {"ok": false, "error": "..."}. Actions may take seconds;
// call off the main thread. Methods: version, modules, projectRoots, normalizeRoots,
// pathContains, isDirectory, normalizedSelection, analytics, execute, history,
// clearHistory, restore, scan; result store: query, rows, position, finding, selection,
// executeSelection, removeResults, loadResults, synthesize, resultCount.
char *cym_call(const CymEngine *engine, const char *request_json);
void cym_string_free(char *string);

// Starts {"modules": [...], "context": {...}, "concurrency": 3, "store": false} on a
// background thread. With "store": true, findings replace those modules' rows in the
// engine's result store and only throttled {"type": "stored"} counts are delivered.
// Events arrive one at a time; the last is always {"type": "finished", "cancelled": bool},
// after which context is never used again. Returns NULL, with no callbacks, if invalid.
CymScan *cym_scan_start(const CymEngine *engine, const char *request_json,
                        CymScanCallback callback, void *context);
// Requests cancellation; the finished event still follows.
void cym_scan_cancel(const CymScan *scan);
// Cancels and releases the handle. Callbacks may continue until the finished event.
void cym_scan_free(CymScan *scan);

#ifdef __cplusplus
}
#endif

#endif
