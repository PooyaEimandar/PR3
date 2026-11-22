/* The board contract (doc/11, "The board contract"). A board port supplies
 * board/<name>/pr3_board.h, a compile-time capability manifest, and the callbacks below.
 * Memory comes from static arrays sized by the manifest; there is no heap.
 *
 * Every callback is called from the board's single event loop, never from an interrupt,
 * and none may block. */
#ifndef PR3_BOARD_H
#define PR3_BOARD_H

#include <pr3/base.h>

/* Capability flags for the manifest. */
#define PR3_TRANSPORT_UART 0x1u
#define PR3_TRANSPORT_BLE 0x2u
#define PR3_TRANSPORT_WIFI 0x4u

#define PR3_STORAGE_NONE 0x0u
#define PR3_STORAGE_FLASH 0x1u

#define PR3_CLOCK_MONOTONIC 0x1u
#define PR3_CLOCK_WALL 0x2u

#define PR3_ENTROPY_NONE 0x0u
#define PR3_ENTROPY_RING_OSCILLATOR 0x1u
#define PR3_ENTROPY_TRNG 0x2u

#define PR3_TRUST_NONE 0x0u         /* no key storage: the gateway profile */
#define PR3_TRUST_SECURE_FLASH 0x1u /* keys in flash the board protects */

/* Monotonic milliseconds since an arbitrary start. */
uint64_t pr3_board_now_ms(void);

/* Fills out with n bytes from the board's entropy source; PR3_E_NOT_FOUND when the board
 * has none (PR3_ENTROPY_NONE). Writes nothing on failure. */
PR3_NODISCARD pr3_status pr3_board_entropy(uint8_t *out, size_t n);

/* Queues up to n bytes for the serial port and returns how many it took; never blocks. */
size_t pr3_board_uart_write(const uint8_t *p, size_t n);

/* Moves up to cap received bytes into out and returns how many; never blocks. */
size_t pr3_board_uart_poll(uint8_t *out, size_t cap);

/* One line of optional diagnostics; a board without a log ignores it. */
void pr3_board_log(pr3_str line);

/* Feeds the watchdog; the event loop calls it once per iteration. */
void pr3_board_watchdog_kick(void);

/* Checks a manifest at compile time: every key is defined and the budgets fit the board. */
#define PR3_BOARD_CHECK_MANIFEST()                                                                           \
    PR3_STATIC_ASSERT(sizeof(PR3_BOARD_NAME) > 1, "PR3_BOARD_NAME");                                         \
    PR3_STATIC_ASSERT(PR3_BOARD_ADDRESS_BITS == 32 || PR3_BOARD_ADDRESS_BITS == 64,                          \
                      "PR3_BOARD_ADDRESS_BITS is 32 or 64");                                                 \
    PR3_STATIC_ASSERT(PR3_BOARD_ADDRESS_BITS == sizeof(void *) * 8,                                          \
                      "PR3_BOARD_ADDRESS_BITS matches the target");                                          \
    PR3_STATIC_ASSERT(PR3_BOARD_PR3_RAM_BUDGET <= PR3_BOARD_RAM_BYTES, "RAM budget fits the board");         \
    PR3_STATIC_ASSERT(PR3_BOARD_PR3_FLASH_BUDGET <= PR3_BOARD_FLASH_BYTES, "flash budget fits the board");   \
    PR3_STATIC_ASSERT((PR3_BOARD_TRANSPORTS) != 0, "at least one transport");                                \
    PR3_STATIC_ASSERT((PR3_BOARD_CLOCK) & PR3_CLOCK_MONOTONIC, "a monotonic clock");                         \
    PR3_STATIC_ASSERT(((PR3_BOARD_STORAGE) | (PR3_BOARD_ENTROPY) | (PR3_BOARD_TRUST) | 1u) != 0,             \
                      "storage, entropy and trust are declared")

#endif /* PR3_BOARD_H */
