#!/usr/bin/env bash
# Re-applies the benchmark overlay to an already flashed card mounted at
# /Volumes/bootfs (no root needed: the FAT partition is user-writable).
# Usage: scripts/apply-bootfs-overlay.sh
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
O="$REPO_ROOT/pi/bootfs-overlay"
B=/Volumes/bootfs
[[ -f $B/cmdline.txt ]] || { echo "error: $B not mounted" >&2; exit 1; }
cp "$O/user-data" "$B/user-data"
cp "$O/meta-data" "$B/meta-data"
touch "$B/ssh"
grep -q '^dtoverlay=dwc2$' "$B/config.txt" || printf '\n# USB gadget mode for the benchmark link (Pi Zero USB port)\ndtoverlay=dwc2\n' >>"$B/config.txt"
grep -q 'modules-load=dwc2,g_ether' "$B/cmdline.txt" || sed -i '' 's/rootwait/rootwait modules-load=dwc2,g_ether/' "$B/cmdline.txt"
cp "$REPO_ROOT/dist/xmr-gate-armv6" "$B/xmr-gate-armv6"
cp "$O/xmr-firstrun.sh" "$B/xmr-firstrun.sh"
rm -f "$B/xmr-gate-result.txt" "$B/xmr-firstrun.log" "$B/xmr-firstrun-diag.txt" "$B/xmr-boot-diag.txt"
# Arm the one-shot setup (systemd-run-generator; same mechanism the Raspberry Pi Imager uses).
if ! grep -q 'systemd.run=' "$B/cmdline.txt"; then
    printf '%s systemd.run=/boot/firmware/xmr-firstrun.sh systemd.run_success_action=reboot systemd.unit=kernel-command-line.target\n' "$(tr -d '\n' <"$B/cmdline.txt")" >"$B/cmdline.txt"
fi
dot_clean -m "$B" 2>/dev/null || true
sync
echo "--- config.txt (dwc2 lines) ---"; grep -n 'dwc2\|^\[' "$B/config.txt"
echo "--- cmdline.txt ---"; cat "$B/cmdline.txt"
echo "--- previous result? ---"; [[ -f $B/xmr-gate-result.txt ]] && cat "$B/xmr-gate-result.txt" || echo "none"
