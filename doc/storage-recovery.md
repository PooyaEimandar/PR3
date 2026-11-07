# Encrypted memories and tasks

The app opens `memory.sqlite` through SQLCipher using a random 256-bit database key. This covers memories, source excerpts, full-text indexes, task chats, checkpoints, and continuity records stored in that database. Encryption is enabled on first launch of this implementation; it is not an optional settings switch.

The implemented platform adapters are Apple Keychain (macOS/iOS) and Android Keystore. The macOS build and Android arm64 APK are checked locally; physical-device permission, lock, and recovery behavior still needs acceptance testing. Windows/Linux secure database-key adapters are not implemented: those apps show storage unavailable and cannot run tasks, rather than saving personal data unencrypted. This is a release blocker for those platforms, not completion of M3-E1 on every platform.

## Key custody and recovery

- Apple stores a separate generic-password item under the app's identity, with `WhenUnlockedThisDeviceOnly` accessibility and synchronization disabled. The database key is separate from the Secure Enclave action-signing key.
- Android persists only an AES-GCM-wrapped database key in the app's backup-excluded directory. Its AES wrapping key stays in Android Keystore. A verified recovery creates a fresh wrapping-key alias, durably writes and reads back the new wrapper, then retires the old alias. Losing or invalidating the old wrapping key therefore does not prevent recovery with the correct code. No storage key depends on the action-approval key.
- SQLCipher necessarily receives the database key in native process memory. This is protection for stored files, not protection against a compromised running app or OS. Rust-owned database-key buffers are zeroized on drop; this is not a guarantee that every platform/IPC copy is erased immediately.
- Settings → **Storage** → **Show recovery code** requires device authentication on Apple/Android. The code is shown only in that dialog, cleared on dismissal/backgrounding, and never copied to the clipboard automatically or included in model tools, chats, or traces.
- The recovery code is the high-entropy database key in a readable format, not a password or a second factor. Keep it outside the device, for example in a password manager. Anyone with the code and the encrypted database can read its contents.
- If a key is missing, unreadable, or does not open the existing file, PR3 shows **Storage is locked**, blocks sending messages/actions and starting hybrid service, and preserves the database. It does not create a replacement key/database or switch to volatile task storage. Unlocking the OS and restarting retries access.
- Entering a recovery code first opens the existing database read-only and checks database integrity and page HMACs. Only a successful check permits updating the platform key store. The app asks the user to close and reopen afterward; recovery never executes a task or restores an action approval.

A code restores access to existing files. It cannot recover an erased database or a lost phone's files. A portable encrypted backup/import workflow, recovery-code rotation, replacement-device enrollment, and restoring shared ownership safely on a different device remain separate work. Do not copy a shared-task database between devices as an ownership-transfer mechanism.

## Upgrade and failure behavior

Key creation and migration are serialized across app processes. The database has a second lifetime lock, so another PR3 process cannot open it during migration or normal encrypted use.

For an existing plaintext database, PR3 persists the new key first, takes an exclusive SQLite lock, checkpoints any WAL, and exports all tables/indexes into a private encrypted staging file. It preserves `user_version`, verifies the copy with SQLite integrity and SQLCipher HMAC checks, syncs the file, then atomically replaces the original. An interrupted copy leaves the original canonical and is retried on the next launch. Failure before replacement leaves the source intact. SQLCipher compatibility is fixed to version 4, temporary tables stay in memory, and a missing cipher engine is an error rather than an ignored `PRAGMA key`.

Migration removes the live plaintext file; it cannot erase old filesystem snapshots, backups, or physical remnants on flash storage. Existing plaintext diagnostic traces and explicitly exported chats are outside database encryption. Tracing remains opt-in. Contacts are held in memory and reread from the device address book; this change does not add a persistent contacts database. Pairing credentials and approval/audit files are separate from the database and retain their existing protection.

## Acceptance on Android

Use synthetic data first and save the recovery code before testing key loss.

1. Upgrade a build with a remembered fact and an unfinished task. Verify both survive the migration, restart, correction, and deletion. Pending/uncertain actions must still require review and must not replay.
2. Open Storage, authenticate, save the code somewhere private, and dismiss the dialog. Check that the code disappears when the app backgrounds. Canceling authentication must reveal nothing.
3. Lock/unlock and restart the phone; repeat after changing biometrics. Confirm ordinary storage access and the separate email-approval prompt still behave correctly.
4. With an isolated debug test database, remove its wrapping key without deleting the database. Check that the app shows the locked state, cannot send messages or actions, and leaves the file intact. A wrong code must change neither the database nor the key store. The right code, followed by restart, must restore the same facts and tasks.
5. Repeat with an invalidated wrapping key and with interrupted migration/key-wrapper writes. Verify recovery uses the fresh alias and no successful/uncertain email handoff is replayed.

[Recorded checks and artifact hashes](../bench/rebaseline/evidence/storage-2026-11-07.json) cover encrypted round trips, plaintext migration, interrupted/blocked migration, wrong keys, tampered ciphertext, missing keys, failed vault writes, and an isolated native UI recovery workflow. The UI fixture replaces only key custody/authentication with a marked temporary fixture; production SQLCipher, IPC, storage recovery, and chat gating execute normally. It is debug-only and cannot be compiled into a release.

References: [SQLCipher key/export/integrity APIs](https://www.zetetic.net/sqlcipher/sqlcipher-api/), [Android Keystore](https://developer.android.com/privacy-and-security/keystore), [Apple device-only unlocked Keychain accessibility](https://developer.apple.com/documentation/security/ksecattraccessiblewhenunlockedthisdeviceonly).
