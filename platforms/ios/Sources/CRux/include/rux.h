// ABI between the iOS host (Swift) and the Rust core.
// Must be kept in sync with `crates/rux-ios/src/lib.rs` (struct RuxHostVTable).

#ifndef RUX_H
#define RUX_H

#include <stdint.h>

#pragma clang assume_nonnull begin

/// Callbacks provided by Swift; Rust invokes them on the main thread.
/// Strings are NUL-terminated UTF-8 and only valid for the duration of the call.
typedef struct RuxHostVTable {
    void *ctx;
    void (*create_node)(void *ctx, uint64_t id, const char *kind);
    void (*set_prop)(void *ctx, uint64_t id, const char *name, const char *value);
    void (*remove_prop)(void *ctx, uint64_t id, const char *name);
    void (*insert_child)(void *ctx, uint64_t parent, uint64_t child, uint32_t index);
    void (*remove_child)(void *ctx, uint64_t parent, uint64_t child);
    void (*release_node)(void *ctx, uint64_t id);
    void (*set_root)(void *ctx, uint64_t id);
    void (*listen)(void *ctx, uint64_t id, const char *event);
    void (*unlisten)(void *ctx, uint64_t id, const char *event);
} RuxHostVTable;

/// Starts the Rust app and performs the first render.
void rux_start(RuxHostVTable vtable);

/// Sends an event from UIKit to Rust. `value` may be NULL.
void rux_dispatch(uint64_t id, const char *event, const char *_Nullable value);

/// Tears down the Rust app.
void rux_stop(void);

#pragma clang assume_nonnull end

#endif
