package app.dualaux

import android.Manifest
import android.animation.ValueAnimator
import android.app.*
import android.bluetooth.BluetoothA2dp
import android.bluetooth.BluetoothManager
import android.bluetooth.BluetoothProfile
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.provider.Settings
import android.content.pm.ServiceInfo
import android.content.res.ColorStateList
import android.graphics.Color
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.media.*
import android.media.projection.MediaProjectionManager
import android.os.*
import android.view.Gravity
import android.view.View
import android.view.ViewGroup.LayoutParams.MATCH_PARENT
import android.view.ViewGroup.LayoutParams.WRAP_CONTENT
import android.widget.*

const val MAX = 5
private const val BG = 0xFF050505.toInt()
private const val CARD = 0xFF0D0D0D.toInt()
private const val LINE = 0xFF1C1C1C.toInt()
private const val DIM = 0xFF787878.toInt()
private const val ON = 0xFF4ADE80.toInt()
private const val WARN = 0xFFE0A030.toInt()

// A2DP, wired headphones/headset, USB headset/device, hearing aid, BLE headset/speaker
private val OUTPUTS = setOf(8, 3, 4, 22, 11, 23, 26, 27)

// Debug builds also list the built-in earpiece/speaker so the flow can be tested on an emulator.
fun outputs(c: Context): List<AudioDeviceInfo> {
    val debug = c.applicationInfo.flags and android.content.pm.ApplicationInfo.FLAG_DEBUGGABLE != 0
    return (c.getSystemService(Context.AUDIO_SERVICE) as AudioManager)
        .getDevices(AudioManager.GET_DEVICES_OUTPUTS).filter { it.type in OUTPUTS || (debug && it.type in 1..2) }
}

private fun name(d: AudioDeviceInfo): String {
    val kind = when (d.type) { 8 -> "Bluetooth"; 26, 27 -> "Bluetooth LE"; 23 -> "Hearing aid"; 22, 11 -> "USB"; 1, 2 -> "Built-in"; else -> "Wired" }
    return "${d.productName} · $kind"
}

class MainActivity : Activity() {
    private class Slot(var id: Int, var vol: Float = 1f)

    private val slots = ArrayList<Slot>()
    private var devs = emptyList<AudioDeviceInfo>()
    private var on = false
    // Debug builds (emulator) can mirror with one device so the capture path can be tested.
    private val minSlots by lazy { if (applicationInfo.flags and android.content.pm.ApplicationInfo.FLAG_DEBUGGABLE != 0) 1 else 2 }
    private lateinit var list: LinearLayout
    private lateinit var btn: TextView
    private lateinit var status: TextView
    private lateinit var dot: View
    private lateinit var msg: TextView
    private val states = ArrayList<TextView>()  // one per card: whether Android really routed audio to that device
    private val handler = Handler(Looper.getMainLooper())
    private val poll = object : Runnable {
        override fun run() { showStates(); if (on) handler.postDelayed(this, 500) }
    }
    private val am by lazy { getSystemService(AUDIO_SERVICE) as AudioManager }
    private val cb = object : AudioDeviceCallback() {
        override fun onAudioDevicesAdded(a: Array<AudioDeviceInfo>) = sync()
        override fun onAudioDevicesRemoved(a: Array<AudioDeviceInfo>) = sync()
    }

    // Android connects several Bluetooth headphones but hands apps audio for only the one "active" headset.
    // We read the connected list so the others can be shown and explained instead of silently missing.
    private var a2dp: BluetoothProfile? = null
    private var idle = emptyList<String>()
    private val bt by lazy { getSystemService(BluetoothManager::class.java)?.adapter }
    private val btChanged = object : BroadcastReceiver() {
        override fun onReceive(c: Context, i: Intent) = sync()
    }

    private fun dp(x: Int) = (x * resources.displayMetrics.density).toInt()
    private fun round(fill: Int, r: Int, line: Int = LINE) = GradientDrawable().apply {
        setColor(fill); cornerRadius = dp(r).toFloat(); setStroke(dp(1), line)
    }
    private fun text(t: String, size: Float, color: Int, bold: Boolean = false) = TextView(this).apply {
        text = t; textSize = size; setTextColor(color); if (bold) typeface = Typeface.DEFAULT_BOLD
    }

    override fun onCreate(s: Bundle?) {
        super.onCreate(s)
        window.statusBarColor = BG; window.navigationBarColor = BG
        val perms = arrayListOf(Manifest.permission.RECORD_AUDIO, Manifest.permission.POST_NOTIFICATIONS)
        if (Build.VERSION.SDK_INT >= 31) perms += Manifest.permission.BLUETOOTH_CONNECT
        requestPermissions(perms.toTypedArray(), 1)
        bt?.getProfileProxy(this, object : BluetoothProfile.ServiceListener {
            override fun onServiceConnected(p: Int, proxy: BluetoothProfile) { a2dp = proxy; sync() }
            override fun onServiceDisconnected(p: Int) { a2dp = null }
        }, BluetoothProfile.A2DP)

        dot = View(this)
        status = text("Idle", 12f, DIM)
        val head = LinearLayout(this).apply {
            gravity = Gravity.CENTER_VERTICAL
            addView(text("DualAux", 22f, Color.WHITE, true), LinearLayout.LayoutParams(0, WRAP_CONTENT, 1f))
            addView(dot, LinearLayout.LayoutParams(dp(8), dp(8)).apply { rightMargin = dp(8) })
            addView(status)
        }
        list = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
        msg = text("", 12f, DIM).apply { gravity = Gravity.CENTER }
        btn = text("Start", 15f, Color.BLACK, true).apply {
            gravity = Gravity.CENTER; setOnClickListener { toggle() }
        }
        val scroll = ScrollView(this).apply { isVerticalScrollBarEnabled = false; addView(list) }
        setContentView(LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL; setBackgroundColor(BG)
            setPadding(dp(22), dp(56), dp(22), dp(24))
            addView(head, LinearLayout.LayoutParams(MATCH_PARENT, WRAP_CONTENT).apply { bottomMargin = dp(18) })
            addView(scroll, LinearLayout.LayoutParams(MATCH_PARENT, 0, 1f))
            addView(msg, LinearLayout.LayoutParams(MATCH_PARENT, WRAP_CONTENT).apply { bottomMargin = dp(8) })
            addView(btn, LinearLayout.LayoutParams(MATCH_PARENT, dp(52)))
        })
    }

    override fun onResume() {
        super.onResume()
        on = Mirror.running
        am.registerAudioDeviceCallback(cb, null)
        val f = IntentFilter(BluetoothA2dp.ACTION_CONNECTION_STATE_CHANGED)
        if (Build.VERSION.SDK_INT >= 33) registerReceiver(btChanged, f, RECEIVER_NOT_EXPORTED) else registerReceiver(btChanged, f)
        sync()
    }

    override fun onPause() { am.unregisterAudioDeviceCallback(cb); unregisterReceiver(btChanged); super.onPause() }

    override fun onDestroy() { a2dp?.let { bt?.closeProfileProxy(BluetoothProfile.A2DP, it) }; super.onDestroy() }

    override fun onRequestPermissionsResult(req: Int, perms: Array<String>, res: IntArray) = sync()

    private fun unused() = devs.firstOrNull { d -> slots.none { it.id == d.id } }

    private fun sync() {
        devs = outputs(this)
        // Compare by the last two address bytes: Android may mask the rest of a Bluetooth address.
        val usable = devs.map { it.address.takeLast(5).uppercase() }.toSet()
        idle = try {
            a2dp?.connectedDevices.orEmpty().filter { it.address.takeLast(5).uppercase() !in usable }.map { it.name ?: it.address }
        } catch (e: SecurityException) { emptyList() } // Bluetooth permission not granted
        if (!on) {
            slots.removeAll { s -> devs.none { it.id == s.id } }
            while (slots.size < minSlots) slots += Slot(unused()?.id ?: break)
        }
        render()
    }

    private fun showStates() {
        val n = slots.size
        val ok = (0 until n).count { Mirror.state[it] == 1 }
        for ((i, t) in states.withIndex()) {
            val (txt, col) = when (if (on) Mirror.state[i] else -1) {
                0 -> "Starting…" to DIM
                1 -> "Playing" to ON
                2 -> "Android didn't route audio here. Reconnect this device, or see Limits in the README" to 0xFFE0A030.toInt()
                3 -> "Disconnected or failed" to 0xFFE05050.toInt()
                else -> "" to DIM
            }
            t.text = txt; t.setTextColor(col); t.visibility = if (txt.isEmpty()) View.GONE else View.VISIBLE
        }
        if (on) msg.text = when {
            ok == n -> "Playing on $n outputs"
            (0 until n).any { Mirror.state[it] == 0 } -> "Starting…"
            else -> "Playing on $ok of $n outputs"
        }
    }

    private fun render() {
        list.removeAllViews(); states.clear()
        for ((i, s) in slots.withIndex()) list.addView(card(i, s), LinearLayout.LayoutParams(MATCH_PARENT, WRAP_CONTENT).apply { bottomMargin = dp(12) })
        if (!on && slots.size < MAX && unused() != null) {
            list.addView(text("+   Add device", 13f, DIM).apply {
                gravity = Gravity.CENTER; background = round(BG, 20)
                setOnClickListener { slots += Slot(unused()!!.id); render() }
            }, LinearLayout.LayoutParams(MATCH_PARENT, dp(48)).apply { bottomMargin = dp(12) })
        }
        if (idle.isNotEmpty()) list.addView(idleNote(), LinearLayout.LayoutParams(MATCH_PARENT, WRAP_CONTENT).apply { bottomMargin = dp(12) })
        if (!on) list.addView(text("Connect headphones in Bluetooth settings  ›", 13f, DIM).apply {
            gravity = Gravity.CENTER; setPadding(0, dp(10), 0, dp(10))
            setOnClickListener { startActivity(Intent(Settings.ACTION_BLUETOOTH_SETTINGS)) }
        }, LinearLayout.LayoutParams(MATCH_PARENT, WRAP_CONTENT))
        if (devs.size < minSlots) msg.text = if (idle.isNotEmpty()) "Add wired or USB headphones as the second output" else "Connect two headphones to begin"
        else if (!on) msg.text = ""
        paint(on, animate = false)
        handler.removeCallbacks(poll); if (on) handler.post(poll)
    }

    private fun idleNote() = LinearLayout(this).apply {
        orientation = LinearLayout.VERTICAL; setPadding(dp(18), dp(16), dp(18), dp(16))
        background = round(CARD, 20, 0xFF3A2E14.toInt())
        addView(text("CONNECTED, BUT ANDROID WON'T SHARE IT", 10.5f, WARN))
        addView(text(idle.joinToString("\n"), 14f, Color.WHITE), LinearLayout.LayoutParams(MATCH_PARENT, WRAP_CONTENT).apply { topMargin = dp(8) })
        addView(text("Android plays to only one Bluetooth headset at a time and keeps the others connected but silent, " +
            "for every app. On this phone, add wired or USB headphones for more outputs. " +
            "For up to 5 Bluetooth headphones, use DualAux on a Windows PC.", 12f, DIM),
            LinearLayout.LayoutParams(MATCH_PARENT, WRAP_CONTENT).apply { topMargin = dp(8) })
    }

    private fun card(i: Int, s: Slot): View {
        val dev = devs.firstOrNull { it.id == s.id }
        val c = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL; setPadding(dp(18), dp(16), dp(18), dp(16))
            background = round(CARD, 20, if (on) 0xFF1E402C.toInt() else LINE)
        }
        val head = LinearLayout(this).apply { gravity = Gravity.CENTER_VERTICAL }
        head.addView(text("OUTPUT ${i + 1}", 10.5f, DIM), LinearLayout.LayoutParams(0, WRAP_CONTENT, 1f))
        if (!on && slots.size > minSlots) head.addView(text("×", 20f, DIM).apply {
            setPadding(dp(8), 0, 0, 0); setOnClickListener { slots.remove(s); render() }
        })
        c.addView(head)

        c.addView(text((dev?.let(::name) ?: "None") + "   ▾", 14f, Color.WHITE).apply {
            setPadding(dp(14), dp(12), dp(14), dp(12)); background = round(Color.BLACK, 12)
            if (!on) setOnClickListener { v ->
                PopupMenu(this@MainActivity, v).apply {
                    devs.filter { d -> d.id == s.id || slots.none { it.id == d.id } }.forEach { menu.add(0, it.id, 0, name(it)) }
                    setOnMenuItemClickListener { s.id = it.itemId; render(); true }
                }.show()
            }
        }, LinearLayout.LayoutParams(MATCH_PARENT, WRAP_CONTENT).apply { topMargin = dp(8) })

        val pct = text("${(s.vol * 100).toInt()}%", 12f, DIM)
        val seek = SeekBar(this).apply {
            max = 100; progress = (s.vol * 100).toInt()
            progressTintList = ColorStateList.valueOf(Color.WHITE); thumbTintList = ColorStateList.valueOf(Color.WHITE)
            progressBackgroundTintList = ColorStateList.valueOf(0xFF2A2A2A.toInt())
            setOnSeekBarChangeListener(object : SeekBar.OnSeekBarChangeListener {
                override fun onProgressChanged(b: SeekBar, p: Int, user: Boolean) {
                    s.vol = p / 100f; pct.text = "$p%"; Mirror.vols[i] = s.vol
                }
                override fun onStartTrackingTouch(b: SeekBar) {}
                override fun onStopTrackingTouch(b: SeekBar) {}
            })
        }
        c.addView(LinearLayout(this).apply {
            gravity = Gravity.CENTER_VERTICAL
            addView(seek, LinearLayout.LayoutParams(0, WRAP_CONTENT, 1f)); addView(pct)
        }, LinearLayout.LayoutParams(MATCH_PARENT, WRAP_CONTENT).apply { topMargin = dp(6) })
        val st = text("", 12f, DIM).apply { visibility = View.GONE }
        states += st
        c.addView(st, LinearLayout.LayoutParams(MATCH_PARENT, WRAP_CONTENT).apply { topMargin = dp(8) })
        return c
    }

    private fun paint(live: Boolean, animate: Boolean = true) {
        val to = if (live) 1f else 0f
        fun apply(t: Float) {
            fun mix(a: Int, b: Int) = android.animation.ArgbEvaluator().evaluate(t, a, b) as Int
            btn.background = round(mix(Color.WHITE, BG), 26, mix(Color.WHITE, ON))
            btn.setTextColor(mix(Color.BLACK, ON))
            dot.background = round(mix(0xFF343434.toInt(), ON), 4, mix(0xFF343434.toInt(), ON))
            status.setTextColor(mix(DIM, ON))
        }
        btn.text = if (live) "Stop" else "Start"; status.text = if (live) "Live" else "Idle"
        if (!animate) return apply(to)
        ValueAnimator.ofFloat(1f - to, to).apply { duration = 250; addUpdateListener { apply(it.animatedValue as Float) } }.start()
    }

    private fun toggle() {
        if (on) { stopService(Intent(this, Mirror::class.java)); on = false; msg.text = ""; render(); return }
        if (devs.size < minSlots || slots.size < minSlots) return
        msg.text = "Choose \"Entire screen\" in the next prompt to capture all audio"
        startActivityForResult((getSystemService(MEDIA_PROJECTION_SERVICE) as MediaProjectionManager).createScreenCaptureIntent(), 7)
    }

    @Deprecated("fine for one result")
    override fun onActivityResult(req: Int, res: Int, data: Intent?) {
        if (req != 7 || res != RESULT_OK || data == null) return
        for ((i, s) in slots.withIndex()) Mirror.vols[i] = s.vol
        Mirror.running = true  // onResume runs right after this and must not reset the UI to Idle
        Mirror.state.fill(0)
        startForegroundService(Intent(this, Mirror::class.java)
            .putExtra("code", res).putExtra("data", data).putExtra("ids", slots.map { it.id }.toIntArray()))
        on = true; msg.text = "Starting…"; render()
        // the service may fail to start (e.g. no mic permission); resync with what it reports
        Handler(Looper.getMainLooper()).postDelayed({ if (on && !Mirror.running) { on = false; msg.text = "Couldn't start. Check permissions."; render() } }, 1200)
    }
}

