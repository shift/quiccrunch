#!/usr/bin/env python3
"""Compute medians and QuicCrunch/tonic ratios from the Raft+tonic CSV.

Also prints the observed 1-minute load range so provisional numbers cannot be
quoted without the context they were measured in.
"""
import csv
import statistics
import sys
from collections import defaultdict

path = sys.argv[1] if len(sys.argv) > 1 else "benchmarks/raft_append_vs_grpc.csv"
rows = defaultdict(list)
loads = []
with open(path, newline="") as f:
    for row in csv.DictReader(f):
        key = (int(row["payload_bytes"]), int(row["depth"]))
        rows[(row["transport"], key)].append(row)
        loads.append(float(row["load_start"]))
        loads.append(float(row["load_end"]))

cols = ["ops", "rate_per_s", "p50_us", "p95_us", "p99_us", "e2e_p99_us",
        "enqueue_p99_us", "server_p99_us", "sched_late_p99_us", "load_start", "load_end", "errors"]
med = {}
for (transport, key), group in rows.items():
    med[(transport, key)] = {c: statistics.median(float(r[c]) for r in group) for c in cols}
    med[(transport, key)]["n"] = len(group)

print("| payload | depth | tonic ops/s | QUIC ops/s | QUIC/tonic | tonic p50 | QUIC p50 | tonic p99 | QUIC p99 | winner |")
print("|---:|---:|---:|---:|---:|---:|---:|---:|---:|:---|")
for key in sorted({k for (_, k) in rows}):
    t = med.get(("tonic", key))
    q = med.get(("quic", key))
    if not t or not q:
        continue
    ratio = q["rate_per_s"] / t["rate_per_s"] if t["rate_per_s"] else 0.0
    if ratio > 1.02:
        winner = "QuicCrunch"
    elif ratio < 0.98:
        winner = "tonic"
    else:
        winner = "QuicCrunch" if q["p99_us"] < t["p99_us"] else "tonic"
    print(f"| {key[0]} B | {key[1]} | {t['rate_per_s']:.0f} | {q['rate_per_s']:.0f} | "
          f"{ratio:.2f} | {t['p50_us']:.0f} | {q['p50_us']:.0f} | "
          f"{t['p99_us']:.0f} | {q['p99_us']:.0f} | {winner} |")

print()
print("| payload | depth | transport | ops/s | p50 | p95 | p99 | e2e p99 | server p99 | sched-late p99 | load start/end | errors | n |")
print("|---:|---:|:---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|")
for key in sorted({k for (_, k) in rows}):
    for transport in ("tonic", "quic"):
        m = med.get((transport, key))
        if not m:
            continue
        print(f"| {key[0]} B | {key[1]} | {transport} | {m['rate_per_s']:.0f} | "
              f"{m['p50_us']:.0f} | {m['p95_us']:.0f} | {m['p99_us']:.0f} | "
              f"{m['e2e_p99_us']:.0f} | {m['server_p99_us']:.0f} | "
              f"{m['sched_late_p99_us']:.0f} | {m['load_start']:.2f}/{m['load_end']:.2f} | {m['errors']:.0f} | {m['n']:.0f} |")
print()
print(f"Load (1-min average) across all cells: min {min(loads):.2f}, max {max(loads):.2f}.")
print()
print("Max sustained rate by transport/payload (median over repeats, best depth):")
for transport in ("tonic", "quic"):
    for payload in (512, 4096):
        candidates = [m["rate_per_s"] for (t, (p, _)), m in med.items() if t == transport and p == payload]
        if candidates:
            print(f"- {transport} {payload} B: {max(candidates):.0f} ops/s")