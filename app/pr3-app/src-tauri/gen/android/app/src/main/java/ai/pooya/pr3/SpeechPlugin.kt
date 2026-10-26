package ai.pooya.pr3

import android.Manifest
import android.app.Activity
import android.content.Intent
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.speech.RecognitionListener
import android.speech.RecognitionSupport
import android.speech.RecognitionSupportCallback
import android.speech.RecognizerIntent
import android.speech.SpeechRecognizer
import android.speech.tts.TextToSpeech
import app.tauri.PermissionState
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.Permission
import app.tauri.annotation.PermissionCallback
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Channel
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSArray
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import androidx.core.content.ContextCompat
import java.util.Locale

@InvokeArg
class StartArgs {
  /** Receives {"partial": text} while the user speaks, or {"error": why, "code": …}. */
  lateinit var onEvent: Channel
  lateinit var locale: String
}

@InvokeArg
class SpeakArgs {
  lateinit var text: String
  lateinit var locale: String
}

/**
 * The Android half of pr3-speech (docs Sec. 4.3). Recognition uses only the on-device
 * recognizer (Android 12+); where a phone has none, PR3 offers no voice input rather
 * than send audio to a server. Replies are spoken by TextToSpeech with a voice that
 * needs no network. Both need the activity's main thread, so every call runs there.
 */
@TauriPlugin(
  permissions = [Permission(strings = [Manifest.permission.RECORD_AUDIO], alias = "microphone")]
)
class SpeechPlugin(private val activity: Activity) : Plugin(activity) {
  private val main = Handler(Looper.getMainLooper())
  private var recognizer: SpeechRecognizer? = null
  private var session: Session? = null
  private var tts: TextToSpeech? = null
  private var ttsReady: Boolean? = null
  private val waitingForTts = mutableListOf<(TextToSpeech?) -> Unit>()
  /** How often to look for an on-device voice, half a second apart. */
  private val voiceTries = 6

  /** Whether this phone recognizes speech on the device, and in which language. */
  @Command
  fun status(invoke: Invoke) {
    val result = JSObject()
    result.put("onDevice", onDeviceRecognition())
    result.put("locale", Locale.getDefault().toLanguageTag())
    invoke.resolve(result)
  }

  /**
   * The languages this phone recognizes on the device: {"installed": [...], "available":
   * [...]}, BCP 47 tags; available ones download on first use. Android 13 and later can
   * tell; before that, only the phone's own language is offered.
   */
  @Command
  fun languages(invoke: Invoke) {
    main.post {
      if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU || !onDeviceRecognition()) {
        val result = JSObject()
        result.put("installed", JSArray())
        result.put("available", JSArray())
        invoke.resolve(result)
        return@post
      }
      // A recognizer of its own, so a voice note in progress is not disturbed.
      val probe = SpeechRecognizer.createOnDeviceSpeechRecognizer(activity)
      val intent = Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH)
        .putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL, RecognizerIntent.LANGUAGE_MODEL_FREE_FORM)
      probe.checkRecognitionSupport(
        intent,
        ContextCompat.getMainExecutor(activity),
        object : RecognitionSupportCallback {
          override fun onSupportResult(support: RecognitionSupport) {
            probe.destroy()
            val result = JSObject()
            result.put("installed", JSArray(support.installedOnDeviceLanguages))
            result.put(
              "available",
              JSArray(support.supportedOnDeviceLanguages + support.pendingOnDeviceLanguages)
            )
            invoke.resolve(result)
          }

          override fun onError(error: Int) {
            probe.destroy()
            invoke.reject("could not list the speech languages (error $error)", "failed")
          }
        }
      )
    }
  }

  /** Starts listening, asking for the microphone the first time. */
  @Command
  fun start(invoke: Invoke) {
    if (getPermissionState("microphone") == PermissionState.GRANTED) {
      begin(invoke)
    } else {
      requestPermissionForAlias("microphone", invoke, "microphoneAnswered")
    }
  }

  @PermissionCallback
  private fun microphoneAnswered(invoke: Invoke) {
    if (getPermissionState("microphone") == PermissionState.GRANTED) {
      begin(invoke)
    } else {
      invoke.reject("the microphone", "denied")
    }
  }

  private fun begin(invoke: Invoke) {
    val args = invoke.parseArgs(StartArgs::class.java)
    main.post {
      if (session?.done == false) {
        invoke.reject("already listening", "busy")
        return@post
      }
      if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S ||
        !SpeechRecognizer.isOnDeviceRecognitionAvailable(activity)
      ) {
        invoke.reject("no on-device speech recognition on this phone", "unavailable")
        return@post
      }
      tts?.stop()
      val engine = recognizer
        ?: SpeechRecognizer.createOnDeviceSpeechRecognizer(activity).also { recognizer = it }
      val started = Session(engine, args.onEvent, args.locale)
      session = started
      started.listen()
      invoke.resolve()
    }
  }

  /** Stops listening and answers with the transcript: {"text": …}. */
  @Command
  fun stop(invoke: Invoke) {
    main.post {
      val current = session
      if (current == null) {
        invoke.reject("not listening", "idle")
        return@post
      }
      current.stop(invoke)
    }
  }

  /** Stops listening and discards what was heard. */
  @Command
  fun cancel(invoke: Invoke) {
    main.post {
      session?.let {
        it.done = true
        it.engine.cancel()
      }
      session = null
      invoke.resolve()
    }
  }

  /** Speaks a reply, interrupting anything still being spoken. */
  @Command
  fun speak(invoke: Invoke) {
    val args = invoke.parseArgs(SpeakArgs::class.java)
    main.post {
      withTts { engine ->
        if (engine == null) {
          invoke.reject("text-to-speech is unavailable on this phone", "unavailable")
        } else {
          speakWith(engine, args, invoke, voiceTries)
        }
      }
    }
  }

  private fun speakWith(engine: TextToSpeech, args: SpeakArgs, invoke: Invoke, tries: Int) {
    val locale = Locale.forLanguageTag(args.locale)
    // Only a voice that runs on the phone: none that needs the network.
    val voice = engine.voices.orEmpty()
      .filter {
        !it.isNetworkConnectionRequired &&
          it.locale.language == locale.language &&
          !it.features.orEmpty().contains(TextToSpeech.Engine.KEY_FEATURE_NOT_INSTALLED)
      }
      .sortedWith(compareBy({ it.locale != locale }, { -it.quality }, { it.latency }))
      .firstOrNull()
    if (voice == null) {
      // A freshly started engine may still be installing its voices; wait for them.
      if (tries > 1) {
        main.postDelayed({ speakWith(engine, args, invoke, tries - 1) }, 500)
      } else {
        invoke.reject("no on-device voice for ${locale.displayName}", "unavailable")
      }
      return
    }
    engine.voice = voice
    if (engine.speak(args.text, TextToSpeech.QUEUE_FLUSH, null, "pr3-reply") == TextToSpeech.SUCCESS) {
      invoke.resolve()
    } else {
      invoke.reject("could not speak", "failed")
    }
  }

  @Command
  fun stopSpeaking(invoke: Invoke) {
    main.post {
      tts?.stop()
      invoke.resolve()
    }
  }

  private fun onDeviceRecognition(): Boolean =
    Build.VERSION.SDK_INT >= Build.VERSION_CODES.S &&
      SpeechRecognizer.isOnDeviceRecognitionAvailable(activity)

  /** Runs `use` once TextToSpeech has started, with null if it cannot. */
  private fun withTts(use: (TextToSpeech?) -> Unit) {
    when (ttsReady) {
      true -> use(tts)
      false -> use(null)
      null -> {
        waitingForTts.add(use)
        if (tts == null) {
          tts = TextToSpeech(activity) { status ->
            main.post {
              ttsReady = status == TextToSpeech.SUCCESS
              val ready = if (ttsReady == true) tts else null
              waitingForTts.forEach { it(ready) }
              waitingForTts.clear()
            }
          }
        }
      }
    }
  }

  /**
   * One push-to-talk session. The recognizer ends a segment on its own after a pause;
   * while the user still holds the button, it starts another and the transcript grows.
   */
  private inner class Session(
    val engine: SpeechRecognizer,
    private val events: Channel,
    private val locale: String
  ) : RecognitionListener {
    var done = false
    private var heard = ""
    private var segment = ""
    private var stopping: Invoke? = null
    private var failure: String? = null
    private var failureCode = "failed"

    private fun intent(): Intent = Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH)
      .putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL, RecognizerIntent.LANGUAGE_MODEL_FREE_FORM)
      .putExtra(RecognizerIntent.EXTRA_LANGUAGE, locale)
      .putExtra(RecognizerIntent.EXTRA_PARTIAL_RESULTS, true)
      .putExtra(RecognizerIntent.EXTRA_PREFER_OFFLINE, true)

    fun listen() {
      engine.setRecognitionListener(this)
      engine.startListening(intent())
    }

    fun stop(invoke: Invoke) {
      if (done) {
        answer(invoke)
        return
      }
      stopping = invoke
      engine.stopListening()
      // If the recognizer has not answered within two seconds, what it heard stands.
      main.postDelayed({ if (!done) end(null) }, 2000)
    }

    private fun transcript(): String =
      listOf(heard, segment).filter { it.isNotBlank() }.joinToString(" ").trim()

    private fun answer(invoke: Invoke) {
      session = null
      val why = failure
      if (why == null) {
        val result = JSObject()
        result.put("text", transcript())
        invoke.resolve(result)
      } else {
        invoke.reject(why, failureCode)
      }
    }

    /** Ends the session; a failure before the user lets go is reported when they do. */
    private fun end(why: String?, code: String = "failed") {
      if (done) return
      done = true
      failure = why
      failureCode = code
      val waiting = stopping
      if (waiting != null) {
        answer(waiting)
      } else if (why != null) {
        // The user still holds the button: tell them now, not when they let go.
        val event = JSObject()
        event.put("error", why)
        event.put("code", code)
        events.send(event)
      }
    }

    override fun onPartialResults(partial: Bundle) {
      if (done) return
      segment = partial.getStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION)?.firstOrNull() ?: return
      val event = JSObject()
      event.put("partial", transcript())
      events.send(event)
    }

    override fun onResults(results: Bundle) {
      if (done) return
      results.getStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION)?.firstOrNull()?.let { segment = it }
      heard = transcript()
      segment = ""
      if (stopping != null) end(null) else listen()
    }

    override fun onError(code: Int) {
      if (done) return
      when {
        stopping != null -> end(null)
        // A pause while the user still holds the button: keep listening.
        code == SpeechRecognizer.ERROR_NO_MATCH || code == SpeechRecognizer.ERROR_SPEECH_TIMEOUT -> listen()
        // The language's on-device model is not on the phone yet: Android 13+ downloads
        // it on request. Only the model is downloaded; no audio leaves the phone.
        code == SpeechRecognizer.ERROR_LANGUAGE_UNAVAILABLE &&
          Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU -> {
          engine.triggerModelDownload(intent())
          val language = Locale.forLanguageTag(locale).displayName
          end("Android is downloading the on-device speech model for $language; try again in a minute", "unavailable")
        }
        code == SpeechRecognizer.ERROR_LANGUAGE_NOT_SUPPORTED ||
          code == SpeechRecognizer.ERROR_LANGUAGE_UNAVAILABLE -> end(describe(code), "unavailable")
        else -> end(describe(code))
      }
    }

    override fun onReadyForSpeech(params: Bundle?) {}
    override fun onBeginningOfSpeech() {}
    override fun onRmsChanged(rmsdB: Float) {}
    override fun onBufferReceived(buffer: ByteArray?) {}
    override fun onEndOfSpeech() {}
    override fun onEvent(eventType: Int, params: Bundle?) {}
  }

  private fun describe(code: Int): String = when (code) {
    SpeechRecognizer.ERROR_AUDIO -> "the microphone failed"
    SpeechRecognizer.ERROR_INSUFFICIENT_PERMISSIONS -> "PR3 may not use the microphone"
    SpeechRecognizer.ERROR_RECOGNIZER_BUSY -> "the speech recognizer is busy"
    SpeechRecognizer.ERROR_LANGUAGE_NOT_SUPPORTED -> "this language has no on-device speech recognition"
    SpeechRecognizer.ERROR_LANGUAGE_UNAVAILABLE -> "this language's on-device speech model is not installed"
    else -> "speech recognition failed (error $code)"
  }
}
