#!/usr/bin/env python3
"""Offline CSR_PERF_REPORT analyzer. Standard library only; never launches a process."""
from __future__ import annotations

import argparse
from datetime import datetime, timedelta, timezone
import hashlib
import json
import math
from pathlib import Path
import re
import statistics


ANSI = re.compile(r"\x1b\[[0-?]*[ -/]*[@-~]")
STAMP = re.compile(r"\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(?:\.\d+)?(?:Z|[+-]\d\d:\d\d)")
NUMBER = r"(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?"
MARKERS = {
    "perf interval: ": "interval", "perf context: ": "context",
    "perf simulation census: ": "simulation", "perf scene census: ": "scene",
    "perf asset census: ": "assets", "perf character census: ": "characters",
}
COUNTS = {
    "simulation": "tick tick_rate sim_seconds competitive round players bots alive items projectiles fields".split(),
    "scene": "entities mesh_entities hidden_local view_visible_any no_frustum skinned_meshes ui_nodes audio_players impact_fx cameras active_cameras".split(),
    "assets": "meshes images world_materials standard_materials particle_materials".split(),
    "characters": "instances bodies pending physical".split(),
}
CASE_KEYS = "map bots players tick_rate resolution present_requested frame_latency_hint competitive camera".split()
LIMITS = [
    "Camera pose/aim is NOT LOGGED by this diagnostics format; actual matched camera evidence is a missing admission requirement.",
    "Local player alive state is NOT LOGGED (aggregate alive is insufficient); this is a missing admission requirement.",
    "No external presentation trace is supplied or measured here. wpr.exe availability alone is not presentation evidence.",
    "Time<Real> cadence and nested CPU wall scopes do not measure input-to-display latency or GPU execution time.",
    "GPU diagnostics are retained-history means with sample counts, not interval totals or frame quantiles.",
    "Census counts are endpoint-only retained CPU assets/entities, not GPU memory, draw calls, or proof of a leak.",
]


def utc(value):
    parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    if parsed.tzinfo is None:
        raise ValueError("UTC timestamp needs an explicit timezone")
    return parsed.astimezone(timezone.utc)


def iso(value):
    return value.isoformat().replace("+00:00", "Z")


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def fields(body):
    """Scalar log fields; debug structs/quantile arrays are parsed separately."""
    found = re.findall(r"\b([a-zA-Z_][a-zA-Z_0-9]*)=([^\s]+)", body)
    if len({key for key, _ in found}) != len(found):
        raise ValueError("duplicate scalar fields")
    return dict(found)


def numeric(value, integer=False, positive=False):
    if not re.fullmatch(r"\d+" if integer else NUMBER, value):
        raise ValueError(f"invalid nonnegative number: {value}")
    result = int(value) if integer else float(value)
    if (integer and result > 2**64 - 1) or not math.isfinite(result) or (positive and result <= 0):
        raise ValueError(f"invalid finite positive number: {value}")
    return result


def boolean(value):
    if value not in ("true", "false"):
        raise ValueError(f"invalid boolean: {value}")
    return value == "true"


def parse_interval(body):
    f = fields(body)
    result = {key: numeric(f[key], positive=True) for key in
              ("elapsed_real_s", "report_wall_s", "sampled_real_s", "mean_ms", "max_ms")}
    result.update({key: numeric(f[key], integer=True) for key in
                   ("frames", "overflow", "rejected", "context_changes")})
    result["mixed_context"] = boolean(f["mixed_context"])
    q = re.search(r"\bp50_p95_p99_ms=(None|Some\(\[([^\]]*)\]\))(?=\s|$)", body)
    if not q:
        raise ValueError("missing/invalid p50_p95_p99_ms")
    result["quantiles_ms"] = None if q[1] == "None" else [numeric(v.strip(), positive=True) for v in q[2].split(",")]
    if result["frames"] == 0:
        raise ValueError("zero valid frames")
    if result["overflow"] != max(0, result["frames"] - 8192):
        raise ValueError("overflow does not match the 8192-sample bound")
    if result["quantiles_ms"] is None:
        if not result["overflow"]:
            raise ValueError("missing full-interval quantiles without overflow")
    else:
        q = result["quantiles_ms"]
        if result["overflow"] or len(q) != 3 or q != sorted(q) or q[-1] > result["max_ms"] + .000051:
            raise ValueError("invalid or truncated interval quantiles")
    if result["mean_ms"] > result["max_ms"] + .0001:
        raise ValueError("mean exceeds maximum")
    # mean/max have four printed decimal places; sampled_real_s has six.
    if abs(result["mean_ms"] * result["frames"] / 1000 - result["sampled_real_s"]) > result["frames"] * .00005 / 1000 + .000001:
        raise ValueError("frames * mean_ms disagrees with sampled_real_s beyond print precision")
    if result["mixed_context"] != (result["context_changes"] != 0):
        raise ValueError("mixed_context/context_changes disagree")
    return result


CONTEXT = re.compile(
    r"endpoint_only=true map=(?P<map>\S+) mixed_context=(?P<mixed>true|false) "
    r"observed=Some\(FrameContext \{ state: (?P<state>\w+), page: (?P<page>None|Some\(\w+\)), "
    r"in_match: (?P<in_match>true|false), window: Some\(WindowContext \{ "
    r"size: \((?P<width>\d+), (?P<height>\d+)\), focused: (?P<focused>true|false), "
    r"present_requested: (?P<present>\w+), frame_latency_hint: (?P<latency>None|Some\(\d+\)) \}\), "
    r"screenshot: (?P<screenshot>true|false), input_enabled: (?P<input_enabled>true|false), "
    r"local: Some\(LocalContext \{ id: (?P<local_id>\d+), spectating: (?P<spectating>None|Some\(\d+\)), "
    r"noclip: (?P<noclip>true|false) \}\) \}\)"
)


def parse_context(body):
    match = CONTEXT.fullmatch(body)
    if not match:
        raise ValueError("missing or unrecognized FrameContext/window/local metadata")
    f = match.groupdict()
    return {
        "map": f["map"], "state": f["state"], "page": f["page"],
        **{k: boolean(f[k]) for k in ("in_match", "focused", "screenshot", "input_enabled", "noclip")},
        "mixed_context": boolean(f["mixed"]), "local_id": int(f["local_id"]),
        "spectating": f["spectating"], "resolution": [int(f["width"]), int(f["height"])],
        "present_requested": f["present"],
        "frame_latency_hint": None if f["latency"] == "None" else int(re.search(r"\d+", f["latency"])[0]),
    }


def parse_census(kind, body):
    f = fields(body)
    return {key: boolean(f[key]) if key == "competitive" else numeric(f[key], integer=key != "sim_seconds")
            for key in COUNTS[kind]}


def parse_stage(body):
    m = re.fullmatch(rf"frame stage ([\w.]+): calls=(\d+) total=({NUMBER})ms mean=({NUMBER})ms max=({NUMBER})ms", body)
    if not m:
        raise ValueError("unrecognized CPU stage")
    n, total, mean, maximum = int(m[2]), float(m[3]), float(m[4]), float(m[5])
    if not all(math.isfinite(v) for v in (total, mean, maximum)) or n <= 0 or mean > maximum + .0011 or abs(total - n * mean) > (n + 1) * .00051:
        raise ValueError("invalid CPU stage totals/counts")
    return m[1], {"calls": n, "total_ms": total, "mean_ms": mean, "max_ms": maximum}


def parse_gpu(body):
    m = re.fullmatch(rf"render diagnostic (render/\S+): history_mean=({NUMBER})(\S*) samples=(\d+)", body)
    if not m or int(m[4]) <= 0:
        raise ValueError("unrecognized render history diagnostic")
    return m[1], {"history_mean": numeric(m[2]), "unit": m[3], "samples": int(m[4])}


def parse_log(raw):
    intervals, errors = [], []
    current = None
    last_utc = None
    for line_no, raw_line in enumerate(raw.decode("utf-8", errors="replace").splitlines(), 1):
        line = ANSI.sub("", raw_line)
        marker = next((m for m in MARKERS if m in line), None)
        if marker:
            kind, body = MARKERS[marker], line.split(marker, 1)[1].strip()
        elif "frame stage " in line:
            kind, body = "stages", "frame stage " + line.split("frame stage ", 1)[1].strip()
        elif "render diagnostic " in line:
            kind, body = "gpu_history", "render diagnostic " + line.split("render diagnostic ", 1)[1].strip()
        else:
            if "perf " in line:
                errors.append(f"line {line_no}: unrecognized perf record")
            continue
        timestamp = None
        try:
            stamp = STAMP.search(line.split(marker if marker else body.split(":", 1)[0], 1)[0])
            if not stamp:
                raise ValueError("missing UTC log timestamp")
            timestamp = utc(stamp[0])
            if last_utc is not None and timestamp < last_utc:
                errors.append(f"line {line_no}: UTC clock reversed or records reordered")
            last_utc = timestamp
        except ValueError as exc:
            errors.append(f"line {line_no}: {exc}")
        if kind == "interval":
            current = {"line": line_no, "utc": iso(timestamp) if timestamp else None,
                       "stages": {}, "gpu_history": {}, "errors": []}
            intervals.append(current)
        if current is None:
            errors.append(f"line {line_no}: orphan {kind} record before interval")
            continue
        try:
            if kind in ("stages", "gpu_history"):
                name, value = parse_stage(body) if kind == "stages" else parse_gpu(body)
                if name in current[kind]:
                    raise ValueError(f"duplicate {kind} {name}")
                current[kind][name] = value
            else:
                if kind in current:
                    raise ValueError(f"duplicate {kind} record")
                current[kind] = parse_interval(body) if kind == "interval" else parse_context(body) if kind == "context" else parse_census(kind, body)
        except (ValueError, KeyError) as exc:
            current["errors"].append(f"line {line_no}: invalid {kind}: {exc}")
    if b"\xef\xbf\xbd" in raw or "\ufffd" in raw.decode("utf-8", errors="replace"):
        errors.append("UTF-8 decoding replacement found; raw bytes are retained")
    if not intervals:
        errors.append("no perf interval records")
    previous = None
    for row in intervals:
        reasons = row["errors"].copy()
        for key in ("interval", "context", *COUNTS):
            if key not in row:
                reasons.append(f"missing {key} metadata")
        c, i, sim = row.get("context", {}), row.get("interval", {}), row.get("simulation", {})
        if i.get("mixed_context") or c.get("mixed_context"):
            reasons.append("mixed_context=true")
        if i.get("rejected", 0):
            reasons.append("invalid frame samples were rejected")
        for key, expected in (("state", "Match"), ("page", "None"), ("in_match", True),
                              ("focused", True), ("screenshot", False), ("input_enabled", True),
                              ("noclip", False), ("spectating", "None")):
            if key in c and c[key] != expected:
                reasons.append(f"context {key}={c[key]} is not ordinary focused local Match play")
        if c.get("resolution") and min(c["resolution"]) <= 0:
            reasons.append("invalid resolution")
        if i and c and i["mixed_context"] != c["mixed_context"]:
            reasons.append("interval/context mixed flag disagreement")
        if sim:
            if not sim["tick_rate"] or abs(sim["tick"] / max(sim["tick_rate"], 1) - sim["sim_seconds"]) > .002:
                reasons.append("tick/tick_rate disagrees with actual sim_seconds")
            if sim["bots"] > sim["players"] or sim["alive"] > sim["players"]:
                reasons.append("invalid simulation counts")
        row["case_observed"] = {k: c[k] for k in ("map", "resolution", "present_requested", "frame_latency_hint") if k in c}
        row["case_observed"].update({k: sim[k] for k in ("bots", "players", "tick_rate", "competitive") if k in sim})
        if row["utc"] and i:
            row["utc_start"] = iso(utc(row["utc"]) - timedelta(seconds=i["report_wall_s"]))
        if previous:
            p = previous.get("interval", {})
            if p and i:
                real_delta = i["elapsed_real_s"] - p["elapsed_real_s"]
                if real_delta <= 0:
                    errors.append(f"line {row['line']}: real clock reversed/reset or duplicate interval")
                if row["utc"] and previous["utc"]:
                    utc_delta = (utc(row["utc"]) - utc(previous["utc"])).total_seconds()
                    if abs(utc_delta - real_delta) > .05 or abs(utc_delta - i["report_wall_s"]) > .05:
                        errors.append(f"line {row['line']}: UTC/real/report-wall clock alignment drift or missing intervals")
            ps = previous.get("simulation", {})
            if ps and sim:
                row["sim_start_s"] = ps["sim_seconds"]
                if sim["sim_seconds"] <= ps["sim_seconds"]:
                    reasons.append("simulation time did not advance or reset")
            row["boundary_stable"] = not previous["exclusion_reasons"] and previous["case_observed"] == row["case_observed"] and previous.get("context") == row.get("context")
        else:
            row["boundary_stable"] = False
        row["exclusion_reasons"] = reasons
        row["eligible_context"] = not reasons
        previous = row
    return intervals, errors


def regression(xs, ys):
    if len(xs) < 2:
        return {"slope_per_sim_s": None}
    xm, ym = statistics.mean(xs), statistics.mean(ys)
    xx = sum((x - xm) ** 2 for x in xs)
    return {"slope_per_sim_s": sum((x - xm) * (y - ym) for x, y in zip(xs, ys)) / xx if xx else None}


def correlation(xs, ys):
    if len(xs) < 2:
        return None
    xm, ym = statistics.mean(xs), statistics.mean(ys)
    den = math.sqrt(sum((x - xm) ** 2 for x in xs) * sum((y - ym) ** 2 for y in ys))
    return sum((x - xm) * (y - ym) for x, y in zip(xs, ys)) / den if den else None


def summarize(rows):
    if not rows:
        return {"intervals": 0, "frames": 0, "overall_quantiles_ms": None}
    frames = sum(r["interval"]["frames"] for r in rows)
    result = {
        "intervals": len(rows), "frames": frames,
        "utc_start": rows[0].get("utc_start"), "utc_end": rows[-1]["utc"],
        "sim_start_s": rows[0].get("sim_start_s"), "sim_end_s": rows[-1]["simulation"]["sim_seconds"],
        "sampled_real_s": sum(r["interval"]["sampled_real_s"] for r in rows),
        "report_wall_s": sum(r["interval"]["report_wall_s"] for r in rows),
        "weighted_mean_ms": sum(r["interval"]["mean_ms"] * r["interval"]["frames"] for r in rows) / frames,
        "max_ms": max(r["interval"]["max_ms"] for r in rows),
        "overflow_intervals": sum(r["interval"]["overflow"] > 0 for r in rows),
        "overall_quantiles_ms": None,
        "quantile_method": "exact nearest-rank within each non-overflow interval; ranges below are NOT window quantiles",
    }
    qs = [r["interval"]["quantiles_ms"] for r in rows if r["interval"]["quantiles_ms"] is not None]
    result["interval_quantile_ranges_ms"] = {key: [min(q[n] for q in qs), max(q[n] for q in qs)] if qs else None for n, key in enumerate(("p50", "p95", "p99"))}
    result["quantile_intervals"] = len(qs)
    result["sampled_real_mean_ms"] = result["sampled_real_s"] * 1000 / frames
    xs = [r["simulation"]["sim_seconds"] for r in rows]
    means = [r["interval"]["mean_ms"] for r in rows]
    result["mean_ms_drift"] = regression(xs, means)
    counts = {}
    for kind, keys in COUNTS.items():
        for key in keys:
            if key in ("tick", "tick_rate", "sim_seconds", "competitive", "round"):
                continue
            ys = [r[kind][key] for r in rows]
            counts[f"{kind}.{key}"] = {
                "first": ys[0], "last": ys[-1], "min": min(ys), "max": max(ys), "delta": ys[-1] - ys[0],
                "pattern": "plateau" if min(ys) == max(ys) else "nondecreasing" if ys == sorted(ys) else "nonincreasing" if ys == sorted(ys, reverse=True) else "variable",
                **regression(xs, ys), "pearson_with_mean_ms": correlation(ys, means),
            }
    result["endpoint_count_drift"] = counts
    result["cpu_wall_scopes_nonadditive"] = {}
    for name in sorted({name for r in rows for name in r["stages"]}):
        stages = [r["stages"][name] for r in rows if name in r["stages"]]
        calls = sum(s["calls"] for s in stages)
        total = sum(s["total_ms"] for s in stages)
        result["cpu_wall_scopes_nonadditive"][name] = {"reported_intervals": len(stages), "calls": calls, "total_ms": total, "mean_per_call_ms": total / calls, "max_ms": max(s["max_ms"] for s in stages)}
    result["gpu_retained_history_ranges"] = {}
    for name in sorted({name for r in rows for name in r["gpu_history"]}):
        values = [r["gpu_history"][name] for r in rows if name in r["gpu_history"]]
        result["gpu_retained_history_ranges"][name] = {"history_mean_range": [min(v["history_mean"] for v in values), max(v["history_mean"] for v in values)], "units": sorted({v["unit"] for v in values}), "samples_range": [min(v["samples"] for v in values), max(v["samples"] for v in values)], "reported_intervals": len(values)}
    return result


def validate_receipt(receipt, raw_sha):
    reasons = []
    if not isinstance(receipt, dict):
        return ["missing run receipt (build, case and helper phase metadata)"]
    if receipt.get("schema_version") != 1:
        reasons.append("unsupported/missing receipt schema_version")
    if receipt.get("log_sha256") != raw_sha:
        reasons.append("receipt log_sha256 does not match retained raw log")
    build = receipt.get("build", {})
    if not isinstance(build, dict):
        build = {}
    for key, length in (("head", 40), ("sha256", 64)):
        if not isinstance(build.get(key), str) or not re.fullmatch(rf"[0-9a-fA-F]{{{length}}}", build[key]):
            reasons.append(f"missing/invalid build.{key}")
    for key in ("profile", "exe", "source_working_directory"):
        if not isinstance(build.get(key), str) or not build[key].strip():
            reasons.append(f"missing build.{key}")
    for key in ("source_clean", "immutable_executable"):
        if build.get(key) is not True:
            reasons.append(f"build.{key} must be true in the copy/build receipt")
    case = receipt.get("case", {})
    if not isinstance(case, dict):
        case = {}
    for key in CASE_KEYS:
        if key not in case:
            reasons.append(f"missing case.{key}")
    camera = case.get("camera")
    if not isinstance(camera, dict) or not camera:
        reasons.append("case.camera must record the intended matched pose/aim")
    else:
        position = camera.get("position")
        values = position + [camera.get("pitch"), camera.get("yaw")] if isinstance(position, list) and len(position) == 3 else []
        if len(values) != 5 or any(type(v) not in (int, float) or not math.isfinite(v) for v in values):
            reasons.append("case.camera needs finite position[3], pitch and yaw")
    for key in ("bots", "players", "tick_rate"):
        value = case.get(key)
        if type(value) is not int or value < (1 if key != "bots" else 0):
            reasons.append(f"invalid case.{key}")
    resolution = case.get("resolution")
    if not isinstance(resolution, list) or len(resolution) != 2 or any(type(v) is not int or v <= 0 for v in resolution):
        reasons.append("invalid case.resolution")
    if type(case.get("competitive")) is not bool:
        reasons.append("invalid case.competitive")
    if not isinstance(case.get("map"), str) or not case.get("map"):
        reasons.append("invalid case.map")
    if case.get("present_requested") not in ("Immediate", "Mailbox", "Fifo", "FifoRelaxed", "AutoVsync", "AutoNoVsync"):
        reasons.append("invalid case.present_requested")
    latency = case.get("frame_latency_hint")
    if latency is not None and (type(latency) is not int or latency <= 0):
        reasons.append("invalid case.frame_latency_hint")
    if not isinstance(receipt.get("windows"), list) or not receipt["windows"]:
        reasons.append("missing helper windows with actual sim_seconds and UTC bounds")
    return reasons


def analyze(raw, receipt=None, source="<bytes>", receipt_raw=None):
    rows, errors = parse_log(raw)
    sha = digest(raw)
    receipt_reasons = validate_receipt(receipt, sha)
    case = receipt.get("case", {}) if isinstance(receipt, dict) else {}
    if not isinstance(case, dict):
        case = {}
    for row in rows:
        mismatches = [f"receipt case.{key} does not match actual log" for key, value in row["case_observed"].items() if key in case and case[key] != value]
        row["exclusion_reasons"].extend(mismatches)
        row["eligible_context"] = not row["exclusion_reasons"]
    windows = []
    definitions = receipt.get("windows", []) if isinstance(receipt, dict) else []
    if not isinstance(definitions, list):
        definitions = []
    seen = set()
    for definition in definitions:
        problems, selected = [], []
        try:
            if not isinstance(definition, dict):
                raise ValueError("window must be an object")
            name = definition["name"]
            if not isinstance(name, str) or not name or name in seen:
                raise ValueError("missing/duplicate window name")
            seen.add(name)
            if definition["phase"] != "stable_match":
                raise ValueError("helper phase is not stable_match (loading is never early gameplay)")
            start, end = utc(definition["utc_start"]), utc(definition["utc_end"])
            ss, se = definition["sim_start_s"], definition["sim_end_s"]
            if not all(isinstance(v, (int, float)) and not isinstance(v, bool) and math.isfinite(v) for v in (ss, se)) or not 0 <= ss < se or start >= end:
                raise ValueError("invalid helper sim/UTC bounds")
            bounded = [r for r in rows if r.get("utc_start") and utc(r["utc_start"]) >= start and utc(r["utc"]) <= end]
            for row in bounded:
                if not row["eligible_context"] or not row["boundary_stable"]:
                    problems.append(f"line {row['line']}: excluded/unstable interval inside helper UTC window")
                elif row.get("sim_start_s", -1) < ss or row["simulation"]["sim_seconds"] > se:
                    problems.append(f"line {row['line']}: helper UTC and actual sim bounds disagree")
                else:
                    selected.append(row)
            if len(selected) < 2:
                problems.append("fewer than two complete stable intervals in both helper time bounds")
            if selected and len({json.dumps(r["case_observed"], sort_keys=True) for r in selected}) != 1:
                problems.append("case changed within helper window")
            # Disjoint fragments must never silently become a stable window.
            for a, b in zip(selected, selected[1:]):
                if rows.index(b) != rows.index(a) + 1:
                    problems.append("noncontiguous stable intervals")
            if selected:
                if (utc(selected[0]["utc_start"]) - start).total_seconds() > selected[0]["interval"]["report_wall_s"] + .05 or (end - utc(selected[-1]["utc"])).total_seconds() > selected[-1]["interval"]["report_wall_s"] + .05:
                    problems.append("helper window has an uncovered UTC edge larger than one interval")
                if selected[0]["sim_start_s"] - ss > selected[0]["simulation"]["sim_seconds"] - selected[0]["sim_start_s"] + .002 or se - selected[-1]["simulation"]["sim_seconds"] > selected[-1]["simulation"]["sim_seconds"] - selected[-1]["sim_start_s"] + .002:
                    problems.append("helper window has an uncovered simulation edge larger than one interval")
        except (KeyError, TypeError, ValueError) as exc:
            problems.append(f"invalid helper window: {exc}")
        windows.append({"definition": definition, "valid_descriptive_window": not problems and not errors and not receipt_reasons,
                        "exclusion_reasons": problems + errors + receipt_reasons, "summary": summarize(selected),
                        "interval_lines": [r["line"] for r in selected]})
    early_late = {}
    valid = {w["definition"]["name"]: w for w in windows if w["valid_descriptive_window"]}
    if "early" in valid and "late" in valid:
        a, b = valid["early"], valid["late"]
        if a["definition"]["sim_end_s"] > b["definition"]["sim_start_s"] or utc(a["definition"]["utc_end"]) > utc(b["definition"]["utc_start"]):
            errors.append("early/late windows overlap or are reversed")
        else:
            x, y = a["summary"], b["summary"]
            early_late = {"mean_ms_delta": y["weighted_mean_ms"] - x["weighted_mean_ms"], "max_ms_delta": y["max_ms"] - x["max_ms"], "count_endpoint_delta": {k: y["endpoint_count_drift"][k]["last"] - v["last"] for k, v in x["endpoint_count_drift"].items()}, "interpretation": "Descriptive co-occurrence/drift only; no causal attribution or performance acceptance."}
    eligible = [r for r in rows if r["eligible_context"]]
    if errors:
        for window in windows:
            window["valid_descriptive_window"] = False
            window["exclusion_reasons"] = list(dict.fromkeys(window["exclusion_reasons"] + errors))
    return {"schema_version": 1, "source": {"path": source, "sha256": sha, "bytes": len(raw), "receipt_sha256": digest(receipt_raw) if receipt_raw is not None else None},
            "receipt": receipt, "log_errors": errors, "receipt_errors": receipt_reasons,
            "limitations": LIMITS, "paired_admission": False,
            "admission_missing": LIMITS[:2], "intervals": rows, "windows": windows, "early_late": early_late,
            "eligible_interval_count": len(eligible), "excluded_interval_count": len(rows) - len(eligible),
            "window_quantiles_available": False}


def compare(baseline, candidate):
    reasons = []
    for label, report in (("baseline", baseline), ("candidate", candidate)):
        reasons += [f"{label}: {e}" for e in report["log_errors"] + report["receipt_errors"]]
        reasons += [f"{label}: invalid helper window" for w in report["windows"] if not w["valid_descriptive_window"]]
    a, b = baseline.get("receipt") or {}, candidate.get("receipt") or {}
    if a.get("case") != b.get("case"):
        reasons.append("paired case/camera metadata differs")
    ab, bb = a.get("build"), b.get("build")
    if not isinstance(ab, dict) or not isinstance(bb, dict) or ab.get("profile") != bb.get("profile"):
        reasons.append("paired immutable build profiles differ")
    windows = {}
    aw = {w["definition"].get("name"): w for w in baseline["windows"] if w["valid_descriptive_window"]}
    bw = {w["definition"].get("name"): w for w in candidate["windows"] if w["valid_descriptive_window"]}
    if set(aw) != set(bw) or not aw:
        reasons.append("paired valid helper windows are missing or differ")
    if not {"early", "late"} <= set(aw) & set(bw):
        reasons.append("paired early and late helper windows are required")
    for name in sorted(set(aw) & set(bw)):
        x, y = aw[name], bw[name]
        if any(x["definition"][k] != y["definition"][k] for k in ("phase", "sim_start_s", "sim_end_s")):
            reasons.append(f"paired {name} sim-time bounds differ")
        windows[name] = {"baseline": x["summary"], "candidate": y["summary"], "weighted_mean_ms_delta": y["summary"]["weighted_mean_ms"] - x["summary"]["weighted_mean_ms"], "max_ms_delta": y["summary"]["max_ms"] - x["summary"]["max_ms"]}
    return {"paired_admission": False, "metadata_aligned_descriptive_only": not reasons,
            "rejection_reasons": reasons, "admission_missing": LIMITS[:2], "windows": windows,
            "interpretation": "Descriptive deltas only. Current logs cannot admit a matched-camera/local-alive performance claim or latency claim."}


def markdown(report):
    lines = ["# CSR performance log analysis", "", "Paired performance admission: **NOT ADMITTED**.", "", f"Raw SHA-256: `{report['source']['sha256']}` ({report['source']['bytes']} bytes).", "",
             f"Intervals: {report['eligible_interval_count']} context-eligible, {report['excluded_interval_count']} excluded.", ""]
    for key in ("log_errors", "receipt_errors", "limitations"):
        lines += [f"- {reason}" for reason in report[key]]
    lines += ["", "Window statistics use all-frame count-weighted means and maxima. Per-interval p50/p95/p99 are exact nearest-rank values when there is no overflow. Their ranges are **not overall window quantiles**; overall p95 is unavailable.", ""]
    for window in report["windows"]:
        s = window["summary"]
        lines += [f"## Window {window['definition'].get('name', '<invalid>') if isinstance(window['definition'], dict) else '<invalid>'}", "", f"Valid descriptive window: {window['valid_descriptive_window']}. Helper bounds: `{json.dumps(window['definition'], sort_keys=True)}`.", ""]
        lines += [f"- {reason}" for reason in window["exclusion_reasons"]]
        if not s["intervals"]:
            continue
        lines += ["", f"{s['intervals']} complete intervals; {s['frames']} frames; weighted mean {s['weighted_mean_ms']:.4f} ms; max {s['max_ms']:.4f} ms.",
                  f"Sampled real time {s['sampled_real_s']:.6f} s; report wall time {s['report_wall_s']:.3f} s. Actual sim bounds {s['sim_start_s']} to {s['sim_end_s']} s; UTC {s['utc_start']} to {s['utc_end']}.",
                  f"Per-interval quantile ranges (ms): `{json.dumps(s['interval_quantile_ranges_ms'], sort_keys=True)}`; {s['quantile_intervals']} intervals with complete samples, {s['overflow_intervals']} overflow intervals.", "",
                  "CPU wall scopes are nested/overlapping, include waits and must not be added across names. Values below are per-scope observations.", "", "| CPU wall scope | Calls | Total ms | Mean/call ms | Max ms | Reported intervals |", "|---|---:|---:|---:|---:|---:|"]
        for name, v in s["cpu_wall_scopes_nonadditive"].items():
            lines.append(f"| {name} | {v['calls']} | {v['total_ms']:.3f} | {v['mean_per_call_ms']:.3f} | {v['max_ms']:.3f} | {v['reported_intervals']} |")
        lines += ["", "GPU retained-history ranges (not interval means/totals):", ""]
        lines += [f"- `{name}`: `{json.dumps(v, sort_keys=True)}`" for name, v in s["gpu_retained_history_ranges"].items()]
        lines += ["", "Endpoint census drift/correlation (co-occurrence only, no cause or leak claim):", "", "| Count | First | Last | Min | Max | Pattern | Slope/sim s | r with mean ms |", "|---|---:|---:|---:|---:|---|---:|---:|"]
        for name, v in s["endpoint_count_drift"].items():
            lines.append(f"| {name} | {v['first']} | {v['last']} | {v['min']} | {v['max']} | {v['pattern']} | {v.get('slope_per_sim_s')} | {v['pearson_with_mean_ms']} |")
    if report["early_late"]:
        lines += ["", "Early/late descriptive drift:", "", f"`{json.dumps(report['early_late'], sort_keys=True)}`"]
    lines += ["", "## Excluded intervals", ""]
    for row in report["intervals"]:
        if row["exclusion_reasons"]:
            lines.append(f"- Line {row['line']}, UTC {row['utc']}: {'; '.join(row['exclusion_reasons'])}.")
    return "\n".join(lines) + "\n"


def read_receipt(path):
    if not path:
        return None, None
    raw = Path(path).read_bytes()
    def invalid_constant(value):
        raise ValueError(f"non-finite JSON constant in receipt: {value}")
    value = json.loads(raw, parse_constant=invalid_constant)
    if not isinstance(value, dict):
        raise ValueError("receipt root must be an object")
    return value, raw


def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("log", type=Path)
    p.add_argument("--receipt", type=Path)
    p.add_argument("--out", type=Path, required=True, help="new output directory (never overwrite evidence)")
    p.add_argument("--compare-log", type=Path)
    p.add_argument("--compare-receipt", type=Path)
    args = p.parse_args(argv)
    if args.compare_receipt and not args.compare_log:
        p.error("--compare-receipt requires --compare-log")
    try:
        receipt, receipt_raw = read_receipt(args.receipt)
        raw = args.log.read_bytes()
        report = analyze(raw, receipt, str(args.log.resolve()), receipt_raw)
        other_raw = other_receipt_raw = other = None
        if args.compare_log:
            other_receipt, other_receipt_raw = read_receipt(args.compare_receipt)
            other_raw = args.compare_log.read_bytes()
            other = analyze(other_raw, other_receipt, str(args.compare_log.resolve()), other_receipt_raw)
        # Parse all inputs before creating output. Existing output fails closed.
        args.out.mkdir(parents=True, exist_ok=False)
        (args.out / "raw.log").write_bytes(raw)
        if receipt_raw is not None:
            (args.out / "receipt.json").write_bytes(receipt_raw)
        (args.out / "report.json").write_text(json.dumps(report, indent=2, allow_nan=False) + "\n", encoding="utf-8")
        (args.out / "report.md").write_text(markdown(report), encoding="utf-8")
        if other is not None:
            (args.out / "candidate.raw.log").write_bytes(other_raw)
            if other_receipt_raw is not None:
                (args.out / "candidate.receipt.json").write_bytes(other_receipt_raw)
            (args.out / "candidate.report.json").write_text(json.dumps(other, indent=2, allow_nan=False) + "\n", encoding="utf-8")
            (args.out / "candidate.report.md").write_text(markdown(other), encoding="utf-8")
            paired = compare(report, other)
            (args.out / "comparison.json").write_text(json.dumps(paired, indent=2, allow_nan=False) + "\n", encoding="utf-8")
            (args.out / "comparison.md").write_text("# Descriptive paired comparison\n\nPaired performance admission: **NOT ADMITTED**.\n\n" + paired["interpretation"] + "\n\n" + "\n".join(f"- {v}" for v in paired["rejection_reasons"] + paired["admission_missing"]) + "\n\n" + "\n".join(f"- {name}: weighted mean delta {v['weighted_mean_ms_delta']:.4f} ms; max delta {v['max_ms_delta']:.4f} ms." for name, v in paired["windows"].items()) + "\n", encoding="utf-8")
        print(f"Wrote offline evidence to {args.out}; paired admission NOT ADMITTED")
        return 2 if report["log_errors"] or report["receipt_errors"] or any(not w["valid_descriptive_window"] for w in report["windows"]) or (other is not None and not paired["metadata_aligned_descriptive_only"]) else 0
    except (OSError, ValueError, TypeError) as exc:
        p.exit(1, f"perf_report: {exc}\n")


if __name__ == "__main__":
    raise SystemExit(main())
