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
| Pi Zero 1.3, 20 passes | pending | | | |

Emulation output is committed as `dist/xmr-gate-emulation-check.txt`.
