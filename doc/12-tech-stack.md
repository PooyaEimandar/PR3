# Tech stack

C everywhere PR3 writes product code. Platform languages only where an OS API requires them: Swift for the Apple vault and speech bridges, Objective-C for the Apple accessibility bridges, Java for the Android bridges, and Metal Shading Language and GLSL for GPU kernels. C++ appears only in `bench/mlperf/`, because MLCommons LoadGen is a C++ library, and inside third-party libraries written in it ([D-22](19-decisions-and-risks.md)). Every third-party library is pinned by commit in `third_party/manifest.txt`, fetched by `tools/fetch-third-party.sh`, and built by our CMake with our flags. Versions below were current on 21 Nov 2026 and are re-checked when a milestone opens.

| Where | Stack |
| --- | --- |
| Language | C17 as the floor, with C23 features used through `include/pr3/compiler.h` when the compiler has them: `<stdckdint.h>`, `[[nodiscard]]`, `counted_by` ([D-05](19-decisions-and-risks.md)) |
| Compilers | Apple Clang 21 (Xcode SDK 27.2) and upstream Clang 21 on macOS and Linux; GCC 15 on Linux; MSVC 19.5x and `clang-cl` on Windows; Android NDK r29 Clang; `arm-none-eabi-gcc` 14 for the Pico; ESP-IDF 5.5's RISC-V GCC 14 for the ESP32-C3 |
| Build | CMake 4.x (3.28 minimum) with presets, Ninja; one static library per module and an amalgamated `libpr3.a` ([D-06](19-decisions-and-risks.md)) |
| Tests | PR3's own harness (`test/pr3_test.h`): no dependencies, runs on hosts, phones over `adb`, and boards over UART ([D-16](19-decisions-and-risks.md)) |
| Fuzzing | libFuzzer with ASan and UBSan; corpora seeded from the golden vectors and fixtures |
| Static analysis | Clang static analyzer, `clang-tidy`, GCC `-fanalyzer`, CBMC 6, Frama-C 31 (WP) for the trusted side ([D-19](19-decisions-and-risks.md)) |
| Crypto | mbedTLS 3.6.8 LTS (SHA-256, HMAC, HKDF, P-256 ECDSA, X25519, ChaCha20-Poly1305); BLAKE2s from the BLAKE2 authors' own C code; libsodium 1.0.22 for SPAKE2's Edwards arithmetic in profiles that pair ([D-07](19-decisions-and-risks.md)) |
| JSON | PR3's own bounded tape parser and canonical writer ([D-08](19-decisions-and-risks.md)); shortest float digits from Ryu's C implementation (`ulfjack/ryu` at commit `4c0618b0`, `d2s.c` only, the small tables on boards) |
| CBOR | PR3's own canonical codec ([D-09](19-decisions-and-risks.md)) |
| Inference | llama.cpp release b11429 as the llama.cpp backend ([D-12](19-decisions-and-risks.md)); the PR3 engine ([part 06](06-inference.md)) |
| Storage | SQLCipher 4.20.0 on SQLite 3.54.1 with FTS5; CommonCrypto provider on Apple, libtomcrypt elsewhere ([D-13](19-decisions-and-risks.md)) |
| HTTP | libcurl 8.x with the mbedTLS backend, HTTPS only ([D-23](19-decisions-and-risks.md)) |
| HTML | lexbor 2.x with allocations routed to PR3 regions ([D-24](19-decisions-and-risks.md)) |
| Unicode data | Tables generated from the Unicode Character Database 17.0 by `tools/gen-unicode/` ([D-14](19-decisions-and-risks.md)) |
| Graphical app | SDL3 3.4.x, SDL3_ttf 3.x with HarfBuzz and FreeType, SheenBidi 2.x ([D-02](19-decisions-and-risks.md), [D-10](19-decisions-and-risks.md)) |
| Fonts | Vazirmatn and Noto Sans (OFL) |
| Apple bridges | Swift 6 packages `PR3Vault` and `PR3Speech`, Objective-C for accessibility, built by CMake through SwiftPM |
| Android bridges | Java through JNI from `PR3Activity`, Gradle from SDL3's `android-project` template |
| Boards | Pico SDK 2.x; ESP-IDF 5.5 |
| CI | GitHub Actions on hosted macOS, Linux and Windows runners; physical-device steps recorded by the owner ([part 14](14-build-and-quality.md)) |
| Benchmarks | PR3's C microbenchmarks for the module budgets; MLCommons LoadGen 6.0.17 (C++17, `mlcommons/inference` at commit `3fbc3299`) for MLPerf Inference runs ([D-25](19-decisions-and-risks.md)); AgentDojo and its Python scorers for agent security ([part 15](15-benchmarks.md)) |

## Licences

PR3 stays Apache-2.0. Every linked dependency is permissive: mbedTLS (Apache-2.0), libsodium (ISC), BLAKE2 (CC0), Ryu (Apache-2.0 or Boost), llama.cpp and ggml (MIT), SQLite (public domain), SQLCipher (BSD-style), libtomcrypt (public domain / WTFPL), libcurl (curl, MIT-style), lexbor (Apache-2.0), SDL3 and SDL3_ttf (zlib), HarfBuzz (MIT), FreeType (FTL), SheenBidi (Apache-2.0), fonts (OFL). Benchmark builds also link MLCommons LoadGen (Apache-2.0) and run AgentDojo (MIT); neither ships. `tools/licenses.sh` writes the notice file and the SBOM for each artifact from `third_party/manifest.txt`.

## Dropped on purpose

| Dropped | Why |
| --- | --- |
| A general-purpose allocator inside the libraries | Arenas, pools and the buddy heap give the same service with known bounds ([part 03](03-memory-safety.md)) |
| cJSON, yyjson, jansson | Allocation per node or a DOM, and none writes PR3's canonical form; a 1500-line parser we own is easier to bound, fuzz and prove |
| OpenSSL | Size, and the profiles share one crypto library that also runs on the boards |
| Nuklear, Dear ImGui, GTK, Qt | No complex-script shaping or bidirectional layout (Nuklear, ImGui), or far more than a 2D chat app needs (GTK, Qt) |
| AccessKit | Adds a second language toolchain to the shipped app for five small bridges ([D-11](19-decisions-and-risks.md)) |
| A regex engine | The tokenizer's pre-tokenizer and the link scanner are hand-written scanners, as fast and bounded by construction |
| A general TOML library | The configuration needs a small, fuzzed subset reader |
| MLPerf Client as the official benchmark | It bundles vendor execution providers with no plug-in point for another engine, has no Android build and is ONNX-first; LoadGen measures any engine on any OS ([D-25](19-decisions-and-risks.md)) |

---

[Back to the index](../README.md)
