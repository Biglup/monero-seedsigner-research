#!/usr/bin/env python3
"""Takes an outputs-export snapshot of the view-only wallet (monero-wallet-rpc on
port 38085), computes the key image reply with xmr-signer, imports that reply
into the view-only wallet with monero-wallet-cli (the RPC has no blob import),
restarts the RPC, and writes fixtures/<label>/{outputs.bin,keyimages.bin} with
a provenance note.

Usage: snapshot_outputs.py <label>
"""
import binascii, hashlib, json, os, subprocess, sys, time, urllib.request

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
LOCAL = os.path.join(ROOT, "local")
RPC = "http://127.0.0.1:38085"

def rpc(method, params=None):
    req = urllib.request.Request(RPC + "/json_rpc", data=json.dumps({"jsonrpc": "2.0", "id": "0", "method": method, "params": params or {}}).encode(), headers={"Content-Type": "application/json"})
    r = json.load(urllib.request.urlopen(req, timeout=900))
    if "error" in r:
        raise RuntimeError("%s: %s" % (method, r["error"]))
    return r["result"]

def main():
    label = sys.argv[1]
    fdir = os.path.join(ROOT, "fixtures", label)
    os.makedirs(fdir, exist_ok=True)
    rpc("refresh")
    height = rpc("get_height")["height"]
    outs = rpc("incoming_transfers", {"transfer_type": "all", "account_index": 0}).get("transfers", [])
    blob = binascii.unhexlify(rpc("export_outputs", {"all": True})["outputs_data_hex"])
    outputs_path = os.path.join(fdir, "outputs.bin")
    open(outputs_path, "wb").write(blob)
    ki_path = os.path.join(fdir, "keyimages.bin")
    env = dict(os.environ, XMR_SEED_FILE=os.path.join(LOCAL, "stagenet-seed.txt"))
    out = subprocess.run([os.path.join(ROOT, "target", "release", "xmr-signer"), "keyimages", outputs_path, ki_path], env=env, capture_output=True, text=True)
    print(out.stderr.strip())
    if out.returncode != 0:
        sys.exit(1)
    kblob = open(ki_path, "rb").read()

    # import the reply into the view-only wallet: stop the RPC, CLI import, restart
    subprocess.run(["pkill", "-f", "rpc-bind-port 38085"])
    time.sleep(3)
    cli = [os.path.join(LOCAL, "monero-cli", "monero-wallet-cli"), "--stagenet", "--wallet-file", os.path.join(LOCAL, "wallets", "viewonly"), "--password", "", "--daemon-address", "node.monerodevs.org:38089", "--trusted-daemon", "--log-file", os.path.join(LOCAL, "logs", "cli.log"), "--command", "import_key_images", ki_path]
    r = subprocess.run(cli, capture_output=True, text=True)
    imported = [l for l in r.stdout.splitlines() if "imported" in l.lower() or "error" in l.lower()]
    print("\n".join(imported[-2:]))
    subprocess.Popen([os.path.join(LOCAL, "monero-cli", "monero-wallet-rpc"), "--stagenet", "--daemon-address", "node.monerodevs.org:38089", "--trusted-daemon", "--rpc-bind-port", "38085", "--disable-rpc-login", "--wallet-file", os.path.join(LOCAL, "wallets", "viewonly"), "--password", "", "--log-file", os.path.join(LOCAL, "logs", "wallet-rpc-viewonly.log"), "--log-level", "0"], stdout=open(os.path.join(LOCAL, "logs", "wallet-rpc-viewonly.out"), "a"), stderr=subprocess.STDOUT)
    for _ in range(30):
        time.sleep(2)
        try:
            rpc("get_version")
            break
        except Exception:
            pass
    b = rpc("get_balance", {"account_index": 0})
    print("view-only wallet after import: balance %.6f unlocked %.6f" % (b["balance"] / 1e12, b["unlocked_balance"] / 1e12))

    with open(os.path.join(fdir, "PROVENANCE.md"), "a") as f:
        f.write("# Snapshot %s (%d received outputs)\n\n" % (label, len(outs)))
        f.write("Stagenet test wallet (see fixtures/snap50/PROVENANCE.md for the wallet and history). Outputs export taken with monero-wallet-rpc 0.18.5.1 `export_outputs {all: true}` from the view-only wallet at chain height %d on %s; this is the same wallet2 code path Feather's wizard uses (`export_outputs_to_str`), so the bytes are format-identical to a Feather export of the same wallet state.\n\n" % (height, time.strftime("%Y-%m-%d %H:%M UTC", time.gmtime())))
        f.write("| File | Size | sha256 |\n|---|---|---|\n")
        f.write("| outputs.bin | %d | %s |\n" % (len(blob), hashlib.sha256(blob).hexdigest()))
        f.write("| keyimages.bin (xmr-signer keyimages) | %d | %s |\n" % (len(kblob), hashlib.sha256(kblob).hexdigest()))
    print("snapshot %s: %d outputs, export %d bytes, key images %d bytes" % (label, len(outs), len(blob), len(kblob)))

if __name__ == "__main__":
    main()
