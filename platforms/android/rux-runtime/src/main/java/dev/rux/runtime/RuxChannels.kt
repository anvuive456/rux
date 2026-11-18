package dev.rux.runtime

import android.content.Context
import android.os.Handler
import android.os.Looper
import android.util.Log

/**
 * Native API registry (`#[rux::host_api]`): Rust calls [invoke], which runs the implementation
 * registered under the API name (`"<crate>.<Trait>"`).
 *
 * Implementations are registered by the generated `dev.rux.generated.RuxRegistrant` (from
 * `[package.metadata.rux.native]` of the app and its plugins), which [RuxHost.start] runs;
 * or by hand with the generated `<Api>.setUp(impl)`.
 */
object RuxChannels {
    /** Decodes a call's arguments, runs the implementation, answers through [reply]. */
    fun interface Handler {
        fun handle(method: Int, args: RuxReader, reply: RuxReply)
    }

    private val handlers = HashMap<String, Handler>()

    /** Registers (or with `null`, removes) the implementation of [api]. Main thread. */
    fun register(api: String, handler: Handler?) {
        if (handler == null) handlers.remove(api) else handlers[api] = handler
    }

    /** Runs the generated registrant, if the app has native APIs. */
    internal fun registerGenerated(context: Context) {
        try {
            Class.forName("dev.rux.generated.RuxRegistrant")
                .getMethod("register", Context::class.java)
                .invoke(null, context)
        } catch (_: ClassNotFoundException) {
            // No native APIs.
        } catch (e: ReflectiveOperationException) {
            Log.e("rux", "registering native APIs failed", e.cause ?: e)
        }
    }

    /** Called from Rust (JNI) on the main thread. */
    @JvmStatic
    fun invoke(api: String, method: Int, callId: Long, args: ByteArray) {
        val reply = RuxReply(callId)
        val handler = handlers[api]
            ?: return reply.error(RuxError("not_implemented", "no implementation registered for $api"))
        try {
            handler.handle(method, RuxReader(args), reply)
        } catch (e: Throwable) {
            reply.error(e)
        }
    }
}

/** Answers one native API call — exactly once, from any thread. */
class RuxReply internal constructor(private val callId: Long) {
    /** Encodes a successful result with [write]; a thrown exception becomes an error. */
    fun success(write: (RuxWriter) -> Unit) {
        val w = RuxWriter()
        try {
            w.u8(0)
            write(w)
        } catch (e: Throwable) {
            return error(e)
        }
        send(w.bytes())
    }

    /** Completes an async call from a [Result]. */
    fun <T> complete(result: Result<T>, write: (RuxWriter, T) -> Unit) {
        result.fold({ v -> success { w -> write(w, v) } }, { e -> error(e) })
    }

    fun error(e: Throwable) {
        val w = RuxWriter()
        w.u8(1)
        w.string((e as? RuxError)?.code ?: e.javaClass.simpleName)
        w.string(e.message ?: e.toString())
        send(w.bytes())
    }

    fun notImplemented(method: Int) = error(RuxError("not_implemented", "unknown method #$method"))

    private fun send(payload: ByteArray) {
        if (Looper.myLooper() == Looper.getMainLooper()) {
            RuxNative.nativeReply(callId, payload)
        } else {
            main.post { RuxNative.nativeReply(callId, payload) }
        }
    }

    private companion object {
        val main = Handler(Looper.getMainLooper())
    }
}
