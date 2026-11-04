# Android email acceptance

This verifies the device flow; automated tests do not prove that a particular phone or mail app handles permissions and drafts correctly. Use a contact you control. PR3 never presses Send in the email app.

Build the arm64 debug APK with `script/build-android.sh`. Output: `app/pr3-app/src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk`. Install/update PR3 and configure a default email app. Install a local model in PR3 before the offline check.

Record phone model, Android version, PR3 build, local model, email app/version and whether a computer is paired. For local latency measurements, unpair the computer first.

| Check | Expected result |
|---|---|
| Fresh install → Settings → Contacts | Sync off, no sample recipients; pill switch on the right, PR3 green colors, red removal button |
| Enable → deny OS permission | Sync stays off, no imported contacts, guidance to device Settings |
| Allow Contacts in Android Settings, then enable in PR3 | Switch stays on. Settings contains only Sync and Remove; contact editing stays in the device Contacts app |
| Ask to email a contact, including a non-Latin name | Approval shows the correct name and email address from the device |
| Two people with one name, or one person with two email addresses | PR3 asks which reference to use; a vague name cannot silently choose a recipient |
| Disable, restart PR3 | List empty and sync off; device contacts unchanged |
| Enable, then Remove all contacts, restart | List empty and sync off; no immediate reimport; device contacts unchanged |
| Revoke Contacts in Android Settings after syncing | Next panel refresh/email attempt clears imported contacts and refuses stale recipients |
| Ask for an email, then Decline | No mail app opens; no claim of sending |
| Approve a draft | PR3 shows exact recipients/subject/body, performs device check, then opens the mail app for review |
| Review in mail app | Recipient, subject, Unicode text and line breaks match the PR3 approval; no unexpected cc/bcc |
| Cancel/back out of mail app | PR3 still says draft opened and delivery unverified, never sent; task remains open |
| Airplane mode with a local model and synced contacts | PR3 can prepare/open the draft; delivery requires connectivity and the mail app's sending behavior |
| Remove/change the recipient while the PR3 approval is pending | Approving that stale draft fails and asks for a fresh draft |
| No email app configured | A clear error or the email app's setup flow; PR3 never claims delivery |

End-to-end scenario:

1. `Remember that the project review meeting starts at 14:00.` Check the recorded save.
2. In Settings → Memory, edit that fact to `The project review meeting starts at 15:00.` Verify only the new version is active.
3. Restart PR3; resume the meeting task.
4. `Email <your synced contact name> the current project review meeting time from my confirmed memory.`
5. The draft must say 15:00 (or 3 pm), with no old 14:00/2 pm claim. Review the exact address and approve.
6. Review in the mail app. Send only if intended; check that app's outbox/sent/error state. PR3 cannot verify this provider result.
7. Return to PR3 and explicitly Finish the task when your work is done.

For latency, record at least five warm repetitions and one cold start separately: request → approval card; approval/device check → mail app; memory correction review. Note model load time, network state, thermal state and failures. These are measurements to collect on the phone, not extrapolations from desktop timings.

The future **30-day user-zero diary** means ordinary use over a month: each day record tasks attempted/completed, corrections, unwanted interruptions, drafts accepted/edited/rejected and estimated minutes saved (negative when PR3 costs time). No automatic enrollment or subscription is involved. It has not started; cross-device continuity, encryption/key recovery and broader release checks are still roadmap gates.
