"""Summarizes `concordia run --bench-latency` reports: python3 -I latency.py <report.json>..."""
import json
import sys


def pct(values, p):
    if not values:
        return float("nan")
    s = sorted(values)
    return s[max(0, min(len(s) - 1, int(round(p / 100 * len(s) + 0.5)) - 1))]


for path in sys.argv[1:]:
    report = json.load(open(path))
    hz = report.get("hz", 120)
    frames = [f for f in report["frames"] if f.get("presented_ms") is not None]
    tagged = [f for f in frames if f.get("input_time_ms") is not None and f.get("seq") is not None]
    first = {}
    for f in tagged:
        if f["seq"] not in first or f["presented_ms"] < first[f["seq"]]:
            first[f["seq"]] = f["presented_ms"] - f["input_time_ms"]
    latency = list(first.values())
    shown = sorted(f["presented_ms"] for f in frames)
    intervals = [b - a for a, b in zip(shown, shown[1:])]
    span = (shown[-1] - shown[0]) if len(shown) > 1 else 0
    fps = len(intervals) / span * 1000 if span else 0
    queue = [f["submitted_ms"] - f["queued_ms"] for f in frames if f.get("submitted_ms") is not None]
    gpu = [f["gpu_ms"] for f in frames if f.get("gpu_ms") is not None]
    cpu = [f["cpu_ms"] for f in frames]
    print(f"{path}: {len(frames)} frames, {fps:.1f} fps @ {hz} Hz")
    print(f"  input->photon p50 {pct(latency, 50):.1f}  p95 {pct(latency, 95):.1f}  max {max(latency or [0]):.1f} ms  ({len(latency)} inputs)")
    print(f"  queued->submitted p50 {pct(queue, 50):.1f}  p95 {pct(queue, 95):.1f};  cpu p50 {pct(cpu, 50):.1f};  gpu p50 {pct(gpu, 50):.1f} ms")
