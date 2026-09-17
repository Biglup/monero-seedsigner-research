# Snapshot snap500 (521 received outputs)

Stagenet test wallet (see fixtures/snap50/PROVENANCE.md for the wallet and history). Outputs export taken with monero-wallet-rpc 0.18.5.1 `export_outputs {all: true}` from the view-only wallet at chain height 2209454 on 2026-09-17 07:23 UTC; this is the same wallet2 code path Feather's wizard uses (`export_outputs_to_str`), so the bytes are format-identical to a Feather export of the same wallet state.

| File | Size | sha256 |
|---|---|---|
| outputs.bin | 57296 | 6efe617d06348debe50ba07ef037b4d942909244712e37b738fa6b3d6ddea1df |
| keyimages.bin (xmr-signer keyimages) | 50180 | 3ef19285494ebd717a3a1a91e2c83b919560cc41fac5c26c13bd2a587e43a329 |

## Unsigned and signed transaction sets (monero-wallet-rpc 0.18.5.1 view-only wallet, 2026-09-17)

Built with scripts/snapshot_txs.py: a transfer to subaddress 0/2 whose amount only the N largest spendable outputs of one subaddress can fund (the rest frozen for the call), ring size 16, priority 1; input count verified by parsing the set. Two earlier attempts with sweep_all produced 1-input sets and were discarded (their txids are not part of the fixtures). Signed by xmr-signer, submitted through the same wallet.

| Inputs | From subaddress | Unsigned bytes | sha256 | Signed bytes | sha256 | txid | Height |
|---|---|---|---|---|---|---|---|
| 2 | 0/14 | 4198 | 5f8d0c5f6e96f2ea151a581a2a3ce5f6f987e162802d22b59cb7f55db34f2f58 | 6739 | 385d84c709094522c6214be5c6e82819ddacc2aef1a789d51d1b1e6fee6b7a32 | 78b1faae5900160ab0450b8e2433f50f1bb6dbe0411f4a5098a6826f6305a4f1 | 2209461 |
| 16 | 0/0 | 22082 | 30594d73d3f7f2999b9cb11fd417b2277fc5e55ccc11fb22b2c32d090a24c6cb | 35151 | 700e428919224389d153bbb4991be44754b5ca0095db6cc7a66511325df55d64 | c859d374ee36e4f751506d60ecc619ef69d080503d54216a992405138ba10ceb | 2209462 |
