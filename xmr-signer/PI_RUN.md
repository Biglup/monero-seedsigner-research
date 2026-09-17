# Running the day-one gate on a Raspberry Pi Zero 1.3

`dist/xmr-gate-armv6` is a statically linked ARMv6 (hard-float, musl) build
of `xmr-gate`, targeting `arm-unknown-linux-musleabihf`. It embeds the 256
`generate_key_image` vectors from monero-project/monero and computes each key
image with monero-oxide's `monero-wallet` crate, exactly as that crate does in
`SignableTransaction::sign`. It reads nothing from disk and writes nothing.

Rebuild from source with `scripts/build-armv6.sh xmr-gate 1` on the
development machine (needs rustup target `arm-unknown-linux-musleabihf`,
cargo-zigbuild and zig 0.15).

## Steps

Copy the binary to the Pi (SSH, or by microSD if the Pi has no network):

```
scp dist/xmr-gate-armv6 pi@<host>:
```

On the Pi:

```
chmod +x ./xmr-gate-armv6
uname -a; grep Model /proc/cpuinfo; grep MemTotal /proc/meminfo
./xmr-gate-armv6 20
```

The argument is the number of passes over the 256-vector set. Copy the
whole `RESULT` line back verbatim; `ok` must equal `256 * passes` and `bad`
must be 0. Timings are microseconds per key image; `peak_rss_raw` is in
kilobytes on Linux.

## Results

| Run | ok | bad | ki median us | peak RSS |
|---|---|---|---|---|
| Host, macOS arm64, 4 passes | 1024 | 0 | 53 | 1.6 MB |
| Docker linux/arm/v6 qemu, 1 pass (emulated, timing meaningless) | 256 | 0 | 1223 | 11.7 MB (inflated by qemu) |
| Pi Zero 1.3 (BCM2835, 6.18.50+rpt-rpi-v6, 437132 kB MemTotal), 20 passes | 5120 | 0 | 3891 (min 3870, max 34150) | 1488 kB |

Emulation output is committed as `dist/xmr-gate-emulation-check.txt`; the
on-device run, including the `/proc/cpuinfo` identification, as
`dist/pi-gate-results.txt`. The Pi has no clock source, so the timestamp in
that file is wrong; the run happened on 2026-09-17.

Toolchain: rustc 1.96.0 (ac68faa20 2026-05-25), cargo-zigbuild 0.23.0, zig 0.15.2, target arm-unknown-linux-musleabihf, release
profile opt-level 3 + LTO. monero-oxide commit 731657ae3385be667abb556266369a497bc86f13.

## Benchmark card setup (Pi Zero 1.3 has no network)

The Pi Zero 1.3 has no wireless and no Ethernet. The benchmark card is
Raspberry Pi OS Lite (32-bit, 2026-09-15 trixie) with the Pi's USB port in
gadget mode, so a single data cable to the development machine carries
power and an Ethernet link. `scripts/flash-bench-card.sh` writes the image
and applies `pi/bootfs-overlay/` to the FAT boot partition; nothing else is
needed. `scripts/apply-bootfs-overlay.sh` re-applies the overlay to an
already flashed card.

What the overlay does, and why each piece exists:

- `config.txt`: `dtoverlay=dwc2` under `[all]` puts the USB controller in
  device mode. Note the stock file already has `dtoverlay=dwc2,dr_mode=host`
  in its `[cm5]` section; an "already present" check must match the exact
  line or it will skip the append (that bug cost one boot cycle here).
- `cmdline.txt`: `modules-load=dwc2,g_ether` loads the Ethernet gadget.
- `xmr-firstrun.sh`, armed once via `systemd.run=` on the kernel command
  line (the same systemd-run-generator mechanism the Raspberry Pi Imager
  uses): runs the gate, creates user `pi` with the development machine's
  SSH key, enables SSH, writes a NetworkManager profile for `usb0` in
  shared mode (the Pi is 10.42.0.1 and serves DHCP to the host), removes
  itself from `cmdline.txt` and reboots. It writes `xmr-gate-result.txt`,
  `xmr-firstrun.log` and `xmr-firstrun-diag.txt` to the boot partition, so
  results come back even if the link never comes up.
- NetworkManager ships `85-nm-unmanaged.rules`, which marks USB gadget
  interfaces unmanaged (`ENV{DEVTYPE}=="gadget", ENV{NM_UNMANAGED}="1"`), so
  a connection profile alone never activates. The overlay adds a
  `[device-usb0] managed=1` drop-in and a later udev rule that clears the
  flag. This was the reason the first two boots enumerated on the host but
  never brought the link up.
- cloud-init `user-data`/`meta-data` are also applied for the very first
  boot of a fresh image, but cloud-init on this image did not re-run after
  an `instance_id` bump, so nothing depends on it.

After the setup reboot: `ssh pi@10.42.0.1` (or `pi@raspberrypi.local`).
The card is measurement infrastructure only; the SeedSigner image never
carries any of this.


## Running spike-bench on the Pi

`dist/spike-bench-armv6` runs the whole pipeline over the committed fixtures
(one directory per wallet snapshot: `outputs.bin`, `unsigned_2in.bin`,
`unsigned_16in.bin`), 20 measured iterations per phase after one warmup, and
prints per-phase min / median / mean / max in microseconds, payload sizes,
animated-QR frame counts at 30, 120 and 150 bytes per fragment, peak RSS and a
machine-readable `RESULT` line. Rebuild with
`scripts/build-armv6.sh spike-bench` (the emulation smoke test needs the
fixtures mounted, so it is skipped for this binary).

The seed of the stagenet test wallet is needed to decrypt the fixtures. It is
not in the repository; ask the author, or reproduce the fixtures with your own
stagenet wallet (scripts/fanout.py, scripts/snapshot_txs.py). Copy binary and
fixtures to RAM-backed storage on the Pi so nothing touches the card. Use
`/tmp` (tmpfs on this image), not `/dev/shm`: systemd-logind's `RemoveIPC=yes`
deletes a user's `/dev/shm` files whenever their last SSH session ends, which
silently discarded two earlier runs here.

```
ssh pi@<host> 'mkdir -p /tmp/xmr/fixtures'
scp dist/spike-bench-armv6 pi@<host>:/tmp/xmr/
scp -r fixtures/snap50 fixtures/snap200 fixtures/snap500 pi@<host>:/tmp/xmr/fixtures/
ssh pi@<host>
cd /tmp/xmr && chmod +x spike-bench-armv6
XMR_SEED='word1 ... word25' ./spike-bench-armv6 fixtures --iters 20
```

The run takes a few minutes per snapshot (the 16-input signing alone is about
10 s per iteration). Raw on-device outputs are committed as
`dist/pi-spike-bench-*.txt`.
