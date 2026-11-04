package ai.pooya.pr3

import android.Manifest
import android.app.Activity
import android.provider.ContactsContract.CommonDataKinds.Email
import app.tauri.PermissionState
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.Permission
import app.tauri.annotation.PermissionCallback
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSArray
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.util.concurrent.Executors

@InvokeArg
class ContactsArgs { var request: Boolean = false }

/** Reads only names and email addresses. Never writes to the device address book. */
@TauriPlugin(permissions = [Permission(strings = [Manifest.permission.READ_CONTACTS], alias = "contacts")])
class ContactsPlugin(private val activity: Activity) : Plugin(activity) {
  private val worker = Executors.newSingleThreadExecutor()

  @Command
  fun read(invoke: Invoke) {
    val args = invoke.parseArgs(ContactsArgs::class.java)
    if (getPermissionState("contacts") == PermissionState.GRANTED) {
      load(invoke)
    } else if (args.request) {
      requestPermissionForAlias("contacts", invoke, "contactsAnswered")
    } else {
      denied(invoke)
    }
  }

  @PermissionCallback
  private fun contactsAnswered(invoke: Invoke) {
    if (getPermissionState("contacts") == PermissionState.GRANTED) load(invoke)
    else denied(invoke)
  }

  private fun denied(invoke: Invoke) {
    invoke.resolve(JSObject().put("permission", "denied").put("contacts", JSArray()))
  }

  private fun load(invoke: Invoke) {
    worker.execute {
      try {
        val rows = JSArray()
        val projection = arrayOf(Email.DISPLAY_NAME_PRIMARY, Email.ADDRESS)
        val cursor = activity.contentResolver.query(Email.CONTENT_URI, projection, null, null, null)
          ?: throw IllegalStateException("The address book is unavailable")
        cursor.use {
          while (it.moveToNext()) {
            rows.put(JSObject().put("name", it.getString(0).orEmpty()).put("address", it.getString(1).orEmpty()))
            if (rows.length() > 20000) throw IllegalStateException("The address book has too many email entries to sync")
          }
        }
        // Permission may have been revoked while the provider was being read.
        if (getPermissionState("contacts") != PermissionState.GRANTED) denied(invoke)
        else invoke.resolve(JSObject().put("permission", "granted").put("contacts", rows))
      } catch (_: SecurityException) {
        denied(invoke)
      } catch (e: Exception) {
        invoke.reject(e.message ?: "Could not read contacts")
      }
    }
  }
}
