package dev.rux.runtime

import android.content.Context
import android.graphics.Color
import android.graphics.drawable.GradientDrawable
import android.text.Editable
import android.text.TextWatcher
import android.util.TypedValue
import android.view.View
import android.view.ViewGroup
import android.widget.Button
import android.widget.EditText
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView

/**
 * Android implementation of the mutation protocol (`rux_core::Mutation`).
 *
 * Rust calls the public methods below over JNI, always on the main thread.
 * Method names & signatures must match `crates/rux-android/src/lib.rs`.
 */
class RuxHost(private val context: Context, private val container: ViewGroup) {
    private val nodes = HashMap<Long, View>()
    private val watchers = HashMap<Long, TextWatcher>()

    /** true while applying mutations — suppresses synchronously fired events (e.g. setText → TextWatcher). */
    private var applying = false

    fun start() {
        // Native API implementations (re-registered when the Activity is recreated).
        RuxChannels.registerGenerated(context)
        RuxNative.nativeStart(this)
        // `rux run` passes its dev server address to debug builds (hot reload).
        (context as? android.app.Activity)?.intent?.getStringExtra(RuxDevClient.EXTRA)?.let {
            RuxDevClient.connect(context, it)
        }
    }
    fun stop() = RuxNative.nativeStop()

    // --- mutation protocol ---------------------------------------------------

    fun createNode(id: Long, kind: String) {
        nodes[id] = when (kind) {
            "column" -> LinearLayout(context).apply { orientation = LinearLayout.VERTICAL }
            "row" -> LinearLayout(context).apply { orientation = LinearLayout.HORIZONTAL }
            "scroll" -> ScrollView(context)
            "text" -> TextView(context)
            "button" -> Button(context).apply { isAllCaps = false }
            "text_input" -> EditText(context).apply { isSingleLine = true }
            else -> FrameLayout(context) // "view" and unknown kinds
        }
    }

    fun setProp(id: Long, name: String, value: String) = mutate(id) { v ->
        when (name) {
            "text" -> (v as? TextView)?.text = value
            "value" -> (v as? EditText)?.let { if (it.text.toString() != value) it.setText(value) }
            "placeholder" -> (v as? TextView)?.hint = value
            "font_size" -> (v as? TextView)?.setTextSize(TypedValue.COMPLEX_UNIT_SP, value.toFloat())
            "color" -> (v as? TextView)?.setTextColor(parseColor(value))
            "background" -> v.setBackgroundColor(parseColor(value))
            "padding" -> dp(value).let { v.setPadding(it, it, it, it) }
            "spacing" -> (v as? LinearLayout)?.let { setSpacing(it, dp(value)) }
            "enabled" -> v.isEnabled = value == "true"
        }
    }

    fun removeProp(id: Long, name: String) = mutate(id) { v ->
        when (name) {
            "text", "value" -> (v as? TextView)?.text = ""
            "placeholder" -> (v as? TextView)?.hint = null
            "background" -> v.background = null
            "padding" -> v.setPadding(0, 0, 0, 0)
            "spacing" -> (v as? LinearLayout)?.showDividers = LinearLayout.SHOW_DIVIDER_NONE
            "enabled" -> v.isEnabled = true
        }
    }

    fun insertChild(parent: Long, child: Long, index: Int) {
        val p = nodes[parent] as? ViewGroup ?: return
        val c = nodes[child] ?: return
        (c.parent as? ViewGroup)?.removeView(c)
        p.addView(c, index.coerceIn(0, p.childCount))
    }

    fun removeChild(parent: Long, child: Long) {
        val p = nodes[parent] as? ViewGroup ?: return
        nodes[child]?.let { p.removeView(it) }
    }

    fun releaseNode(id: Long) {
        val v = nodes.remove(id) ?: return
        v.setOnClickListener(null)
        watchers.remove(id)?.let { (v as? EditText)?.removeTextChangedListener(it) }
    }

    fun setRoot(id: Long) {
        val root = nodes[id] ?: return
        (root.parent as? ViewGroup)?.removeView(root)
        container.removeAllViews()
        container.addView(
            root,
            ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT),
        )
    }

    fun listen(id: Long, event: String) {
        val v = nodes[id] ?: return
        when (event) {
            "click" -> v.setOnClickListener { dispatch(id, "click", null) }
            "change" -> (v as? EditText)?.let { et ->
                val w = object : TextWatcher {
                    override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) {}
                    override fun onTextChanged(s: CharSequence?, start: Int, before: Int, count: Int) {}
                    override fun afterTextChanged(s: Editable?) = dispatch(id, "change", s?.toString() ?: "")
                }
                watchers.put(id, w)?.let { et.removeTextChangedListener(it) }
                et.addTextChangedListener(w)
            }
        }
    }

    fun unlisten(id: Long, event: String) {
        val v = nodes[id] ?: return
        when (event) {
            "click" -> v.setOnClickListener(null)
            "change" -> watchers.remove(id)?.let { (v as? EditText)?.removeTextChangedListener(it) }
        }
    }

    // --- helpers ------------------------------------------------------------

    private fun dispatch(id: Long, event: String, value: String?) {
        if (applying) return
        applying = true
        try {
            RuxNative.nativeDispatch(id, event, value)
        } finally {
            applying = false
        }
    }

    private inline fun mutate(id: Long, block: (View) -> Unit) {
        val v = nodes[id] ?: return
        val prev = applying
        applying = true
        try {
            block(v)
        } finally {
            applying = prev
        }
    }

    private fun dp(value: String): Int =
        TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, value.toFloat(), context.resources.displayMetrics).toInt()

    /** `#AARRGGBB` → Android color int. */
    private fun parseColor(value: String): Int =
        runCatching { value.removePrefix("#").toLong(16).toInt() }.getOrDefault(Color.MAGENTA)

    private fun setSpacing(layout: LinearLayout, px: Int) {
        layout.dividerDrawable = GradientDrawable().apply {
            setColor(Color.TRANSPARENT)
            setSize(px, px)
        }
        layout.showDividers = LinearLayout.SHOW_DIVIDER_MIDDLE
    }
}
