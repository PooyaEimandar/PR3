# Device and embedded profiles

Where PR3 runs, what each device is expected to do, and the contract a new board signs to join. The first test devices are the ones the [roadmap](16-roadmap.md) starts on.

## Profiles

| Profile | First physical target | User-visible acceptance |
| --- | --- | --- |
| Development and first test devices | A Mac on Apple silicon running macOS, and a physical Android phone | Both run the same C test runner from the first build ([FND-02](17-delivery-plan.md)). M1: an offline, cancellable chat with the same pinned Qwen3.5-2B model on each. M2: the same chat in the SDL3 app |
| Full personal app or node | Android and macOS first, then iOS, Windows and Linux | Every feature in [part 10](10-app-sdl3.md) on each OS: local and paired operation, storage and recovery, lifecycle and accessibility. Each shipped model and device configuration is qualified before its default changes (`config/pr3.toml`, [part 07](07-memory-and-storage.md#configuration)) |
| Embedded Linux node | Raspberry Pi 5, 8 GB, 64-bit Linux, headless | Networking off: load the pinned Qwen3.5-2B GGUF, answer from an imported document with a verified quotation, reopen the task and delete it. Also serve a paired phone ([board](https://www.raspberrypi.com/products/raspberry-pi-5/)) |
| Networked microcontroller | ESP32-C3-DevKitM-1, RV32IMC, ESP-IDF and FreeRTOS | Pair with the owner's node, ask a bounded question over Wi-Fi, interrupt it, reconnect, revoke the pairing. Local status works offline. The board's credential-storage assumptions are published ([board guide](https://docs.espressif.com/projects/esp-idf/en/stable/esp32c3/hw-reference/esp32c3/user-guide-devkitm-1.html)) |
| Serial accessory | Raspberry Pi Pico (RP2040, Cortex-M0+), UART, no RTOS | Local status with the cable disconnected. With a gateway: send a question, show a streamed answer over serial, cancel it, show disconnection explicitly. The gateway owns credentials and personal data ([board](https://www.raspberrypi.com/products/raspberry-pi-pico/)) |

A board without room for the weights, the context, the recurrent state, the scratch buffers and the application cannot run a model itself; the pinned 2B file alone is 1 280 835 840 bytes. It joins as a client of a node or as an accessory behind a gateway. A device has a supported profile only after a physical run.

## The board contract

A board port is a header and a small set of callbacks. The header is a compile-time capability manifest, checked by the build and sent in the accessory protocol's `hello`:

```c
/* board/<name>/pr3_board.h */
#define PR3_BOARD_NAME            "pico"
#define PR3_BOARD_ADDRESS_BITS    32
#define PR3_BOARD_RAM_BYTES       (264u * 1024u)
#define PR3_BOARD_FLASH_BYTES     (2u * 1024u * 1024u)
#define PR3_BOARD_PR3_RAM_BUDGET  (32u * 1024u)    /* incremental, stack and buffers included */
#define PR3_BOARD_PR3_FLASH_BUDGET (96u * 1024u)   /* incremental */
#define PR3_BOARD_TRANSPORTS      (PR3_TRANSPORT_UART)
#define PR3_BOARD_STORAGE         (PR3_STORAGE_NONE)
#define PR3_BOARD_CLOCK           (PR3_CLOCK_MONOTONIC)       /* no wall clock without a gateway */
#define PR3_BOARD_ENTROPY         (PR3_ENTROPY_RING_OSCILLATOR)
#define PR3_BOARD_TRUST           (PR3_TRUST_NONE)            /* no key storage: gateway profile */
```

| Callback | Purpose |
| --- | --- |
| `pr3_board_now_ms` | Monotonic milliseconds |
| `pr3_board_entropy` | Fill a buffer from the board's entropy source, or report none |
| `pr3_board_uart_write`, `pr3_board_uart_poll` | Non-blocking serial I/O |
| `pr3_board_log` | Optional diagnostics |
| `pr3_board_watchdog_kick` | Feed the watchdog from the event loop |

Initial targets use 8-bit bytes and 32- or 64-bit addressing. The portable libraries are freestanding: they need `stdint.h`, `stddef.h`, `stdbool.h`, `float.h`, `string.h`'s `mem*` functions, `stdatomic.h` where the core has lock-free atomics, and nothing else. On a core without them (the Pico's Cortex-M0+, RV32IMC without the A extension) the SPSC ring uses volatile indices with fences instead, which is safe between one interrupt handler and the main loop. Ryu's few hosted includes are met by `src/json/freestanding/` in board builds. Board images also call the compiler's runtime helpers (64-bit division, soft-float); a freestanding link of the accessory image lists every external symbol, and the list is checked in CI ([FND-02](17-delivery-plan.md)). Memory for the board comes from static arrays sized by the manifest; there is no heap.

## Budgets

- **Pico accessory:** at most **32 KiB incremental peak RAM** and **96 KiB incremental flash**, measured against the same board support package with an empty event loop. RAM includes the stack's high-water mark and every buffer. The whole firmware's footprint and the remaining headroom are published beside both numbers.
- **ESP32-C3 client:** whole-image results including radio, RTOS, transport and crypto, plus the PR3 increment over an empty ESP-IDF Wi-Fi application.
- Budgets are frozen for the exact configuration before it is optimized ([EMB-06](17-delivery-plan.md)).

Measuring: linker maps for flash; a painted stack checked for its high-water mark; arena high-water marks; per-request bytes on the wire; watchdog resets counted over the soak.

## Key storage on boards

| Board | Assumption |
| --- | --- |
| Pico | None. The accessory holds no credentials and no personal data; everything sensitive stays on the gateway |
| ESP32-C3 | The device's static X25519 key in NVS with NVS encryption keyed by the eFuse HMAC peripheral, flash encryption and secure boot enabled in release images. A board shipped without them uses the gateway profile instead |
| Pi 5 | A node like a computer: Linux key custody (Secret Service, or a file readable only by the service user on a headless system, stated as such) |

## Porting a new board

`board/README.md` gives the procedure: copy the closest board directory, fill the manifest, implement the callbacks, build the empty event loop and record its footprint, build the accessory or client image and record the increment, run the host loopback tests against the board over its transport, then the physical checklist (stream, cancel, power-cycle mid-answer, reconnect). An 8- or 16-bit board, or one smaller than the Pico, can qualify a reduced accessory profile by passing the same checklist under a measured RAM and flash gate ([EMB-06](17-delivery-plan.md)).

---

[Back to the index](../README.md)
