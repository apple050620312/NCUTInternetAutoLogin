package tw.edu.ncut.autologin.mobile

import android.Manifest
import android.app.Activity
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat
import androidx.appcompat.app.AppCompatActivity
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.util.concurrent.Executors

@InvokeArg
class SaveArgs {
    var username: String = ""
    var password: String? = null
    var interval: Long = 30
}

@TauriPlugin
class NcutPlugin(private val activity: Activity) : Plugin(activity) {
    private val store = Store(activity)
    private val executor = Executors.newSingleThreadExecutor()
    @Volatile private var generation = 0
    override fun onResume(activity: AppCompatActivity) {
        if (store.enabled) {
            try { ContextCompat.startForegroundService(activity, Intent(activity, NcutService::class.java)) }
            catch (_: Exception) { store.enabled = false; store.record("stopped") }
        }
    }
    private fun resolve(invoke: Invoke) { invoke.resolve(JSObject(store.snapshot().toString())) }
    @Command fun snapshot(invoke: Invoke) { resolve(invoke) }
    @Command fun save(invoke: Invoke) {
        try {
            val args = invoke.parseArgs(SaveArgs::class.java)
            store.save(args.username.trim(), args.password, args.interval)
            generation++
            if (store.enabled) ContextCompat.startForegroundService(activity, Intent(activity, NcutService::class.java))
            resolve(invoke)
        } catch (error: IllegalArgumentException) { invoke.reject(error.message ?: "請確認帳號設定") }
        catch (_: Exception) { invoke.reject("設定未儲存，請稍後重試") }
    }
    @Command fun start(invoke: Invoke) {
        if (store.username.isEmpty() || store.password().isNullOrEmpty()) { invoke.reject("請先儲存帳號與密碼"); return }
        if (Build.VERSION.SDK_INT >= 33 && ContextCompat.checkSelfPermission(activity, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) {
            ActivityCompat.requestPermissions(activity, arrayOf(Manifest.permission.POST_NOTIFICATIONS), 204)
            invoke.reject("請允許連線通知後，再啟動自動連線"); return
        }
        try {
            store.enabled = true
            store.record("checking")
            ContextCompat.startForegroundService(activity, Intent(activity, NcutService::class.java))
            resolve(invoke)
        } catch (_: Exception) { store.enabled = false; invoke.reject("無法啟動自動連線，請稍後重試") }
    }
    @Command fun stop(invoke: Invoke) {
        generation++
        store.enabled = false
        activity.stopService(Intent(activity, NcutService::class.java))
        store.record("stopped")
        resolve(invoke)
    }
    @Command fun check(invoke: Invoke) { network(invoke, false) }
    @Command fun connect(invoke: Invoke) { network(invoke, true) }
    private fun network(invoke: Invoke, login: Boolean) {
        val current = generation
        executor.execute {
            try {
                if (current != generation) { resolve(invoke); return@execute }
                store.record("checking")
                val status = NativeBridge.run(store, login)
                if (current == generation) store.record(status)
                resolve(invoke)
            } catch (_: Exception) { if (current == generation) store.record("unstable"); invoke.reject("無法確認連線，請稍後重試") }
        }
    }
    @Command fun forget(invoke: Invoke) {
        generation++
        store.enabled = false
        activity.stopService(Intent(activity, NcutService::class.java))
        try { store.forget(); resolve(invoke) } catch (_: Exception) { invoke.reject("無法移除帳號，請稍後重試") }
    }
}
