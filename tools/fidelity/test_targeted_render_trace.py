#!/usr/bin/env python3
"""Authored UNRUN R20 standard-library fixtures. No game, processes or files.
Future parent command: python -B -m unittest discover -s tools/fidelity -p test_targeted_render_trace.py
"""
import contextlib
import importlib.util
import io
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("trace_summary", Path(__file__).with_name("summarize-targeted-render-trace.py"))
trace = importlib.util.module_from_spec(spec)
spec.loader.exec_module(trace)

# Independent literal contract: intentionally not generated from parser constants.
HEADER = "# csr_targeted_render_trace_v2 run=r20-fixture feature=targeted-render-trace bevy=0.19.1 clock=monotonic_cpu_wall_ns start_s=30 end_s=60 wall_ns=90000000000 max_records=100 max_bytes=8192"
MAP = [
    "# system 0 bevy_pbr::render::skin::extract_skins",
    "# system 1 bevy_pbr::render::skin::prepare_skins",
    "# system 2 bevy_render::erased_render_asset::prepare_erased_assets<bevy_pbr::mesh_material::MeshMaterial3d<client::material::WorldMaterial>>",
    "# system 3 bevy_pbr::render::mesh::prepare_mesh_bind_groups",
    "# system 4 bevy_render::renderer::render_system",
]
COLUMNS = "kind\tframe\tsim_tick\tsim_s\tstart_ns\tduration_ns\tsystem\tspan\tthread\tcontext_changed"


def point(kind, frame, tick, sim, at):
    return [kind, str(frame), str(tick), sim, str(at), "", "", "", "", ""]


def invocation(frame, tick, sim, system, start, duration):
    return ["sample", str(frame), str(tick), sim, str(start), str(duration), str(system),
            str(system + 1), f"ThreadId({system + 1})", "false"]


def rows():
    result = [point("lower_previous", 99, 3838, "29.990000000", 1)]
    for frame, tick, sim, at, kind in [(100, 3840, "30.000000000", 10, "lower"),
                                       (101, 7678, "59.990000000", 50, "context")]:
        result.append(point(kind, frame, tick, sim, at))
        # Different threads/functions overlap. System 1 finishes before system 0,
        # even though 0 starts earlier. Persistent span IDs are reused next frame.
        result.append(invocation(frame, tick, sim, 1, at + 11, 9))
        result.append(invocation(frame, tick, sim, 0, at + 10, 20))
        for system in (2, 3, 4):
            result.append(invocation(frame, tick, sim, system, at + 10 + system, 20))
    result.append(point("upper", 102, 7680, "60.000000000", 100))
    return result


def artifact(body=None, header=HEADER, mapping=None, columns=COLUMNS, reason="window_end", mask=31, scopes=0):
    body = rows() if body is None else body
    lines = [header, *(MAP if mapping is None else mapping), columns,
             *("\t".join(row) for row in body),
             f"# end reason={reason} records={len(body)} seen_mask={mask} open_scopes={scopes}"]
    return ("\n".join(lines) + "\n").encode("utf-8")


class TraceStructure(unittest.TestCase):
    def invalid(self, raw):
        with self.assertRaises(ValueError):
            trace.analyze(raw)

    def test_complete_producer_shape_concurrency_and_reused_spans(self):
        result = trace.analyze(artifact())
        self.assertTrue(result["structural_complete"])
        self.assertFalse(result["evidence_complete"])
        self.assertEqual(result["covered"], [2] * 5)
        self.assertEqual(result["gaps"], 0)
        self.assertEqual(result["lower_previous"], (99, 3838, 29.99, 1))
        self.assertEqual(result["upper"], (102, 7680, 60.0, 100))
        self.assertEqual(result["samples"][0], [20 / 1e6] * 2)

    def test_r19_late_599_first_context_and_missing_lower_receipts(self):
        late = [point("lower", 100, 7667, "59.900000000", 10)]
        late += [invocation(100, 7667, "59.900000000", i, 20 + i, 10) for i in range(5)]
        late += [point("upper", 101, 7680, "60.000000000", 100)]
        self.invalid(artifact(late))
        self.invalid(artifact(rows()[1:]))
        self.invalid(artifact([row for row in rows() if row[0] != "lower"]))

    def test_missing_upper_or_later_function_does_not_complete(self):
        self.assertFalse(trace.analyze(artifact(rows()[:-1]))["structural_complete"])
        body = [row for row in rows() if not (row[0] == "sample" and row[1] == "101" and row[6] == "4")]
        result = trace.analyze(artifact(body))
        self.assertFalse(result["structural_complete"])
        self.assertEqual(result["covered"], [2, 2, 2, 2, 1])
        self.assertEqual(len(result["contexts"]), 2)  # explicit missing 1/2 denominator

    def test_skipped_context_and_boundary_gap_are_incomplete(self):
        body = [row for row in rows() if row[1] != "101"]
        result = trace.analyze(artifact(body))
        self.assertFalse(result["structural_complete"])
        self.assertEqual(result["gaps"], 1)
        body = rows()
        body[0][1] = "98"
        self.assertFalse(trace.analyze(artifact(body))["structural_complete"])

    def test_r19_missing_map_metadata_wrong_clock_or_monomorph(self):
        self.invalid(artifact(mapping=[]))
        for field in ("feature=targeted-render-trace", "bevy=0.19.1", "clock=monotonic_cpu_wall_ns"):
            self.invalid(artifact(header=HEADER.replace(" " + field, "")))
        for old, new in [("clock=monotonic_cpu_wall_ns", "clock=microseconds"),
                         ("bevy=0.19.1", "bevy=0.19.0"),
                         ("feature=targeted-render-trace", "feature=default")]:
            self.invalid(artifact(header=HEADER.replace(old, new)))
        wrong = MAP.copy()
        wrong[2] = wrong[2].replace("client::material::WorldMaterial", "other::WorldMaterial")
        self.invalid(artifact(mapping=wrong))
        self.invalid(artifact(mapping=MAP[::-1]))

    def test_duplicate_unknown_keys_and_exact_schema(self):
        for suffix in (" run=other", " clock=monotonic_cpu_wall_ns", " unknown=x"):
            self.invalid(artifact(header=HEADER + suffix))
        self.invalid(artifact().replace(b"seen_mask=31", b"seen_mask=31 seen_mask=31"))
        self.invalid(artifact(columns=COLUMNS.replace("\tspan\tthread", ""),
                              body=[row[:7] + row[9:] for row in rows()]))
        self.invalid(artifact(columns=COLUMNS.replace("\tthread", "\tspan")))
        self.invalid(artifact(columns=COLUMNS.replace("\tspan\tthread", "\tthread\tspan")))

    def test_r19_pre_context_sample_and_backwards_contexts(self):
        body = rows()
        body[1][4] = "100"
        body[2][4:6] = ["1", "1"]
        self.invalid(artifact(body))
        for column, value in [(1, "99"), (2, "1"), (3, "29.5"), (4, "9")]:
            body = rows()
            body[7][column] = value  # second active context
            self.invalid(artifact(body))
        body = rows()
        body[-1][4] = "60"  # upper before last completed sample
        self.invalid(artifact(body))

    def test_identity_domains_sample_context_and_chronology(self):
        for column, value in [(1, "-1"), (1, "4294967296"), (2, "-1"), (3, "nan"),
                              (4, "-1"), (5, "18446744073709551616"), (6, "5"),
                              (7, "0"), (7, "-1"), (8, "ThreadId(0)"), (8, "1"),
                              (8, "ThreadId(18446744073709551616)"), (9, "true")]:
            body = rows()
            body[2][column] = value
            self.invalid(artifact(body))
        body = rows()
        body[2][1] = "101"
        self.invalid(artifact(body))
        body = rows()
        body[3][7] = "2"  # span 2 was already mapped to system 1
        self.invalid(artifact(body))
        body = rows()
        body[2], body[3] = body[3], body[2]  # backwards exit order
        self.invalid(artifact(body))
        body = rows()
        body.insert(3, invocation(100, 3840, "30.000000000", 1, 25, 10))
        self.invalid(artifact(body))  # same system overlap despite ordered exits

    def test_hard_limits_truncation_footer_and_context_schema(self):
        for old, new in [("start_s=30", "start_s=nan"), ("end_s=60", "end_s=151"),
                         ("wall_ns=90000000000", "wall_ns=300000000001"),
                         ("max_records=100", "max_records=100001"),
                         ("max_bytes=8192", "max_bytes=4095"), ("run=r20-fixture", "run=bad/run")]:
            self.invalid(artifact(header=HEADER.replace(old, new)))
        raw = artifact()
        self.invalid(raw[:-1])
        self.invalid(raw[:raw.rfind(b"# end")])
        self.invalid(raw + b"# end reason=window_end records=14 seen_mask=31 open_scopes=0\n")
        self.invalid(raw + b"sample\n")
        self.invalid(raw.replace(b"records=14", b"records=13"))
        self.invalid(raw.replace(b"seen_mask=31", b"seen_mask=15"))
        self.invalid(raw.replace(b"reason=window_end", b"reason=invented"))
        body = rows()
        body[1][7] = "1"
        self.invalid(artifact(body))
        for reason in ("app_exit", "wall_limit", "record_cap", "byte_cap", "missing_function"):
            self.assertFalse(trace.analyze(artifact(reason=reason))["structural_complete"])
        self.assertFalse(trace.analyze(artifact(scopes=1))["structural_complete"])

    def test_no_receipt_join_or_evidence_success_path_exists(self):
        # Missing identity receipts cannot accidentally obtain normal/evidence success.
        with self.assertRaisesRegex(ValueError, "no parent receipt validator"):
            trace.summarize("never-opened.tsv")
        # Supplying arbitrary/mismatched receipt/expected-run arguments is rejected,
        # rather than pretending to validate them. Only structural mode is supported.
        with contextlib.redirect_stderr(io.StringIO()):
            for arguments in (["never-opened.tsv"],
                              ["--structural-only", "never-opened.tsv", "--receipt", "mismatched.json"],
                              ["--structural-only", "never-opened.tsv", "--expected-run", "other"]):
                with self.assertRaises(SystemExit) as error:
                    trace.main(arguments)
                self.assertEqual(error.exception.code, 2)
        self.assertIs(trace.analyze(artifact())["evidence_complete"], False)


if __name__ == "__main__":
    unittest.main()
