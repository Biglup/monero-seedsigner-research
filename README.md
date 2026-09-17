# Monero cold signing on a Pi Zero 1.3

This is a research spike, not a product. The question was simple: can a SeedSigner class device (Raspberry Pi Zero 1.3, one ARMv6 core, 512 MB, no wireless silicon, nothing persisted) act as a Monero cold signer against an unmodified, released Feather Wallet, and if so, how slow is it. I already have a SeedSigner fork for Cardano and a similar spike for Zcash, so the plan was to reuse the shell and only write the signing core.

Short answer: yes it works, Feather 2.8.1 broadcast a transaction signed on the device and it got mined on stagenet, and no, the UX on todays protocol is not great. The interesting part is that the same hardware is comfortably fast under FCMP++, and I measured that too.

## What was built

`libmonero-signer` is about 1500 lines of Rust on top of [monero-oxide](https://github.com/monero-oxide/monero-oxide). It speaks wallet2's own cold signing payloads, the same bytes Feather's offline signing wizard wraps in the Keystone `xmr-output`, `xmr-keyimage`, `xmr-txunsigned` and `xmr-txsigned` UR types. There is nothing Keystone specific in there btw, the four types are a bare CBOR byte string around wallet2's blobs, so the whole job is wallet2 format compatibility. The byte level spec I reverse engineered from Feather's source and its monero fork is in [xmr-signer/FORMATS.md](xmr-signer/FORMATS.md).

Every payload is encrypted to the view key with ChaCha20 under a CryptoNight hash of the key, and authenticated with a Monero Schnorr signature. The device derives everything from the 25 word seed in memory, decrypts the outputs export, recovers the one time keys (subaddresses and additional tx keys included), computes and signs the key images, and for the unsigned set it constructs the full transaction on the device: one time output keys, view tags, ECDH amounts, Bulletproof+, one CLSAG per input over the 16 ring members the hot wallet picked. Under the current protocol the cold side really does build the whole thing, the fee isnt even a parameter, is implied by the amounts.

monero-oxide's high level builder is fee rate driven so it couldnt be used as is, the transaction is assembled from its primitives instead. The gaps I had to fill were small: a reader/writer for wallet2's binary archive structs, CryptoNight (Cuprate's pure Rust crate), the one member ring signature wallet2 wants on each key image, and the Schnorr signature for the encryption wrapper.

## What was validated

Key images: the signer's reply for a 33 output export matched wallet2's key images one by one, wallet2's signatures verify under my verifier and vice versa, and the official monero-wallet-cli imported the blob into a view only wallet whose balance then matched the full wallet.

Signing: a signed set produced from a wallet-rpc view only wallet was accepted by that wallet and by the stagenet daemon (the serialized tx came out at 2191 bytes, exactly the weight wallet2 predicted, which was a nice sign). Then the real test, Feather 2.8.1 view only wallet, export outputs, import the device key images, build a tx, export the unsigned set, import the device signed set, broadcast. Mined in stagenet block 2209404, txid `8a4268bbdc24b2d82c06ca7a5db34cccaf7c96a93d4e5fd56944ef3ff18847b2`. No Feather change needed.

Cupcake (Cake's phone signer) goes through monero_c which is wallet2 again with the same four UR types, so it should work unchanged, I only checked that by reading the code tho.

## Numbers

All from the real Pi Zero Rev 1.3, static `arm-unknown-linux-musleabihf` release build, medians of 20 runs after a warmup. Raw outputs are committed under `xmr-signer/dist/`. Three stagenet wallet snapshots with 51, 215 and 521 received outputs.

| Wallet outputs | Outputs export | Key image reply | Unsigned tx (2 in) | Signed set (2 in) |
|---|---|---|---|---|
| 51 | 20545 bytes | 5060 bytes | 4275 bytes | 6803 bytes |
| 215 | 33381 bytes | 20804 bytes | 4120 bytes | 6666 bytes |
| 521 | 57296 bytes | 50180 bytes | 4198 bytes | 6739 bytes |

A 16 input transaction is about 22 KB unsigned and 35 KB signed regardless of wallet size. The 51 output export is fatter per output than the others because 33 of those outputs came from transactions paying 15 subaddresses at once, wallet2 attaches 16 additional tx public keys to each of those, about 580 bytes per output instead of about 80. The 215 and 521 snapshots were built with plain payments and are the representative ones.

| Operation on the Pi Zero 1.3 | Median |
|---|---|
| Decrypt and parse any payload | 0.74 s (almost all of it CryptoNight) |
| Key images, 51 outputs | 1.66 s |
| Key images, 215 outputs | 4.2 s |
| Key images, 521 outputs | 8.9 s |
| Sign 2 inputs | 4.5 s |
| Sign 16 inputs | 9.0 to 10.1 s |

Peak RSS stays around 4 MB for the whole thing. Key images are about 16 ms per output plus one CryptoNight for the reply, signing doesnt depend on wallet size.

QR frames at the SeedSigner shell's high density (120 bytes per fragment): the 521 output export is 478 frames, its key image reply 419, a 2 input unsigned tx 35, its signed set 56, a 16 input signed set 293. At the shell's default 30 bytes per fragment multiply by four, which is unusable for this, the density setting was tuned for PSBTs.

## Honest assessment

The compute is in hardware wallet territory (Ledger's Monero app is known for taking minutes, Trezor tens of seconds), the phone based Cupcake is the outlier that signs instantly. What kills the UX is the protocol, not the 4.5 seconds: a view only wallet cant know its balance until the cold side hands it key images, so every cold signer today does the four step dance, and the unsigned tx carries 16 ring members per input. Feather's default export only sends outputs whose key images it doesnt know yet, so the big sync is a one time cost, but even a routine 2 input spend is around 90 frames each way on a 240x240 LCD at 6 fps plus the two CryptoNight hashes. Feasible, slow, IMO nobody is going to love it.

## FCMP++ changes the picture

Under FCMP++ with the hot/cold design in [seraphis-migration/monero#52](https://github.com/seraphis-migration/monero/pull/52) the cold side only produces the spend authorization and linkability proof per input, the membership proof and the range proof move to the hot wallet at submission time, and with Carrot keys the hot wallet can compute key images itself so the key image round trip disappears. monero-oxide's `fcmp++` branch already exposes that proof as a standalone API, so I measured it on the same hardware (`xmr-signer/fcmp-bench`, synthetic outputs as in the crate's own test, branch commit 31c26d96):

| Inputs | Rerandomize | SA/L prove | Total | Proof bytes | Today |
|---|---|---|---|---|---|
| 1 | 17 ms | 44 ms | 61 ms | 384 | ~4.3 s |
| 2 | 34 ms | 87 ms | 121 ms | 768 | 4.5 s |
| 16 | 269 ms | 700 ms | 969 ms | 6144 | 9 to 10 s |

So roughly forty times faster for a typical spend and ten times for 16 inputs, with a reply of a few hundred bytes per input instead of a full transaction. Caveats: the proposal format and its encryption arent specified yet (if it keeps todays wrapper thats 0.7 s of CryptoNight per payload on this CPU, which would then be the dominant cost), and the PR was closed on 2026-09-14 to be resplit, so this is design intent, not merged code. I believe the right way to read the two tables together is that the hardware was never the problem, and a stateless open signer on commodity parts makes a lot more sense as a fork day device than as something to use on the current protocol.

## Decisions

Rust with monero-oxide over the C++ wallet2 path: the day one gate (monero-oxide key images cross compiled to ARMv6, all 256 upstream test vectors correct on the Pi) passed on the first try, so the fallback was never needed. Pi Zero 1.3 over the 2 W: kept, the 2 W would be about five times faster but it has a radio, which is the whole point of the 1.3 in the SeedSigner world. Fixtures: the outputs export for the first snapshot and one unsigned/signed pair came from Feather's wizard, the exact 2 and 16 input sets and the other snapshots came from monero-wallet-rpc on a view only wallet, same wallet2 code path, byte identical format, it was just faster than clicking through the GUI for every snapshot. Provenance for every fixture is next to it.

## Layout

- [SPEC.md](SPEC.md): the task as written before starting.
- [REPORT.md](REPORT.md): full tables, findings, decision record, FCMP++ paragraph.
- [xmr-signer/FORMATS.md](xmr-signer/FORMATS.md): the payload formats, from source.
- [xmr-signer/PI_RUN.md](xmr-signer/PI_RUN.md): how every device number was produced, including the bench card setup (the Pi has no network, it runs USB gadget ethernet over a single cable).
- [STAGE2-NOTES.md](STAGE2-NOTES.md): what integrating this into the SeedSigner shell looks like.
- `xmr-signer/crates/`: `libmonero-signer` (the core), `xmr-keys` (25 word mnemonic and key derivation), `xmr-signer-cli`, `spike-bench`, `xmr-gate`.
- `xmr-signer/fcmp-bench/`: the FCMP++ measurement, its own workspace because it pins a different monero-oxide branch.
- `xmr-signer/fixtures/`: stagenet payloads with provenance notes.

Everything is stagenet. The test wallet's private view key is in the report on purpose, the fixtures are encrypted to it, the spend key and seed are not in the repo. The camera round trip, the three device screens and the video (stage 2 in the spec) are not done, the numbers above dont depend on them.

## Reproducing

`scripts/build-armv6.sh spike-bench` builds the static ARMv6 binary with cargo-zigbuild, then follow PI_RUN.md, copy the binary and the fixtures to the Pi and run it with the seed in the environment. Without the seed you can still rebuild everything and rerun the FCMP++ bench, which needs no fixtures.

MIT, Angel Castillo.
