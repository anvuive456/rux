package dev.rux.runtime

import android.content.Context
import android.content.pm.ApplicationInfo
import android.os.Handler
import android.os.Looper
import android.util.Log
import java.io.BufferedInputStream
import java.io.ByteArrayOutputStream
import java.io.DataInputStream
import java.io.File
import java.io.IOException
import java.net.Socket
import java.util.concurrent.CountDownLatch

/**
 * Hot reload client (debug builds): connects to the dev server of `rux run` and loads the
 * library builds it sends.
 *
 * Protocol (one connection, line-based headers):
 * - server → app: `reload <size>\n` or `restart <size>\n`, then `<size>` bytes of the library;
 * - app → server: one report line, `ok …` or `err …`.
 *
 * The server address is never taken from the launch Intent (any app can launch the exported
 * activity with any extras, and the client loads code it receives). `rux run` writes it to
 * [ADDRESS_FILE] in the app's private files with `adb shell run-as`, which only adb (or the
 * app itself) can do, and removes it when the session ends. Only debuggable apps connect, and
 * only to a loopback address (`adb reverse`).
 */
internal object RuxDevClient {
    /** File in [Context.getFilesDir] holding the server address (`host:port`). */
    private const val ADDRESS_FILE = "rux-dev-server"
    private const val TAG = "rux"

    @Volatile private var started = false

    /** Starts the client once per process (the Activity may be recreated), if `rux run` asked. */
    fun connect(context: Context) {
        if (context.applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE == 0) return
        if (started) return
        started = true
        val appContext = context.applicationContext
        Thread({
            try {
                address(appContext)?.let { run(appContext, it) }
            } finally {
                started = false
            }
        }, "rux-dev").apply { isDaemon = true }.start()
    }

    /** The dev server address `rux run` left for this launch, if any (loopback only). */
    private fun address(context: Context): Pair<String, Int>? {
        val text = try {
            File(context.filesDir, ADDRESS_FILE).readText().trim()
        } catch (_: IOException) {
            return null // not launched by `rux run` with hot reload
        }
        val host = text.substringBeforeLast(':', "")
        val port = text.substringAfterLast(':', "").toIntOrNull()
        if (host !in LOOPBACK || port == null) {
            Log.w(TAG, "hot reload: ignoring dev server address \"$text\" (not a loopback address)")
            return null
        }
        return host to port
    }

    private val LOOPBACK = setOf("127.0.0.1", "localhost", "::1")

    private fun run(context: Context, server: Pair<String, Int>) {
        val (host, port) = server
        val address = "$host:$port"
        // Builds are written next to each other: a loaded library can't be replaced in place.
        val dir = File(context.codeCacheDir, "rux-hot").apply {
            deleteRecursively()
            mkdirs()
        }
        val main = Handler(Looper.getMainLooper())
        var build = 0
        try {
            Socket(host, port).use { socket ->
                Log.i(TAG, "hot reload: connected to $address")
                val input = DataInputStream(BufferedInputStream(socket.getInputStream()))
                val output = socket.getOutputStream().bufferedWriter()
                while (true) {
                    val header = readLine(input) ?: break
                    val (command, size) = header.split(' ').let { it[0] to it[1].toInt() }
                    val bytes = ByteArray(size).also { input.readFully(it) }
                    val file = File(dir, "build-${++build}.so").apply { writeBytes(bytes) }

                    // Reload on the UI thread, where the app runs.
                    var report = "err not run"
                    val done = CountDownLatch(1)
                    main.post {
                        report = try {
                            RuxNative.nativeReload(file.path, command == "reload")
                        } catch (t: Throwable) {
                            "err ${t.javaClass.simpleName}: ${t.message}"
                        }
                        done.countDown()
                    }
                    done.await()
                    Log.i(TAG, "hot reload: $report")
                    output.write(report.replace('\n', ' ') + "\n")
                    output.flush()
                }
            }
        } catch (e: IOException) {
            Log.w(TAG, "hot reload: disconnected from $address: ${e.message}")
        }
    }

    private fun readLine(input: DataInputStream): String? {
        val line = ByteArrayOutputStream()
        while (true) {
            val b = input.read()
            if (b == -1) return null
            if (b == '\n'.code) return line.toString(Charsets.UTF_8.name())
            line.write(b)
        }
    }
}
