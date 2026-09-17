# Monero SeedSigner research spike: report

Status: stage 1 in progress. Sections marked "pending" are not yet measured.

## 1. Summary

Pending.

## 2. Measurements

Pending (see SPEC.md section 5 for the table layout).

## 3. Findings

Format survey (2026-09-17, before any fixtures exist), full detail in
`xmr-signer/FORMATS.md`:

- The four Keystone UR types are bare CBOR byte strings around wallet2's own
  cold-signing blobs (magic prefix + view-key encryption + monero binary_archive).
  There is no Keystone-specific structure to implement; the work is wallet2 format
  compatibility.
- Every payload is encrypted with ChaCha20 under a key that is CryptoNight v0 of the
  view secret key, and authenticated with a Monero Schnorr signature by the view key.
  The device must run CryptoNight once per payload; its ARMv6 cost is a stage 1
  measurement.
- Under today's protocol the cold side constructs the entire transaction, including
  the Bulletproof+ range proof and all CLSAGs, from ring data supplied by the hot
  wallet. The fee is implied by amounts, not passed. This rules out monero-oxide's
  high-level `SignableTransaction` builder (fee-rate driven) and means building the
  transaction from its primitives, which monero-oxide does expose.
- Feather can save every payload to a file (outputs, key images, unsigned tx via the
  advanced send dialog, signed tx) and import all three replies from a file, so stage 1
  runs without a camera. Feather can also act as the cold wallet, giving reference
  blobs for the same fixtures.
- Feather 2.8.1 mac-arm64 release verified: GPG signature good from the FeatherWallet
  key shipped in the Feather repository (fingerprint ending CEFBA71C), sha256
  a35f19be74ca59bad96f7331d3ca8e4f56ec47e82c193f5fc50ddbebce017233.
- Cupcake handles the same four UR type strings and delegates to monero_c (wallet2),
  so unchanged compatibility is expected; verified only in stage 2.
- The hot wallet in the Feather wizard must be view-only. A Feather wallet that
  holds the spend key already knows every key image, so its default outputs export
  (`all = false`, unknown key images only) is empty and the round trip is
  meaningless. The demo and the fixtures use a Feather wallet restored from the
  primary address and the private view key.
- Output history is built by self-sends from monero-wallet-rpc 0.18.5.1 on the same
  seed (`xmr-signer/scripts/fanout.py`): each transaction pays 15 own subaddresses
  plus change, 16 owned outputs per transaction, waiting ten blocks between rounds.
  Spent outputs still count for the view-only export, so the 50, 200 and 500
  snapshots are taken from one wallet as its history grows.
- Signer core, first half validated (2026-09-17): `libmonero-signer` decrypts and
  parses a real wallet2 outputs export (33 outputs, from monero-wallet-rpc 0.18.5.1
  on the test wallet), recovers every one-time key including subaddress outputs and
  outputs whose derivation goes through additional tx public keys, and produces key
  images identical to wallet2's for all 33. wallet2's own key image signatures
  verify under the crate's one-member ring signature verifier and vice versa. The
  crate's `xmr-keyimage` reply blob was imported unmodified by the official
  monero-wallet-cli into a view-only wallet, whose balance then matched the full
  wallet. CryptoNight v0 (Cuprate's pure-Rust crate) matches Monero's slow-hash
  vectors. The crate cross-compiles for arm-unknown-linux-musleabihf.
- Payload size depends on how outputs were received. Paying 15 subaddresses in one
  transaction attaches 16 additional tx public keys and every exported output of
  that transaction carries all of them: about 580 bytes per output versus about 80
  for a plain two-output transaction. The first two fan-out transactions used
  subaddress destinations; the remaining history is built with repeated payments to
  the primary address so the 500-output export is representative of a normal
  wallet, and the subaddress case is reported as the worst case.
- Frame counts: Feather emits 150-byte fragments at 80 ms; the SeedSigner shell
  displays 30-byte fragments at its default density (10 low, 120 high).

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
