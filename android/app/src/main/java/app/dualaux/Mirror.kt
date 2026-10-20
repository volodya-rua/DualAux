package app.dualaux

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Intent
import android.content.pm.ServiceInfo
import android.media.*
import android.media.projection.MediaProjectionManager
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.os.Process
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.TimeUnit

/** Captures the phone's media audio and plays it to every chosen output. */
class Mirror : Service() {
    companion object {
        val vols = FloatArray(MAX) { 1f }

        /** Per output: 0 starting, 1 playing on the chosen device, 2 Android routed it elsewhere, 3 failed or disconnected. */
        val state = IntArray(MAX)
        @Volatile var running = false
    }

    @Volatile private var run = false
    private val handler = Handler(Looper.getMainLooper())

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
        val ids = i.getIntArrayExtra("ids")!!
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

        // One track per output. A device that can't be opened is marked failed; the others keep going.
        val tracks = ids.mapIndexed { k, dev ->
            try {
                AudioTrack.Builder().setAudioAttributes(AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_MEDIA).build())
                    .setAudioFormat(out).setBufferSizeInBytes(buf).build().apply {
                        preferredDevice = all.first { it.id == dev }; play()
                    }
            } catch (e: Exception) { state[k] = 3; null }
        }
        check(tracks.any { it != null }) { "no output could be opened" }
        run = true; running = true

        // One queue and writer thread per output, so a slow Bluetooth device only stalls itself.
        val queues = ids.map { ArrayBlockingQueue<ShortArray>(6) }
        tracks.forEachIndexed { k, t ->
            if (t == null) return@forEachIndexed
            Thread {
                while (run) {
                    val b = queues[k].poll(200, TimeUnit.MILLISECONDS) ?: continue
                    t.setVolume(vols[k])
                    if (t.write(b, 0, b.size) < 0) { state[k] = 3; break } // e.g. device disconnected
                }
                t.release()
            }.start()
        }

        // Android may accept the track but route it to a different device; say so instead of failing silently.
        val verify = object : Runnable {
            override fun run() {
                if (!this@Mirror.run) return
                tracks.forEachIndexed { k, t ->
                    if (t != null && state[k] != 3) state[k] = try { if (t.routedDevice?.id == ids[k]) 1 else 2 } catch (e: Exception) { 3 }
                }
                handler.postDelayed(this, 1500)
            }
        }
        handler.postDelayed(verify, 700)

        // ponytail: the original audio still plays on the phone's default route; Android can't mute other apps
        Thread {
            val b = ShortArray(buf / 2)
            rec.startRecording()
            while (run) {
                val n = rec.read(b, 0, b.size)
                if (n <= 0) continue
                val c = b.copyOf(n) // shared read-only by every writer thread
                for (q in queues) if (!q.offer(c)) { q.poll(); q.offer(c) } // drop the oldest rather than build latency
            }
            rec.release(); proj.stop(); running = false
        }.start()
    }

    override fun onDestroy() { run = false; handler.removeCallbacksAndMessages(null) }
}
