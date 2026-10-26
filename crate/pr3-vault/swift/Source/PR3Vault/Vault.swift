// The Apple half of pr3-vault. The approval key lives in the Secure Enclave and signs
// only after Face ID, Touch ID or the device passcode; its wrapped form, usable only by
// this device's Secure Enclave, is kept in a file. Without a Secure Enclave (the iOS
// simulator, older Macs) the Rust side keeps a software key and asks
// pr3_vault_authenticate first.
//
// Each function returns a JSON string the caller frees with pr3_vault_free:
// {"ok":"<hex>"}, {"cancelled":true} or {"error":"<reason>"}. Signing blocks while
// the system shows its prompt, so it is never called on the main thread.

import CryptoKit
import Foundation
import LocalAuthentication

@_cdecl("pr3_vault_free")
public func pr3VaultFree(_ text: UnsafeMutablePointer<CChar>?) {
    free(text)
}

/// The iOS simulator reports a Secure Enclave but cannot create keys that need
/// authentication, so it counts as having none.
@_cdecl("pr3_vault_has_secure_enclave")
public func pr3VaultHasSecureEnclave() -> Bool {
    #if targetEnvironment(simulator)
        return false
    #else
        return SecureEnclave.isAvailable
    #endif
}

/// The public key (65 bytes, X9.63) of the Secure Enclave key whose wrapped form is at
/// `path`, creating the key the first time.
@_cdecl("pr3_vault_enclave_public_key")
public func pr3VaultEnclavePublicKey(_ path: UnsafePointer<CChar>) -> UnsafeMutablePointer<CChar> {
    do {
        let key = try enclaveKey(at: String(cString: path), context: nil)
        return reply(["ok": hex(key.publicKey.x963Representation)])
    } catch {
        return failure(error)
    }
}

/// Signs `message` (ECDSA P-256, SHA-256; DER) with the Secure Enclave key at `path`.
/// The system asks for Face ID, Touch ID or the passcode, showing `reason`.
@_cdecl("pr3_vault_enclave_sign")
public func pr3VaultEnclaveSign(
    _ path: UnsafePointer<CChar>,
    _ message: UnsafePointer<UInt8>,
    _ length: Int,
    _ reason: UnsafePointer<CChar>
) -> UnsafeMutablePointer<CChar> {
    let context = LAContext()
    context.localizedReason = String(cString: reason)
    do {
        let key = try enclaveKey(at: String(cString: path), context: context)
        let signature = try key.signature(for: Data(bytes: message, count: length))
        return reply(["ok": hex(signature.derRepresentation)])
    } catch {
        return failure(error)
    }
}

/// Asks for Face ID, Touch ID or the passcode, showing `reason`.
@_cdecl("pr3_vault_authenticate")
public func pr3VaultAuthenticate(_ reason: UnsafePointer<CChar>) -> UnsafeMutablePointer<CChar> {
    let reason = String(cString: reason)
    let done = DispatchSemaphore(value: 0)
    var result: UnsafeMutablePointer<CChar>?
    LAContext().evaluatePolicy(.deviceOwnerAuthentication, localizedReason: reason) { ok, error in
        result = ok ? reply(["ok": ""]) : failure(error ?? LAError(.authenticationFailed))
        done.signal()
    }
    done.wait()
    return result ?? reply(["error": "no answer from LocalAuthentication"])
}

/// Loads the key at `path`, or creates it if there is no file. An unreadable file is an
/// error, never silently replaced: a new key would be a different approver.
private func enclaveKey(at path: String, context: LAContext?) throws -> SecureEnclave.P256.Signing.PrivateKey {
    let url = URL(fileURLWithPath: path)
    if FileManager.default.fileExists(atPath: path) {
        let wrapped = try Data(contentsOf: url)
        return try SecureEnclave.P256.Signing.PrivateKey(
            dataRepresentation: wrapped, authenticationContext: context)
    }
    var error: Unmanaged<CFError>?
    guard
        let access = SecAccessControlCreateWithFlags(
            nil, kSecAttrAccessibleWhenUnlockedThisDeviceOnly,
            [.privateKeyUsage, .userPresence], &error)
    else {
        throw error!.takeRetainedValue() as Error
    }
    let key = try SecureEnclave.P256.Signing.PrivateKey(accessControl: access)
    try FileManager.default.createDirectory(
        at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
    try key.dataRepresentation.write(to: url, options: .withoutOverwriting)
    return key
}

private func reply(_ object: [String: Any]) -> UnsafeMutablePointer<CChar> {
    let data = (try? JSONSerialization.data(withJSONObject: object)) ?? Data()
    return strdup(String(decoding: data, as: UTF8.self))!
}

/// Cancelling the prompt is an answer, not a failure.
private func failure(_ error: Error) -> UnsafeMutablePointer<CChar> {
    let ns = error as NSError
    let cancelCodes = [LAError.userCancel, .appCancel, .systemCancel].map { $0.rawValue }
    let cancelled =
        (ns.domain == LAErrorDomain && cancelCodes.contains(ns.code))
        || (ns.domain == NSOSStatusErrorDomain && ns.code == Int(errSecUserCanceled))
    return cancelled ? reply(["cancelled": true]) : reply(["error": ns.localizedDescription])
}

private func hex(_ data: Data) -> String {
    data.map { String(format: "%02x", $0) }.joined()
}
