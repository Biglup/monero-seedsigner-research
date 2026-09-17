#!/usr/bin/env python3
"""Builds output history in the stagenet test wallet by self-sending.

Talks to a monero-wallet-rpc (see PI_RUN.md / REPORT.md for how it was started)
that holds the full seed. Each transfer pays 15 of the wallet's own subaddresses
(indices 3..17) so every transaction creates 16 owned outputs including change.
Stops once the wallet has received at least TARGET outputs in total (spent ones
count: a view-only wallet's outputs export includes every output ever received).
Waits for inputs to unlock between rounds. Logs every txid with height for the
fixture provenance notes.

Usage: fanout.py TARGET [--rpc http://127.0.0.1:38084] [--amount 0.0001] [--max-tx-per-round N]
"""
import argparse, json, sys, time, urllib.request

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("target", type=int)
    ap.add_argument("--rpc", default="http://127.0.0.1:38084")
    ap.add_argument("--amount", type=float, default=0.0001)
    ap.add_argument("--dests", type=int, default=15)
    ap.add_argument("--max-tx-per-round", type=int, default=64)
    ap.add_argument("--log", default="local/logs/fanout.jsonl")
    a = ap.parse_args()

    def rpc(method, params=None):
        req = urllib.request.Request(a.rpc + "/json_rpc", data=json.dumps({"jsonrpc": "2.0", "id": "0", "method": method, "params": params or {}}).encode(), headers={"Content-Type": "application/json"})
        r = json.load(urllib.request.urlopen(req, timeout=900))
        if "error" in r:
            raise RuntimeError("%s: %s" % (method, r["error"]))
        return r["result"]

    def log(ev):
        ev["t"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
        with open(a.log, "a") as f:
            f.write(json.dumps(ev) + "\n")
        print(json.dumps(ev), flush=True)

    def received_outputs():
        r = rpc("incoming_transfers", {"transfer_type": "all", "account_index": 0})
        return r.get("transfers", [])

    # make sure destination subaddresses exist
    addrs = rpc("get_address", {"account_index": 0})["addresses"]
    while len(addrs) < 3 + a.dests:
        rpc("create_address", {"account_index": 0})
        addrs = rpc("get_address", {"account_index": 0})["addresses"]
    dests = [x["address"] for x in addrs[3:3 + a.dests]]
    atomic = int(round(a.amount * 1e12))

    while True:
        rpc("refresh")
        outs = received_outputs()
        total = len(outs)
        unlocked = [o for o in outs if not o["spent"] and o.get("unlocked")]
        bal = rpc("get_balance", {"account_index": 0})
        log({"ev": "status", "received_outputs": total, "unspent_unlocked": len(unlocked), "unlocked_balance_xmr": bal["unlocked_balance"] / 1e12, "height": rpc("get_height")["height"]})
        if total >= a.target:
            log({"ev": "done", "received_outputs": total, "target": a.target})
            return
        need_tx = -(-(a.target - total) // (a.dests + 1))  # ceil
        n = min(need_tx, len(unlocked), a.max_tx_per_round)
        if n == 0 or bal["unlocked_balance"] < atomic * a.dests * 2:
            log({"ev": "wait", "reason": "no unlocked inputs" if n == 0 else "unlocked balance too low", "sleep_s": 60})
            time.sleep(60)
            continue
        for i in range(n):
            try:
                r = rpc("transfer", {"destinations": [{"address": d, "amount": atomic} for d in dests], "account_index": 0, "priority": 1, "get_tx_key": False, "do_not_relay": False})
                log({"ev": "tx", "txid": r["tx_hash"], "fee_xmr": r["fee"] / 1e12, "outputs": a.dests + 1, "round_index": i, "of": n})
            except Exception as e:
                log({"ev": "tx_error", "error": str(e), "round_index": i, "of": n})
                break
            time.sleep(2)
        time.sleep(30)

if __name__ == "__main__":
    main()
