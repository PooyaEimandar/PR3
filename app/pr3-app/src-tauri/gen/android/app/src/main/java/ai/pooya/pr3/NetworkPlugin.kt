package ai.pooya.pr3

import android.app.Activity
import android.net.ConnectivityManager
import app.tauri.annotation.Command
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

/**
 * Whether the phone is on a metered network, such as mobile data, so a model download
 * waits for Wi-Fi unless the user allows mobile data in Settings (network.rs). Android
 * calls a network metered when it may cost money, and when there is no network at all.
 */
@TauriPlugin
class NetworkPlugin(private val activity: Activity) : Plugin(activity) {
  @Command
  fun status(invoke: Invoke) {
    val connectivity = activity.getSystemService(ConnectivityManager::class.java)
    val result = JSObject()
    result.put("metered", connectivity?.isActiveNetworkMetered ?: true)
    invoke.resolve(result)
  }
}
