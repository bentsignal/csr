"""Synthetic clean-room fixtures for old 0e9/new cc66e5e contracts; no game data."""
import copy
from datetime import datetime, timedelta, timezone
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from perf_metadata_fixtures import ACTOR, RECT, endpoint_lines, upgrade


SPEC = importlib.util.spec_from_file_location("perf_report", Path(__file__).resolve().parents[1] / "perf_report.py")
perf = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(perf)
EPOCH = datetime(2026, 10, 16, 23, tzinfo=timezone.utc)


def stamp(seconds):
    return perf.iso(EPOCH + timedelta(seconds=seconds))


def block(second, *, mean=10, frames=100, samples=None, overflow=0, mixed=False,
          state="Match", page="None", focused=True, screenshot=False, enabled=True,
          noclip=False, spectating="None", sim=None, entities=100, bodies=2, meshes=30):
    """Debug field spelling/order mirrors frame_report.rs and input_latency.rs."""
    sim = second if sim is None else sim
    maximum = mean * 2
    quantiles = "None" if overflow else f"Some([{mean}, {mean * 1.5}, {maximum}])"
    if samples is not None:
        ordered = sorted(samples)
        frames = len(ordered)
        mean = sum(ordered) / frames
        maximum = max(ordered)
        quantiles = f"Some({[ordered[math_ceil(frames * p / 100) - 1] for p in (50, 95, 99)]})"
    b = lambda value: str(value).lower()
    lines = [
        f"perf interval: elapsed_real_s={second:.3f} report_wall_s=1.000 sampled_real_s={frames * mean / 1000:.6f} frames={frames} mean_ms={mean:.4f} p50_p95_p99_ms={quantiles} max_ms={maximum:.4f} overflow={overflow} rejected=0 mixed_context={b(mixed)} context_changes={int(mixed)}",
        f"perf context: endpoint_only=true map=de_nuke mixed_context={b(mixed)} observed=Some(FrameContext {{ state: {state}, page: {page}, in_match: true, window: Some(WindowContext {{ size: (1920, 1080), focused: {b(focused)}, present_requested: Immediate, frame_latency_hint: Some(1) }}), screenshot: {b(screenshot)}, input_enabled: {b(enabled)}, local: Some(LocalContext {{ id: 1, spectating: {spectating}, noclip: {b(noclip)} }}) }})",
        f"perf simulation census: tick={round(sim * 128)} tick_rate=128 sim_seconds={sim:.3f} competitive=false round=1 players=3 bots=2 alive=3 items=4 projectiles=0 fields=0",
        f"perf scene census: entities={entities} mesh_entities=50 hidden_local=4 view_visible_any=40 no_frustum=5 skinned_meshes=4 ui_nodes=5 audio_players=2 impact_fx=0 cameras=2 active_cameras=2",
        f"perf asset census: meshes={meshes} images=20 world_materials=5 standard_materials=6 particle_materials=2",
        f"perf character census: instances=3 bodies={bodies} pending=0 physical=1",
        "frame stage main: calls=100 total=600.000ms mean=6.000ms max=8.000ms",
        "frame stage render: calls=100 total=400.000ms mean=4.000ms max=6.000ms",
        "frame stage render.prepare: calls=100 total=200.000ms mean=2.000ms max=3.000ms",
        "frame stage render.submit: calls=100 total=100.000ms mean=1.000ms max=2.000ms",
        "render diagnostic render/primary/opaque/gpu: history_mean=0.125ms samples=20",
        "render diagnostic render/primary/opaque/cpu: history_mean=0.050ms samples=20",
    ]
    return "\n".join(f"\x1b[2m{stamp(second)}\x1b[0m \x1b[32m INFO\x1b[0m client::input_latency: {line}" for line in lines) + "\n"


def math_ceil(value):
    return int(-(-value // 1))


def fixture(**kwargs):
    return "".join(block(s, **kwargs) for s in range(1, 7)).encode()


def receipt(raw):
    return {
        "schema_version": 1, "log_sha256": hashlib.sha256(raw).hexdigest(),
        "build": {"head": "a" * 40, "sha256": "b" * 64, "profile": "release", "exe": "D:/private/immutable/csr.exe", "source_working_directory": "D:/private/source", "source_clean": True, "immutable_executable": True},
        "case": {"map": "de_nuke", "bots": 2, "players": 3, "tick_rate": 128, "resolution": [1920, 1080], "present_requested": "Immediate", "frame_latency_hint": 1, "competitive": False, "camera": {"position": [1, 2, 3], "pitch": 0, "yaw": 90}},
        "windows": [
            {"name": "early", "phase": "stable_match", "utc_start": stamp(1), "utc_end": stamp(3), "sim_start_s": 1, "sim_end_s": 3},
            {"name": "late", "phase": "stable_match", "utc_start": stamp(4), "utc_end": stamp(6), "sim_start_s": 4, "sim_end_s": 6},
        ],
    }


def analyze(raw=None, rec=None):
    raw = fixture() if raw is None else raw
    return perf.analyze(raw, receipt(raw) if rec is None else rec)


class EndpointFormatTests(unittest.TestCase):
    def test_old_dust_display_title_retained_and_exact_supported_alias_normalized(self):
        raw = upgrade(fixture().decode()).replace('map=de_nuke ', 'map=Dust II (2017) ').encode()
        rec = receipt(raw)
        rec['case']['map'] = 'de_dust2_2017'
        rec['case']['resolution'] = [2560, 1440]
        result = perf.analyze(raw, rec)
        self.assertEqual(result['log_errors'], [])
        self.assertEqual(result['eligible_interval_count'], 6)
        self.assertEqual(result['intervals'][0]['context']['map_logged'], 'Dust II (2017)')
        self.assertEqual(result['intervals'][0]['context']['map'], 'de_dust2_2017')
        self.assertFalse(result['paired_admission'])

    def test_display_title_alias_does_not_match_other_versions_or_forged_fields(self):
        body = next(x.split('perf context: ', 1)[1] for x in block(1).splitlines() if 'perf context: ' in x)
        body = perf.ANSI.sub('', body)
        other = perf.parse_context(body.replace('map=de_nuke ', 'map=Dust II (2019) '))
        self.assertEqual(other['map'], 'Dust II (2019)')
        for label in ['Dust II (2017) mixed_context=false', 'Dust II\n(2017)', 'Dust II=2017']:
            with self.subTest(label=label), self.assertRaises(ValueError):
                perf.parse_context(body.replace('map=de_nuke ', f'map={label} '))

    def new_report(self, text=None, resolution=(2560, 1440)):
        raw = (upgrade(fixture().decode()) if text is None else text).encode()
        rec = receipt(raw)
        rec['case']['resolution'] = list(resolution)
        rec['requested_cli_resolution'] = [1920, 1080]
        return perf.analyze(raw, rec)

    def test_new_actor_camera_format_retains_exact_endpoint_data_without_admission(self):
        report = self.new_report()
        self.assertEqual(report['log_errors'], [])
        self.assertEqual(report['eligible_interval_count'], 6)
        self.assertTrue(all(w['valid_descriptive_window'] for w in report['windows']))
        row = report['intervals'][0]
        self.assertEqual(row['context']['authoritative_local'], dict(
            id=1, life_epoch=2, alive=True, team='CounterTerrorist', active_slot=0, weapon_def_index=7))
        self.assertEqual(set(row['player_endpoints']), {'local', 'view_target'})
        self.assertEqual(row['player_endpoints']['local']['origin_source_inches'], [-128, 64.5, 32])
        camera = row['world_camera_endpoint']
        self.assertEqual(camera['raw_debug_fields']['physical_viewport_rect'], RECT)
        self.assertEqual(row['endpoint_metadata']['actual_camera_viewport_pixels'],
                         dict(min=[0, 0], max=[2560, 1440], size=[2560, 1440]))
        self.assertEqual(camera['fov_vertical_horizontal_degrees'], [73.7398, 106.2602])
        self.assertEqual(camera['missing_fields'], [])
        self.assertIn('viewport', camera['none_fields'])  # None is an explicit default, not an omitted key
        self.assertEqual(row['endpoint_metadata']['missing_fields'], [])
        self.assertTrue(row['endpoint_metadata']['local_alive_at_context_endpoint'])
        self.assertFalse(row['endpoint_metadata']['whole_interval_camera_pose_proven'])
        self.assertFalse(report['paired_admission'])
        self.assertFalse(perf.compare(report, copy.deepcopy(report))['paired_admission'])

    def test_observed_1440_and_cli_1080_are_separate_receipt_contracts(self):
        report = self.new_report()
        resolution = report['resolution_metadata']
        self.assertEqual(resolution['requested_cli_resolution'], [1920, 1080])
        self.assertEqual(resolution['observed_window_physical_resolutions'], [[2560, 1440]])
        self.assertEqual(resolution['camera_physical_viewport_rects_debug'], [RECT])
        bad = self.new_report(resolution=(1920, 1080))
        self.assertEqual(bad['eligible_interval_count'], 0)
        self.assertIn('case.resolution', str(bad['intervals'][0]['exclusion_reasons']))
        self.assertIn('actual physical resolution', perf.markdown(report))
        self.assertIsNone(analyze()['resolution_metadata']['requested_cli_resolution'])

    def test_new_context_without_endpoint_lines_keeps_missing_observations_explicit(self):
        report = self.new_report(upgrade(fixture().decode(), endpoints=False))
        row = report['intervals'][0]
        self.assertTrue(row['context']['actor_metadata_logged'])
        self.assertTrue(row['endpoint_metadata']['local_alive_at_context_endpoint'])
        self.assertEqual(row['endpoint_metadata']['missing_fields'], [
            'player_endpoint.local', 'player_endpoint.view_target', 'view_intent_endpoint', 'world_camera_endpoint'])
        self.assertFalse(report['paired_admission'])

    def test_missing_actor_context_and_unavailable_camera_are_not_invented(self):
        text = upgrade(fixture().decode(), actor='None')
        text = '\n'.join(line.split('perf world camera endpoint:')[0] +
                         'perf world camera endpoint: endpoint_only=true source=main_world_last '
                         'interval_camera_stability=unproven candidates=0 selection=unavailable'
                         if 'perf world camera endpoint:' in line else line for line in text.splitlines())
        report = self.new_report(text)
        row = report['intervals'][0]
        self.assertEqual(report['log_errors'], [])
        self.assertEqual(report['eligible_interval_count'], 0)
        self.assertIn('context.authoritative_local', row['endpoint_metadata']['missing_fields'])
        self.assertIn('world_camera_endpoint.physical_viewport_rect', row['endpoint_metadata']['missing_fields'])
        self.assertIsNone(row['endpoint_metadata']['local_alive_at_context_endpoint'])

    def test_mixed_actor_lifecycle_and_weapon_changes_exclude_complete_intervals(self):
        for old, new in [('life_epoch: 2', 'life_epoch: 3'), ('alive: true', 'alive: false'),
                         ('team: CounterTerrorist', 'team: Terrorist'), ('active_slot: 0', 'active_slot: 1'),
                         ('weapon_def_index: Some(7)', 'weapon_def_index: Some(16)')]:
            with self.subTest(change=new):
                text = ''.join(upgrade(block(n, mixed=n == 2)).replace(old, new) if n == 2
                               else upgrade(block(n)) for n in range(1, 7))
                report = self.new_report(text)
                self.assertFalse(report['intervals'][1]['eligible_context'])
                self.assertFalse(report['windows'][0]['valid_descriptive_window'])

    def test_context_actor_boundary_changes_and_format_changes_have_no_stable_boundary(self):
        for text in (upgrade(block(1)) + upgrade(block(2)).replace('life_epoch: 2', 'life_epoch: 3'),
                     block(1).replace('size: (1920, 1080)', 'size: (2560, 1440)') + upgrade(block(2))):
            report = self.new_report(text)
            self.assertFalse(report['intervals'][1]['boundary_stable'])

    def test_missing_partial_endpoint_fields_are_explicit(self):
        report = self.new_report(upgrade(fixture().decode()).replace(
            ' origin_source_inches=Some([-128.0, 64.5, 32.0])', ''))
        self.assertIn('player_endpoint.local.origin_source_inches',
                      report['intervals'][0]['endpoint_metadata']['missing_fields'])
        self.assertFalse(report['paired_admission'])

    def test_bad_endpoint_values_roles_duplicates_and_context_disagreement_exclude(self):
        text = upgrade(fixture().decode())
        for old, new in [('role_valid=true', 'role_valid=false'), ('candidates=1', 'candidates=2'),
                         ('is_active=true', 'is_active=false'), ('requested_id=1', 'requested_id=99'),
                         ('global_matrix_bevy_cols=[1.0', 'global_matrix_bevy_cols=[NaN'),
                         ('render_eye_source_inches=[-128.0', 'render_eye_source_inches=[Infinity'),
                         ('render_eye_source_inches=[-128.0', 'render_eye_source_inches=[' + '9' * 400),
                         ('render_eye_source_inches=[-128.0, 64.5, 96.0]', 'render_eye_source_inches=[1, 2]'),
                         ('role=local', 'role=bot'), ('endpoint_only=true source=', 'endpoint_only=false source='),
                         ('candidates=1', 'candidates=1 candidates=1')]:
            with self.subTest(change=new):
                self.assertEqual(self.new_report(text.replace(old, new))['eligible_interval_count'], 0)
        # Context is unchanged while a single reported authoritative player disagrees.
        changed = text.replace('requested_id=1 authoritative=' + ACTOR,
                               'requested_id=1 authoritative=' + ACTOR.replace('life_epoch: 2', 'life_epoch: 3'))
        self.assertEqual(self.new_report(changed)['eligible_interval_count'], 0)
        lines = text.splitlines(keepends=True)
        lines.insert(4, lines[4])
        self.assertTrue(any(r['errors'] for r in self.new_report(''.join(lines))['intervals']))

    def test_physical_camera_viewport_is_independent_of_window_and_checks_bounds(self):
        text = upgrade(fixture().decode()).replace(RECT,
            'Some(URect { min: UVec2(10, 20), max: UVec2(1930, 1100) })')
        row = self.new_report(text)['intervals'][0]
        self.assertEqual(row['context']['resolution'], [2560, 1440])
        self.assertEqual(row['endpoint_metadata']['actual_camera_viewport_pixels'],
                         dict(min=[10, 20], max=[1930, 1100], size=[1920, 1080]))
        self.assertIsNone(analyze()['intervals'][0]['endpoint_metadata']['actual_camera_viewport_pixels'])
        for value in ('Some(URect { min: UVec2(2560, 0), max: UVec2(2560, 1440) })',
                      'Some(URect { min: UVec2(0, 0), max: UVec2(4294967296, 1440) })', 'bad'):
            self.assertEqual(self.new_report(upgrade(fixture().decode()).replace(RECT, value))['eligible_interval_count'], 0)

    def test_resolution_change_inside_window_is_excluded_even_without_mixed_flag(self):
        text = ''.join(upgrade(block(n), resolution=(1920, 1080) if n == 2 else (2560, 1440))
                       for n in range(1, 7))
        report = self.new_report(text)
        self.assertFalse(report['windows'][0]['valid_descriptive_window'])
        self.assertFalse(report['intervals'][1]['eligible_context'])
        self.assertFalse(report['intervals'][2]['boundary_stable'])

    def test_truncated_debug_missing_context_field_and_absent_local_resource_fail_closed(self):
        text = upgrade(fixture().decode())
        for old, new in [('sub_camera_view=None', 'sub_camera_view=Some('),
                         (', authoritative_view_target: ' + ACTOR, ''),
                         (endpoint_lines()[0], 'perf player endpoint: endpoint_only=true local_resource=false')]:
            with self.subTest(change=new):
                report = self.new_report(text.replace(old, new))
                self.assertEqual(report['eligible_interval_count'], 0)
                self.assertFalse(report['paired_admission'])

    def test_moving_camera_endpoints_never_prove_interval_pose(self):
        text = ''.join(upgrade(block(n)).replace('Vec3(-3.2512', f'Vec3({n}.0') for n in range(1, 7))
        report = self.new_report(text)
        self.assertTrue(all(w['valid_descriptive_window'] for w in report['windows']))
        self.assertTrue(all(not r['endpoint_metadata']['whole_interval_camera_pose_proven'] for r in report['intervals']))
        self.assertFalse(report['paired_admission'])


class ActualFormatTests(unittest.TestCase):
    def test_actual_debug_contract_ansi_utc_census_stages_history(self):
        report = analyze()
        self.assertEqual(report["log_errors"], [])
        self.assertEqual(report["receipt_errors"], [])
        self.assertEqual(report["source"]["sha256"], hashlib.sha256(fixture()).hexdigest())
        self.assertTrue(all(w["valid_descriptive_window"] for w in report["windows"]))
        self.assertEqual(report["windows"][0]["interval_lines"], [13, 25])
        summary = report["windows"][0]["summary"]
        self.assertEqual(summary["frames"], 200)
        self.assertEqual(summary["weighted_mean_ms"], 10)
        self.assertEqual(summary["max_ms"], 20)
        self.assertEqual(summary["gpu_retained_history_ranges"]["render/primary/opaque/gpu"]["samples_range"], [20, 20])
        self.assertIsNone(summary["overall_quantiles_ms"])
        self.assertFalse(report["paired_admission"])
        self.assertIn("unproven", " ".join(report["admission_missing"]))
        self.assertIn("context.authoritative_local", report["intervals"][0]["endpoint_metadata"]["missing_fields"])

    def test_count_weighted_mean_and_sampled_seconds_not_report_wall(self):
        raw = (block(1) + block(2, mean=4, frames=250) + block(3, mean=20, frames=100)).encode()
        report = analyze(raw)
        s = report["windows"][0]["summary"]
        self.assertAlmostEqual(s["weighted_mean_ms"], 3000 / 350)
        self.assertEqual(s["sampled_real_s"], 3)
        self.assertEqual(s["report_wall_s"], 2)
        self.assertEqual(s["max_ms"], 40)
        self.assertEqual(s["interval_quantile_ranges_ms"]["p95"], [6, 30])
        self.assertIsNone(s["overall_quantiles_ms"])

    def test_exact_nearest_rank_small_sample_fixture_no_averaged_window_p95(self):
        raw = (block(1) + block(2, samples=[1] * 19 + [100]) + block(3, samples=[7, 8])).encode()
        report = analyze(raw)
        self.assertEqual(report["intervals"][1]["interval"]["quantiles_ms"], [1, 1, 100])
        self.assertEqual(report["intervals"][2]["interval"]["quantiles_ms"], [7, 8, 8])
        s = report["windows"][0]["summary"]
        self.assertEqual(s["interval_quantile_ranges_ms"]["p95"], [1, 8])
        self.assertAlmostEqual(s["weighted_mean_ms"], (119 + 15) / 22)
        self.assertIsNone(s["overall_quantiles_ms"])

    def test_overflow_retains_all_frame_mean_max_but_no_quantiles(self):
        raw = fixture(frames=8193, overflow=1)
        r = analyze(raw)
        s = r["windows"][0]["summary"]
        self.assertTrue(r["windows"][0]["valid_descriptive_window"])
        self.assertEqual(s["frames"], 16386)
        self.assertEqual(s["overflow_intervals"], 2)
        self.assertEqual(s["quantile_intervals"], 0)
        self.assertIsNone(s["interval_quantile_ranges_ms"]["p95"])

    def test_prefix_quantiles_on_overflow_are_rejected(self):
        raw = fixture(frames=8193, overflow=1).replace(b"p50_p95_p99_ms=None", b"p50_p95_p99_ms=Some([10, 15, 20])")
        r = analyze(raw)
        self.assertEqual(r["eligible_interval_count"], 0)
        self.assertIn("truncated", str(r["intervals"][0]["exclusion_reasons"]))

    def test_invalid_sample_math_and_rejected_samples(self):
        for old, new in [(b"frames=100", b"frames=99"), (b"mean_ms=10.0000", b"mean_ms=NaN"), (b"rejected=0", b"rejected=1"), (b"p50_p95_p99_ms=Some([10, 15.0, 20])", b"p50_p95_p99_ms=None")]:
            with self.subTest(new=new):
                r = analyze(fixture().replace(old, new))
                self.assertEqual(r["eligible_interval_count"], 0)

    def test_cpu_scopes_kept_separate_no_additive_total(self):
        s = analyze()["windows"][0]["summary"]
        self.assertEqual(s["cpu_wall_scopes_nonadditive"]["render"]["total_ms"], 800)
        self.assertEqual(s["cpu_wall_scopes_nonadditive"]["render.prepare"]["total_ms"], 400)
        self.assertNotIn("cpu_total_ms", s)
        self.assertIn("must not be added", perf.markdown(analyze()))


class AdmissionTests(unittest.TestCase):
    def test_all_disallowed_contexts(self):
        for kwargs in [dict(mixed=True), dict(state="Loading"), dict(state="MainMenu"), dict(page="Some(Pause)"), dict(focused=False), dict(screenshot=True), dict(enabled=False), dict(noclip=True), dict(spectating="Some(2)")]:
            with self.subTest(kwargs=kwargs):
                r = analyze(fixture(**kwargs))
                self.assertEqual(r["eligible_interval_count"], 0)
                self.assertFalse(any(w["valid_descriptive_window"] for w in r["windows"]))

    def test_missing_context_window_local_and_each_census(self):
        raw = fixture()
        for marker in [b"perf context:", b"perf simulation census:", b"perf scene census:", b"perf asset census:", b"perf character census:"]:
            with self.subTest(marker=marker):
                r = analyze(b"\n".join(line for line in raw.splitlines() if marker not in line))
                self.assertEqual(r["eligible_interval_count"], 0)
        for old, new in [(b"window: Some(WindowContext", b"window: None(WindowContext"), (b"local: Some(LocalContext", b"local: None(LocalContext")]:
            self.assertEqual(analyze(raw.replace(old, new))["eligible_interval_count"], 0)

    def test_no_receipt_produces_intervals_but_no_invented_early_gameplay(self):
        r = perf.analyze(fixture())
        self.assertEqual(r["eligible_interval_count"], 6)
        self.assertEqual(r["windows"], [])
        self.assertTrue(r["receipt_errors"])

    def test_actual_case_must_match_receipt(self):
        for key, value in [("map", "de_dust2"), ("bots", 4), ("players", 5), ("tick_rate", 64), ("resolution", [1280, 720]), ("present_requested", "Fifo"), ("frame_latency_hint", 2), ("competitive", True)]:
            with self.subTest(key=key):
                raw = fixture()
                rec = receipt(raw)
                rec["case"][key] = value
                r = analyze(raw, rec)
                self.assertEqual(r["eligible_interval_count"], 0)
                self.assertFalse(any(w["valid_descriptive_window"] for w in r["windows"]))

    def test_required_case_and_immutable_build_receipts(self):
        for group, key in [("build", k) for k in ("head", "sha256", "profile", "exe", "source_working_directory", "source_clean", "immutable_executable")] + [("case", k) for k in perf.CASE_KEYS]:
            with self.subTest(group=group, key=key):
                raw = fixture()
                rec = receipt(raw)
                del rec[group][key]
                r = analyze(raw, rec)
                self.assertTrue(r["receipt_errors"])
                self.assertFalse(any(w["valid_descriptive_window"] for w in r["windows"]))

    def test_log_hash_binding_and_malformed_receipts(self):
        raw = fixture()
        rec = receipt(raw)
        rec["log_sha256"] = "c" * 64
        self.assertIn("log_sha256", str(analyze(raw, rec)["receipt_errors"]))
        for rec in [{}, {"case": [], "build": []}, {"windows": [None, "early", {}]}]:
            with self.subTest(rec=rec):
                r = perf.analyze(raw, rec)
                self.assertFalse(r["paired_admission"])
                self.assertTrue(r["receipt_errors"])
                json.dumps(r, allow_nan=False)

    def test_loading_is_not_early_match_even_if_endpoint_later_is_match(self):
        raw = (block(1, state="Loading") + block(2, mixed=True) + block(3) + block(4) + block(5) + block(6)).encode()
        r = analyze(raw)
        self.assertFalse(r["windows"][0]["valid_descriptive_window"])
        self.assertTrue(r["windows"][1]["valid_descriptive_window"])
        rec = receipt(fixture())
        rec["windows"][0]["phase"] = "loading"
        self.assertFalse(analyze(fixture(), rec)["windows"][0]["valid_descriptive_window"])

    def test_helper_requires_both_actual_sim_and_utc_alignment(self):
        raw = fixture()
        for key, value in [("sim_start_s", 100), ("sim_end_s", 200), ("utc_start", stamp(100)), ("utc_end", stamp(200))]:
            with self.subTest(key=key):
                rec = receipt(raw)
                rec["windows"][0][key] = value
                self.assertFalse(analyze(raw, rec)["windows"][0]["valid_descriptive_window"])

    def test_paused_reset_and_tick_rate_sim_clocks(self):
        for raw in [(block(1) + block(2, sim=1) + block(3, sim=0)).encode(), fixture().replace(b"tick_rate=128", b"tick_rate=64")]:
            r = analyze(raw)
            self.assertFalse(r["windows"][0]["valid_descriptive_window"])
            self.assertTrue(any(row["exclusion_reasons"] for row in r["intervals"]))

    def test_camera_alive_missing_even_when_pair_receipts_align(self):
        r = analyze()
        pair = perf.compare(r, copy.deepcopy(r))
        self.assertTrue(pair["metadata_aligned_descriptive_only"])
        self.assertFalse(pair["paired_admission"])
        self.assertEqual(len(pair["admission_missing"]), 2)
        self.assertIn("Local alive/identity", pair["admission_missing"][1])

    def test_pair_profile_camera_phase_and_case_mismatch(self):
        base = analyze()
        for group, key, value in [("build", "profile", "play"), ("case", "camera", {"position": [9, 9, 9]}), ("case", "bots", 10)]:
            rec = receipt(fixture())
            rec[group][key] = value
            self.assertFalse(perf.compare(base, analyze(fixture(), rec))["metadata_aligned_descriptive_only"])
        rec = receipt(fixture())
        rec["windows"] = rec["windows"][:1]
        self.assertFalse(perf.compare(base, analyze(fixture(), rec))["metadata_aligned_descriptive_only"])

    def test_overlapping_early_late_and_invalid_camera_are_not_admitted(self):
        raw = fixture()
        rec = receipt(raw)
        rec["windows"][1] = {**rec["windows"][0], "name": "late"}
        r = analyze(raw, rec)
        self.assertIn("overlap", str(r["log_errors"]))
        self.assertFalse(any(w["valid_descriptive_window"] for w in r["windows"]))
        for camera in [{"position": [1, 2]}, {"position": [1, 2, 3], "pitch": True, "yaw": 0}]:
            rec = receipt(raw)
            rec["case"]["camera"] = camera
            self.assertIn("case.camera", str(analyze(raw, rec)["receipt_errors"]))

    def test_malformed_stage_and_gpu_history_exclude_interval(self):
        raw = fixture()
        for old, new in [(b"calls=100 total=600.000ms", b"calls=0 total=600.000ms"), (b"history_mean=0.125ms", b"history_mean=NaNms")]:
            r = analyze(raw.replace(old, new))
            self.assertEqual(r["eligible_interval_count"], 0)
            self.assertFalse(any(w["valid_descriptive_window"] for w in r["windows"]))


class ClockAndDriftTests(unittest.TestCase):
    def test_utc_reorder_duplicate_real_reset_and_clock_jump_fail_closed(self):
        variants = [
            (block(1) + block(3) + block(2)).encode(),
            (block(1) + block(1) + block(2)).encode(),
            fixture().replace(stamp(3).encode(), stamp(30).encode()),
            fixture().replace(b"elapsed_real_s=3.000", b"elapsed_real_s=1.000"),
            (block(1) + block(3) + block(4)).encode(),
        ]
        for raw in variants:
            with self.subTest(raw=raw[:50]):
                r = analyze(raw)
                self.assertTrue(r["log_errors"])
                self.assertFalse(any(w["valid_descriptive_window"] for w in r["windows"]))

    def test_missing_utc_duplicate_records_unknown_perf_and_orphans(self):
        raw = fixture()
        variants = [
            perf.ANSI.sub("", raw.decode()).replace(stamp(2), "").encode(),
            raw.replace(b"perf asset census: meshes=30", b"perf unknown census: meshes=30"),
            raw.splitlines()[1] + b"\n" + raw,
            raw.replace(raw.splitlines()[4], raw.splitlines()[4] + b"\n" + raw.splitlines()[4]),
        ]
        for variant in variants:
            with self.subTest(variant=variant[:50]):
                r = analyze(variant)
                self.assertTrue(r["log_errors"] or any(row["exclusion_reasons"] for row in r["intervals"]))
                self.assertFalse(r["windows"][0]["valid_descriptive_window"])

    def test_asset_body_plateau_then_retained_count_and_mean_drift(self):
        raw = "".join(block(s, mean=10 if s <= 3 else 10 + s, entities=100 + max(0, s - 3), bodies=2 + max(0, s - 3), meshes=30 + max(0, s - 3)) for s in range(1, 7)).encode()
        r = analyze(raw)
        early, late = [w["summary"] for w in r["windows"]]
        self.assertEqual(early["endpoint_count_drift"]["characters.bodies"]["pattern"], "plateau")
        self.assertEqual(late["endpoint_count_drift"]["assets.meshes"]["pattern"], "nondecreasing")
        self.assertAlmostEqual(late["endpoint_count_drift"]["assets.meshes"]["pearson_with_mean_ms"], 1)
        self.assertEqual(r["early_late"]["mean_ms_delta"], 5.5)
        self.assertEqual(r["early_late"]["count_endpoint_delta"]["characters.bodies"], 3)
        self.assertIn("no causal attribution", r["early_late"]["interpretation"])


class OfflineCliTests(unittest.TestCase):
    def test_outputs_archive_exact_ansi_raw_receipts_hash_and_never_overwrite(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            raw = fixture()
            (root / "input.log").write_bytes(raw)
            rec = json.dumps(receipt(raw)).encode()
            (root / "run.json").write_bytes(rec)
            args = [str(root / "input.log"), "--receipt", str(root / "run.json"), "--out", str(root / "analysis")]
            self.assertEqual(perf.main(args), 0)
            self.assertEqual((root / "analysis/raw.log").read_bytes(), raw)
            self.assertEqual((root / "analysis/receipt.json").read_bytes(), rec)
            report = json.loads((root / "analysis/report.json").read_text(encoding="utf-8"))
            self.assertEqual(report["source"]["receipt_sha256"], hashlib.sha256(rec).hexdigest())
            self.assertIn("NOT ADMITTED", (root / "analysis/report.md").read_text(encoding="utf-8"))
            with self.assertRaises(SystemExit):
                perf.main(args)
            self.assertEqual((root / "input.log").read_bytes(), raw)

    def test_pair_and_missing_receipt_exit_codes(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            raw = fixture()
            (root / "a.log").write_bytes(raw)
            (root / "run.json").write_text(json.dumps(receipt(raw)), encoding="utf-8")
            self.assertEqual(perf.main([str(root / "a.log"), "--out", str(root / "missing")]), 2)
            args = [str(root / "a.log"), "--receipt", str(root / "run.json"), "--compare-log", str(root / "a.log"), "--compare-receipt", str(root / "run.json"), "--out", str(root / "pair")]
            self.assertEqual(perf.main(args), 0)
            self.assertTrue((root / "pair/comparison.md").exists())
            self.assertFalse(json.loads((root / "pair/comparison.json").read_text(encoding="utf-8"))["paired_admission"])


if __name__ == "__main__":
    unittest.main()
