#!/usr/bin/env bash
# Builds the Android app. With no arguments: a debug APK for arm64.
#
# llama.cpp's CPU kernels are built for ARMv8.2 with int8 dot products, which every
# arm64 phone core since about 2018 has; the bindings' default, plain ARMv8.0, made
# 4-bit models about 2.5 times slower. The flag is set here and not in .cargo/config.toml
# because llama-cpp-sys-2 passes every GGML_* variable to every target, Apple ones too.
# Its CMake step does not reconfigure an existing build, so after changing the flag run
#   cargo clean -p llama-cpp-sys-2 --target aarch64-linux-android
#
# Usage: script/build-android.sh [cargo tauri android build arguments]
set -euo pipefail

: "${ANDROID_HOME:=$HOME/Library/Android/sdk}"
if [ -z "${NDK_HOME:-}" ]; then
  NDK_HOME="$(ls -d "$ANDROID_HOME"/ndk/* 2>/dev/null | sort -V | tail -1)"
fi
if [ -z "${JAVA_HOME:-}" ] && [ -x /usr/libexec/java_home ]; then
  JAVA_HOME="$(/usr/libexec/java_home -v 21)"
fi
export ANDROID_HOME NDK_HOME JAVA_HOME
export ANDROID_NDK="$NDK_HOME" ANDROID_API_LEVEL="${ANDROID_API_LEVEL:-24}"
export GGML_CPU_ARM_ARCH=armv8.2-a+dotprod

if [ $# -eq 0 ]; then
  set -- --debug --apk --target aarch64
fi
cd "$(dirname "$0")/../app/pr3-app"
exec cargo tauri android build "$@"
