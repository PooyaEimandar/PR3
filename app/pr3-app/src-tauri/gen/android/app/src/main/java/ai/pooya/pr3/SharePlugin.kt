package ai.pooya.pr3

import android.app.Activity
import android.content.ClipData
import android.content.Intent
import android.net.Uri
import androidx.core.content.FileProvider
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin
import java.io.File

@InvokeArg
class ShareArgs {
  /** Files in the app's cache folder, which the manifest's FileProvider serves. */
  lateinit var paths: Array<String>
}

/**
 * Hands an exported task to the share sheet (share.rs): the user picks the app it goes
 * to, and that app gets read access to these files only, through the FileProvider.
 */
@TauriPlugin
class SharePlugin(private val activity: Activity) : Plugin(activity) {
  @Command
  fun share(invoke: Invoke) {
    try {
      val args = invoke.parseArgs(ShareArgs::class.java)
      val authority = "${activity.packageName}.fileprovider"
      val uris = ArrayList<Uri>(args.paths.map { FileProvider.getUriForFile(activity, authority, File(it)) })
      if (uris.isEmpty()) {
        invoke.reject("nothing to share")
        return
      }
      val send = Intent(Intent.ACTION_SEND_MULTIPLE).apply {
        // Markdown and JSON have no narrower type in common.
        type = "*/*"
        putParcelableArrayListExtra(Intent.EXTRA_STREAM, uris)
        // The grant reaches the chosen app through the clip data.
        clipData = ClipData.newRawUri(null, uris[0]).also { clip ->
          uris.drop(1).forEach { clip.addItem(ClipData.Item(it)) }
        }
        addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
      }
      activity.startActivity(Intent.createChooser(send, "Export task"))
      invoke.resolve()
    } catch (e: Exception) {
      invoke.reject(e.message ?: e.toString())
    }
  }
}
