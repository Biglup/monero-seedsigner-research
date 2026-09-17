#!/usr/bin/env bash
# Writes Raspberry Pi OS Lite to a microSD card and applies the benchmark
# overlay (cloud-init user-data, SSH enable, USB gadget ethernet, xmr-gate
# binary). Destructive to the target disk. Run with sudo on macOS.
#
# Usage: sudo scripts/flash-bench-card.sh <disk id, e.g. disk12> <path/to/raspios.img>
#
# Image used for this spike: 2026-09-15-raspios-trixie-armhf-lite.img.xz from
# https://downloads.raspberrypi.com/raspios_lite_armhf/images/raspios_lite_armhf-2026-09-15/
# sha256 c766b3fb279b95c12cb4dd22d06f8eab31972c372675d05bd0ca95b060523a7f

set -euo pipefail

DISK="${1:?disk id required, e.g. disk12}"
IMG="${2:?path to decompressed .img required}"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OVERLAY="$REPO_ROOT/pi/bootfs-overlay"
GATE="$REPO_ROOT/dist/xmr-gate-armv6"

[[ $EUID -eq 0 ]] || { echo "error: run with sudo" >&2; exit 1; }
[[ -f "$IMG" ]] || { echo "error: image not found: $IMG" >&2; exit 1; }
[[ -f "$GATE" ]] || { echo "error: $GATE missing, run scripts/build-armv6.sh first" >&2; exit 1; }
[[ -f "$OVERLAY/user-data" ]] || { echo "error: overlay missing" >&2; exit 1; }

INFO="$(diskutil info "/dev/$DISK")"
grep -q 'Device Location: *External' <<<"$INFO" || { echo "error: /dev/$DISK is not an external disk, refusing" >&2; exit 1; }
grep -q 'Removable Media: *Removable' <<<"$INFO" || { echo "error: /dev/$DISK is not removable media, refusing" >&2; exit 1; }
echo "target: /dev/$DISK"
diskutil list "/dev/$DISK"
echo "writing $IMG ($(stat -f %z "$IMG") bytes) in 5 seconds, ctrl-c to abort"
sleep 5

diskutil unmountDisk "/dev/$DISK"
dd if="$IMG" of="/dev/r$DISK" bs=4m status=progress
sync

echo "waiting for the boot partition to mount"
for _ in $(seq 1 30); do
    [[ -d /Volumes/bootfs ]] && break
    sleep 1
done
if [[ ! -d /Volumes/bootfs ]]; then
    diskutil mountDisk "/dev/$DISK" || true
    sleep 3
fi
[[ -f /Volumes/bootfs/cmdline.txt ]] || { echo "error: /Volumes/bootfs did not appear" >&2; exit 1; }
B=/Volumes/bootfs

cp "$OVERLAY/user-data" "$B/user-data"
touch "$B/ssh"
if ! grep -q '^dtoverlay=dwc2$' "$B/config.txt"; then
    printf '\n# USB gadget mode for the benchmark link (Pi Zero USB port)\ndtoverlay=dwc2\n' >>"$B/config.txt"
fi
if ! grep -q 'modules-load=dwc2,g_ether' "$B/cmdline.txt"; then
    sed -i '' 's/rootwait/rootwait modules-load=dwc2,g_ether/' "$B/cmdline.txt"
fi
cp "$GATE" "$B/xmr-gate-armv6"
cp "$OVERLAY/meta-data" "$B/meta-data"
cp "$OVERLAY/xmr-firstrun.sh" "$B/xmr-firstrun.sh"
# Arm the one-shot setup (systemd-run-generator, as the Raspberry Pi Imager does).
if ! grep -q 'systemd.run=' "$B/cmdline.txt"; then
    printf '%s systemd.run=/boot/firmware/xmr-firstrun.sh systemd.run_success_action=reboot systemd.unit=kernel-command-line.target\n' "$(tr -d '\n' <"$B/cmdline.txt")" >"$B/cmdline.txt"
fi
dot_clean -m "$B" 2>/dev/null || true
sync

echo "--- config.txt tail ---"; tail -3 "$B/config.txt"
echo "--- cmdline.txt ---"; cat "$B/cmdline.txt"
echo "--- bootfs additions ---"; ls -la "$B/user-data" "$B/ssh" "$B/xmr-gate-armv6"
diskutil eject "/dev/$DISK"
echo "done: card ejected. Boot the Pi from its USB (data) port. First boot runs the gate and setup, then reboots; allow five minutes, then: ssh pi@10.42.0.1"
