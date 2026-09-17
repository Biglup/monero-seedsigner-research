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
