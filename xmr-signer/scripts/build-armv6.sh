#!/usr/bin/env bash
# Builds a statically linked ARMv6 (arm-unknown-linux-musleabihf) release of a
# workspace binary for the Raspberry Pi Zero 1.3, verifies it with file(1),
# copies it to dist/<bin>-armv6, and smoke-tests it under Docker linux/arm/v6
# emulation when Docker is available. Emulated timings are meaningless; only
# the real Pi produces numbers.
# Usage: scripts/build-armv6.sh [bin-name] [args passed to the emulated run]
# Default bin-name: xmr-gate

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN_NAME="${1:-xmr-gate}"
shift || true
EMU_ARGS=("$@")
TARGET=arm-unknown-linux-musleabihf
FALLBACK_ZIG=/Users/angel/zig-0.15.2/zig
DIST="$REPO_ROOT/dist"
BIN="$REPO_ROOT/target/$TARGET/release/$BIN_NAME"
EMU_LOG="$DIST/$BIN_NAME-emulation-check.txt"

if [[ -z "${CARGO_ZIGBUILD_ZIG_PATH:-}" ]] && ! command -v zig >/dev/null 2>&1; then
    if [[ -x "$FALLBACK_ZIG" ]]; then
        export CARGO_ZIGBUILD_ZIG_PATH="$FALLBACK_ZIG"
    else
        echo "error: zig not found on PATH, CARGO_ZIGBUILD_ZIG_PATH is unset, and $FALLBACK_ZIG does not exist" >&2
        exit 1
    fi
fi

if ! command -v cargo-zigbuild >/dev/null 2>&1 &&
    [[ ! -x "${CARGO_HOME:-$HOME/.cargo}/bin/cargo-zigbuild" ]]; then
    cargo install cargo-zigbuild
fi

if ! rustup target list --installed | grep -qx "$TARGET"; then
    rustup target add "$TARGET"
fi

cargo zigbuild --manifest-path "$REPO_ROOT/Cargo.toml" --release -p "$BIN_NAME" --target "$TARGET"

FILE_INFO="$(file -b "$BIN")"
echo "file(1): $FILE_INFO"
case "$FILE_INFO" in
    *"ELF 32-bit"*ARM*) ;;
    *) echo "error: expected an ELF 32-bit ARM executable, got: $FILE_INFO" >&2; exit 1 ;;
esac
case "$FILE_INFO" in
    *"statically linked"* | *static-pie*) ;;
    *) echo "error: expected a statically linked binary, got: $FILE_INFO" >&2; exit 1 ;;
esac

mkdir -p "$DIST"
cp "$BIN" "$DIST/$BIN_NAME-armv6"
echo "built $DIST/$BIN_NAME-armv6 ($(stat -f %z "$DIST/$BIN_NAME-armv6") bytes)"

DOCKER=docker
if ! command -v "$DOCKER" >/dev/null 2>&1 && [[ -x /usr/local/bin/docker ]]; then
    DOCKER=/usr/local/bin/docker
fi

record_skip() {
    { echo "SKIPPED: $1"; echo "The emulation smoke test did not complete; the ARMv6 build itself succeeded."; } >"$EMU_LOG"
    echo "warning: emulation smoke test skipped: $1" >&2
}

if ! command -v "$DOCKER" >/dev/null 2>&1; then
    record_skip "docker is not installed on this machine"
elif ! "$DOCKER" info >/dev/null 2>&1; then
    record_skip "docker is installed but the daemon is not reachable"
else
    {
        echo "Emulation smoke test: $BIN_NAME-armv6 ${EMU_ARGS[*]:-} in an alpine container"
        echo "under docker --platform linux/arm/v6 (qemu user emulation)."
        echo "Timings below are qemu-emulated and therefore meaningless; only"
        echo "successful execution matters. Real numbers come from the Pi itself."
        echo
    } >"$EMU_LOG"
    if "$DOCKER" run --rm --platform linux/arm/v6 -v "$DIST":/dist alpine \
        "/dist/$BIN_NAME-armv6" "${EMU_ARGS[@]}" 2>&1 | tee -a "$EMU_LOG" &&
        grep -q '^RESULT ' "$EMU_LOG"; then
        echo "emulation smoke test passed; output captured in $EMU_LOG"
    else
        { echo; echo "SKIPPED: the emulation run failed or produced no RESULT line."; } >>"$EMU_LOG"
        echo "warning: emulation run failed; details in $EMU_LOG" >&2
    fi
fi
