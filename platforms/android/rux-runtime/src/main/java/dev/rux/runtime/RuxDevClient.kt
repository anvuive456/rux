package dev.rux.runtime

import android.content.Context
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
 */
internal object RuxDevClient {
    /** Intent extra carrying the server address (`host:port`), set by `rux run`. */
    const val EXTRA = "rux.devServer"
    private const val TAG = "rux"

    @Volatile private var started = false

    /** Starts the client once per process (the Activity may be recreated). */
    fun connect(context: Context, address: String) {
        if (started) return
        started = true
        val appContext = context.applicationContext
        Thread({ run(appContext, address) }, "rux-dev").apply { isDaemon = true }.start()
    }

    private fun run(context: Context, address: String) {
        val (host, port) = address.split(':').let { it[0] to it[1].toInt() }
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
        } finally {
            started = false
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
