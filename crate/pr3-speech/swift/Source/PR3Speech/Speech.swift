// The Apple half of pr3-speech. Recognition runs on the device only
// (requiresOnDeviceRecognition): where a language's on-device model is missing, opening
// fails rather than sending audio to a server. Replies are spoken with the system's
// on-device voices.
//
// Functions that answer return a JSON string the caller frees with pr3_speech_free:
// {"ok": …}, {"denied": "<what>"}, {"unavailable": "<why>"} or {"error": "<reason>"}.
// Listening is one session at a time, whose events are read with pr3_speech_next.
// Calls that wait for the user (permission prompts) or for audio are never made on the
// main thread.

import AVFoundation
import Foundation
import Speech

@_cdecl("pr3_speech_free")
public func pr3SpeechFree(_ text: UnsafeMutablePointer<CChar>?) {
    free(text)
}

private let lock = NSLock()
private var recognizer: SFSpeechRecognizer?
private var session: Session?
/// Made on the first reply spoken, so listening alone never sets up audio output.
private var synthesizer: AVSpeechSynthesizer?

/// Opens the on-device recognizer for `locale` (a BCP 47 tag; empty for the system's
/// language) and answers with the locale it recognizes. Without an on-device model for
/// that language, it tries the same language in another region, then gives up.
@_cdecl("pr3_speech_open")
public func pr3SpeechOpen(_ locale: UnsafePointer<CChar>) -> UnsafeMutablePointer<CChar> {
    let wanted = String(cString: locale)
    let preferred = wanted.isEmpty ? Locale.current : Locale(identifier: wanted)
    guard let found = onDeviceRecognizer(for: preferred) else {
        let name = Locale.current.localizedString(forIdentifier: preferred.identifier) ?? preferred.identifier
        return reply(["unavailable": "no on-device speech recognition for \(name) on this device"])
    }
    // Results arrive on this queue, not the main thread, which a host may be blocking.
    found.queue = OperationQueue()
    lock.lock()
    recognizer = found
    lock.unlock()
    return reply(["ok": tag(found.locale.identifier)])
}

/// Asks for speech recognition and the microphone, the first time with the system's
/// prompts, and waits for the answers.
@_cdecl("pr3_speech_authorize")
public func pr3SpeechAuthorize() -> UnsafeMutablePointer<CChar> {
    if speechStatus() != .authorized {
        return reply(["denied": "speech recognition"])
    }
    if !microphoneAllowed() {
        return reply(["denied": "the microphone"])
    }
    return reply(["ok": ""])
}

/// Starts listening on the microphone.
@_cdecl("pr3_speech_start")
public func pr3SpeechStart() -> UnsafeMutablePointer<CChar> {
    lock.lock()
    let opened = recognizer
    lock.unlock()
    guard let opened = opened else {
        return reply(["error": "speech recognition is not open"])
    }
    guard opened.isAvailable else {
        return reply(["unavailable": "speech recognition is unavailable right now"])
    }
    // Current before it begins, so a voice that stops meanwhile leaves the audio
    // session to it.
    let new = Session()
    lock.lock()
    session = new
    lock.unlock()
    stopSpeaking()
    do {
        try new.begin(with: opened)
    } catch {
        new.finish(with: ["error": describe(error)])
        return reply(["error": describe(error)])
    }
    return reply(["ok": ""])
}

/// The listening session's next event, waiting up to `timeoutMs`: {"partial": text} as
/// the user speaks; then one of {"final": text}, {"cancelled": true} or {"error": why},
/// after which the microphone is released; or {"timeout": true}.
@_cdecl("pr3_speech_next")
public func pr3SpeechNext(_ timeoutMs: Int) -> UnsafeMutablePointer<CChar> {
    guard let current = currentSession() else {
        return reply(["error": "not listening"])
    }
    return reply(current.next(timeout: Double(timeoutMs) / 1000))
}

/// Stops listening; the session ends with the final transcript.
@_cdecl("pr3_speech_stop")
public func pr3SpeechStop() {
    currentSession()?.stop()
}

/// Stops listening and discards what was heard.
@_cdecl("pr3_speech_cancel")
public func pr3SpeechCancel() {
    currentSession()?.cancel()
}

/// Speaks `text` with the best installed voice for `locale` (empty for the system's
/// language), interrupting anything still being spoken. Answers once speaking starts,
/// with the voice's name.
@_cdecl("pr3_speech_speak")
public func pr3SpeechSpeak(
    _ text: UnsafePointer<CChar>,
    _ locale: UnsafePointer<CChar>
) -> UnsafeMutablePointer<CChar> {
    let wanted = String(cString: locale)
    let language = tag(wanted.isEmpty ? Locale.current.identifier : wanted)
    #if os(iOS)
        do {
            let audio = AVAudioSession.sharedInstance()
            try audio.setCategory(.playback, mode: .spokenAudio, options: .duckOthers)
            try audio.setActive(true)
        } catch {
            return reply(["error": describe(error)])
        }
    #endif
    let utterance = AVSpeechUtterance(string: String(cString: text))
    utterance.voice = bestVoice(for: language)
    let speaker = voice()
    speaker.stopSpeaking(at: .immediate)
    speaker.speak(utterance)
    return reply(["ok": utterance.voice?.name ?? ""])
}

@_cdecl("pr3_speech_stop_speaking")
public func pr3SpeechStopSpeaking() {
    stopSpeaking()
}

private func currentSession() -> Session? {
    lock.lock()
    defer { lock.unlock() }
    return session
}

private func listening() -> Bool {
    currentSession()?.isOver == false
}

/// One push-to-talk session: the microphone feeds the recognizer until stopped.
private final class Session {
    private let engine = AVAudioEngine()
    private let request = SFSpeechAudioBufferRecognitionRequest()
    private var task: SFSpeechRecognitionTask?
    private let condition = NSCondition()
    private var events: [[String: Any]] = []
    private var latest = ""
    private var stopping = false
    private var over = false

    var isOver: Bool {
        condition.lock()
        defer { condition.unlock() }
        return over
    }

    func begin(with recognizer: SFSpeechRecognizer) throws {
        #if os(iOS)
            let audio = AVAudioSession.sharedInstance()
            try audio.setCategory(.record, mode: .measurement, options: .duckOthers)
            try audio.setActive(true, options: .notifyOthersOnDeactivation)
        #endif
        request.shouldReportPartialResults = true
        // The audio never leaves the device.
        request.requiresOnDeviceRecognition = true
        if #available(macOS 13, iOS 16, *) {
            request.addsPunctuation = true
        }
        let input = engine.inputNode
        let format = input.outputFormat(forBus: 0)
        guard format.sampleRate > 0, format.channelCount > 0 else {
            throw SpeechFailure("no microphone is available")
        }
        let request = self.request
        input.installTap(onBus: 0, bufferSize: 1024, format: format) { buffer, _ in
            request.append(buffer)
        }
        engine.prepare()
        try engine.start()
        task = recognizer.recognitionTask(with: request) { [weak self] result, error in
            self?.handle(result, error)
        }
    }

    private func handle(_ result: SFSpeechRecognitionResult?, _ error: Error?) {
        if let result = result {
            let text = result.bestTranscription.formattedString
            if result.isFinal {
                finish(with: ["final": text])
            } else {
                condition.lock()
                if !over {
                    latest = text
                    events.append(["partial": text])
                    condition.signal()
                }
                condition.unlock()
            }
        } else if let error = error {
            condition.lock()
            let heard = latest
            let stopped = stopping
            condition.unlock()
            let ns = error as NSError
            // 1110: no speech was detected, which is an empty note, not a failure.
            let silence = ns.domain == "kAFAssistantErrorDomain" && ns.code == 1110
            finish(with: stopped || silence ? ["final": heard] : ["error": ns.localizedDescription])
        }
    }

    /// The recognizer finishes with what it heard; if it has not answered within two
    /// seconds, the last partial transcript stands.
    func stop() {
        condition.lock()
        stopping = true
        condition.unlock()
        detach()
        request.endAudio()
        DispatchQueue.global().asyncAfter(deadline: .now() + 2) { [self] in
            condition.lock()
            let heard = latest
            condition.unlock()
            finish(with: ["final": heard])
        }
    }

    func cancel() {
        task?.cancel()
        finish(with: ["cancelled": true])
    }

    func next(timeout: TimeInterval) -> [String: Any] {
        condition.lock()
        defer { condition.unlock() }
        let deadline = Date().addingTimeInterval(timeout)
        while events.isEmpty {
            if !condition.wait(until: deadline) {
                return ["timeout": true]
            }
        }
        return events.removeFirst()
    }

    /// Ends the session once: releases the microphone, then posts the last event.
    func finish(with event: [String: Any]) {
        condition.lock()
        if over {
            condition.unlock()
            return
        }
        over = true
        condition.unlock()
        release()
        condition.lock()
        events.append(event)
        condition.broadcast()
        condition.unlock()
    }

    private func detach() {
        if engine.isRunning {
            engine.stop()
        }
        engine.inputNode.removeTap(onBus: 0)
    }

    private func release() {
        detach()
        #if os(iOS)
            if !speaking() {
                try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
            }
        #endif
    }
}

private func voice() -> AVSpeechSynthesizer {
    lock.lock()
    defer { lock.unlock() }
    if let made = synthesizer {
        return made
    }
    let made = AVSpeechSynthesizer()
    made.delegate = Speaker.shared
    synthesizer = made
    return made
}

private func existingVoice() -> AVSpeechSynthesizer? {
    lock.lock()
    defer { lock.unlock() }
    return synthesizer
}

private func speaking() -> Bool {
    existingVoice()?.isSpeaking == true
}

/// Gives the audio back to other apps once PR3 has finished speaking.
private final class Speaker: NSObject, AVSpeechSynthesizerDelegate {
    static let shared = Speaker()

    func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didFinish _: AVSpeechUtterance) {
        release(synthesizer)
    }

    func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didCancel _: AVSpeechUtterance) {
        release(synthesizer)
    }

    private func release(_ synthesizer: AVSpeechSynthesizer) {
        #if os(iOS)
            if !synthesizer.isSpeaking && !listening() {
                try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
            }
        #endif
    }
}

private func stopSpeaking() {
    if let speaker = existingVoice(), speaker.isSpeaking {
        speaker.stopSpeaking(at: .immediate)
    }
}

/// The highest-quality voice installed for `language` (enhanced or premium where the
/// user has downloaded one), never a novelty voice or the user's Personal Voice.
private func bestVoice(for language: String) -> AVSpeechSynthesisVoice? {
    let fallback = AVSpeechSynthesisVoice(language: language)
    let base = language.split(separator: "-").first.map(String.init) ?? language
    let candidates = AVSpeechSynthesisVoice.speechVoices().filter { voice in
        let matches = voice.language == language || (fallback == nil && voice.language.hasPrefix(base + "-"))
        if !matches {
            return false
        }
        if #available(macOS 14, iOS 17, *) {
            if voice.voiceTraits.contains(.isNoveltyVoice) || voice.voiceTraits.contains(.isPersonalVoice) {
                return false
            }
        }
        return true
    }
    let best = candidates.max { $0.quality.rawValue < $1.quality.rawValue }
    if let best = best, best.quality.rawValue > (fallback?.quality.rawValue ?? 0) {
        return best
    }
    return fallback ?? best
}

private func onDeviceRecognizer(for preferred: Locale) -> SFSpeechRecognizer? {
    if let exact = SFSpeechRecognizer(locale: preferred), exact.supportsOnDeviceRecognition {
        return exact
    }
    guard let language = preferred.languageCode else {
        return nil
    }
    // The same language in another region whose on-device model is present, the
    // language's main region first: English (Canada) gets English (US) when only that
    // is downloaded.
    let main = mainRegion(of: language).map { "\(language)-\($0)" }
    let others = SFSpeechRecognizer.supportedLocales()
        .filter { $0.languageCode == language && $0.identifier != preferred.identifier }
        .sorted { a, b in
            let (ta, tb) = (tag(a.identifier), tag(b.identifier))
            return (ta == main ? 0 : 1, ta) < (tb == main ? 0 : 1, tb)
        }
    for locale in others {
        if let other = SFSpeechRecognizer(locale: locale), other.supportsOnDeviceRecognition {
            return other
        }
    }
    return nil
}

/// Where a language is mainly spoken, from the system's likely-subtags data: `en` → `US`.
private func mainRegion(of language: String) -> String? {
    guard #available(macOS 13, iOS 16, *) else {
        return nil
    }
    let likely = Locale.Language(identifier: language).maximalIdentifier
    return Locale.Language(identifier: likely).region?.identifier
}

private func speechStatus() -> SFSpeechRecognizerAuthorizationStatus {
    let status = SFSpeechRecognizer.authorizationStatus()
    if status != .notDetermined {
        return status
    }
    let done = DispatchSemaphore(value: 0)
    var answer = status
    SFSpeechRecognizer.requestAuthorization { granted in
        answer = granted
        done.signal()
    }
    done.wait()
    return answer
}

private func microphoneAllowed() -> Bool {
    let done = DispatchSemaphore(value: 0)
    var allowed = false
    #if os(macOS)
        switch AVCaptureDevice.authorizationStatus(for: .audio) {
        case .authorized:
            return true
        case .notDetermined:
            AVCaptureDevice.requestAccess(for: .audio) { granted in
                allowed = granted
                done.signal()
            }
        default:
            return false
        }
    #else
        if #available(iOS 17, *) {
            switch AVAudioApplication.shared.recordPermission {
            case .granted:
                return true
            case .undetermined:
                AVAudioApplication.requestRecordPermission { granted in
                    allowed = granted
                    done.signal()
                }
            default:
                return false
            }
        } else {
            switch AVAudioSession.sharedInstance().recordPermission {
            case .granted:
                return true
            case .undetermined:
                AVAudioSession.sharedInstance().requestRecordPermission { granted in
                    allowed = granted
                    done.signal()
                }
            default:
                return false
            }
        }
    #endif
    done.wait()
    return allowed
}

private struct SpeechFailure: LocalizedError {
    let errorDescription: String?

    init(_ why: String) {
        errorDescription = why
    }
}

/// `en_US@rg=gbzzzz` → `en-US`.
private func tag(_ identifier: String) -> String {
    let base = identifier.split(separator: "@").first.map(String.init) ?? identifier
    return base.replacingOccurrences(of: "_", with: "-")
}

private func describe(_ error: Error) -> String {
    (error as NSError).localizedDescription
}

private func reply(_ object: [String: Any]) -> UnsafeMutablePointer<CChar> {
    let data = (try? JSONSerialization.data(withJSONObject: object)) ?? Data()
    return strdup(String(decoding: data, as: UTF8.self))!
}
