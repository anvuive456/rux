package dev.rux.runtime

import java.io.ByteArrayOutputStream
import java.nio.ByteBuffer
import java.nio.ByteOrder

/**
 * Binary wire format of native API calls — must match `rux_native::codec` (Rust):
 * little-endian; strings and lists are prefixed by a u32 length; optionals by a 0/1 byte;
 * enums are their i32 variant index; structs are their fields in order.
 *
 * Used by the code `rux build` generates from `#[rux::host_api]` declarations.
 */
class RuxWriter {
    private val out = ByteArrayOutputStream()
    private val scratch = ByteBuffer.allocate(8).order(ByteOrder.LITTLE_ENDIAN)

    fun bool(v: Boolean) = out.write(if (v) 1 else 0)
    fun u8(v: Int) = out.write(v)
    fun int(v: Int) = put { putInt(v) }
    fun long(v: Long) = put { putLong(v) }
    fun double(v: Double) = put { putDouble(v) }

    fun string(v: String) {
        val bytes = v.toByteArray(Charsets.UTF_8)
        int(bytes.size)
        out.write(bytes)
    }

    fun <T> optional(v: T?, write: (T) -> Unit) {
        if (v == null) {
            u8(0)
        } else {
            u8(1)
            write(v)
        }
    }

    fun <T> list(v: List<T>, write: (T) -> Unit) {
        int(v.size)
        v.forEach(write)
    }

    fun bytes(): ByteArray = out.toByteArray()

    private inline fun put(block: ByteBuffer.() -> Unit) {
        scratch.clear()
        scratch.block()
        out.write(scratch.array(), 0, scratch.position())
    }
}

class RuxReader(bytes: ByteArray) {
    private val buf = ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN)

    fun bool(): Boolean = when (val b = buf.get().toInt()) {
        0 -> false
        1 -> true
        else -> throw RuxError("decode", "invalid bool $b")
    }

    fun int(): Int = buf.getInt()
    fun long(): Long = buf.getLong()
    fun double(): Double = buf.getDouble()

    fun string(): String {
        val bytes = ByteArray(int())
        buf.get(bytes)
        return String(bytes, Charsets.UTF_8)
    }

    fun <T> optional(read: () -> T): T? = when (val tag = buf.get().toInt()) {
        0 -> null
        1 -> read()
        else -> throw RuxError("decode", "invalid optional tag $tag")
    }

    fun <T> list(read: () -> T): List<T> = List(int()) { read() }
}

/** An error returned to Rust as `NativeError { code, message }`. */
class RuxError(val code: String, message: String) : Exception(message)
