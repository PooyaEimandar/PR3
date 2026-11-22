/* Banned functions (doc/03, "Banned functions"). The last include of every portable
 * .c under src/ except src/platform/: using one of these after this point is a
 * compile error. tools/check-banned.sh repeats the check for code that slips past. */
#ifndef PR3_POISON_H
#define PR3_POISON_H

#if defined(__GNUC__) || defined(__clang__)
/* With _FORTIFY_SOURCE some C libraries define these as macros over checked builtins;
 * a poisoned name cannot be a macro, and nothing after this point may use them. */
#undef strcpy
#undef strcat
#undef strncpy
#undef strncat
#undef sprintf
#undef vsprintf
#undef snprintf
#undef vsnprintf
#undef printf
#undef fprintf
#undef vprintf
#undef vfprintf
/* Allocation: arenas and pools instead. */
#pragma GCC poison malloc calloc realloc free strdup strndup aligned_alloc
/* Unbounded or truncating string functions: pr3_buf and pr3_str instead. */
#pragma GCC poison strcpy strcat strncpy strncat sprintf vsprintf snprintf vsnprintf gets strtok
/* NUL-terminated scanning and locale-dependent parsing: pr3_str_eq, pr3_parse_u64 and
 * the JSON number parser instead. */
#pragma GCC poison strlen strcmp strncmp atoi atol atoll strtol strtoul strtoll strtoull strtod sscanf
/* Output and files: the host's log and file callbacks instead. */
#pragma GCC poison printf fprintf vprintf vfprintf puts fputs fopen
/* Hidden global state: the host's entropy and clock instead. */
#pragma GCC poison rand srand time clock
/* Non-local exits: statuses, and PR3_CHECK for broken invariants. */
#pragma GCC poison setjmp longjmp signal exit abort
#elif defined(_MSC_VER)
#pragma deprecated(malloc, calloc, realloc, free, _strdup, strcpy, strcat, strncpy, strncat)
#pragma deprecated(sprintf, vsprintf, snprintf, vsnprintf, strtok, strlen, strcmp, strncmp)
#pragma deprecated(atoi, atol, strtol, strtoul, strtod, sscanf, printf, fprintf, puts, fopen)
#pragma deprecated(rand, srand, time, clock, setjmp, longjmp, signal, exit, abort)
#endif

#endif /* PR3_POISON_H */
