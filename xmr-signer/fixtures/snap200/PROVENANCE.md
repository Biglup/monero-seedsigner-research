# Snapshot snap200 (215 received outputs)

Stagenet test wallet (see fixtures/snap50/PROVENANCE.md for the wallet and history). Outputs export taken with monero-wallet-rpc 0.18.5.1 `export_outputs {all: true}` from the view-only wallet at chain height 2209425; this is the same wallet2 code path Feather's wizard uses (`export_outputs_to_str`), so the bytes are format-identical to a Feather export of the same wallet state.

| File | Size | sha256 |
|---|---|---|
| outputs.bin | 33381 | 5050870ec198b95711cfa36a36ab971e4b79a6cd225cc609e90a073ec6924107 |
| keyimages.bin (xmr-signer keyimages) | 20804 | 2741f4c7360a8f360baa43345fb6555b6617d0f5118f5304b4e31940344493a4 |

## Unsigned and signed transaction sets (monero-wallet-rpc 0.18.5.1 view-only wallet)

Built with scripts/snapshot_txs.py: sweep_all of exactly N unlocked outputs of one subaddress to subaddress 0/2, ring size 16, priority 1. Signed by xmr-signer, submitted through the same wallet.

| Inputs | From subaddress | Unsigned bytes | sha256 | Signed bytes | sha256 | txid | Height |
|---|---|---|---|---|---|---|---|
| 16 | 0/0 | 21635 | 49e6b407650c0e1e6eb65079e6b733416564ab3dcaa9787698a2874da695c570 | 34573 | c8b39c16670edd951d22828313daf46cc288360d798318f4923295319b429f2c | 454e25b316331bfb09a6d05906b83b4b05efde5a6b0e2dd62b5a766288347724 | 2209430 |
| 2 | 0/6 | 4120 | 6b692bfa542e2099eb870bd76e52395dd6bc6fadff00da3e259a62772db79051 | 6666 | ab20a1e0caf2ed13860a9097c6bfa483d4fb009ca18e3e1547213ea899a26a4a | f503d27594b6706be877173f2557c795c3c57f0882528cb1a25c0d6b18ec6e79 | 2209430 |
