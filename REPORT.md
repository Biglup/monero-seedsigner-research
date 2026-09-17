# Monero SeedSigner research spike: report

Status: stage 1 in progress. Sections marked "pending" are not yet measured.

## 1. Summary

Pending.

## 2. Measurements

Pending (see SPEC.md section 5 for the table layout).

## 3. Findings

Pending.

## 4. Decision record

### Rust (monero-oxide) vs C++ (wallet2): Rust

Day-one gate, 2026-09-17. monero-oxide's `monero-wallet` crate (commit
731657ae3385be667abb556266369a497bc86f13, default branch) cross-compiles to a
static `arm-unknown-linux-musleabihf` binary with cargo-zigbuild (zig
0.15.2, rustc 1.96.0 (ac68faa20 2026-05-25)) and runs on the Pi Zero 1.3 (BCM2835,
kernel 6.18.50+rpt-rpi-v6, 437132 kB MemTotal). The gate binary
`xmr-signer/crates/xmr-gate` computed 5120 key images (20 passes over the 256
`generate_key_image` vectors from monero-project/monero) with 0 mismatches,
median 3891 us per key image, peak RSS 1488 kB. Raw output:
`xmr-signer/dist/pi-gate-results.txt`. The C++ wallet2 fallback was not
needed and was not attempted.

### Pi Zero 1.3 vs Pi Zero 2 W: Pi Zero 1.3

Kept. The gate ran natively on ARMv6 in the target memory budget, so there is
no reason to move to aarch64. Reconsider only if signing at 16 inputs turns
out to exceed the budget in stage 1 step 4.

### Benchmark transport

The Pi Zero 1.3 has no network. Benchmarks run on a Raspberry Pi OS card with
the USB port in gadget mode over a single cable; see `xmr-signer/PI_RUN.md`.
This is measurement infrastructure, not part of the device image.

## 5. FCMP++ paragraph

Pending.

## 6. Links

Repo: this directory. Video: pending.
