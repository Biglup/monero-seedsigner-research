# Monero cold signing on a Pi Zero 1.3

Feasibility spike: a SeedSigner class device (Raspberry Pi Zero 1.3, one ARMv6 core, 512 MB, no wireless, nothing persisted) as a Monero cold signer for an unmodified Feather Wallet. Stagenet only. The camera round trip and the device UI are not part of this, only the signing core and the numbers.

## Signer

`libmonero-signer`, about 1500 lines of Rust on [monero-oxide](https://github.com/monero-oxide/monero-oxide). It consumes and produces wallet2's cold signing payloads, the same bytes Feather's offline signing wizard sends as the Keystone `xmr-output`, `xmr-keyimage`, `xmr-txunsigned` and `xmr-txsigned` UR types. Those four types are just a CBOR byte string around wallet2's blobs, there is nothing else to them. Byte level spec from Feather's source in [xmr-signer/FORMATS.md](xmr-signer/FORMATS.md).

Payloads are encrypted to the view key (ChaCha20, key = CryptoNight of the view key, Monero Schnorr signature for authentication). The device derives the keys from the 25 word seed in memory, decrypts the outputs export, recovers the one time keys (subaddresses, additional tx keys), computes and signs key images, and for the unsigned set builds the complete transaction: output keys, view tags, ECDH amounts, Bulletproof+, one CLSAG per input over the ring the hot wallet chose. On the current protocol the cold side builds all of it, the fee is implied by the amounts.

monero-oxide's builder is fee rate driven so the tx is assembled from its primitives. What had to be written on top: wallet2 binary archive reader/writer, CryptoNight (Cuprate's pure Rust crate), the one member ring signature per key image, the Schnorr signature of the wrapper.

## Validation

Key images for a 33 output export match wallet2's one by one, signatures verify both ways, and monero-wallet-cli imported the reply into a view only wallet with the expected balance. A signed set from a wallet-rpc view only wallet was accepted by that wallet and the daemon, 2191 bytes, the exact weight wallet2 predicted. With Feather 2.8.1 (view only wallet, file transport): export outputs, import device key images, build tx, export unsigned, import device signed set, broadcast. Mined in stagenet block 2209404, txid `8a4268bbdc24b2d82c06ca7a5db34cccaf7c96a93d4e5fd56944ef3ff18847b2`. No Feather changes.

Cupcake goes through monero_c, which is wallet2 with the same four UR types, so it should work unchanged. Only checked by reading the code.

## Numbers

Pi Zero Rev 1.3, static `arm-unknown-linux-musleabihf` release build, medians of 20 runs after a warmup. Raw outputs in `xmr-signer/dist/`. Three stagenet snapshots, 51, 215 and 521 received outputs.

| Wallet outputs | Outputs export | Key image reply | Unsigned tx (2 in) | Signed set (2 in) |
|---|---|---|---|---|
| 51 | 20545 bytes | 5060 bytes | 4275 bytes | 6803 bytes |
| 215 | 33381 bytes | 20804 bytes | 4120 bytes | 6666 bytes |
| 521 | 57296 bytes | 50180 bytes | 4198 bytes | 6739 bytes |

A 16 input tx is about 22 KB unsigned and 35 KB signed at any wallet size. The 51 output export is fat because 33 of its outputs come from txs paying 15 subaddresses each (wallet2 attaches 16 additional tx pubkeys to every one of them, about 580 bytes per output vs about 80). The other two snapshots use plain payments.

| Operation | Median |
|---|---|
| Decrypt + parse any payload | 0.74 s, almost all CryptoNight |
| Key images, 51 / 215 / 521 outputs | 1.66 s / 4.2 s / 8.9 s |
| Sign 2 inputs | 4.5 s |
| Sign 16 inputs | 9.0 to 10.1 s |

Peak RSS about 4 MB. Key images are about 16 ms per output plus one CryptoNight for the reply, signing does not depend on wallet size.

QR frames at 120 bytes per fragment (the SeedSigner shell's high density): 521 output export 478, its key image reply 419, 2 input unsigned tx 35, its signed set 56, 16 input signed set 293. The shell's default is 30 bytes per fragment, four times that, it was tuned for PSBTs.

Compute wise this is hardware wallet territory (Ledger's Monero app takes minutes, Trezor tens of seconds, Cupcake on a phone is instant). The protocol is the problem: a view only wallet needs the cold side's key images before it knows its balance, and the unsigned tx carries 16 ring members per input. Feather only exports outputs with unknown key images so the big sync happens once, but a routine 2 input spend is still around 90 frames each way on a 240x240 LCD at 6 fps plus two CryptoNight hashes. Works, slow.

## FCMP++

With the hot/cold design in [seraphis-migration/monero#52](https://github.com/seraphis-migration/monero/pull/52) the cold side only produces the spend authorization and linkability proof per input, membership and range proofs move to the hot wallet, and with Carrot keys the hot wallet computes key images itself so the sync round trip goes away. monero-oxide's `fcmp++` branch exposes that proof, measured on the same hardware (`xmr-signer/fcmp-bench`, synthetic outputs as in the crate's test, branch commit 31c26d96):

| Inputs | Rerandomize | SA/L prove | Total | Proof bytes | Current protocol |
|---|---|---|---|---|---|
| 1 | 17 ms | 44 ms | 61 ms | 384 | ~4.3 s |
| 2 | 34 ms | 87 ms | 121 ms | 768 | 4.5 s |
| 16 | 269 ms | 700 ms | 969 ms | 6144 | 9 to 10 s |

Proposal format and encryption are not specified yet (keeping todays wrapper would add 0.7 s of CryptoNight per payload here) and the PR was closed on 2026-09-14 to be split up, so this is design intent. The hardware is fine, the current protocol is what makes it slow.

## Decisions

Rust/monero-oxide over the wallet2 C++ path: the day one gate (monero-oxide key images on ARMv6, all 256 upstream vectors) passed, the fallback was never needed. Pi Zero 1.3 over the 2 W: the 2 W is about five times faster but has a radio. Fixtures: first snapshot outputs export and one unsigned/signed pair from Feather's wizard, the exact 2 and 16 input sets and the other snapshots from monero-wallet-rpc on a view only wallet (same wallet2 code path, identical format). Provenance next to every fixture.

## Layout

- [SPEC.md](SPEC.md): task as written before starting.
- [REPORT.md](REPORT.md): full tables, findings, decision record.
- [xmr-signer/FORMATS.md](xmr-signer/FORMATS.md): payload formats.
- [xmr-signer/PI_RUN.md](xmr-signer/PI_RUN.md): how each number was produced, bench card setup (USB gadget ethernet, the Pi has no network).
- [STAGE2-NOTES.md](STAGE2-NOTES.md): integration into the SeedSigner shell.
- `xmr-signer/crates/`: `libmonero-signer`, `xmr-keys`, `xmr-signer-cli`, `spike-bench`, `xmr-gate`.
- `xmr-signer/fcmp-bench/`: FCMP++ measurement, separate workspace (different monero-oxide branch).
- `xmr-signer/fixtures/`: stagenet payloads with provenance.

The test wallet's private view key is in the report, the fixtures are encrypted to it. Seed and spend key are not in the repo.

`scripts/build-armv6.sh spike-bench` builds the ARMv6 binary with cargo-zigbuild, PI_RUN.md has the rest. The FCMP++ bench needs no fixtures or seed.

MIT, Angel Castillo.
