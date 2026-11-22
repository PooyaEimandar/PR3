#!/bin/sh
# Banned functions in the portable libraries (doc/03, "Banned functions"), for code
# that slips past pr3/poison.h: a call in a file that forgot the include, or before it.
# Also checks that every portable .c includes pr3/poison.h as its last #include.
# src/platform/ is exempt: it is where the heap and the OS are allowed. So is
# src/json/freestanding/, the hosted declarations Ryu's own source needs on boards.
set -eu
cd "$(dirname "$0")/.."

banned='malloc|calloc|realloc|free|strdup|strndup|aligned_alloc|strcpy|strcat|strncpy|strncat'
banned="$banned|sprintf|vsprintf|snprintf|vsnprintf|gets|strtok|strlen|strcmp|strncmp|atoi|atol|atoll"
banned="$banned|strtol|strtoul|strtoll|strtoull|strtod|sscanf|printf|fprintf|vprintf|vfprintf|puts"
banned="$banned|fputs|fopen|rand|srand|time|clock|setjmp|longjmp|signal|exit|abort"

files=$(find src include -type f \( -name '*.c' -o -name '*.h' \) ! -path 'src/platform/*' ! -path 'src/json/freestanding/*' ! -name poison.h | sort)
status=0

for f in $files; do
  # Drop // comments, one-line /* */ comments and comment continuation lines, then look
  # for a banned name used as a call.
  hits=$(sed -e 's://.*$::' -e 's:/\*.*\*/::g' -e 's:^[[:space:]]*\*.*$::' "$f" |
    grep -nE "(^|[^A-Za-z0-9_.>])($banned)[[:space:]]*\(" || true)
  if [ -n "$hits" ]; then
    echo "$hits" | sed "s|^|$f:|"
    status=1
  fi
done

for f in $(echo "$files" | grep '^src/.*\.c$' || true); do
  last=$(grep -E '^[[:space:]]*#[[:space:]]*include' "$f" | tail -n 1 | tr -d ' \t')
  if [ "$last" != '#include<pr3/poison.h>' ]; then
    echo "$f: the last #include must be <pr3/poison.h>"
    status=1
  fi
done

[ $status -eq 0 ] && echo "check-banned: $(echo "$files" | wc -l | tr -d ' ') files clean"
exit $status
