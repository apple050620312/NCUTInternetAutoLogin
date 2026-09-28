package tw.edu.ncut.autologin.mobile

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Intent
import android.os.IBinder
import java.util.concurrent.Executors
import java.util.concurrent.ScheduledFuture
import java.util.concurrent.TimeUnit

class NcutService : Service() {
    private lateinit var store: Store
    private val executor = Executors.newSingleThreadScheduledExecutor()
    private var job: ScheduledFuture<*>? = null
    @Volatile private var generation = 0
    override fun onCreate() {
        super.onCreate()
        store = Store(this)
        getSystemService(NotificationManager::class.java).createNotificationChannel(NotificationChannel("connection", "校園網路自動連線", NotificationManager.IMPORTANCE_LOW))
    }
    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == "STOP") {
            store.enabled = false
            store.record("stopped")
            stopSelf()
            return START_NOT_STICKY
        }
        if (!store.enabled) { stopSelf(); return START_NOT_STICKY }
        startForeground(204, notification(store.status))
        job?.cancel(false)
        val current = ++generation
        job = executor.scheduleWithFixedDelay({
            if (store.enabled && generation == current) {
                val status = runCatching { NativeBridge.run(store, true) }.getOrDefault("unstable")
                if (store.enabled && generation == current) {
                    store.record(status)
                    getSystemService(NotificationManager::class.java).notify(204, notification(status))
                }
            }
        }, 0, store.interval, TimeUnit.SECONDS)
        return START_STICKY
    }
    private fun notification(status: String): Notification {
        val text = when (status) {
            "online" -> "網路已連線"
            "needs_login", "checking", "authenticating" -> "正在連接校園網路"
            "missing_credentials" -> "請確認帳號設定"
            "login_failed" -> "登入未完成，稍後重試"
            else -> "等待網路連線"
        }
        val launch = packageManager.getLaunchIntentForPackage(packageName) ?: Intent()
        val open = PendingIntent.getActivity(this, 0, launch, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        val stop = PendingIntent.getService(this, 1, Intent(this, NcutService::class.java).setAction("STOP"), PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        return Notification.Builder(this, "connection").setContentTitle("NCUT 校園網路").setContentText(text)
            .setSmallIcon(android.R.drawable.stat_sys_upload_done).setContentIntent(open).setOngoing(true)
            .addAction(Notification.Action.Builder(null, "暫停", stop).build()).build()
    }
    override fun onDestroy() {
        generation++
        job?.cancel(false)
        executor.shutdownNow()
        super.onDestroy()
    }
    override fun onBind(intent: Intent?): IBinder? = null
}
