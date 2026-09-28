package tw.edu.ncut.autologin.mobile

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import org.json.JSONArray
import org.json.JSONObject
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

class Store(context: Context) {
    companion object { private val historyLock = Any() }
    private val prefs = context.getSharedPreferences("ncut", Context.MODE_PRIVATE)
    var enabled: Boolean
        get() = prefs.getBoolean("enabled", false)
        set(value) { prefs.edit().putBoolean("enabled", value).apply() }
    val username: String get() = prefs.getString("username", "") ?: ""
    val interval: Long get() = prefs.getLong("interval", 30)
    val status: String get() = prefs.getString("status", "stopped") ?: "stopped"

    private fun key(): SecretKey {
        val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (store.getKey("ncut.credentials.v1", null) as? SecretKey)?.let { return it }
        return KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore").apply {
            init(KeyGenParameterSpec.Builder("ncut.credentials.v1", KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM).setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE).build())
        }.generateKey()
    }
    fun password(): String? = runCatching {
        val stored = prefs.getString("password", null) ?: return null
        val parts = stored.split(':', limit = 2)
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, Base64.decode(parts[0], Base64.NO_WRAP)))
        String(cipher.doFinal(Base64.decode(parts[1], Base64.NO_WRAP)), Charsets.UTF_8)
    }.getOrNull()

    fun save(username: String, password: String?, interval: Long) {
        require(username.isNotBlank() && username.length <= 256) { "請輸入校園帳號" }
        require(interval in 15..3600) { "檢查間隔須介於 15 至 3600 秒" }
        val secret = password?.takeIf { it.isNotEmpty() } ?: if (username == this.username) password() else null
        require(!secret.isNullOrEmpty()) { "請輸入密碼" }
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.ENCRYPT_MODE, key())
        val encrypted = Base64.encodeToString(cipher.iv, Base64.NO_WRAP) + ":" + Base64.encodeToString(cipher.doFinal(secret.toByteArray(Charsets.UTF_8)), Base64.NO_WRAP)
        check(prefs.edit().putString("username", username).putString("password", encrypted).putLong("interval", interval).commit()) { "設定未儲存，請稍後重試" }
    }
    fun record(status: String) = synchronized(historyLock) {
        val history = runCatching { JSONArray(prefs.getString("history", "[]")) }.getOrDefault(JSONArray())
        if (status != this.status) {
            history.put(JSONObject().put("at", System.currentTimeMillis() / 1000).put("status", status))
            while (history.length() > 100) history.remove(0)
        }
        prefs.edit().putString("status", status).putString("history", history.toString()).apply()
    }
    fun snapshot(): JSONObject = JSONObject().put("username", username).put("interval", interval)
        .put("enabled", enabled).put("status", status).put("hasPassword", password() != null)
        .put("history", runCatching { JSONArray(prefs.getString("history", "[]")) }.getOrDefault(JSONArray()))
    fun forget() {
        synchronized(historyLock) { check(prefs.edit().clear().commit()) }
        val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        store.deleteEntry("ncut.credentials.v1")
    }
}
