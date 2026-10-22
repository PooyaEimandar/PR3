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

@InvokeArg
class SignArgs {
  /** The receipt's signing bytes, in hex. */
  lateinit var message: String
  /** What is being approved, shown on the prompt. */
  lateinit var reason: String
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
      val info = BiometricPrompt.PromptInfo.Builder()
        .setTitle("Approve with PR3")
        .setSubtitle(args.reason)
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
