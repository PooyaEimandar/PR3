/* The JSON module's private interface: the number scanner, shared by the parser and the
 * CBMC harness of the number path. */
#ifndef PR3_JSON_INTERNAL_H
#define PR3_JSON_INTERNAL_H

#include <pr3/json.h>

/* Scans the number that starts at p (p < end, *p is '-' or a digit) into *tok and sets
 * *after to the byte after it. PR3_E_INVALID, with *reason, if the bytes there are not a
 * JSON number or the number is beyond the range of a double; *after then points at the
 * offending byte. */
PR3_NODISCARD pr3_status pr3_json_scan_number(const uint8_t *p, const uint8_t *end, const uint8_t **after,
                                              pr3_json_token *tok, pr3_str *reason);

#endif
