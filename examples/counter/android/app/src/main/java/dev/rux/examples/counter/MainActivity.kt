package dev.rux.examples.counter

import android.app.Activity
import android.os.Bundle
import android.widget.FrameLayout
import dev.rux.runtime.RuxHost
import dev.rux.runtime.RuxNative

class MainActivity : Activity() {
    private lateinit var host: RuxHost

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        RuxNative.load("counter") // libcounter.so

        val root = FrameLayout(this).apply { fitsSystemWindows = true }
        setContentView(root)

        host = RuxHost(this, root)
        host.start()
    }

    override fun onDestroy() {
        host.stop()
        super.onDestroy()
    }
}
