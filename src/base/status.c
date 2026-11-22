/* Status codes and their text. */
#include <pr3/base.h>

#include <pr3/poison.h>

pr3_str pr3_status_text(pr3_status s) {
    switch (s) {
    case PR3_OK: return PR3_S("ok");
    case PR3_E_NOMEM: return PR3_S("out of memory");
    case PR3_E_LIMIT: return PR3_S("limit exceeded");
    case PR3_E_INVALID: return PR3_S("invalid input");
    case PR3_E_UTF8: return PR3_S("invalid UTF-8");
    case PR3_E_STATE: return PR3_S("not allowed now");
    case PR3_E_NOT_FOUND: return PR3_S("not found");
    case PR3_E_IO: return PR3_S("I/O error");
    case PR3_E_CRYPTO: return PR3_S("verification failed");
    case PR3_E_CANCELLED: return PR3_S("cancelled");
    }
    return PR3_S("unknown status");
}
