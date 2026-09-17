# Snapshot "about 50 outputs" (51 received outputs)

Stagenet test wallet, primary address
59dsC6xRXfGB8KMEt85wFjjWwfPJsHiMFF3Lt3iJzjkQ77U3QvQKFKXcGWdQab2GYZ39yvuddmuFXLo35NXZKmENAf6VdwR.
History: 1 faucet payment to subaddress 0/1, then self-sends built by
scripts/fanout.py (2 transactions paying 15 subaddresses each, 1 paying the
primary address 15 times) plus 1 device-signed self-send. All payloads are
view-key encrypted; the view key is public in REPORT.md, spend key is not.

| File | Produced by | Size | sha256 |
|---|---|---|---|
| outputs.bin | Feather 2.8.1 (mac-arm64), view-only wallet, Tools > Key image sync > step 1 "Export outputs" with "Export all outputs", saved 2026-09-17 13:56 local, chain height about 2209395 | 20545 | 0d5959ef21b0d65c309d568d5c563f4d2c6e89ffe199928006d9dc08b5727bb1 |
| keyimages.bin | xmr-signer keyimages (libmonero-signer) from outputs.bin, 2026-09-17 | 5060 | 6867a7b0db680cab582aaaecb367d4e01d11ea392879d17b3c76dedf111be472 |

Note: 33 of the 51 outputs come from transactions paying 15 subaddresses and
carry 16 additional tx public keys each (about 580 bytes per output); the
other 18 are plain (about 80 bytes per output).

## Unsigned and signed transaction sets (monero-wallet-rpc 0.18.5.1 view-only wallet, 2026-09-17 06:13 UTC)

Built with scripts/snapshot_txs.py: sweep_all of exactly N unlocked outputs of one subaddress to subaddress 0/2, ring size 16, priority 1. Signed by xmr-signer, submitted through the same wallet.

| Inputs | From subaddress | Unsigned bytes | sha256 | Signed bytes | sha256 | txid | Height |
|---|---|---|---|---|---|---|---|
| 16 | 0/0 | 21790 | d7bf8eb93032c9af0d99e7d6caedca7ca6dabc0b0f9dd4d706c40e846d631d34 | 34673 | 0f979a617ccc69b878c4bc4a1833dccc443cec97b5f452e6f012dd2af700dcf1 | f1cf58ce3935db31a4b4b9e3af6dfdaf475da493bc761ec675ae77694bf6912d | 2209410 |
| 2 | 0/10 | 4275 | 190f7686f264ac22d89e6fa1cd731c4c64beb88d635ac9c578f70099ac778c03 | 6803 | 52d572b5585ee7092dd0b5b25c785ba5fa2ef3f08ba288ea1654bb5ebf228b3c | 5afd3caf83ebf2e28568007bbaf85f8e9d277865a505f373a263822c222d05e7 | 2209410 |
