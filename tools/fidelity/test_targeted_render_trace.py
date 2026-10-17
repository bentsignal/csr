#!/usr/bin/env python3
"""Standard-library source fixtures; R26 additions UNRUN. No game, processes or files.
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

# Independent v3 contract: no parser/producer constants or runtime output used.
PHASE_HEADER = "# csr_targeted_render_trace_v3 run=r26-fixture feature=targeted-render-trace bevy=0.19.1 clock=monotonic_cpu_wall_ns scope_set=render-phases start_s=30 end_s=60 wall_ns=90000000000 max_records=100 max_bytes=8192"
PHASE_MAP = [
    "# scope 0 kind=system name=bevy_render::view::window::prepare_windows target=bevy_ecs::system::function_system level=INFO",
    "# scope 1 kind=system name=bevy_pbr::material::queue_material_meshes target=bevy_ecs::system::function_system level=INFO",
    "# scope 2 kind=system name=bevy_render::view::prepare_view_targets target=bevy_ecs::system::function_system level=INFO",
    "# scope 3 kind=span name=queue_submit target=bevy_core_pipeline::schedule level=INFO",
    "# scope 4 kind=span name=present_frames target=bevy_render::renderer level=INFO",
]
PHASE_COLUMNS = "kind\tframe\tsim_tick\tsim_s\tstart_ns\tduration_ns\tscope\tspan\tthread\tcontext_changed"
PHASE_BODY = (
    "lower_previous\t99\t3838\t29.99\t1\t\t\t\t\t\n"
    "lower\t100\t3840\t30\t10\t\t\t\t\t\n"
    "sample\t100\t3840\t30\t24\t6\t4\t5\tThreadId(1)\tfalse\n"
    "sample\t100\t3840\t30\t23\t8\t3\t4\tThreadId(2)\tfalse\n"
    "sample\t100\t3840\t30\t22\t10\t2\t3\tThreadId(1)\tfalse\n"
    "sample\t100\t3840\t30\t21\t12\t1\t2\tThreadId(2)\tfalse\n"
    "sample\t100\t3840\t30\t20\t14\t0\t1\tThreadId(1)\tfalse\n"
    "context\t101\t7678\t59.99\t50\t\t\t\t\t\n"
    "sample\t101\t7678\t59.99\t64\t6\t4\t5\tThreadId(1)\tfalse\n"
    "sample\t101\t7678\t59.99\t63\t8\t3\t4\tThreadId(2)\tfalse\n"
    "sample\t101\t7678\t59.99\t62\t10\t2\t3\tThreadId(1)\tfalse\n"
    "sample\t101\t7678\t59.99\t61\t12\t1\t2\tThreadId(2)\tfalse\n"
    "sample\t101\t7678\t59.99\t60\t14\t0\t1\tThreadId(1)\tfalse\n"
    "upper\t102\t7680\t60\t100\t\t\t\t\t\n"
)


def phase_artifact(body=None, **changes):
    # Reuse only the wire-row assembler. Schema/map/body above are independent
    # literals; they neither consult nor derive from trace.PHASE_* constants.
    options = dict(header=PHASE_HEADER, mapping=PHASE_MAP, columns=PHASE_COLUMNS)
    options.update(changes)
    body = [line.split("\t") for line in PHASE_BODY.splitlines()] if body is None else body
    return artifact(body, **options)


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

    def test_r21_fractional_20_tick_producer_display_contract(self):
        # Independent literal rows, not calculated by the parser or by a float
        # formatter here. Match the Rust collector fixture's World::time f32/20
        # values converted to f64 and serialized with Rust Display, including
        # all five samples in each context and reversed completion order.
        header = "# csr_targeted_render_trace_v2 run=r23-fractional feature=targeted-render-trace bevy=0.19.1 clock=monotonic_cpu_wall_ns start_s=0.0500000009 end_s=0.2 wall_ns=90000000000 max_records=100 max_bytes=8192"
        literal = (
            "lower_previous\t1\t1\t0.05000000074505806\t1\t\t\t\t\t\n"
            "lower\t2\t2\t0.10000000149011612\t10\t\t\t\t\t\n"
            "sample\t2\t2\t0.10000000149011612\t24\t6\t4\t5\tThreadId(1)\tfalse\n"
            "sample\t2\t2\t0.10000000149011612\t23\t8\t3\t4\tThreadId(1)\tfalse\n"
            "sample\t2\t2\t0.10000000149011612\t22\t10\t2\t3\tThreadId(1)\tfalse\n"
            "sample\t2\t2\t0.10000000149011612\t21\t12\t1\t2\tThreadId(1)\tfalse\n"
            "sample\t2\t2\t0.10000000149011612\t20\t14\t0\t1\tThreadId(1)\tfalse\n"
            "context\t3\t3\t0.15000000596046448\t50\t\t\t\t\t\n"
            "sample\t3\t3\t0.15000000596046448\t64\t6\t4\t5\tThreadId(1)\tfalse\n"
            "sample\t3\t3\t0.15000000596046448\t63\t8\t3\t4\tThreadId(1)\tfalse\n"
            "sample\t3\t3\t0.15000000596046448\t62\t10\t2\t3\tThreadId(1)\tfalse\n"
            "sample\t3\t3\t0.15000000596046448\t61\t12\t1\t2\tThreadId(1)\tfalse\n"
            "sample\t3\t3\t0.15000000596046448\t60\t14\t0\t1\tThreadId(1)\tfalse\n"
            "upper\t4\t4\t0.20000000298023224\t100\t\t\t\t\t\n"
        )
        body = [line.split("\t") for line in literal.splitlines()]
        result = trace.analyze(artifact(body, header=header))
        self.assertTrue(result["structural_complete"])
        self.assertFalse(result["evidence_complete"])
        self.assertEqual(result["covered"], [2] * 5)
        self.assertEqual(result["gaps"], 0)
        self.assertEqual(result["lower_previous"], (1, 1, 0.05000000074505806, 1))
        self.assertEqual(result["lower"], (2, 2, 0.10000000149011612, 10))
        self.assertEqual(result["upper"], (4, 4, 0.20000000298023224, 100))
        self.assertLess(result["lower_previous"][2], result["start"])
        # Old fixed-nine predecessor crosses START and must still be rejected;
        # there is no epsilon or relaxed comparison to hide the producer bug.
        body[0][3] = "0.050000001"
        self.invalid(artifact(body, header=header))

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
        for old, new in [("start_s=30", "start_s=nan"), ("start_s=30", "start_s=0"),
                         ("start_s=30", "start_s=-0"), ("end_s=60", "end_s=151"),
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

    def test_v3_literal_contract_all_five_population_concurrency_and_reused_spans(self):
        result = trace.analyze(phase_artifact())
        self.assertTrue(result["structural_complete"])
        self.assertFalse(result["evidence_complete"])
        self.assertEqual(result["scope_set"], "render-phases")
        self.assertEqual(result["labels"], ("prepare_windows (system)", "queue_material_meshes (system)",
                                           "prepare_view_targets (system)", "queue_submit (span)", "present_frames (span)"))
        self.assertEqual(result["covered"], [2] * 5)
        self.assertEqual(result["samples"], [[14 / 1e6] * 2, [12 / 1e6] * 2,
                                             [10 / 1e6] * 2, [8 / 1e6] * 2, [6 / 1e6] * 2])
        self.assertEqual(result["gaps"], 0)
        self.assertEqual(result["upper"], (102, 7680, 60.0, 100))
        # Existing v2 retains its own schema and population, without scope_set.
        self.assertIsNone(trace.analyze(artifact())["scope_set"])

    def test_v3_omitted_direct_span_is_incomplete_even_when_seen_earlier(self):
        for scope in ("3", "4"):
            body = [line.split("\t") for line in PHASE_BODY.splitlines()]
            body = [row for row in body if not (row[0] == "sample" and row[1] == "101" and row[6] == scope)]
            result = trace.analyze(phase_artifact(body))
            self.assertFalse(result["structural_complete"])
            self.assertFalse(result["evidence_complete"])
            expected = [2] * 5
            expected[int(scope)] = 1
            self.assertEqual(result["covered"], expected)

    def test_v3_exact_typed_mapping_unknown_profiles_and_mixed_schema_rejected(self):
        for old, new in [("scope_set=render-phases", "scope_set=legacy"),
                         ("scope_set=render-phases", "scope_set=render-phases-extra"),
                         ("_v3 ", "_v4 "), ("_v3 ", "_v2 ")]:
            self.invalid(phase_artifact(header=PHASE_HEADER.replace(old, new)))
        self.invalid(phase_artifact(header=PHASE_HEADER.replace(" scope_set=render-phases", "")))
        self.invalid(phase_artifact(header=PHASE_HEADER + " scope_set=render-phases"))
        self.invalid(artifact(header=HEADER + " scope_set=render-phases"))
        self.invalid(phase_artifact(mapping=MAP))
        self.invalid(artifact(mapping=PHASE_MAP))
        self.invalid(phase_artifact(columns=COLUMNS))
        self.invalid(artifact(columns=PHASE_COLUMNS))
        self.invalid(phase_artifact(mapping=PHASE_MAP[::-1]))
        self.invalid(phase_artifact(mapping=PHASE_MAP[:4]))
        self.invalid(phase_artifact(mapping=PHASE_MAP + [PHASE_MAP[4]]))
        for index, old, new in [(0, "kind=system", "kind=span"),
                                (1, "queue_material_meshes", "queue_material_meshes<WorldMaterial>"),
                                (2, "prepare_view_targets", "prepare_view_targets_extra"),
                                (3, "kind=span", "kind=system"),
                                (3, "target=bevy_core_pipeline::schedule", "target=other"),
                                (3, "name=queue_submit", "name=queue_submit_extra"),
                                (3, "level=INFO", "level=DEBUG"),
                                (4, "target=bevy_render::renderer", "target=other"),
                                (4, "name=present_frames", "name=present_frames_extra"),
                                (4, "level=INFO", "level=WARN")]:
            mapping = PHASE_MAP.copy()
            mapping[index] = mapping[index].replace(old, new)
            self.invalid(phase_artifact(mapping=mapping))

    def test_v3_preserves_domains_chronology_bounds_and_structural_only_limits(self):
        for column, value in [(1, "4294967296"), (2, "-1"), (3, "nan"), (4, "-1"),
                              (5, "18446744073709551616"), (6, "5"), (7, "0"),
                              (8, "ThreadId(0)"), (9, "true")]:
            body = [line.split("\t") for line in PHASE_BODY.splitlines()]
            body[2][column] = value
            self.invalid(phase_artifact(body))
        self.invalid(phase_artifact().replace(b"start_s=30", b"start_s=0"))
        self.invalid(phase_artifact().replace(b"max_records=100", b"max_records=100001"))
        self.invalid(phase_artifact().replace(b"max_bytes=8192", b"max_bytes=4095"))
        self.invalid(phase_artifact().replace(b"wall_ns=90000000000", b"wall_ns=300000000001"))
        self.invalid(phase_artifact().replace(b"clock=monotonic_cpu_wall_ns", b"clock=gpu"))
        self.invalid(phase_artifact()[:-1])
        body = [line.split("\t") for line in PHASE_BODY.splitlines()]
        body[2], body[3] = body[3], body[2]
        self.invalid(phase_artifact(body))  # reversed completion chronology
        body = [line.split("\t") for line in PHASE_BODY.splitlines()]
        body[1][4] = "100"
        self.invalid(phase_artifact(body))  # samples precede active context
        body = [line.split("\t") for line in PHASE_BODY.splitlines()]
        body[3][7] = "5"
        self.invalid(phase_artifact(body))  # direct span identity remapped
        for reason in ("app_exit", "wall_limit", "record_cap", "byte_cap", "missing_function"):
            self.assertFalse(trace.analyze(phase_artifact(reason=reason))["structural_complete"])
        self.assertFalse(trace.analyze(phase_artifact(scopes=1))["structural_complete"])
        self.assertFalse(trace.analyze(phase_artifact(body=rows()[:-1]))["structural_complete"])


if __name__ == "__main__":
    unittest.main()
