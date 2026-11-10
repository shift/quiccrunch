#!/usr/bin/env python3
"""Median-of-runs summary for the tonic vs QuicCrunch comparison matrix."""
import csv
import statistics
import sys
from collections import defaultdict

NUM_FIELDS = [
    "throughput_msg_s", "rtt_p50_us", "rtt_p95_us", "rtt_p99_us", "rtt_p999_us",
    "e2e_p50_us", "e2e_p99_us", "queue_p50_us", "errors", "timeouts", "cutoff",
]

A = "tonic"   # transport A: tonic / h2 over TCP+TLS
B = "quic"    # transport B: QuicCrunch


def canon(name):
    return A if name.startswith("tonic") else B


def load(path):
    cells = defaultdict(list)
    loads = []
    levels = set()
    with open(path, newline="") as fh:
        for row in csv.DictReader(fh):
            row["transport"] = canon(row["transport"])
            key = (
                row["transport"],
                int(row["workers"]),
                int(row["connections"]),
                int(row["depth"]),
                float(row["delay_ms"]),
                int(row["msg_size"]),
            )
            cells[key].append(row)
            if row.get("loadavg"):
                loads.append(float(row["loadavg"].split("/")[0]))
            if row.get("log_level"):
                levels.add(row["log_level"])
    return cells, loads, levels


def good_rows(cells, key):
    """Rows that completed without errors/timeouts. Failures are reported
    separately; they may not enter latency medians."""
    return [r for r in cells.get(key, [])
            if float(r["errors"]) == 0 and float(r["timeouts"]) == 0]


def med(cells, key, field):
    rows = good_rows(cells, key)
    return statistics.median([float(r[field]) for r in rows]) if rows else float("nan")


def fmt(v, nd=0):
    if v != v:
        return "-"
    return f"{v:,.{nd}f}"


def ratio(b, a):
    return b / a if a == a and b == b and a else float("nan")


def winner(a, b):
    r = ratio(b, a)
    if r != r:
        return "-"
    if r > 1.02:
        return "QUIC"
    if r < 0.98:
        return "tonic"
    return "tie"


def main(path):
    cells, loads, levels = load(path)
    delays = sorted({k[4] for k in cells})
    conns_all = sorted({k[2] for k in cells})
    depths = sorted({k[3] for k in cells})
    big = max(k[5] for k in cells)

    print("# tonic vs QuicCrunch — measured medians")
    print()
    print(f"Source: `{path}`. Medians are over runs that completed without")
    print("errors or timeouts; failed runs are counted in the reliability section.")
    print()
    print(f"- effective log level: {', '.join(sorted(levels)) or 'unknown'}")
    if loads:
        print(f"- 1-minute load average while running: {min(loads):.2f} – {max(loads):.2f}")
    print()

    # Throughput
    for delay in delays:
        print(f"## Throughput (msg/s), one-way delay {delay:g} ms")
        print()
        print("| conns | depth | tonic | QUIC | QUIC/tonic | winner |")
        print("|---|---|---|---|---|---|")
        for conns in conns_all:
            for depth in depths:
                ka = (A, 4, conns, depth, delay, big)
                kb = (B, 4, conns, depth, delay, big)
                if ka not in cells and kb not in cells:
                    continue
                a, b = med(cells, ka, "throughput_msg_s"), med(cells, kb, "throughput_msg_s")
                print(f"| {conns} | {depth} | {fmt(a)} | {fmt(b)} | {fmt(ratio(b, a), 2)} | {winner(a, b)} |")
        print()

    # Latency
    for field, title in (("rtt_p50_us", "RTT p50"), ("rtt_p99_us", "RTT p99"),
                         ("rtt_p999_us", "RTT p99.9")):
        print(f"## {title} (µs), 1 KiB payload")
        print()
        for delay in delays:
            print(f"### delay {delay:g} ms")
            print()
            print("| conns | depth | tonic | QUIC | QUIC/tonic |")
            print("|---|---|---|---|---|")
            for conns in conns_all:
                for depth in depths:
                    ka = (A, 4, conns, depth, delay, big)
                    kb = (B, 4, conns, depth, delay, big)
                    if ka not in cells and kb not in cells:
                        continue
                    a, b = med(cells, ka, field), med(cells, kb, field)
                    print(f"| {conns} | {depth} | {fmt(a)} | {fmt(b)} | {fmt(ratio(b, a), 2)} |")
            print()

    # End-to-end and queue at 1 KiB
    print("## End-to-end p50 / p99 and client-queue p50 (µs), delay 0 ms")
    print()
    print("| conns | depth | tonic e2e p50 | QUIC e2e p50 | tonic e2e p99 | QUIC e2e p99 | tonic q p50 | QUIC q p50 |")
    print("|---|---|---|---|---|---|---|---|")
    for conns in conns_all:
        for depth in depths:
            ka = (A, 4, conns, depth, 0.0, big)
            kb = (B, 4, conns, depth, 0.0, big)
            if ka not in cells and kb not in cells:
                continue
            print(
                f"| {conns} | {depth} | {fmt(med(cells,ka,'e2e_p50_us'),0)} | "
                f"{fmt(med(cells,kb,'e2e_p50_us'),0)} | {fmt(med(cells,ka,'e2e_p99_us'),0)} | "
                f"{fmt(med(cells,kb,'e2e_p99_us'),0)} | {fmt(med(cells,ka,'queue_p50_us'),1)} | "
                f"{fmt(med(cells,kb,'queue_p50_us'),1)} |")
    print()

    # Worker scaling (QUIC only)
    one = [k for k in cells if k[0] == B and k[1] == 1]
    if one:
        print("## QuicCrunch worker scaling (delay 0 ms, 1 KiB)")
        print()
        print("| conns | depth | 1 worker | 4 workers | 4w/1w | failed runs |")
        print("|---|---|---|---|---|---|")
        for conns in sorted({k[2] for k in one}):
            for depth in depths:
                k1 = (B, 1, conns, depth, 0.0, big)
                k4 = (B, 4, conns, depth, 0.0, big)
                if k1 not in cells:
                    continue
                failed = sum(1 for r in cells[k1]
                             if float(r["errors"]) or float(r["timeouts"]))
                a, b = med(cells, k1, "throughput_msg_s"), med(cells, k4, "throughput_msg_s")
                print(f"| {conns} | {depth} | {fmt(a)} | {fmt(b)} | {fmt(ratio(b, a), 2)} | {failed}/{len(cells[k1])} |")
        print()

    # Reliability
    print("## Reliability")
    print()
    bad = 0
    total = 0
    for key, rows in cells.items():
        for r in rows:
            total += 1
            if float(r["errors"]) or float(r["timeouts"]):
                bad += 1
    print(f"- {total - bad}/{total} runs completed with zero errors and zero timeouts.")
    if bad:
        print("- failed runs:")
        for key, rows in sorted(cells.items()):
            fails = [r for r in rows if float(r["errors"]) or float(r["timeouts"])]
            if fails:
                print(f"  - {key[0]} w={key[1]} c={key[2]} d={key[3]} delay={key[4]:g}: "
                      f"{len(fails)}/{len(rows)} runs failed (errors="
                      f"{'/'.join(r['errors'] for r in fails)})")
    print()

    # Win tally
    print("## Win tally (throughput, median, ±2 % dead band)")
    print()
    for delay in delays:
        wins = {"tonic": 0, "QUIC": 0, "tie": 0}
        for conns in conns_all:
            for depth in depths:
                ka = (A, 4, conns, depth, delay, big)
                kb = (B, 4, conns, depth, delay, big)
                if ka not in cells and kb not in cells:
                    continue
                wins[winner(med(cells, ka, "throughput_msg_s"), med(cells, kb, "throughput_msg_s"))] += 1
        print(f"- delay {delay:g} ms: tonic {wins['tonic']}, QUIC {wins['QUIC']}, tie {wins['tie']}")
    print()


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit("usage: comparison-summarize.py <matrix.csv>")
    main(sys.argv[1])