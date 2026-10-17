#!/usr/bin/env python3
"""Bounded v2 trace STRUCTURE ONLY. Authored UNRUN in R20.
No executable/build/context receipt reader exists here; evidence_complete is
always False, even on exit 0. Parent evidence review remains mandatory.
"""
import argparse
import math
import re
import statistics
import sys
from pathlib import Path

MAX_BYTES = 16 * 1024 * 1024
MAX_RECORDS = 100_000
U64 = (1 << 64) - 1
SYSTEMS = (
    "bevy_pbr::render::skin::extract_skins",
    "bevy_pbr::render::skin::prepare_skins",
    "bevy_render::erased_render_asset::prepare_erased_assets<bevy_pbr::mesh_material::MeshMaterial3d<client::material::WorldMaterial>>",
    "bevy_pbr::render::mesh::prepare_mesh_bind_groups",
    "bevy_render::renderer::render_system",
)
LABELS = ("extract_skins", "prepare_skins", "WorldMaterial prepare_erased_assets",
          "prepare_mesh_bind_groups", "render_system (combined)")
COLUMNS = "kind frame sim_tick sim_s start_ns duration_ns system span thread context_changed".split()
REASONS = set("window_end record_cap byte_cap wall_limit context_lost invalid_sim_time context_rewind missed_start context_gap context_changed missing_function overlapping_same_system scope_mismatch missing_render_app app_exit".split())


def require(ok, message):
    if not ok:
        raise ValueError(message)


def uint(value, maximum=U64, minimum=0):
    require(re.fullmatch(r"0|[1-9][0-9]{0,19}", value) is not None, "invalid integer domain")
    result = int(value)
    require(minimum <= result <= maximum, "integer outside domain")
    return result


def finite(value):
    require(len(value) <= 64 and re.fullmatch(r"-?[0-9]+(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?", value) is not None,
            "invalid numeric domain")
    result = float(value)
    require(math.isfinite(result) and result >= 0, "invalid finite nonnegative number")
    return result


def fields(line, prefix, keys):
    require(line.startswith(prefix), "missing metadata/footer")
    result = {}
    for item in line[len(prefix):].split(" "):
        key, separator, value = item.partition("=")
        require(separator and key in keys and key not in result and value, "unknown/duplicate metadata key")
        result[key] = value
    require(set(result) == set(keys), "missing metadata keys")
    return result


def analyze(raw):
    require(len(raw) <= MAX_BYTES, "artifact exceeds hard byte cap")
    text = raw.decode("utf-8")
    require(text.endswith("\n") and "\r" not in text, "incomplete/noncanonical artifact")
    lines = text.splitlines()
    require(8 <= len(lines) <= MAX_RECORDS + 8, "line bound")
    meta = fields(lines[0], "# csr_targeted_render_trace_v2 ",
                  "run feature bevy clock start_s end_s wall_ns max_records max_bytes".split())
    require(re.fullmatch(r"[A-Za-z0-9_.-]{1,64}", meta["run"]) is not None, "invalid run identity")
    require((meta["feature"], meta["bevy"], meta["clock"]) ==
            ("targeted-render-trace", "0.19.1", "monotonic_cpu_wall_ns"), "unsupported feature/bevy/clock")
    require(lines[1:6] == [f"# system {i} {name}" for i, name in enumerate(SYSTEMS)], "exact ordered system map required")
    require(lines[6].split("\t") == COLUMNS, "exact unique ten-column schema required")
    footer = fields(lines[-1], "# end ", "reason records seen_mask open_scopes".split())
    require(footer["reason"] in REASONS, "unknown terminal reason")
    start, end = finite(meta["start_s"]), finite(meta["end_s"])
    require(0 < start < end and end - start <= 120, "invalid hard window bounds (positive start requires a predecessor)")
    wall = uint(meta["wall_ns"], 300_000_000_000, 1)
    cap = uint(meta["max_records"], MAX_RECORDS, 1)
    byte_cap = uint(meta["max_bytes"], MAX_BYTES, 4096)
    require(len(raw) <= byte_cap, "declared byte cap exceeded")
    rows = lines[7:-1]
    require(len(rows) == uint(footer["records"], cap), "record count/cap mismatch")
    mask, scopes = uint(footer["seen_mask"], 31), uint(footer["open_scopes"], 5)
    contexts, samples = [], [[] for _ in SYSTEMS]
    counts = [0] * 5
    covered = [0] * 5
    spans = {}
    system_end = [0] * 5
    previous = active = lower_previous = lower = upper = None
    completion = 0
    gaps = 0
    seen = 0
    for line in rows:
        values = line.split("\t")
        require(len(values) == len(COLUMNS), "partial/wide row")
        kind, frame_s, tick_s, sim_text, at_s, duration_s, system_s, span_s, thread, changed = values
        frame, tick, sim, at = uint(frame_s, (1 << 32) - 1), uint(tick_s), finite(sim_text), uint(at_s, wall - 1)
        identity = (frame, tick, sim)
        if kind == "sample":
            require(active is not None and identity == active[:3], "sample lacks active context identity")
            require(changed == "false", "changed context")
            system, duration = uint(system_s, 4), uint(duration_s)
            span = uint(span_s, minimum=1)
            match = re.fullmatch(r"ThreadId\(([1-9][0-9]{0,19})\)", thread)
            require(match is not None, "invalid process-local thread identity")
            uint(match[1], minimum=1)
            finish = at + duration
            require(active[3] <= at <= finish < wall, "sample precedes context or exceeds deadline")
            require(finish >= completion, "backwards completion chronology")
            require(at >= system_end[system], "same-system overlap")
            require(span not in spans or spans[span] == system, "span mapped to different systems")
            spans[span] = system
            system_end[system] = finish
            completion = finish
            samples[system].append(duration / 1e6)
            if counts[system] == 0:
                covered[system] += 1
            counts[system] += 1
            seen |= 1 << system
            continue
        require(kind in ("lower_previous", "lower", "context", "upper"), "unknown row kind/comment")
        require(all(value == "" for value in values[5:]), "nonempty context-only fields")
        require(at >= completion, "context precedes completed invocation")
        if previous is not None:
            require(frame > previous[0] and tick >= previous[1] and sim >= previous[2]
                    and at >= previous[3], "backwards/duplicate context")
            gaps += frame - previous[0] - 1
        point = (*identity, at)
        if kind == "lower_previous":
            require(previous is None and lower_previous is None and sim < start, "invalid lower predecessor")
            lower_previous = point
        elif kind == "lower":
            require(lower is None and previous == lower_previous and lower_previous is not None
                    and start <= sim < end, "missing/late lower crossing receipt")
            lower = point
            active = point
            contexts.append(point)
            counts = [0] * 5
        elif kind == "context":
            require(active is not None and upper is None and start <= sim < end, "context outside active interval")
            active = point
            contexts.append(point)
            counts = [0] * 5
        else:
            require(lower is not None and upper is None and active is not None and sim >= end, "invalid upper receipt")
            upper = point
            active = None
        require(upper is None or kind == "upper", "rows after upper receipt")
        previous = point
        completion = at
    require(mask == seen, "seen mask does not match rows")
    structural_complete = (footer["reason"] == "window_end" and scopes == 0
                           and lower_previous is not None and lower is not None and upper is not None
                           and gaps == 0 and bool(contexts) and all(n == len(contexts) for n in covered))
    return dict(run=meta["run"], start=start, end=end, reason=footer["reason"],
                structural_complete=structural_complete, evidence_complete=False,
                contexts=contexts, gaps=gaps, covered=covered, samples=samples,
                lower_previous=lower_previous, lower=lower, upper=upper)


def summarize(path, *, structural_only=False):
    require(structural_only, "no parent receipt validator: explicit --structural-only required; evidence completeness unavailable")
    with Path(path).open("rb") as stream:
        result = analyze(stream.read(MAX_BYTES + 1))
    print(f"STRUCTURE ONLY run={result['run']} sim=[{result['start']},{result['end']}) "
          f"reason={result['reason']} structural_complete={result['structural_complete']} evidence_complete=False")
    print(f"observed_contexts={len(result['contexts'])} missing_contexts={result['gaps']} "
          f"observed_range={result['contexts'][0] if result['contexts'] else None}.."
          f"{result['contexts'][-1] if result['contexts'] else None}")
    print(f"lower_previous={result['lower_previous']} lower={result['lower']} upper={result['upper']}")
    print("Inclusive function-system wall milliseconds; concurrent scopes must never be summed as frame/GPU time.")
    for label, values, covered in zip(LABELS, result["samples"], result["covered"]):
        denominator = len(result["contexts"])
        population = f"covered_contexts={covered}/{denominator} missing_observed_contexts={denominator-covered}"
        if not values:
            print(f"{label}: n=0 {population}")
            continue
        values.sort()
        print(f"{label}: n={len(values)} {population} mean={statistics.mean(values):.4f} "
              f"median={statistics.median(values):.4f} p95={values[math.ceil(len(values)*.95)-1]:.4f} max={values[-1]:.4f}")
    print("Parent build/exe/cwd/wrapper/map/pose receipts are NOT joined or validated; no evidence-qualified success.")
    return 0 if result["structural_complete"] else 2


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--structural-only", action="store_true", required=True,
                        help="required acknowledgement: no parent receipts or evidence completeness")
    parser.add_argument("trace")
    args = parser.parse_args(argv)
    try:
        return summarize(args.trace, structural_only=args.structural_only)
    except (ValueError, OSError, UnicodeError) as error:
        print(f"invalid targeted trace: {error}; evidence_complete=False", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
