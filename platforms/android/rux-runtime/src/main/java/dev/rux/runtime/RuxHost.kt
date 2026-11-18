package dev.rux.runtime

import android.content.Context
import android.content.res.ColorStateList
import android.graphics.Color
import android.graphics.drawable.Drawable
import android.graphics.drawable.GradientDrawable
import android.text.Editable
import android.text.TextWatcher
import android.util.TypedValue
import android.view.ContextThemeWrapper
import android.view.View
import android.view.ViewGroup
import android.widget.EditText
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import com.google.android.material.button.MaterialButton
import com.google.android.material.textfield.TextInputEditText
import com.google.android.material.textfield.TextInputLayout
import com.google.android.material.textview.MaterialTextView

/**
 * Android implementation of the mutation protocol (`rux_core::Mutation`).
 *
 * Rust calls the public methods below over JNI, always on the main thread.
 * Method names & signatures must match `crates/rux-android/src/lib.rs`.
 *
 * Views are Material 3 (`MaterialButton`, outlined `TextInputLayout`, ...), styled by the
 * activity's theme: `Theme.Rux` or a theme extending it. If the activity's theme is not a
 * Material one, `Theme.Rux` is applied to rux's views so they still work.
 */
class RuxHost(private val hostContext: Context, private val container: ViewGroup) {
    /** Context views are created with (Material theme guaranteed). */
    private val context: Context = hostContext.withMaterialTheme()
    private val nodes = HashMap<Long, View>()
    private val watchers = HashMap<Long, TextWatcher>()

    /** Values a prop had before rux first set it, restored by `removeProp`. */
    private val defaults = HashMap<Long, HashMap<String, Any?>>()

    /** true while applying mutations — suppresses synchronously fired events (e.g. setText → TextWatcher). */
    private var applying = false

    fun start() {
        // Native API implementations (re-registered when the Activity is recreated).
        RuxChannels.registerGenerated(hostContext)
        RuxNative.nativeStart(this)
        // Hot reload, when launched by `rux run` (debug builds only; see RuxDevClient).
        RuxDevClient.connect(hostContext)
    }
    fun stop() = RuxNative.nativeStop()

    // --- mutation protocol ---------------------------------------------------

    fun createNode(id: Long, kind: String) {
        nodes[id] = when (kind) {
            "column" -> LinearLayout(context).apply { orientation = LinearLayout.VERTICAL }
            "row" -> LinearLayout(context).apply { orientation = LinearLayout.HORIZONTAL }
            "scroll" -> ScrollView(context)
            "text" -> MaterialTextView(context)
            "button" -> MaterialButton(context).apply { isAllCaps = false }
            "text_input" -> TextInputLayout(context).apply {
                // Created with the layout's context: it carries the outlined box's theme overlay.
                addView(TextInputEditText(this.context).apply { isSingleLine = true })
            }
            else -> FrameLayout(context) // "view" and unknown kinds
        }
    }

    fun setProp(id: Long, name: String, value: String) = mutate(id) { v ->
        if (name in RESTORABLE) rememberDefault(id, name, v)
        when (name) {
            "text" -> textOf(v)?.text = value
            "value" -> textOf(v)?.let { if (it.text.toString() != value) it.setText(value) }
            "placeholder" -> when (v) {
                is TextInputLayout -> v.hint = value // floating label
                is TextView -> v.hint = value
            }
            "font_size" -> textOf(v)?.setTextSize(TypedValue.COMPLEX_UNIT_SP, value.toFloat())
            "color" -> textOf(v)?.setTextColor(parseColor(value))
            "background" -> setBackground(v, parseColor(value))
            "padding" -> dp(value).let { v.setPadding(it, it, it, it) }
            "spacing" -> (v as? LinearLayout)?.takeIf { it !is TextInputLayout }?.let { setSpacing(it, dp(value)) }
            "enabled" -> v.isEnabled = value == "true"
        }
    }

    fun removeProp(id: Long, name: String) = mutate(id) { v ->
        when (name) {
            "text", "value" -> textOf(v)?.text = ""
            "placeholder" -> when (v) {
                is TextInputLayout -> v.hint = null
                is TextView -> v.hint = null
            }
            "font_size", "color", "background" -> restoreDefault(id, name, v)
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
        watchers.remove(id)?.let { textOf(v)?.removeTextChangedListener(it) }
        defaults.remove(id)
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
            "change" -> (textOf(v) as? EditText)?.let { et ->
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
            "change" -> watchers.remove(id)?.let { textOf(v)?.removeTextChangedListener(it) }
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

    /** The view showing a node's text: the field inside a `TextInputLayout`, else the view itself. */
    private fun textOf(v: View): TextView? = when (v) {
        is TextInputLayout -> v.editText
        else -> v as? TextView
    }

    /**
     * Background color without losing the view's own look: buttons keep their shape, ripple
     * and pressed / disabled states (only the tint changes); text fields fill their box.
     */
    private fun setBackground(v: View, color: Int) = when (v) {
        is MaterialButton -> v.backgroundTintList = ColorStateList.valueOf(color)
        is TextInputLayout -> v.boxBackgroundColor = color
        else -> v.setBackgroundColor(color)
    }

    private fun rememberDefault(id: Long, name: String, v: View) {
        val saved = defaults.getOrPut(id) { HashMap() }
        if (name in saved) return
        saved[name] = when (name) {
            "font_size" -> textOf(v)?.textSize
            "color" -> textOf(v)?.textColors
            else -> when (v) { // "background"
                is MaterialButton -> v.backgroundTintList
                is TextInputLayout -> v.boxBackgroundColor
                else -> v.background
            }
        }
    }

    private fun restoreDefault(id: Long, name: String, v: View) {
        val saved = defaults[id] ?: return
        if (name !in saved) return
        val value = saved.remove(name)
        when (name) {
            "font_size" -> (value as? Float)?.let { textOf(v)?.setTextSize(TypedValue.COMPLEX_UNIT_PX, it) }
            "color" -> (value as? ColorStateList)?.let { textOf(v)?.setTextColor(it) }
            else -> when (v) { // "background"
                is MaterialButton -> v.backgroundTintList = value as? ColorStateList
                is TextInputLayout -> v.boxBackgroundColor = value as? Int ?: Color.TRANSPARENT
                else -> v.background = value as? Drawable
            }
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

    private companion object {
        /** Props whose theme default `removeProp` restores. */
        val RESTORABLE = setOf("font_size", "color", "background")

        /** `this`, or `this` with `Theme.Rux` applied when its theme is not a Material one. */
        fun Context.withMaterialTheme(): Context {
            val attrs = obtainStyledAttributes(intArrayOf(com.google.android.material.R.attr.isMaterialTheme))
            val material = attrs.getBoolean(0, false)
            attrs.recycle()
            return if (material) this else ContextThemeWrapper(this, R.style.Theme_Rux)
        }
    }
}
