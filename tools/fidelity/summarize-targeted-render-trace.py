#!/usr/bin/env python3
"""Offline CPU wall summary for the bounded R18 TSV. No game/process access.
Authored UNRUN in R18. A complete file is diagnostic evidence, not admission.
"""
import csv
import io
import math
import re
import statistics
import sys
from pathlib import Path

MAX_BYTES = 16 * 1024 * 1024
LABELS = ("extract_skins", "prepare_skins", "WorldMaterial prepare_erased_assets",
          "prepare_mesh_bind_groups", "render_system (combined)")


def summarize(path):
    with Path(path).open("rb") as stream:
        raw = stream.read(MAX_BYTES + 1)
    if len(raw) > MAX_BYTES:
        raise ValueError("artifact exceeds maximum byte cap")
    text = raw.decode("utf-8")
    lines = text.splitlines()
    if not text.endswith("\n") or not lines or not lines[0].startswith("# csr_targeted_render_trace_v1 "):
        raise ValueError("incomplete or unknown artifact")
    metadata = dict(re.findall(r"(\w+)=([^ ]+)", lines[0]))
    ends = [line for line in lines if line.startswith("# end ")]
    if len(ends) != 1 or ends[0] != lines[-1]:
        raise ValueError("missing/duplicate/nonterminal footer (hard stop or write failure)")
    footer = dict(re.findall(r"(\w+)=([^ ]+)", ends[0]))
    start, end = float(metadata["start_s"]), float(metadata["end_s"])
    wall = int(metadata["wall_ns"])
    if not (math.isfinite(start) and math.isfinite(end) and 0 <= start < end):
        raise ValueError("invalid capture window")
    if len(raw) > int(metadata["max_bytes"]):
        raise ValueError("declared byte cap exceeded")
    body = "\n".join(line for line in lines if not line.startswith("#"))
    reader = csv.DictReader(io.StringIO(body), delimiter="\t")
    contexts, samples, counts = {}, [[] for _ in LABELS], 0
    for row in reader:
        counts += 1
        if None in row or any(value is None for value in row.values()):
            raise ValueError("partial row")
        frame, tick, sim_s = int(row["frame"]), int(row["sim_tick"]), float(row["sim_s"])
        at = int(row["start_ns"])
        if not (start <= sim_s < end and 0 <= at < wall):
            raise ValueError("record outside declared window")
        if row["kind"] == "context":
            if frame in contexts:
                raise ValueError("duplicate frame context")
            contexts[frame] = (tick, sim_s)
        elif row["kind"] == "sample":
            system, duration = int(row["system"]), int(row["duration_ns"])
            if not (0 <= system < len(LABELS) and duration >= 0 and at + duration < wall):
                raise ValueError("invalid sample")
            if contexts.get(frame) != (tick, sim_s) or row["context_changed"] != "false":
                raise ValueError("sample context mismatch")
            samples[system].append(duration / 1e6)
        else:
            raise ValueError("unknown record kind")
    if counts != int(footer["records"]) or counts > int(metadata["max_records"]):
        raise ValueError("record count/cap mismatch")
    complete = (footer["reason"] == "window_end" and footer["open_scopes"] == "0"
                and footer["seen_mask"] == "31" and all(samples))
    print(f"run={metadata['run']} sim=[{start},{end}) frames={len(contexts)} reason={footer['reason']} complete={complete}")
    print("CPU wall milliseconds per function-system invocation; inclusive, possibly concurrent; never sum as frame/GPU time.")
    for label, values in zip(LABELS, samples):
        if not values:
            print(f"{label}: MISSING")
            continue
        values.sort()
        p95 = values[math.ceil(len(values) * .95) - 1]
        print(f"{label}: n={len(values)} mean={statistics.mean(values):.4f} median={statistics.median(values):.4f} p95={p95:.4f} max={values[-1]:.4f}")
    return 0 if complete else 2


if __name__ == "__main__":
    try:
        if len(sys.argv) != 2:
            raise ValueError("usage: summarize-targeted-render-trace.py PRIVATE_TRACE.tsv")
        sys.exit(summarize(sys.argv[1]))
    except (ValueError, KeyError, OSError, UnicodeError) as error:
        print(f"invalid targeted trace: {error}", file=sys.stderr)
        sys.exit(2)
