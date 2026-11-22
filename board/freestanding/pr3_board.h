/* The manifest of the freestanding demo: a generic 32-bit core with a serial port and
 * nothing else, the smallest board the contract allows (doc/11). */
#ifndef PR3_BOARD_MANIFEST_H
#define PR3_BOARD_MANIFEST_H

#define PR3_BOARD_NAME "freestanding"
#define PR3_BOARD_ADDRESS_BITS 32
#define PR3_BOARD_RAM_BYTES (264u * 1024u)
#define PR3_BOARD_FLASH_BYTES (2u * 1024u * 1024u)
#define PR3_BOARD_PR3_RAM_BUDGET (32u * 1024u)
#define PR3_BOARD_PR3_FLASH_BUDGET (96u * 1024u)
#define PR3_BOARD_TRANSPORTS (PR3_TRANSPORT_UART)
#define PR3_BOARD_STORAGE (PR3_STORAGE_NONE)
#define PR3_BOARD_CLOCK (PR3_CLOCK_MONOTONIC)
#define PR3_BOARD_ENTROPY (PR3_ENTROPY_NONE)
#define PR3_BOARD_TRUST (PR3_TRUST_NONE)

#endif
