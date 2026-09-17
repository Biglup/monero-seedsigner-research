# Monero SeedSigner research spike: report

Status: stage 1 complete (numbers, fixtures, signer). Stage 2 (camera round trip, SeedSigner UI, video) not started.

## 1. Summary

A Raspberry Pi Zero 1.3 (single ARMv6 core, 512 MB, no wireless) can act as a
Monero cold signer for an unmodified Feather Wallet 2.8.1. The signer core,
`libmonero-signer`, is about 1500 lines of Rust on top of monero-oxide. It takes
wallet2's own cold-signing payloads, the ones Feather's offline signing wizard wraps
in the Keystone `xmr-*` UR types, decrypts them with the view key, computes key
images, and constructs and signs the transaction on the device (Bulletproof+ and
CLSAGs included), returning wallet2's signed set. Feather imported such a set from a
file and broadcast it; the transaction was mined on stagenet. On the device, a
2-input transaction signs in about 4.5 s and a 16-input one in about 10 s, with
about 4 MB peak RSS; key images cost about 32 ms per output plus a fixed 0.7 s
CryptoNight per payload. What this does not prove: the camera round trip and its
scan times (stage 2), the SeedSigner UI integration, mainnet, FCMP++.

## 2. Measurements

All numbers from the Raspberry Pi Zero Rev 1.3 (BCM2835, ARMv6, 437132 kB MemTotal,
kernel 6.18.50+rpt-rpi-v6), static `arm-unknown-linux-musleabihf` release build of
spike-bench, 20 measured iterations after one warmup. Raw outputs, including the
per-phase min / mean / max, in `xmr-signer/dist/pi-spike-bench-snap{50,200,500}.txt`.
Wallet sizes: 51, 215 and 521 received outputs; balance about 0.09 XMR throughout
(stagenet). QR frames are UR fountain sequence lengths, the minimum frames to
display, at the SeedSigner shell's default 30 bytes per fragment, with the value at
120 bytes per fragment (SeedSigner "high" density) in parentheses. Feather displays
at 150 bytes per fragment. Peak RSS is for the whole spike-bench process.

Note on the 51-output export: 33 of those outputs come from transactions paying 15
subaddresses each and carry 16 additional tx public keys apiece (about 580 bytes per
output). The 215 and 521 snapshots were built with plain payments, about 80 bytes per
output, so their per-output cost is the representative one.

| Wallet outputs | Outputs export bytes | QR frames | Key image export bytes | QR frames | Unsigned tx bytes (2 in) | QR frames | Signed tx bytes | QR frames |
|---|---|---|---|---|---|---|---|---|
| 51 | 20545 | 685 (172) | 5060 | 169 (43) | 4275 | 143 (36) | 6803 | 227 (57) |
| 215 | 33381 | 1113 (279) | 20804 | 694 (174) | 4120 | 138 (35) | 6666 | 223 (56) |
| 521 | 57296 | 1910 (478) | 50180 | 1673 (419) | 4198 | 141 (36) | 6739 | 225 (57) |

16-input transaction per snapshot:

| Wallet outputs | Unsigned tx bytes (16 in) | QR frames | Signed tx bytes | QR frames |
|---|---|---|---|---|
| 51 | 21790 | 727 (182) | 34673 | 1156 (289) |
| 215 | 21635 | 722 (181) | 34573 | 1153 (289) |
| 521 | 22082 | 737 (185) | 35151 | 1172 (293) |

| Operation (Pi Zero 1.3) | Median ms (20 runs) | Peak RSS MB |
|---|---|---|
| Parse outputs export (51 outputs) | 743 | 3.9 |
| Compute key images (51 outputs) | 1658 | 3.9 |
| Parse unsigned tx (2 inputs, 51-output wallet) | 739 | 3.9 |
| Sign (2 inputs, 51-output wallet) | 4508 | 3.9 |
| Sign (16 inputs, 51-output wallet) | 10054 | 3.9 |
| Parse outputs export (215 outputs) | 747 | 4.0 |
| Compute key images (215 outputs) | 4199 | 4.0 |
| Parse unsigned tx (2 inputs, 215-output wallet) | 739 | 4.0 |
| Sign (2 inputs, 215-output wallet) | 4463 | 4.0 |
| Sign (16 inputs, 215-output wallet) | 10000 | 4.0 |
| Parse outputs export (521 outputs) | 754 | 3.9 |
| Compute key images (521 outputs) | 8931 | 3.9 |
| Parse unsigned tx (2 inputs, 521-output wallet) | 741 | 3.9 |
| Sign (2 inputs, 521-output wallet) | 4470 | 3.9 |
| Sign (16 inputs, 521-output wallet) | 9035 | 3.9 |

Signing time does not depend on wallet size; key image time is linear in outputs at
about 16 ms per output plus one 0.7 s CryptoNight for the reply. Each parse row is
dominated by the CryptoNight hash of the payload decryption; each sign row includes
another for the reply encryption.

| Camera round trip (stage 2) | Seconds |
|---|---|
| Scan outputs export (500 outputs) | stage 2, not yet measured |
| Scan unsigned tx (16 inputs) | stage 2, not yet measured |
| Feather scans key images | stage 2, not yet measured |
| Feather scans signed tx | stage 2, not yet measured |
| Power on to signed tx (2 inputs, SeedQR + passphrase) | stage 2, not yet measured |

Versions: monero-oxide 731657ae3385be667abb556266369a497bc86f13; Feather 2.8.1
(mac-arm64, GPG-verified); monero-wallet-rpc/cli 0.18.5.1; Pi image Raspberry Pi
OS Lite (trixie, armhf), kernel 6.18.50+rpt-rpi-v6; rustc 1.96.0,
cargo-zigbuild 0.23.0, zig 0.15.2; cuprate-cryptonight at Cuprate/cuprate
4f8fcd1bf468f566fcd89c460d50de4adb6825bf. Cupcake/Cake: not tested (compatible by
code inspection, see findings).

## 3. Findings

Format survey (before any fixtures existed), full detail in
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
- Signer core, first half validated: `libmonero-signer` decrypts and
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
- Signer core, second half validated: `libmonero-signer` parses a
  wallet2 unsigned tx set (2 inputs, ring 16, RingCT type 6), recovers the spent
  one-time keys, builds the transaction from monero-oxide primitives (one-time
  output keys, view tags, ECDH amounts, Bulletproof+, CLSAGs, sorted inputs,
  tx_extra with re-encrypted payment id) and emits a signed tx set. The
  serialized transaction is 2191 bytes, exactly the weight wallet2 predicted. The
  view-only wallet in monero-wallet-rpc 0.18.5.1 parsed the signed set and the
  stagenet daemon accepted the transaction into its pool (txid
  b2c80c12780a16b2a38b6f0424d3d774f6928499ddbdddef3e48e8913efcda9c). No Feather
  involvement yet; Feather fixtures come next.
- First on-device timings (release build of the `xmr-signer` CLI on the Pi
  Zero 1.3, single runs, development vectors from monero-wallet-rpc, not the final
  Feather fixtures): parse plus decrypt of a 51-output export (20545 bytes) 754 ms;
  key images and signatures for 51 outputs 1640 ms; parse plus decrypt of a 2-input
  unsigned set 749 ms; construct and sign 2 inputs with ring 16 (Bulletproof+ for 2
  outputs, 2 CLSAGs, encrypted reply) 4.8 s. Each decrypt or encrypt pays one
  CryptoNight hash, about 0.7 s on this CPU, so a full round trip carries four of
  them. Final numbers (20-run medians, peak RSS) come from spike-bench on the Feather
  fixtures.
- Feather round trip, file transport: Feather 2.8.1 view-only wallet
  exported its outputs, imported the signer's key images, built a transaction,
  exported the unsigned set, and after the signer produced the signed set Feather
  imported it and broadcast it. The transaction (1 input, ring 16, txid
  8a4268bbdc24b2d82c06ca7a5db34cccaf7c96a93d4e5fd56944ef3ff18847b2) was mined in
  stagenet block 2209404. No Feather change was needed.
- Pitfall for the demo script: two unsigned sets created before the first one is
  broadcast can share an input, and the second then fails in Feather with "double
  spend". Create, sign and broadcast one transaction at a time.
- Feather's coin control: "Spend" on selected coins only marks preferred inputs and
  wallet2 still uses as few as needed; "Sweep selected outputs" spends exactly the
  selected coins, which is how the 2-input and 16-input fixtures are made.
- Cupcake (cake-tech/cupcake 7a8754a) via monero_c (mrcyjanek/monero_c 7352f1c,
  patch `0005-UR-functions.patch`): the same four UR type strings, the same wallet2
  strings (`export_outputs_to_str`, `parse_unsigned_tx_from_str`,
  `sign_tx_dump_to_str`, key image export) wrapped as a bare CBOR byte string, default
  130 bytes per fragment. By code inspection the device's payloads are compatible
  unchanged. Not exercised against the app.
- Frame counts: Feather emits 150-byte fragments at 80 ms; the SeedSigner shell
  displays 30-byte fragments at its default density (10 low, 120 high).

## 4. Decision record

### Rust (monero-oxide) vs C++ (wallet2): Rust

Day-one gate. monero-oxide's `monero-wallet` crate (commit
731657ae3385be667abb556266369a497bc86f13, default branch) cross-compiles to a
static `arm-unknown-linux-musleabihf` binary with cargo-zigbuild (zig
0.15.2, rustc 1.96.0) and runs on the Pi Zero 1.3 (BCM2835,
kernel 6.18.50+rpt-rpi-v6, 437132 kB MemTotal). The gate binary
`xmr-signer/crates/xmr-gate` computed 5120 key images (20 passes over the 256
`generate_key_image` vectors from monero-project/monero) with 0 mismatches,
median 3891 us per key image, peak RSS 1488 kB. Raw output:
`xmr-signer/dist/pi-gate-results.txt`. The C++ wallet2 fallback was not
needed and was not attempted.

### Pi Zero 1.3 vs Pi Zero 2 W: Pi Zero 1.3

Kept for the measurements. UX assessment: on today's protocol the 1.3 is feasible but slow (4.5 s per typical spend plus QR volume); under FCMP++ the measured SA/L cost makes it comfortable. Kept. The gate ran natively on ARMv6 in the target memory budget, so there is
no reason to move to aarch64. Reconsider only if signing at 16 inputs turns
out to exceed the budget in stage 1 step 4.

### Benchmark transport

The Pi Zero 1.3 has no network. Benchmarks run on a Raspberry Pi OS card with
the USB port in gadget mode over a single cable; see `xmr-signer/PI_RUN.md`.
This is measurement infrastructure, not part of the device image.

## 5. FCMP++ paragraph

Source: seraphis-migration/monero PR #52, "wallet: complete hot-cold implementation
for Carrot/FCMP++" (closed with the note that it will be re-submitted as
smaller pieces once its dependencies merge; tracked in issue #53). Under FCMP++ the
cold side no longer builds the whole transaction. The PR's protocol goals are a
compact exported Carrot outputs format, a compact signed transaction format that
carries only the spend-authorization-and-linkability proofs and key image
associations, and deferred FCMP membership and Bulletproof+ proving to the hot
wallet at submission time; proposals and signing are stateless, and a cold wallet
can also initiate a proposal. For this device that means the two expensive pieces
measured here, the range proof and the ring-dependent CLSAG over 16 hot-supplied
ring members per input, move to the host, and the unsigned payload no longer has
to carry ring member data at all. The round trip keeps the same shape (outputs
export, key images, proposal, signed set) and the PR states that new hot and cold
wallets stay compatible with old counterparts until hard fork activation, with the
RPC interface unchanged until then. Two quirks the PR records: sender-receiver
secrets are fetched from the cold wallet by signable transaction hash instead of
txid, and finalizing a proposal requires the private view-incoming key, so a hot
wallet without it cannot submit a signed set (payloads are already encrypted to
that key). The Carrot key hierarchy tests and cold-initiated proposal integration
tests were still open in the PR when it closed, so anything beyond this is not yet
settled upstream.


### FCMP++ cold-side cost, measured

Under FCMP++ the cold side's per-input work is: rerandomize the spent output, open
the input tuple, prove spend authorization and linkability (SA/L). `xmr-signer/fcmp-bench`
measures exactly that with monero-oxide's `fcmp++` branch (commit
31c26d96eaadbba910ffe3613ad8b4cf9c598a93, crate `monero-fcmp-plus-plus`, synthetic
outputs as in the crate's own test), static ARMv6 build, 20 iterations on the Pi Zero
1.3. Raw output in `xmr-signer/dist/pi-fcmp-bench.txt`.

| Inputs | Rerandomize (median ms) | SA/L prove (median ms) | Total | Proof bytes | Today (CLSAG + BP+) |
|---|---|---|---|---|---|
| 1 | 17 | 44 | 61 ms | 384 | about 4.3 s |
| 2 | 34 | 87 | 121 ms | 768 | 4.5 s |
| 16 | 269 | 700 | 969 ms | 6144 | 9.0 to 10.1 s |

Peak RSS 1.1 MB. The membership proof and the range proof are the hot wallet's job
under the hot/cold PR and are not run on the device. Not included: whatever payload
encryption the final format uses (today's wrapper costs 0.7 s of CryptoNight per
payload on this CPU), and the not-yet-specified proposal parsing. On this evidence
the same hardware signs a 16-input transaction about ten times faster after the fork
and a typical 2-input one about forty times faster, with a reply of a few hundred
bytes per input instead of a full transaction.

## 6. Links

Repo: this directory (to be published at github.com/Biglup/monero-seedsigner-research).
Video: stage 2, not yet recorded.
