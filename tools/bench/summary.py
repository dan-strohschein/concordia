"""Prints editor bench reports (`concordia-editor --bench`) as a table: python3 -I summary.py <report.json>..."""
import json
import sys

SCENARIOS = ["orbit", "native_orbit_baseline", "orbit_with_agent_edits", "gizmo_drag", "play"]


def f(x, width=6, digits=1):
    return f"{x:{width}.{digits}f}" if isinstance(x, (int, float)) else f"{'-':>{width}}"


def row(name, s):
    v = s["viewport"]
    lat, iv, cpu, page = v["latency_ms"], v["interval_ms"], v["cpu_ms"], s["page_interval_ms"]
    if name == "play":  # no input during play: pacing only
        lat = {"p50": None, "p95": None}
    edits = s.get("edit_to_panel_ms")
    return (
        f"  {name:24} {f(v['fps'])} fps  missed {v['missed_refreshes']:4}  "
        f"latency p50 {f(lat['p50'])} p95 {f(lat['p95'])}  interval p95 {f(iv['p95'])}  "
        f"cpu p50 {f(cpu['p50'], 5)} p95 {f(cpu['p95'], 5)}  page p95 {f(page['p95'])} max {f(page['max'])}"
        + (f"  edit→panel p50 {edits['p50']:.0f} p95 {edits['p95']:.0f} max {edits['max']:.0f}" if edits else "")
    )


for path in sys.argv[1:]:
    report = json.load(open(path))
    ui = report["ui"]
    if "error" in ui:
        print(f"{path}: FAILED {ui['error']}")
        continue
    print(f"{path}: {ui['entities']} entities, display {ui.get('display_hz') or '?'} Hz, page rAF {ui['refresh_ms']:.1f} ms, dpr {ui['device_pixel_ratio']}, editor RSS {report['editor_rss_mb'] or 0:.0f} MB")
    for name in SCENARIOS:
        print(row(name, ui[name]))
