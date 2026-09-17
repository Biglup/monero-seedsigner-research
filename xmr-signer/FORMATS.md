# Cold-signing payload formats (what libmonero-signer must speak)

Derived on 2026-09-17 from source, not docs:

- Feather 2.8.1 (feather-wallet/feather 948773c, `src/wizard/offline_tx_signing/`,
  `src/qrcode/scanner/URWidget.cpp`, `QrCodeScanWidget.cpp`), which builds against
  feather-wallet/monero 927e269e04afc9b83472cece77023bd66f2f383f (`src/wallet/wallet2.cpp`,
  `wallet2.h`, `src/wallet/api/wallet.cpp`, `src/cryptonote_core/cryptonote_tx_utils.h`,
  `src/ringct/rctTypes.h`, `src/serialization/*`).
- Keystone UR registry, KeystoneHQ/keystone-sdk-rust 7c90bf1,
  `libs/ur-registry/src/monero/*.rs`, `registry_types.rs`.
- Cupcake, cake-tech/cupcake 7a8754a, `lib/coins/monero/wallet.dart`.

All byte layouts below are little-endian. "varint" is the CryptoNote LEB128-style
varint. "binary_archive" is monero's `serialization/binary_archive.h` format:
`FIELD(x)` of a fixed-size POD is the raw bytes, `FIELD(u64)` is 8 raw bytes,
`VARINT_FIELD` is a varint, `bool` is one byte, vectors and strings are varint
count followed by elements, `std::pair` and `std::tuple` are written as an array
(varint element count, then elements), `VERSION_FIELD(n)` is a leading varint.
Inside any container, pair or tuple, unsigned integer elements wider than one
byte are varints (so `vector<size_t>` and `set<uint32_t>` are varint count plus
varint elements, while a top-level `FIELD(uint64_t)` is 8 raw bytes). A map is a
container of pairs: varint count, then per entry varint 2, key, value. Verified
byte by byte against a wallet2 unsigned tx set on 2026-09-17.

## 1. UR layer (identical for all four types)

| UR type | Keystone tag (unused on the wire) | Direction |
|---|---|---|
| `xmr-output` | 8301 | hot -> cold |
| `xmr-keyimage` | 8302 | cold -> hot |
| `xmr-txunsigned` | 8303 | hot -> cold |
| `xmr-txsigned` | 8304 | cold -> hot |

CBOR content is a single bare byte string (major type 2) holding the wallet2 blob.
No tag, no map. Feather encodes with `ur::CborLite::encodeBytes` and decodes with
`decodeBytes`; the Keystone registry decoders accept exactly `Type::Bytes`.
Fountain encoding: Feather emits 150 bytes per fragment at 80 ms per frame
(`Config::URfragmentLength`, `URmsPerFragment`, user adjustable in the UR settings
dialog). The SeedSigner shell (`cardano-seedsigner/src/seedsigner/models/encode_qr.py`)
uses 10 / 30 / 120 bytes per fragment for density Low / Medium (default) / High.
Frame counts in the report are computed with these numbers.

Cupcake handles the same four type strings and hands the payload to monero_c, which is
wallet2, so the blob formats are shared. Compatibility is expected but is verified in
stage 2 only.

## 2. Encryption wrapper (every blob after its magic prefix)

`wallet2::encrypt_with_view_secret_key(plaintext, authenticated = true)`:

```
key   = cn_slow_hash(view_secret_key[32])        # CryptoNight v0, kdf_rounds = 1
iv    = random 8 bytes
ct    = chacha20(key, iv, plaintext)
hash  = keccak256(iv || ct)                      # cn_fast_hash
sig   = monero crypto::generate_signature(hash, view_public_key, view_secret_key)  # 64 bytes (c, r)
blob  = iv || ct || sig
```

Decrypt is the mirror: verify `sig` with `crypto::check_signature` over
`keccak256(iv || ct)` against the view public key, then chacha20. The device must
both decrypt (outputs, unsigned tx) and encrypt (key images, signed tx) with the
same construction, so it needs CryptoNight, ChaCha20 and Monero's Schnorr-style
signature. CryptoNight is a 2 MB scratchpad hash; it runs once per payload and its
cost on ARMv6 is a number to report.

## 3. `xmr-output`: outputs export

```
"Monero output export\004" || encrypt( spend_public_key[32] || view_public_key[32] || body )
body = binary_archive( tuple<u64 offset, u64 total, vector<exported_transfer_details>> )
```

`exported_transfer_details` (VERSION_FIELD(1)):

| Field | Encoding |
|---|---|
| m_pubkey | 32 raw (output one-time public key) |
| m_internal_output_index | varint |
| m_global_output_index | varint |
| m_tx_pubkey | 32 raw |
| m_flags | 1 byte: bit0 spent, bit1 frozen, bit2 rct, bit3 key_image_known, bit4 key_image_request, bit5 key_image_partial |
| m_amount | varint |
| m_additional_tx_keys | vector of 32 raw |
| m_subaddr_index_major | varint |
| m_subaddr_index_minor | varint |

Feather's wizard exports with `all = false` by default (only outputs whose key image
is unknown or requested) and has an "Export all outputs" checkbox. `offset` is the
index of the first exported transfer in the hot wallet's transfer list.

## 4. `xmr-keyimage`: key images reply

Produced on the cold side by `export_key_images_for_outputs_from_str`:

```
"Monero key image export\003" || encrypt( offset_u32_le || spend_public_key[32] || view_public_key[32] || N x ( key_image[32] || signature[64] ) )
```

For each exported output, in order: derive the one-time secret key
(`generate_key_image_helper`: shared secret from tx pubkey or the matching additional
tx pubkey and the view secret, output index, plus the subaddress spend key offset for
(major, minor)), check the derived public key equals `m_pubkey`, `key_image =
x * hash_to_point(m_pubkey)`, and sign with `crypto::generate_ring_signature(prefix_hash =
key_image, key_image, {m_pubkey}, x, sec_index 0)`: a one-member CryptoNote ring
signature over the key image itself. `offset` echoes the outputs export offset. Feather
imports with `import_key_images_from_str`, which checks the two public keys match its
account and that the record count divides evenly.

## 5. `xmr-txunsigned`: unsigned transaction set

```
"Monero unsigned tx set\005" || encrypt( binary_archive(unsigned_tx_set) )
```

`unsigned_tx_set` (VERSION_FIELD(2)): `txes: vector<tx_construction_data>`, then
`new_transfers: tuple<u64, u64, vector<exported_transfer_details>>` (the hot wallet's
full outputs export, so the cold side can refresh key images at the same time).

`tx_construction_data` (no version field):

| Field | Encoding |
|---|---|
| sources | vector<tx_source_entry> |
| change_dts | tx_destination_entry |
| splitted_dsts | vector<tx_destination_entry> (includes change) |
| selected_transfers | vector<varint> (varint count, varint elements) |
| extra | vector<u8> (raw tx_extra bytes) |
| unlock_time | u64 raw (must be 0) |
| use_rct (construction_flags) | 1 byte: bit0 use_rct, bit1 use_view_tags |
| rct_config | VERSION_FIELD(0), varint range_proof_type, varint bp_version |
| dests | vector<tx_destination_entry> (excludes change) |
| subaddr_account | u32 raw |
| subaddr_indices | set<u32> (varint count, varint elements) |

`tx_source_entry`: `outputs: vector<pair<u64 global_index, ctkey{dest[32], mask[32]}>>`
(the ring, real member included, sorted by global index), `real_output: u64` (index into
outputs), `real_out_tx_key[32]`, `real_out_additional_tx_keys: vector<32>`,
`real_output_in_tx_index: u64`, `amount: u64`, `rct: bool`, `mask[32]` (the real output's
commitment mask), `multisig_kLRki` (4 x 32, zero for non-multisig).

`tx_destination_entry`: `original: string`, `amount: varint`, `addr: account_public_address
{spend[32], view[32]}`, `is_subaddress: bool`, `is_integrated: bool`.

Feather builds this with `PendingTransaction::unsignedTxToBin` = `wallet2::dump_tx_to_str`.
The wizard's step 3 has an Export button that writes the same bytes to an
`*_unsigned_monero_tx` file (`PageOTS_ExportUnsignedTx::exportUnsignedTx`).

## 6. What the cold side computes (`wallet2::sign_tx`)

For each `tx_construction_data`:

1. Reject empty sources and non-zero unlock_time.
2. `construct_tx_and_get_tx_key(keys, subaddresses, sources, splitted_dsts,
   change_dts.addr, extra, tx, tx_key, additional_tx_keys, use_rct, rct_config,
   use_view_tags)`: builds the whole transaction on the device, including one-time
   output keys, ECDH amount encryption, view tags, the Bulletproof+ range proof and one
   CLSAG per input using exactly the supplied ring members. The fee is not a parameter:
   it is `sum(source amounts) - sum(splitted_dsts amounts)`.
3. `key_images` string: each input key image as hex followed by a space.
4. `tx_key` in the returned `pending_tx` is set to the identity scalar (1) so the view
   wallet never learns it; `dests`, `change_dts`, `selected_transfers` and the full
   `construction_data` are echoed back.
5. Key images for outputs the new transaction sends back to us (change) go into
   `tx_key_images` keyed by output public key; so do key images for every entry of
   `new_transfers`.

The signed reply:

```
"Monero signed tx set\005" || encrypt( binary_archive(signed_tx_set) )
signed_tx_set (VERSION_FIELD(0)): ptx: vector<pending_tx>, key_images: vector<32>, tx_key_images: map<pubkey[32] -> key_image[32]>
pending_tx (VERSION_FIELD(1)): tx (cryptonote::transaction, full serialization with rct sigs), dust u64, fee u64, dust_added_to_fee bool, change_dts, selected_transfers, key_images string, tx_key[32], additional_tx_keys vector<32>, dests, construction_data, multisig_sigs vector (empty), multisig_tx_key_entropy[32]
```

`signed_tx_set.key_images` is left empty by the string path. Feather's import
(`loadSignedTxFromStr` -> `parse_tx_from_str`) rejects sets with zero or more than one
transaction, then broadcasts `ptx[0].tx`.

## 7. Coverage in monero-oxide 731657a and the gaps

| Need | monero-oxide | Gap |
|---|---|---|
| Ed25519 scalar/point, hash_to_point, key images | `monero_ed25519`, `Point::biased_hash` | none (gate proved it) |
| Output key derivation incl. subaddresses | `monero_wallet::ViewPair`, `Scanner` internals | derivation helpers are partly `pub(crate)`; reimplement the small `generate_key_image_helper` path from tx pubkey + index + subaddress offset |
| CLSAG sign/verify | `monero_clsag::Clsag::sign` | none |
| Bulletproof+ prove | `monero_bulletproofs::Bulletproof::prove_plus` | none |
| Transaction serialization, tx hash, tx_extra | `monero_oxide::transaction`, `extra` | none |
| Transaction construction from fixed sources, dests and fee | `SignableTransaction` takes a fee rate and picks its own structure | build the tx directly from primitives (outputs, ECDH, BP+, CLSAG) instead of using the high-level builder |
| One-member ring signature for key image export | `RingSignature::verify` only | write `sign` (CryptoNote ring signature, ~40 lines) |
| Monero `generate_signature` / `check_signature` (auth on the wrapper) | not present | write it (Schnorr over keccak, ~30 lines) |
| CryptoNight v0 for the chacha key | not present | pure-Rust `cryptonight-hash` 0.1.2, or Cuprate's C-backed crate; benchmark on ARMv6 |
| ChaCha20 (Monero uses the 8-byte IV variant) | `rand_chacha` only | `chacha20` crate (legacy 64-bit nonce mode) |
| binary_archive codec for the wallet2 structs above | not present | write a small reader/writer; formats are fully specified in this file |

Everything in the gap column is well bounded. The two items that decide feasibility on
a Pi Zero 1.3 are Bulletproof+ proving and CryptoNight, both measured in stage 1 step 4.

## 8. Fixture capture map (stage 1 step 2)

| Payload | Where Feather can write a file | File name pattern |
|---|---|---|
| outputs export | wizard step 1, "Export" button | `*_outputs` |
| key images (Feather as cold wallet) | wizard offline mode, "Export" button | `*_keyImages` |
| unsigned tx | wizard step 3, "Export" button | `*_unsigned_monero_tx` |
| signed tx (Feather as cold wallet) | wizard sign step, "Export" button | `*signed_monero_tx` |

All three wizard import steps also accept a file, so device output can be validated
without a camera in stage 1. Feather can itself act as the cold wallet (an offline
Feather with the seed), which gives reference key image and signed-tx blobs for the
same fixtures to compare against, after decryption with the view key.
