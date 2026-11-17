package dev.rux.runtime

/**
 * JNI bridge to the app's Rust library.
 *
 * The `external` functions are implemented by the `rux::main!` macro in the app crate
 * (symbol `Java_dev_rux_runtime_RuxNative_*`).
 */
object RuxNative {
    @Volatile private var loaded = false

    /** Loads `lib<name>.so` — `name` is the `[lib] name` in the app's Cargo.toml. */
    fun load(libName: String) {
        if (loaded) return
        System.loadLibrary(libName)
        loaded = true
    }

    @JvmStatic external fun nativeStart(host: RuxHost)
    @JvmStatic external fun nativeDispatch(id: Long, event: String, value: String?)
    @JvmStatic external fun nativeStop()

    /**
     * Debug builds only: hands the app over to the library build at [path] — hot reload when
     * [keepState], hot restart otherwise. Returns a one-line report (`ok …` / `err …`).
     */
    @JvmStatic external fun nativeReload(path: String, keepState: Boolean): String
}
