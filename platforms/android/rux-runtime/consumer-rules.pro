# Rust calls these methods by name over JNI — they must not be renamed or removed.
-keep class dev.rux.runtime.RuxHost { public *; }
-keep class dev.rux.runtime.RuxNative { *; }
