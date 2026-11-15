# Historical email fixtures

Archived on 2026-11-14. These adapters exist only to reproduce the frozen Personal-200 and email security experiments. They are not part of the shipped app. Opt in with `--features legacy-email-bench` on debug benchmark builds. Release builds reject this feature. Historical evidence and corpora are unchanged. New product evaluation lives in `bench/documents`.

`workflow.test.ts` is the archived UI fixture, now typed and checked by the app's `tsconfig.test.json`. It is excluded from active test execution; replay requires the matching historical UI/native fixture. Frozen benchmark corpora and results remain unchanged.
