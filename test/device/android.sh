#!/bin/sh
# Runs test and benchmark runners on an Android device or emulator over adb (FND-02):
# pushes each runner, the golden vectors and the benchmark workloads, runs them, and
# collects every log and exit code.
#
#   test/device/android.sh OUT_DIR RUNNER...
#
# ANDROID_SERIAL picks the device when more than one is attached. Benchmark runners
# (bench_*) get PR3_BENCH_ARGS, e.g. "--runs 30" or "--runs 3 --min-ms 1" for a smoke
# run. Exits non-zero if any runner fails or cannot be run.
set -eu
[ $# -ge 2 ] || { echo "usage: $0 OUT_DIR RUNNER..." >&2; exit 2; }
root=$(cd "$(dirname "$0")/../.." && pwd)
out=$1
shift
adb=${ADB:-adb}
dir=/data/local/tmp/pr3
mkdir -p "$out"

"$adb" wait-for-device
{
  echo "model: $("$adb" shell getprop ro.product.model | tr -d '\r')"
  echo "android: $("$adb" shell getprop ro.build.version.release | tr -d '\r') (API $("$adb" shell getprop ro.build.version.sdk | tr -d '\r'))"
  echo "build: $("$adb" shell getprop ro.build.fingerprint | tr -d '\r')"
  echo "abi: $("$adb" shell getprop ro.product.cpu.abi | tr -d '\r')"
  echo "emulator: $("$adb" shell getprop ro.kernel.qemu | tr -d '\r')"
} > "$out/device.txt"
cat "$out/device.txt"

"$adb" shell rm -rf "$dir"
"$adb" shell mkdir -p "$dir"
"$adb" push -q "$root/test/vectors" "$dir/vectors" >/dev/null
"$adb" push -q "$root/bench/c/workloads" "$dir/workloads" >/dev/null

failed=0
for runner in "$@"; do
  name=$(basename "$runner")
  "$adb" push -q "$runner" "$dir/$name" >/dev/null
  "$adb" shell chmod 755 "$dir/$name"
  case $name in
    bench_*) args="--workloads $dir/workloads ${PR3_BENCH_ARGS:-}" ;;
    *) args="--vectors $dir/vectors" ;;
  esac
  # adb shell's own exit status is unreliable on old devices; the runner's is echoed.
  "$adb" shell "cd $dir && ./$name $args; echo \"exit=\$?\"" > "$out/$name.log" 2>&1 || true
  code=$(sed -n 's/^exit=\([0-9]*\).*/\1/p' "$out/$name.log" | tail -n 1)
  passed=$(grep -c '^ok ' "$out/$name.log" || true)
  echo "$name: exit ${code:-none}, $passed passed (log: $out/$name.log)"
  [ "${code:-1}" = 0 ] || failed=1
done
exit $failed
