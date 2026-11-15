# Android document acceptance

Record the phone model, Android version, PR3 build, chosen model, and whether inference uses a paired computer. Desktop automated tests do not establish physical-device acceptance.

1. Open Settings → Documents. Check that Contacts is absent and Android no longer lists Contacts as a requested app permission.
2. Choose a local UTF-8 text or Markdown file with `The permit code is JN-371.`. Cancel once, then import. No broad storage permission should be requested; only explicitly selected files are read.
3. Search `permit code` with no model loaded. Inspect the exact passage. Unsupported/binary/oversized files must show an error without corrupting the library.
4. Load the local model. Enable airplane mode. Ask PR3 for the permit code; it must quote the source and display its name/lines. Tap View source.
5. Force-stop and restart PR3. Reopen the task and its source. Try a question whose answer is absent: an unsupported answer must not be presented as a verified quote.
6. Choose Remove, then Cancel; the source must remain. Confirm Remove in red; the snapshot and conversations that used it disappear. Restart and search again; neither the old passage nor its dependent chat should return. The original file stays in the phone's file manager.
7. Repeat while a task is running: removal must wait for Stop/completion. Check an unrelated task remains available after removing a document.
8. Pair with a computer and repeat the question. Tool execution/source storage remain on the phone. Do not assume the document library syncs: source inspection on the computer must report unavailable unless it has the identical snapshot.
9. Measure import time, search time, and time to the **verified final answer** separately. Record warm/cold runs, battery/power state and model/backend. Do not compare desktop timings to phone latency.

Current formats: `.txt`, `.md`, `.markdown`; maximum 512 KiB per file, 200 documents / 16 MiB per library. PDF, OCR, folder watching and automatic document sync remain roadmap work.


## Research acceptance

1. Import two documents with different prices. Tap Research, keep Include the web off, and compare them in airplane mode. Check both quotations, source buttons, gaps and the distinction between model analysis and checked quotations.
2. Stop an active run, restart, and explicitly Retry/Continue. Check task identity and saved sources; New task must not reuse the previous research implicitly.
3. In Settings → Web add a Brave Search API key, then research a public topic with Include the web on. No browser installation should be needed. Check source URLs and snapshots, invalid-key/quota/offline errors, and that no secret is displayed or exported.
4. Remove the key using the red confirmation. Research a topic containing a public HTTPS link: it should still read that link while showing automatic web search as incomplete. Disable web access: no new fetch should succeed; prior snapshots remain inspectable offline.
5. Delete the research task and restart; its saved web sources must be unavailable. Remove a cited document and confirm its dependent local conversations disappear. Confirm unrelated tasks and original files remain.
6. Record end-to-end time for two-source local and online research, including cold/warm model loads. Try Android ↔ computer continuity and record unavailable source snapshots honestly: automatic document/research-source sync is not implemented.
