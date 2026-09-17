#!/usr/bin/env python3
"""Produces the per-snapshot unsigned tx fixtures from the view-only wallet in
monero-wallet-rpc (port 38085), signs them with the xmr-signer CLI, submits
the signed sets through the same wallet, and stores everything under
fixtures/<label>/ with provenance.

Exact input counts: all unlocked unspent outputs of one subaddress except N
are frozen, then sweep_all restricted to that subaddress builds a transaction
that spends exactly those N outputs. Frozen outputs are thawed afterwards.

Usage: snapshot_txs.py <label> [--dest ADDRESS] [--rpc URL]
"""
import argparse, binascii, hashlib, json, os, subprocess, sys, time, urllib.request

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("label")
    ap.add_argument("--dest", default="7BQ5r9JZRCGK5AYDwtyi4jQ8Kob7jGqgicHEBsfARGdNKY1BcuX42hb2Uwb2hxHTuhRd1Y9T67nmogAM8u4iUg2LP43tDhE")
    ap.add_argument("--rpc", default="http://127.0.0.1:38085")
    ap.add_argument("--counts", default="16,2")
    a = ap.parse_args()

    def rpc(method, params=None):
        req = urllib.request.Request(a.rpc + "/json_rpc", data=json.dumps({"jsonrpc": "2.0", "id": "0", "method": method, "params": params or {}}).encode(), headers={"Content-Type": "application/json"})
        r = json.load(urllib.request.urlopen(req, timeout=900))
        if "error" in r:
            raise RuntimeError("%s: %s" % (method, r["error"]))
        return r["result"]

    fdir = os.path.join(ROOT, "fixtures", a.label)
    os.makedirs(fdir, exist_ok=True)
    prov = []
    for n in [int(x) for x in a.counts.split(",")]:
        rpc("refresh")
        height = rpc("get_height")["height"]
        # spendable = unlocked AND at least 10 confirmations (CRYPTONOTE_DEFAULT_TX_SPENDABLE_AGE);
        # the RPC's "unlocked" flag alone is not enough
        outs = [o for o in rpc("incoming_transfers", {"transfer_type": "all", "account_index": 0}).get("transfers", []) if not o["spent"] and o.get("unlocked") and not o.get("frozen") and o.get("block_height", 0) + 10 <= height]
        by_sub = {}
        for o in outs:
            by_sub.setdefault(o["subaddr_index"]["minor"], []).append(o)
        eligible = [s for s in by_sub if len(by_sub[s]) >= n]
        if not eligible:
            best = max(by_sub, key=lambda s: len(by_sub[s])) if by_sub else None
            print("not enough spendable outputs on one subaddress for %d inputs (best %s)" % (n, ("%d on 0/%d" % (len(by_sub[best]), best)) if best is not None else "none"))
            sys.exit(1)
        # prefer the subaddress with the fewest surplus outputs (least freezing)
        sub = min(eligible, key=lambda s: len(by_sub[s]))
        ranked = sorted(by_sub[sub], key=lambda o: -o["amount"])
        keep = ranked[:n]
        freeze = ranked[n:]
        for o in freeze:
            rpc("freeze", {"key_image": o["key_image"]})
        # pay (sum of the kept outputs - fee reserve): with the surplus frozen, only
        # all N kept outputs together can fund it, so the tx must have exactly N inputs
        total_kept = sum(o["amount"] for o in keep)
        amount = total_kept - int(0.0003 * 1e12)
        if amount <= 0:
            print("kept outputs too small to pay the fee reserve (%.8f XMR on 0/%d)" % (total_kept / 1e12, sub))
            sys.exit(1)
        try:
            r = rpc("transfer", {"destinations": [{"address": a.dest, "amount": amount}], "account_index": 0, "subaddr_indices": [sub], "priority": 1, "do_not_relay": True})
        finally:
            for o in freeze:
                rpc("thaw", {"key_image": o["key_image"]})
        blob = binascii.unhexlify(r["unsigned_txset"])
        unsigned = os.path.join(fdir, "unsigned_%din.bin" % n)
        open(unsigned, "wb").write(blob)
        # count inputs by parsing the set with the signer's "show" (authoritative)
        env0 = dict(os.environ, XMR_SEED_FILE=os.path.join(ROOT, "local", "stagenet-seed.txt"))
        shown = subprocess.run([os.path.join(ROOT, "target", "release", "xmr-signer"), "show", unsigned], env=env0, capture_output=True, text=True).stderr
        import re
        m = re.search(r"inputs (\d+)", shown)
        if not m or int(m.group(1)) != n:
            print("expected %d inputs, got: %s" % (n, shown.strip().splitlines()[0] if shown.strip() else "?"))
            os.remove(unsigned)
            sys.exit(1)
        signed = os.path.join(fdir, "signed_%din.bin" % n)
        env = dict(os.environ, XMR_SEED_FILE=os.path.join(ROOT, "local", "stagenet-seed.txt"))
        out = subprocess.run([os.path.join(ROOT, "target", "release", "xmr-signer"), "sign", unsigned, signed], env=env, capture_output=True, text=True)
        print(out.stderr.strip())
        if out.returncode != 0:
            sys.exit(1)
        sblob = open(signed, "rb").read()
        sub_r = rpc("submit_transfer", {"tx_data_hex": binascii.hexlify(sblob).decode()})
        txids = sub_r["tx_hash_list"]
        print("submitted %d-input tx: %s (unsigned %d bytes, signed %d bytes)" % (n, txids, len(blob), len(sblob)))
        h = rpc("get_height")["height"]
        prov.append((n, len(blob), hashlib.sha256(blob).hexdigest(), len(sblob), hashlib.sha256(sblob).hexdigest(), txids[0], h, sub))
        time.sleep(5)
    with open(os.path.join(fdir, "PROVENANCE.md"), "a") as f:
        f.write("\n## Unsigned and signed transaction sets (monero-wallet-rpc 0.18.5.1 view-only wallet)\n\n")
        f.write("Built with scripts/snapshot_txs.py: sweep_all of exactly N unlocked outputs of one subaddress to subaddress 0/2, ring size 16, priority 1. Signed by xmr-signer, submitted through the same wallet.\n\n")
        f.write("| Inputs | From subaddress | Unsigned bytes | sha256 | Signed bytes | sha256 | txid | Height |\n|---|---|---|---|---|---|---|---|\n")
        for n, ub, uh, sb, sh, txid, h, sub in prov:
            f.write("| %d | 0/%d | %d | %s | %d | %s | %s | %d |\n" % (n, sub, ub, uh, sb, sh, txid, h))

if __name__ == "__main__":
    main()
