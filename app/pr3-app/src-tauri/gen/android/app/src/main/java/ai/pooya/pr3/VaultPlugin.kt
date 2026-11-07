package ai.pooya.pr3

import android.app.Activity
import android.content.pm.PackageManager
import android.os.Build
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyPermanentlyInvalidatedException
import android.security.keystore.KeyProperties
import android.security.keystore.StrongBoxUnavailableException
import androidx.biometric.BiometricManager.Authenticators.BIOMETRIC_STRONG
import androidx.biometric.BiometricManager.Authenticators.DEVICE_CREDENTIAL
import androidx.biometric.BiometricPrompt
import androidx.core.content.ContextCompat
import androidx.fragment.app.FragmentActivity
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.math.BigInteger
import android.security.keystore.KeyInfo
import java.security.KeyFactory
import java.security.KeyPairGenerator
import java.security.KeyStore
import java.security.PrivateKey
import java.security.Signature
import java.security.interfaces.ECPublicKey
import java.security.spec.ECGenParameterSpec
import android.util.AtomicFile
import java.io.File
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec
import org.json.JSONObject
import java.util.UUID

@InvokeArg
class SignArgs {
  /** The receipt's signing bytes, in hex. */
  lateinit var message: String
  /** What is being approved, shown on the prompt. */
  lateinit var reason: String
}

@InvokeArg
class StorageKeyArgs {
  lateinit var key: String
}

/**
 * The Android half of pr3-vault (docs Sec. 7.2 item 10). The approval key is a P-256 key
 * in the Android Keystore — StrongBox when the phone has it — that signs only after the
 * user passes BiometricPrompt (a strong biometric, or the screen lock on Android 11+).
 * The private key never leaves the Keystore; Rust gets the public key and signatures.
 */
@TauriPlugin
class VaultPlugin(private val activity: Activity) : Plugin(activity) {
  private val alias = "pr3-approval"
  private val storageAlias = "pr3-memory-wrap-v1"
  private fun storageFile() = AtomicFile(File(activity.noBackupFilesDir, "memory-key-v1"))
  private val storageAad = "PR3 database key v1".toByteArray(Charsets.UTF_8)

  private fun storageAliasOf(record: JSONObject): String {
    require(record.getInt("version") == 1) { "Invalid storage key version" }
    return record.getString("alias").also {
      require(it.matches(Regex("pr3-memory-wrap-v1-[0-9a-f-]{36}"))) { "Invalid storage key alias" }
    }
  }

  /** The Keystore AES key never leaves the system. Only the wrapped SQLCipher key
   * is persisted; it is deliberately independent of biometric approval keys. */
  @Command
  fun storageKey(invoke: Invoke) {
    try {
      val file = storageFile()
      if (!file.baseFile.exists() && !File(file.baseFile.path + ".bak").exists()) {
        invoke.resolve(JSObject().put("key", "")); return
      }
      val record = JSONObject(String(file.readFully(), Charsets.UTF_8))
      val keyAlias = storageAliasOf(record)
      val encoded = record.getString("wrapped")
      require(encoded.length == 122 && encoded.all { it in "0123456789abcdef" }) { "Invalid wrapped key" }
      val wrapped = unhex(encoded)
      require(wrapped.size == 61 && wrapped[0] == 1.toByte()) { "Invalid wrapped storage key" }
      val key = keyStore().getKey(keyAlias, null) as? SecretKey
        ?: throw IllegalStateException("Storage key missing; use the recovery code")
      val cipher = Cipher.getInstance("AES/GCM/NoPadding")
      cipher.init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(128, wrapped.copyOfRange(1, 13)))
      cipher.updateAAD(storageAad)
      val plain = cipher.doFinal(wrapped.copyOfRange(13, wrapped.size))
      try { invoke.resolve(JSObject().put("key", hex(plain))) } finally { plain.fill(0) }
    } catch (e: Exception) {
      invoke.reject("The storage key is unavailable. Unlock the phone and retry, or use the recovery code.")
    }
  }

  @Command
  fun saveStorageKey(invoke: Invoke) {
    try {
      val text = invoke.parseArgs(StorageKeyArgs::class.java).key
      require(text.length == 64 && text.all { it in "0123456789abcdefABCDEF" }) { "Invalid storage key" }
      val file = storageFile()
      val previousAlias = try { storageAliasOf(JSONObject(String(file.readFully(), Charsets.UTF_8))) } catch (_: Exception) { null }
      // Save is called only for first setup or after Rust verifies a recovery code.
      // Use a fresh wrapping key so recovery also works after Keystore invalidation.
      // Retain the old key until the replacement wrapper has been durably read back.
      val keyAlias = "$storageAlias-${UUID.randomUUID()}"
      val key = run {
        val spec = KeyGenParameterSpec.Builder(keyAlias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
          .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
          .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
          .setKeySize(256)
          .setRandomizedEncryptionRequired(true)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) spec.setUnlockedDeviceRequired(true)
        KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore").apply {
          init(spec.build())
        }.generateKey()
      }
      val cipher = Cipher.getInstance("AES/GCM/NoPadding")
      cipher.init(Cipher.ENCRYPT_MODE, key)
      cipher.updateAAD(storageAad)
      val plain = unhex(text)
      val wrapped = try { byteArrayOf(1) + cipher.iv + cipher.doFinal(plain) } finally { plain.fill(0) }
      val record = JSONObject().put("version", 1).put("alias", keyAlias).put("wrapped", hex(wrapped))
        .toString().toByteArray(Charsets.UTF_8)
      val output = file.startWrite()
      try { output.write(record); file.finishWrite(output) }
      catch (e: Exception) { file.failWrite(output); throw e }
      check(file.readFully().contentEquals(record)) { "Storage key write did not persist" }
      if (previousAlias != null) {
        try { keyStore().deleteEntry(previousAlias) } catch (_: Exception) { /* Safe orphan; retry is not needed. */ }
      }
      invoke.resolve()
    } catch (e: Exception) {
      invoke.reject("Cannot save the storage key securely; the database has not been replaced.")
    }
  }

  @Command
  fun authorizeStorageRecovery(invoke: Invoke) {
    activity.runOnUiThread {
      val callback = object : BiometricPrompt.AuthenticationCallback() {
        override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) { invoke.resolve() }
        override fun onAuthenticationError(code: Int, text: CharSequence) { invoke.reject(text.toString()) }
      }
      val info = BiometricPrompt.PromptInfo.Builder()
        .setTitle("Show PR3 recovery code")
        .setDescription("This code unlocks your saved memories and tasks. Keep it private.")
        .setAllowedAuthenticators(androidx.biometric.BiometricManager.Authenticators.BIOMETRIC_WEAK or DEVICE_CREDENTIAL)
      BiometricPrompt(activity as FragmentActivity, ContextCompat.getMainExecutor(activity), callback)
        .authenticate(info.build())
    }
  }

  @Command
  fun publicKey(invoke: Invoke) {
    try {
      val public = publicKeyOrCreate() as ECPublicKey
      val result = JSObject()
      result.put("publicKey", hex(sec1(public)))
      result.put("storage", storage())
      invoke.resolve(result)
    } catch (e: Exception) {
      invoke.reject(e.message ?: e.toString())
    }
  }

  @Command
  fun sign(invoke: Invoke) {
    val args = invoke.parseArgs(SignArgs::class.java)
    val message = unhex(args.message)
    val signature = Signature.getInstance("SHA256withECDSA")
    try {
      signature.initSign(privateKey())
    } catch (e: KeyPermanentlyInvalidatedException) {
      // A new fingerprint or a removed screen lock voids the key. Remove it so the next
      // start makes a new one, which the trusted side then registers.
      keyStore().deleteEntry(alias)
      invoke.reject("the approval key was invalidated by a change to the screen lock or biometrics; restart PR3")
      return
    } catch (e: Exception) {
      invoke.reject(e.message ?: e.toString())
      return
    }

    activity.runOnUiThread {
      val callback = object : BiometricPrompt.AuthenticationCallback() {
        override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) {
          try {
            val unlocked = result.cryptoObject?.signature ?: throw IllegalStateException("no signature")
            unlocked.update(message)
            val reply = JSObject()
            reply.put("signature", hex(unlocked.sign()))
            invoke.resolve(reply)
          } catch (e: Exception) {
            invoke.reject(e.message ?: e.toString())
          }
        }

        override fun onAuthenticationError(code: Int, text: CharSequence) {
          val cancelled = code == BiometricPrompt.ERROR_USER_CANCELED ||
            code == BiometricPrompt.ERROR_NEGATIVE_BUTTON ||
            code == BiometricPrompt.ERROR_CANCELED
          invoke.reject(text.toString(), if (cancelled) "cancelled" else "failed")
        }
      }
      // The description wraps where a subtitle is cut to one line, so the whole
      // reason - recipients first, then the subject - stays on screen.
      val info = BiometricPrompt.PromptInfo.Builder()
        .setTitle("Approve with PR3")
        .setDescription(args.reason)
        .setConfirmationRequired(true)
      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
        info.setAllowedAuthenticators(BIOMETRIC_STRONG or DEVICE_CREDENTIAL)
      } else {
        info.setAllowedAuthenticators(BIOMETRIC_STRONG).setNegativeButtonText("Cancel")
      }
      val prompt = BiometricPrompt(
        activity as FragmentActivity,
        ContextCompat.getMainExecutor(activity),
        callback
      )
      prompt.authenticate(info.build(), BiometricPrompt.CryptoObject(signature))
    }
  }

  private fun keyStore(): KeyStore = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }

  private fun privateKey(): PrivateKey =
    keyStore().getKey(alias, null) as? PrivateKey ?: throw IllegalStateException("no approval key")

  private fun publicKeyOrCreate(): java.security.PublicKey {
    keyStore().getCertificate(alias)?.let { return it.publicKey }
    val strongBox = Build.VERSION.SDK_INT >= Build.VERSION_CODES.P &&
      activity.packageManager.hasSystemFeature(PackageManager.FEATURE_STRONGBOX_KEYSTORE)
    if (strongBox) {
      try {
        return generate(true)
      } catch (e: StrongBoxUnavailableException) {
        // Advertised but unusable for this key: use the phone's TEE instead.
      }
    }
    return generate(false)
  }

  /** Where the key lives: "StrongBox", "TEE" or "software" (emulators). */
  private fun storage(): String {
    val key = privateKey()
    val info = KeyFactory.getInstance(key.algorithm, "AndroidKeyStore")
      .getKeySpec(key, KeyInfo::class.java)
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
      return when (info.securityLevel) {
        KeyProperties.SECURITY_LEVEL_STRONGBOX -> "StrongBox"
        KeyProperties.SECURITY_LEVEL_TRUSTED_ENVIRONMENT -> "TEE"
        else -> "software"
      }
    }
    @Suppress("DEPRECATION")
    return if (info.isInsideSecureHardware) "TEE" else "software"
  }

  /** Throws if the phone has no secure screen lock: without one nothing can be approved. */
  private fun generate(strongBox: Boolean): java.security.PublicKey {
    val spec = KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_SIGN)
      .setAlgorithmParameterSpec(ECGenParameterSpec("secp256r1"))
      .setDigests(KeyProperties.DIGEST_SHA256)
      .setUserAuthenticationRequired(true)
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
      // Every signature needs its own authentication (timeout 0).
      spec.setUserAuthenticationParameters(
        0, KeyProperties.AUTH_BIOMETRIC_STRONG or KeyProperties.AUTH_DEVICE_CREDENTIAL
      )
    } else {
      @Suppress("DEPRECATION")
      spec.setUserAuthenticationValidityDurationSeconds(-1)
    }
    if (strongBox && Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
      spec.setIsStrongBoxBacked(true)
    }
    val generator = KeyPairGenerator.getInstance(KeyProperties.KEY_ALGORITHM_EC, "AndroidKeyStore")
    generator.initialize(spec.build())
    return generator.generateKeyPair().public
  }

  /** The uncompressed SEC1 point: 0x04, then X and Y as 32 bytes each. */
  private fun sec1(key: ECPublicKey): ByteArray {
    fun fixed(n: BigInteger): ByteArray {
      val bytes = n.toByteArray().let { if (it.size > 32) it.copyOfRange(it.size - 32, it.size) else it }
      return ByteArray(32 - bytes.size) + bytes
    }
    return byteArrayOf(4) + fixed(key.w.affineX) + fixed(key.w.affineY)
  }

  private fun hex(bytes: ByteArray): String = bytes.joinToString("") { "%02x".format(it) }

  private fun unhex(text: String): ByteArray =
    ByteArray(text.length / 2) { text.substring(it * 2, it * 2 + 2).toInt(16).toByte() }
}
