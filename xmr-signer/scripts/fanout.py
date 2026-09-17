#!/usr/bin/env python3
"""Builds output history in the stagenet test wallet by self-sending.

Talks to a monero-wallet-rpc that holds the full seed. Each transaction spends
from ONE subaddress (subaddr_indices restriction) and pays 15 of the wallet's
own subaddresses (indices 3..17), so it creates 16 owned outputs including
change, and the amount per output is sized from that subaddress's unlocked
balance so the tree keeps funding itself level after level (fees dominate:
about 0.00013 XMR per 16-output transaction at priority 1).
Stops once the wallet has received at least TARGET outputs in total (spent
ones count: a view-only wallet's outputs export includes every output ever
received). Waits for inputs to unlock between rounds. Logs every txid for the
fixture provenance notes.

Usage: fanout.py TARGET [--rpc http://127.0.0.1:38084] [--dests 15] [--fee-reserve 0.0005]
"""
import argparse, json, time, urllib.request

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("target", type=int)
    ap.add_argument("--rpc", default="http://127.0.0.1:38084")
    ap.add_argument("--dests", type=int, default=15)
    ap.add_argument("--fee-reserve", type=float, default=0.0005, help="XMR kept back per tx for the fee")
    ap.add_argument("--min-source", type=float, default=0.0008, help="minimum unlocked XMR on a subaddress to spend from it")
    ap.add_argument("--log", default="local/logs/fanout.jsonl")
    ap.add_argument("--subaddr-dests", action="store_true", help="pay 15 distinct subaddresses (adds 16 additional tx pubkeys per tx, bloats the outputs export) instead of the primary address 15 times")
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

    addrs = rpc("get_address", {"account_index": 0})["addresses"]
    while len(addrs) < 3 + a.dests:
        rpc("create_address", {"account_index": 0})
        addrs = rpc("get_address", {"account_index": 0})["addresses"]
    dests = [x["address"] for x in addrs[3:3 + a.dests]] if a.subaddr_dests else [addrs[0]["address"]] * a.dests
    reserve = int(a.fee_reserve * 1e12)

    while True:
        rpc("refresh")
        total = len(rpc("incoming_transfers", {"transfer_type": "all", "account_index": 0}).get("transfers", []))
        bal = rpc("get_balance", {"account_index": 0, "all_accounts": False})
        per = {s["address_index"]: s for s in bal.get("per_subaddress", [])}
        sources = sorted([i for i, s in per.items() if s["unlocked_balance"] >= int(a.min_source * 1e12)], key=lambda i: -per[i]["unlocked_balance"])
        log({"ev": "status", "received_outputs": total, "height": rpc("get_height")["height"], "unlocked_xmr": bal["unlocked_balance"] / 1e12, "fundable_subaddresses": sources})
        if total >= a.target:
            log({"ev": "done", "received_outputs": total, "target": a.target})
            return
        need_tx = -(-(a.target - total) // (a.dests + 1))
        if not sources:
            log({"ev": "wait", "reason": "nothing unlocked", "sleep_s": 60})
            time.sleep(60)
            continue
        sent = 0
        # several transactions per source subaddress per round: split its unlocked
        # balance so wallet2 picks a fraction of the outputs for each transaction
        for s in sources:
            if sent >= need_tx:
                break
            k = max(1, min(need_tx - sent, per[s]["num_unspent_outputs"] // 2, per[s]["unlocked_balance"] // (3 * reserve)))
            for _ in range(k):
                b = rpc("get_balance", {"account_index": 0, "address_indices": [s]})
                unlocked = b["per_subaddress"][0]["unlocked_balance"] if b.get("per_subaddress") else 0
                remaining = k - (_)
                amount = (unlocked // remaining - reserve) // a.dests
                if amount <= 0:
                    break
                try:
                    r = rpc("transfer", {"destinations": [{"address": d, "amount": amount} for d in dests], "account_index": 0, "subaddr_indices": [s], "priority": 1, "get_tx_key": False})
                    log({"ev": "tx", "txid": r["tx_hash"], "from_subaddr": s, "amount_per_output_xmr": amount / 1e12, "fee_xmr": r["fee"] / 1e12, "outputs": a.dests + 1})
                    sent += 1
                except Exception as e:
                    log({"ev": "tx_error", "error": str(e), "from_subaddr": s})
                    break
                time.sleep(2)
        time.sleep(60 if sent else 30)

if __name__ == "__main__":
    main()
