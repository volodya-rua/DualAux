package app.dualaux

import android.Manifest
import android.animation.ValueAnimator
import android.app.*
import android.content.Context
import android.content.Intent
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

private const val MAX = 5
private const val BG = 0xFF050505.toInt()
private const val CARD = 0xFF0D0D0D.toInt()
private const val LINE = 0xFF1C1C1C.toInt()
private const val DIM = 0xFF787878.toInt()
private const val ON = 0xFF4ADE80.toInt()

// A2DP, wired headphones/headset, USB headset/device, BLE headset
private val OUTPUTS = setOf(8, 3, 4, 22, 11, 26)

// Debug builds also list the built-in earpiece/speaker so the flow can be tested on an emulator.
private fun outputs(c: Context): List<AudioDeviceInfo> {
    val debug = c.applicationInfo.flags and android.content.pm.ApplicationInfo.FLAG_DEBUGGABLE != 0
    return (c.getSystemService(Context.AUDIO_SERVICE) as AudioManager)
        .getDevices(AudioManager.GET_DEVICES_OUTPUTS).filter { it.type in OUTPUTS || (debug && it.type in 1..2) }
}

private fun name(d: AudioDeviceInfo): String {
    val kind = when (d.type) { 8 -> "Bluetooth"; 26 -> "Bluetooth LE"; 22, 11 -> "USB"; 1, 2 -> "Built-in"; else -> "Wired" }
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
    private val am by lazy { getSystemService(AUDIO_SERVICE) as AudioManager }
    private val cb = object : AudioDeviceCallback() {
        override fun onAudioDevicesAdded(a: Array<AudioDeviceInfo>) = sync()
        override fun onAudioDevicesRemoved(a: Array<AudioDeviceInfo>) = sync()
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
        requestPermissions(arrayOf(Manifest.permission.RECORD_AUDIO, Manifest.permission.POST_NOTIFICATIONS), 1)

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
        sync()
    }

    override fun onPause() { am.unregisterAudioDeviceCallback(cb); super.onPause() }

    private fun unused() = devs.firstOrNull { d -> slots.none { it.id == d.id } }

    private fun sync() {
        devs = outputs(this)
        if (!on) {
            slots.removeAll { s -> devs.none { it.id == s.id } }
            while (slots.size < minSlots) slots += Slot(unused()?.id ?: break)
        }
        render()
    }

    private fun render() {
        list.removeAllViews()
        for ((i, s) in slots.withIndex()) list.addView(card(i, s), LinearLayout.LayoutParams(MATCH_PARENT, WRAP_CONTENT).apply { bottomMargin = dp(12) })
        if (!on && slots.size < MAX && unused() != null) {
            list.addView(text("+   Add device", 13f, DIM).apply {
                gravity = Gravity.CENTER; background = round(BG, 20)
                setOnClickListener { slots += Slot(unused()!!.id); render() }
            }, LinearLayout.LayoutParams(MATCH_PARENT, dp(48)))
        }
        if (devs.size < minSlots) msg.text = "Connect two headphones to begin" else if (!on) msg.text = ""
        paint(on, animate = false)
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
        startForegroundService(Intent(this, Mirror::class.java)
            .putExtra("code", res).putExtra("data", data).putExtra("ids", slots.map { it.id }.toIntArray()))
        on = true; msg.text = "Playing on ${slots.size} outputs"; render()
        // the service may fail to start (e.g. no mic permission); resync with what it reports
        Handler(Looper.getMainLooper()).postDelayed({ if (on && !Mirror.running) { on = false; msg.text = "Couldn't start. Check permissions."; render() } }, 1200)
    }
}

class Mirror : Service() {
    companion object {
        val vols = FloatArray(MAX) { 1f }
        @Volatile var running = false
    }

    @Volatile private var run = false

    override fun onBind(i: Intent?): IBinder? = null

    @Suppress("DEPRECATION")
    override fun onStartCommand(i: Intent, f: Int, id: Int): Int {
        getSystemService(NotificationManager::class.java)
            .createNotificationChannel(NotificationChannel("m", "Mirroring", NotificationManager.IMPORTANCE_LOW))
        startForeground(1, Notification.Builder(this, "m").setContentTitle("DualAux")
            .setContentText("Mirroring to your outputs").setSmallIcon(android.R.drawable.ic_media_play).build(),
            ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PROJECTION)
        try { start(i) } catch (e: Exception) { running = false; stopSelf() }
        return START_NOT_STICKY
    }

    @Suppress("DEPRECATION")
    private fun start(i: Intent) {
        val proj = (getSystemService(MEDIA_PROJECTION_SERVICE) as MediaProjectionManager)
            .getMediaProjection(i.getIntExtra("code", 0), i.getParcelableExtra("data")!!)!!
        val all = outputs(this)
        val rate = 48000
        val cfg = AudioPlaybackCaptureConfiguration.Builder(proj)
            .addMatchingUsage(AudioAttributes.USAGE_MEDIA).addMatchingUsage(AudioAttributes.USAGE_GAME)
            .addMatchingUsage(AudioAttributes.USAGE_UNKNOWN).excludeUid(Process.myUid()).build()
        val rec = AudioRecord.Builder().setAudioPlaybackCaptureConfig(cfg).setAudioFormat(
            AudioFormat.Builder().setEncoding(AudioFormat.ENCODING_PCM_16BIT).setSampleRate(rate)
                .setChannelMask(AudioFormat.CHANNEL_IN_STEREO).build()).build()
        val out = AudioFormat.Builder().setEncoding(AudioFormat.ENCODING_PCM_16BIT).setSampleRate(rate)
            .setChannelMask(AudioFormat.CHANNEL_OUT_STEREO).build()
        val buf = AudioTrack.getMinBufferSize(rate, AudioFormat.CHANNEL_OUT_STEREO, AudioFormat.ENCODING_PCM_16BIT) * 2
        val tracks = i.getIntArrayExtra("ids")!!.map { dev ->
            AudioTrack.Builder().setAudioAttributes(AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_MEDIA).build())
                .setAudioFormat(out).setBufferSizeInBytes(buf).build().apply {
                    preferredDevice = all.first { it.id == dev }; play()
                }
        }
        run = true; running = true
        // ponytail: one thread writes every track in turn, so a stalled device delays the others; split per track if it glitches
        // ponytail: the original audio still plays on the phone's default route; Android can't mute other apps
        Thread {
            val b = ShortArray(buf / 2)
            rec.startRecording()
            while (run) {
                val n = rec.read(b, 0, b.size)
                if (n > 0) tracks.forEachIndexed { k, t -> t.setVolume(vols[k]); t.write(b, 0, n) }
            }
            rec.release(); tracks.forEach { it.release() }; proj.stop(); running = false
        }.start()
    }

    override fun onDestroy() { run = false }
}
