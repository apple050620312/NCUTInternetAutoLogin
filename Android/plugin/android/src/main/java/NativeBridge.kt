package tw.edu.ncut.autologin.mobile

import java.util.concurrent.locks.ReentrantLock

object NativeBridge {
    init { System.loadLibrary("ncut_android") }
    private val operation = ReentrantLock()
    external fun tick(username: String, password: String, login: Boolean): String
    fun run(store: Store, login: Boolean): String {
        operation.lock()
        try {
            return tick(store.username, store.password() ?: "", login)
        } finally { operation.unlock() }
    }
}
