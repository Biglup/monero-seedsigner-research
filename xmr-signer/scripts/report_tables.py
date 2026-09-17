#!/usr/bin/env python3
"""Builds the REPORT.md measurement tables from the raw on-device spike-bench
outputs in dist/pi-spike-bench-<snapshot>.txt. Prints markdown to stdout."""
import glob, os, re, sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

def parse(path):
    d = {"phases": {}, "sizes": {}}
    for line in open(path):
        m = re.match(r"^(snap\d+)\s+(\w+)\s+(\d+)\s+(\d+)\s+(\d+)\s+(\d+)\s*$", line)
        if m:
            d["phases"][m.group(2)] = tuple(int(m.group(i)) for i in range(3, 7))
            continue
        if line.startswith("snap") and "=" in line:
            kv = dict(p.split("=") for p in line.split()[1:])
            if "wallet_outputs" in kv:
                d["sizes"]["outputs"] = kv
            elif kv.get("tx_inputs") == "2":
                d["sizes"]["2in"] = kv
            elif kv.get("tx_inputs") == "16":
                d["sizes"]["16in"] = kv
        m = re.match(r"^peak rss: (\d+) (\w+)", line)
        if m:
            d["rss"] = (int(m.group(1)), m.group(2))
    return d

def frames(kv, key):
    return "%s (%s)" % (kv[key + "@30"], kv[key + "@120"])

def main():
    snaps = {}
    for s in ("snap50", "snap200", "snap500"):
        p = os.path.join(ROOT, "dist", "pi-spike-bench-%s.txt" % s)
        if os.path.exists(p):
            snaps[s] = parse(p)
    print("| Wallet outputs | Outputs export bytes | QR frames | Key image export bytes | QR frames | Unsigned tx bytes (2 in) | QR frames | Signed tx bytes | QR frames |")
    print("|---|---|---|---|---|---|---|---|---|")
    for s, d in snaps.items():
        o = d["sizes"].get("outputs"); t = d["sizes"].get("2in")
        if not o or not t:
            continue
        print("| %s | %s | %s | %s | %s | %s | %s | %s | %s |" % (o["wallet_outputs"], o["outputs_export_bytes"], frames(o, "outputs_frames"), o["keyimage_export_bytes"], frames(o, "keyimages_frames"), t["unsigned_tx_bytes"], frames(t, "unsigned_frames"), t["signed_tx_bytes"], frames(t, "signed_frames")))
    print()
    print("16-input transaction per snapshot:")
    print()
    print("| Wallet outputs | Unsigned tx bytes (16 in) | QR frames | Signed tx bytes | QR frames |")
    print("|---|---|---|---|---|")
    for s, d in snaps.items():
        o = d["sizes"].get("outputs"); t = d["sizes"].get("16in")
        if not o or not t:
            continue
        print("| %s | %s | %s | %s | %s |" % (o["wallet_outputs"], t["unsigned_tx_bytes"], frames(t, "unsigned_frames"), t["signed_tx_bytes"], frames(t, "signed_frames")))
    print()
    print("| Operation (Pi Zero 1.3) | Median ms (20 runs) | Peak RSS MB |")
    print("|---|---|---|")
    rows = []
    for s, d in snaps.items():
        n = d["sizes"].get("outputs", {}).get("wallet_outputs", "?")
        ph = d["phases"]; rss = "%.1f" % (d["rss"][0] / 1024) if "rss" in d and d["rss"][1] == "kilobytes" else "?"
        if "parse_outputs" in ph: rows.append(("Parse outputs export (%s outputs)" % n, ph["parse_outputs"][1], rss))
        if "key_images" in ph: rows.append(("Compute key images (%s outputs)" % n, ph["key_images"][1], rss))
        if "parse_2in" in ph: rows.append(("Parse unsigned tx (2 inputs, %s-output wallet)" % n, ph["parse_2in"][1], rss))
        if "sign_2in" in ph: rows.append(("Sign (2 inputs, %s-output wallet)" % n, ph["sign_2in"][1], rss))
        if "sign_16in" in ph: rows.append(("Sign (16 inputs, %s-output wallet)" % n, ph["sign_16in"][1], rss))
    for name, us, rss in rows:
        print("| %s | %d | %s |" % (name, round(us / 1000), rss))

if __name__ == "__main__":
    main()
