/* Canonical JSON on a board (doc/11, "The board contract"): links the json module with
 * no C library, so the symbols it leaves undefined are listed and checked like the
 * demo's. It is a link check, not part of the accessory image or its budget. */
#include <pr3/board.h>
#include <pr3/json.h>

#include "pr3_board.h"

#include <pr3/poison.h>

static uint8_t pr3_json_link_memory[4096];
static char pr3_json_link_out[256];

PR3_NODISCARD pr3_status pr3_json_link_run(void);

pr3_status pr3_json_link_run(void) {
    static const char in[] = "{\"b\":[1,2.5,-3e-7,18446744073709551615],\"a\":\"caf\\u00e9\"}";
    pr3_arena arena;
    pr3_arena_init(&arena, pr3_json_link_memory, sizeof pr3_json_link_memory, "json");
    pr3_buf b = pr3_buf_make(pr3_json_link_out, sizeof pr3_json_link_out);
    pr3_status s = pr3_json_canonicalize(&arena, (pr3_bytes){(const uint8_t *)in, sizeof in - 1}, &b, NULL);
    if (s == PR3_OK) pr3_board_log((pr3_str){b.ptr, b.len}); /* canonicalize reports a full buffer */
    return s;
}
