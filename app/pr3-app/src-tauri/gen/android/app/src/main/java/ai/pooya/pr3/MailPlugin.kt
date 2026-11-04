package ai.pooya.pr3

import android.app.Activity
import android.content.ActivityNotFoundException
import android.content.Intent
import android.net.Uri
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin

@InvokeArg
class MailArgs { lateinit var uri: String }

@TauriPlugin
class MailPlugin(private val activity: Activity) : Plugin(activity) {
  @Command
  fun open(invoke: Invoke) {
    val args = invoke.parseArgs(MailArgs::class.java)
    val uri = Uri.parse(args.uri)
    if (uri.scheme != "mailto") { invoke.reject("Invalid email draft"); return }
    activity.runOnUiThread {
      try {
        // SENDTO + mailto is restricted to mail apps. The system uses the user's
        // default, or asks them to choose. This never invokes a send API.
        activity.startActivity(Intent(Intent.ACTION_SENDTO, uri))
        invoke.resolve()
      } catch (_: ActivityNotFoundException) {
        invoke.reject("Install and configure an email app, then try again.")
      } catch (e: Exception) {
        invoke.reject(e.message ?: "Could not open the email app")
      }
    }
  }
}
