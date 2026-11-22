/* The freestanding demo (FND-04): the portable libraries on a board with no C library,
 * threads, files, heap or network. Memory is one static array; every allocation that
 * fails returns a status. A board's main calls pr3_demo_run once per event-loop turn. */
#include <pr3/board.h>

#include "pr3_board.h"

#include <pr3/poison.h>

PR3_BOARD_CHECK_MANIFEST();

/* Half the PR3 RAM budget; the rest is left for the stack and the board's buffers. */
static uint8_t pr3_demo_memory[PR3_BOARD_PR3_RAM_BUDGET / 2];

typedef struct pr3_demo_line {
    uint32_t len;
    char text[60];
} pr3_demo_line;

PR3_NODISCARD pr3_status pr3_demo_run(void);

pr3_status pr3_demo_run(void) {
    pr3_arena arena;
    pr3_arena_init(&arena, pr3_demo_memory, sizeof pr3_demo_memory, "demo");

    /* Lines received from the serial port wait in a ring, as they would between an
     * interrupt-fed buffer and the event loop. */
    pr3_ring lines;
    pr3_status s = pr3_ring_init(&lines, &arena, sizeof(pr3_demo_line), _Alignof(pr3_demo_line), 8);
    if (s != PR3_OK) return s;

    pr3_buf out;
    s = pr3_buf_from_arena(&arena, 256, &out);
    if (s != PR3_OK) return s;

    pr3_buf_put(&out, PR3_S("pr3 " PR3_BOARD_NAME " up at "));
    pr3_buf_put_u64(&out, pr3_board_now_ms());
    pr3_buf_put(&out, PR3_S(" ms\r\n"));

    uint8_t rx[64];
    size_t n = pr3_board_uart_poll(rx, sizeof rx);
    pr3_demo_line *line = pr3_ring_reserve(&lines);
    if (line != NULL && n > 0) {
        pr3_str text;
        if (pr3_str_from_bytes((pr3_bytes){rx, n}, &text) != PR3_OK) return PR3_E_UTF8;
        pr3_str kept = pr3_str_clip(text, sizeof line->text);
        memcpy(line->text, kept.ptr, kept.len);
        line->len = (uint32_t)kept.len;
        pr3_ring_publish(&lines);
    }
    for (pr3_demo_line *l; (l = pr3_ring_peek(&lines)) != NULL; pr3_ring_consume(&lines)) {
        pr3_buf_put(&out, PR3_S("echo "));
        pr3_buf_put_bytes(&out, l->text, l->len);
        pr3_buf_put(&out, PR3_S("\r\n"));
    }

    pr3_str msg;
    s = pr3_buf_take(&out, &msg);
    if (s != PR3_OK) return s;
    const uint8_t *p = (const uint8_t *)msg.ptr;
    size_t left = msg.len;
    for (unsigned tries = 0; left > 0 && tries < 1000; tries++) { /* bounded: never blocks the loop */
        size_t sent = pr3_board_uart_write(p, left);
        p += sent;
        left -= sent;
        pr3_board_watchdog_kick();
    }
    pr3_board_log(PR3_S("demo turn done"));
    return left == 0 ? PR3_OK : PR3_E_IO;
}
