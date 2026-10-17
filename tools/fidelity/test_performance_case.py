"""Pure fixtures only: no Windows backend, game, build, UI or owner data."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('performance_case', Path(__file__).with_name('performance_case.py'))
case = importlib.util.module_from_spec(spec)
spec.loader.exec_module(case)
fixture_spec = importlib.util.spec_from_file_location(
    'perf_metadata_fixtures', Path(__file__).resolve().parents[1] / 'tests' / 'perf_metadata_fixtures.py')
metadata = importlib.util.module_from_spec(fixture_spec)
fixture_spec.loader.exec_module(metadata)

HEAD = 'a' * 40


def row(pid, name, parent=0, path=None, created='stable'):
    return dict(pid=pid, name=name, parent=parent, path=path, created=created)


def report(sim, extra=''):
    return (f'perf interval: elapsed_real_s={sim + 1} report_wall_s=1 sampled_real_s=1.000000 frames=100 '
            'mean_ms=10.0000 p50_p95_p99_ms=Some([10.0, 10.0, 10.0]) max_ms=10.0000 '
            'overflow=0 rejected=0 mixed_context=false context_changes=0\n'
            'perf context: endpoint_only=true map=de_nuke mixed_context=false observed=Some(FrameContext { '
            'state: Match, page: None, in_match: true, window: Some(WindowContext { size: (1920, 1080), '
            'focused: true, present_requested: Immediate, frame_latency_hint: Some(1) }), '
            'screenshot: false, input_enabled: true, local: Some(LocalContext { '
            'id: 1, spectating: None, noclip: false }) })' + extra + '\n'
            f'perf simulation census: tick={round(sim * 128)} tick_rate=128 sim_seconds={sim:.3f} '
            'competitive=false round=1 players=6 bots=5 alive=6 items=0 projectiles=0 fields=0\n')


class FakeClock:
    def __init__(self):
        self.now = 0

    def monotonic(self):
        return self.now

    def sleep(self, seconds):
        self.now += seconds


class FakeChild:
    def __init__(self, log, exe, reporter=report):
        self.identity = (42, 'stable', str(exe))
        self.closed = False
        self.log = log
        self.sim = -1
        self.reporter = reporter

    def poll(self):
        self.sim += 1
        self.log.write(self.reporter(self.sim).encode())
        self.log.flush()
        return None

    def close_owned(self):
        self.closed = True


class FakeAdapter:
    def __init__(self, exe, additional=None, ram=20 * 1024**3, cancel=None, reporter=report):
        self.exe, self.additional, self.ram = exe, additional or [], ram
        self.child, self.launches, self.samples, self.cancel = None, [], [], cancel
        self.reporter = reporter

    def free_ram(self):
        return self.ram

    def processes(self):
        rows = self.additional.copy()
        if self.child:
            rows.append(row(42, 'csr.exe', path=str(self.exe)))
            if self.cancel:
                self.cancel.touch()
        return rows

    def launch(self, argv, cwd, env, log):
        self.launches.append((argv, cwd, env))
        self.child = FakeChild(log, self.exe, self.reporter)
        return self.child

    def counters(self, child):
        assert child is self.child
        self.samples.append(child.sim)
        return dict(private_bytes=100, working_bytes=200, cpu_seconds=0.5, gpu_engines=[])


class CaseFixtures(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name).resolve()
        self.exe = self.base / 'immutable' / 'csr.exe'
        self.exe.parent.mkdir()
        self.exe.write_bytes(b'fixture only; not executable')
        self.owner = self.base / 'owner'
        (self.owner / 'config').mkdir(parents=True)
        (self.owner / 'config' / 'user.cfg').write_text('sensitivity=1.2', encoding='utf-8')
        (self.owner / 'config' / 'game.instance.lock').write_text('owner-lock', encoding='utf-8')
        self.steam = self.base / 'steam'
        (self.steam / 'userdata' / '1' / 'config').mkdir(parents=True)
        (self.steam / 'userdata' / '1' / 'config' / 'shortcuts.vdf').write_bytes(b'owner-steam')
        self.game = self.base / 'csgo legacy' / 'csgo'
        self.game.mkdir(parents=True)
        (self.game / 'pak01_dir.vpk').write_bytes(b'fixture marker')
        self.checks = self.base / 'checks.json'
        self.checks.write_text(json.dumps([dict(name='workspace', head=HEAD, exit_code=0)]), encoding='utf-8')
        self.manifest = self.exe.parent / 'build.json'
        self.manifest.write_text(json.dumps(dict(exe=str(self.exe), sha256=case.digest(self.exe), head=HEAD,
            profile='release', source_clean=True, owner_config_unchanged=True, checks=str(self.checks))), encoding='utf-8')
        self.args = argparse.Namespace(executable=str(self.exe), sha256=case.digest(self.exe), head=HEAD,
            manifest=str(self.manifest), private_root=str(self.base / 'private-run'),
            owner_data=str(self.owner), steam_root=str(self.steam), game_dir=str(self.game),
            map='de_nuke', bots=3, max_seconds=660, min_free_gib=12)

    def test_full_fixture_records_three_windows_and_never_admits(self):
        adapter = FakeAdapter(self.exe)
        before = case.snapshot(case.protection_roots(self.owner, self.steam))
        self.assertEqual(case.run(self.args, adapter, FakeClock()), 0)
        root = Path(self.args.private_root)
        windows = json.loads((root / 'windows.json').read_text(encoding='utf-8'))
        self.assertEqual([w['sim_seconds'] for w in windows], [[30, 60], [300, 330], [600, 630]])
        self.assertTrue(all(len(w['records']) == 30 and not w['admitted'] for w in windows))
        self.assertTrue(all(w['needed_metadata'] == case.MISSING for w in windows))
        state = json.loads((root / 'status.json').read_text(encoding='utf-8'))
        self.assertEqual(state['expected_bot_total'], 5)
        self.assertEqual(state['phase'], 'closed')
        self.assertTrue(state['protected_unchanged'])
        self.assertTrue(adapter.child.closed)
        self.assertLess(len(adapter.samples), 100)  # bounded cadence, never per-frame telemetry
        phases = [json.loads(s)['phase'] for s in (root / 'phases.jsonl').read_text(encoding='utf-8').splitlines()]
        self.assertEqual(phases[:3], ['started', 'ready', 'warmup'])
        self.assertIn('measurement', phases)
        self.assertEqual(phases[-1], 'closed')
        self.assertEqual(before, case.snapshot(case.protection_roots(self.owner, self.steam)))
        self.assertFalse((root / 'userdata' / 'config' / 'game.instance.lock').exists())
        self.assertEqual(adapter.launches[0][1], root / 'userdata')

    def test_cancel_marker_stops_only_owned_handle(self):
        adapter = FakeAdapter(self.exe, cancel=Path(self.args.private_root) / 'CANCEL')
        self.assertEqual(case.run(self.args, adapter, FakeClock()), 2)
        self.assertTrue(adapter.child.closed)
        self.assertEqual(len(adapter.launches), 1)

    def test_explicit_sustained_retains_strictly_after_630_stop(self):
        self.args.experiment = 'sustained'
        self.args.max_seconds = None
        adapter = FakeAdapter(self.exe)
        self.assertEqual(case.run(self.args, adapter, FakeClock()), 0)
        self.assertEqual(adapter.child.sim, 631)
        state = json.loads((Path(self.args.private_root) / 'status.json').read_text(encoding='utf-8'))
        self.assertEqual(state['experiment']['scope'], 'NORMAL_SUSTAINED')
        self.assertEqual(state['experiment']['max_wall_seconds'], 900)
        self.assertEqual(state['experiment']['measurement_windows_sim_seconds'], [[30, 60], [300, 330], [600, 630]])

    def test_experiment_enum_and_finite_mode_specific_deadlines(self):
        self.assertEqual(case.experiment_policy()['max_wall_seconds'], 900)
        self.assertEqual(case.experiment_policy()['measurement_windows_sim_seconds'],
                         [[30, 60], [300, 330], [600, 630]])
        for mode in ('unknown', 'HUD-baseline', '', 'hud-no-classic=1'):
            with self.subTest(mode=mode), self.assertRaises(ValueError):
                case.environment(self.base, self.game, mode)
        for mode, valid, invalid in (
                ('sustained', (660, 900, 1200), (120, 659, 1201)),
                ('hud-baseline', (120, 180, 300), (90, 119, 301, 660))):
            for deadline in (*invalid, float('nan'), float('inf'), 180.5, True):
                with self.subTest(mode=mode, deadline=deadline), self.assertRaises(ValueError):
                    case.experiment_policy(mode, deadline)
            for deadline in valid:
                self.assertEqual(case.experiment_policy(mode, deadline)['max_wall_seconds'], deadline)

    def test_cli_rejects_unknown_experiment_before_backend_creation(self):
        argv = ['performance_case.py', '--experiment', 'hud-arbitrary']
        for name in ('executable', 'sha256', 'head', 'manifest', 'private_root', 'owner_data', 'steam_root', 'game_dir', 'map', 'bots'):
            argv.extend(['--' + name.replace('_', '-'), str(getattr(self.args, name))])
        with patch.object(case.sys, 'argv', argv), patch.object(case, 'WindowsAdapter') as backend:
            with patch('sys.stderr'), self.assertRaises(SystemExit) as caught:
                case.main()
            self.assertEqual(caught.exception.code, 2)
            backend.assert_not_called()

    def test_exact_experiment_environments_scrub_inherited_flags_even_zero_values(self):
        expected = {
            'sustained': {}, 'hud-baseline': {},
            'hud-no-notices': {'CSR_NO_CLASSIC_KILL_FEED': '1'},
            'hud-no-classic': {'CSR_NO_CLASSIC_KILL_FEED': '1', 'CSR_NO_CLASSIC_COMBAT_HUD': '1'},
            'hud-render-trace': {'CSR_UI_RENDER_TRACE': '1', 'CSR_COMBAT_HUD_TRACE': '1', 'CSR_KILL_FEED_TRACE': '1'},
        }
        base = {'CSR_DATA_DIR': str(self.base), 'CSR_GAME_DIR': str(self.game),
                'CSR_NO_UPDATE': '1', 'CSR_PERF_REPORT': '1', 'CSR_STEAM_DIR': str(self.base / 'no-steam')}
        inherited = {flag: '0' for flags in expected.values() for flag in flags}
        inherited.update(CSR_FRAME_LOG='1', CSR_UI_WORKER_TRACE='1', CSR_AUTOFIRE='1',
                         CSR_NOCLIP='1', CSR_WATCH_BOT='2', CSR_CAMERA='bad', CSR_ARBITRARY='1', csr_lowercase='1')
        for mode, extra in expected.items():
            with self.subTest(mode=mode), patch.dict(os.environ, inherited):
                env = case.environment(self.base, self.game, mode)
                self.assertEqual({k: v for k, v in env.items() if k.upper().startswith('CSR_')}, base | extra)
                self.assertEqual(env['RUST_LOG'], 'info')

    def test_all_short_modes_stop_at_90_keep_early_window_and_private_receipt_labels(self):
        for mode in case.EXPERIMENTS[1:]:
            with self.subTest(mode=mode):
                self.args.experiment = mode
                self.args.max_seconds = None
                self.args.private_root = str(self.base / mode)
                adapter = FakeAdapter(self.exe)
                self.assertEqual(case.run(self.args, adapter, FakeClock()), 0)
                root = Path(self.args.private_root)
                state = json.loads((root / 'status.json').read_text(encoding='utf-8'))
                receipt = json.loads((root / 'case-receipt.json').read_text(encoding='utf-8'))
                windows = json.loads((root / 'windows.json').read_text(encoding='utf-8'))
                self.assertEqual(adapter.child.sim, 90)
                self.assertTrue(adapter.child.closed)
                self.assertEqual([w['sim_seconds'] for w in windows], [[30, 60]])
                self.assertEqual(len(windows[0]['records']), 30)
                self.assertFalse(windows[0]['admitted'])
                self.assertIn('DIAGNOSTIC_ABLATION', windows[0]['reason'])
                self.assertEqual(state, receipt)
                self.assertEqual(windows[0]['experiment'], state['experiment'])
                policy = state['experiment']
                self.assertEqual(policy['mode'], mode)
                self.assertEqual(policy['scope'], 'DIAGNOSTIC_ABLATION')
                self.assertEqual(policy['stop_sim_seconds'], 90)
                self.assertEqual(policy['max_wall_seconds'], 180)
                self.assertFalse(policy['auto_admission'])
                self.assertFalse(policy['normal_gameplay_fidelity_acceptance'])
                self.assertEqual(policy['environment'], state['environment'])
                self.assertEqual(state['environment'], {k: v for k, v in adapter.launches[0][2].items()
                                                       if k.startswith('CSR_') or k == 'RUST_LOG'})
                self.assertIsNone(policy['CSR_FRAME_LOG'])
                self.assertEqual(policy['trace']['render_submissions_per_host'], 1024 if mode == 'hud-render-trace' else 0)
                self.assertEqual(policy['trace']['review_first_post_ready_completions_with_drawable_notices'], 128)
                self.assertIn('timings are partial wall scopes, not GPU latency', policy['trace']['limits'])

    def test_short_mode_stalled_simulation_hits_wall_guard_and_closes_only_owned_handle(self):
        self.args.experiment = 'hud-no-classic'
        self.args.max_seconds = 120
        adapter = FakeAdapter(self.exe, reporter=lambda sim: report(min(sim, 45)))
        clock = FakeClock()
        self.assertEqual(case.run(self.args, adapter, clock), 1)
        self.assertTrue(adapter.child.closed)
        self.assertEqual(clock.now, 120)
        state = json.loads((Path(self.args.private_root) / 'status.json').read_text(encoding='utf-8'))
        self.assertIn('deadline', state['error'])
        self.assertFalse(state['admitted'])

    def test_source_gate_requires_both_ablations_to_prevent_hud_worker_creation(self):
        source = (Path(__file__).resolve().parents[2] / 'crates/client/src/kill_feed.rs').read_text(encoding='utf-8')
        self.assertIn('let notice_enabled = std::env::var_os("CSR_NO_CLASSIC_KILL_FEED").is_none();', source)
        self.assertIn('&& (notice_enabled || std::env::var_os("CSR_NO_CLASSIC_COMBAT_HUD").is_none())', source)

    def test_full_new_1440_fixture_records_cli_actual_and_preserves_without_admission(self):
        self.args.expected_resolution = [2560, 1440]
        adapter = FakeAdapter(self.exe, reporter=lambda sim: metadata.upgrade(report(sim)))
        self.assertEqual(case.run(self.args, adapter, FakeClock()), 0)
        root = Path(self.args.private_root)
        state = json.loads((root / 'status.json').read_text(encoding='utf-8'))
        self.assertEqual(state['requested_cli_resolution'], [1920, 1080])
        self.assertEqual(state['expected_physical_resolution'], [2560, 1440])
        self.assertEqual(state['observed_physical_resolutions'], [[2560, 1440]])
        self.assertTrue(state['protected_unchanged'])
        self.assertTrue(adapter.child.closed)
        self.assertIn('ready', (root / 'phases.jsonl').read_text(encoding='utf-8'))
        windows = json.loads((root / 'windows.json').read_text(encoding='utf-8'))
        self.assertTrue(all(not w['admitted'] and w['invalid_context_records'] == 0 for w in windows))
        self.assertTrue(all(not w['missing_endpoint_fields'] for w in windows))
        self.assertEqual(windows[0]['records'][0]['parsed']['world_camera_endpoint'][
            'raw_debug_fields']['physical_viewport_rect'], metadata.RECT)

    def test_expected_resolution_contract_validation(self):
        for value in ([1920], [-1, 1080], [0, 1080], [1920, 16385], [True, 1080]):
            self.args.expected_resolution = value
            with self.subTest(value=value), self.assertRaises(ValueError):
                case.validate(self.args)

    def test_default_observes_actual_old_and_new_resolution_without_cli_relabel(self):
        for text in (report(40).replace('size: (1920, 1080)', 'size: (2560, 1440)'),
                     metadata.upgrade(report(40))):
            reports = case.Reports('de_nuke')
            for line in text.splitlines():
                reports.line(line)
            self.assertTrue(reports.ready)
            record = reports.records[0]
            self.assertEqual(record['requested_cli_resolution'], [1920, 1080])
            self.assertEqual(record['actual_physical_resolution'], [2560, 1440])
            self.assertIsNone(record['expected_physical_resolution'])
            self.assertEqual(reports.observed_resolutions, [[2560, 1440]])

    def test_expected_physical_mismatch_missing_and_zero_size_cannot_be_ready(self):
        for text in (metadata.upgrade(report(40)), report(40).replace('size: (1920, 1080)', 'size: (0, 1080)'),
                     report(40).replace('size: (1920, 1080), ', '')):
            reports = case.Reports('de_nuke', (1920, 1080))
            for line in text.splitlines():
                reports.line(line)
            self.assertFalse(reports.ready)
            self.assertTrue(reports.records[0]['errors'])

    def test_old_missing_and_new_endpoint_availability_are_distinct_from_pose_evidence(self):
        for text, missing in ((report(40), True), (metadata.upgrade(report(40)), False)):
            reports = case.Reports('de_nuke')
            for line in text.splitlines():
                reports.line(line)
            record = reports.records[0]
            self.assertEqual(bool(record['endpoint_metadata']['missing_fields']), missing)
            self.assertFalse(record['endpoint_metadata']['whole_interval_camera_pose_proven'])
            self.assertFalse(any(w['admitted'] for w in reports.windows(False)))

    def test_new_missing_and_partial_metadata_are_explicit(self):
        for text in (metadata.upgrade(report(40), endpoints=False),
                     metadata.upgrade(report(40)).replace(' origin_source_inches=Some([-128.0, 64.5, 32.0])', '')):
            reports = case.Reports('de_nuke')
            for line in text.splitlines():
                reports.line(line)
            self.assertTrue(reports.records[0]['endpoint_metadata']['missing_fields'])
            self.assertFalse(any(w['admitted'] for w in reports.windows(False)))

    def test_new_mixed_dead_unavailable_and_inconsistent_endpoints_cannot_be_ready(self):
        text = metadata.upgrade(report(40))
        for old, new in [('mixed_context=false', 'mixed_context=true'), ('alive: true', 'alive: false'),
                         ('role_valid=true', 'role_valid=false'), ('requested_id=1', 'requested_id=2'),
                         ('render_eye_source_inches=[-128.0', 'render_eye_source_inches=[NaN'),
                         (', authoritative_view_target: ' + metadata.ACTOR, ''),
                         ('authoritative_local: ' + metadata.ACTOR, 'authoritative_local: None')]:
            with self.subTest(change=new):
                reports = case.Reports('de_nuke')
                for line in text.replace(old, new).splitlines():
                    reports.line(line)
                self.assertFalse(reports.ready)
                self.assertTrue(reports.records[0]['errors'])

    def test_incremental_new_log_keeps_whole_reports_and_resolution_changes(self):
        log = self.base / 'synthetic.log'
        first = metadata.upgrade(report(30)).encode()
        log.write_bytes(first[:len(first) // 2])
        reports = case.Reports('de_nuke')
        reports.read(log)
        self.assertFalse(reports.records)
        with log.open('ab') as stream:
            stream.write(first[len(first) // 2:] + metadata.upgrade(report(31)).encode() + report(32).encode())
        reports.read(log)
        self.assertEqual(len(reports.records), 3)
        self.assertEqual(reports.observed_resolutions, [[2560, 1440], [1920, 1080]])
        self.assertEqual([r['sim'] for r in reports.windows(False)[0]['records']], [31, 32])
        self.assertFalse(reports.windows(False)[0]['admitted'])

    def test_startup_without_simulation_census_remains_explicit_and_unbounded(self):
        reports = case.Reports('de_nuke')
        startup = '\n'.join(line for line in report(1).replace('state: Match', 'state: Loading').splitlines()
                            if 'perf simulation census:' not in line)
        for line in (startup + '\n' + report(30) + report(31)).splitlines():
            reports.line(line)
        self.assertEqual(len(reports.reports_without_simulation), 1)
        self.assertIsNone(reports.reports_without_simulation[0]['sim'])
        self.assertIn('unbounded report', reports.reports_without_simulation[0]['errors'][-1])
        window = reports.windows(False)[0]
        self.assertEqual(window['unbounded_report_count'], 1)
        self.assertEqual([r['sim'] for r in window['records']], [31])
        self.assertFalse(window['admitted'])

    def test_corrupt_interval_or_duplicate_endpoints_cannot_be_ready(self):
        text = metadata.upgrade(report(40))
        for changed in (text.replace('frames=100', 'frames=99'),
                        text.replace(metadata.endpoint_lines()[2], '\n'.join([metadata.endpoint_lines()[2]] * 2)),
                        text.replace('context_changes=0', 'context_changes=1')):
            reports = case.Reports('de_nuke')
            for line in changed.splitlines():
                reports.line(line)
            self.assertFalse(reports.ready)
            self.assertTrue(reports.records[0]['errors'])

    def test_preflight_other_game_and_non_ci_build_never_launch(self):
        for name in ('csgo.exe', 'cs2.exe', 'csr.exe', 'rustc.exe', 'cargo.exe', 'link.exe'):
            with self.subTest(name=name):
                self.args.private_root = str(self.base / name)
                adapter = FakeAdapter(self.exe, [row(23, name)])
                self.assertEqual(case.run(self.args, adapter, FakeClock()), 1)
                self.assertFalse(adapter.launches)

    def test_ram_floor_never_launches(self):
        adapter = FakeAdapter(self.exe, ram=11 * 1024**3)
        self.assertEqual(case.run(self.args, adapter, FakeClock()), 1)
        self.assertFalse(adapter.launches)

    def test_ci_descendants_contaminate_and_are_never_cleanup_targets(self):
        processes = [row(10, 'Runner.Worker.exe'), row(11, 'cmd.exe', 10), row(12, 'rustc.exe', 11)]
        adapter = FakeAdapter(self.exe, processes)
        self.assertEqual(case.run(self.args, adapter, FakeClock()), 0)
        state = json.loads((Path(self.args.private_root) / 'status.json').read_text(encoding='utf-8'))
        self.assertTrue(state['contaminated'])
        self.assertEqual(state['contamination'][0]['pids'], [10, 11, 12])
        self.assertEqual(adapter.additional, processes)
        self.assertTrue(adapter.child.closed)

    def test_listener_alone_is_idle_and_stable_identity_excludes_only_original(self):
        owned = (42, 'original', str(self.exe))
        original = row(42, 'csr.exe', path=str(self.exe), created='original')
        reused = row(42, 'csr.exe', path=str(self.exe), created='replacement')
        self.assertFalse(case.guard_processes([original], self.exe, owned)[0])
        self.assertTrue(case.guard_processes([reused], self.exe, owned)[0])
        self.assertFalse(case.guard_processes([row(10, 'Runner.Listener.exe')], self.exe)[1])

    def test_steam_game_path_and_unrelated_build_are_blocked_despite_ci(self):
        rows = [row(1, 'Runner.Worker.exe'), row(2, 'odd.exe', path='D:\\Steam\\steamapps\\common\\Title\\odd.exe'),
                row(3, 'cargo.exe', parent=8)]
        blocked, _, _ = case.guard_processes(rows, self.exe)
        self.assertEqual([p['pid'] for p in blocked], [2, 3])

    def test_wallpaper_engine_background_utility_is_not_a_second_game(self):
        rows = [row(1, 'wallpaper32.exe', path='C:/Program Files (x86)/Steam/steamapps/common/wallpaper_engine/wallpaper32.exe'),
                row(2, 'wallpaper64.exe', path='D:/Steam/steamapps/common/wallpaper_engine/wallpaper64.exe'),
                row(3, 'wallpaper32.exe', path='D:/Steam/steamapps/common/Other/wallpaper32.exe'),
                row(4, 'csgo.exe', path='D:/Steam/steamapps/common/wallpaper_engine/csgo.exe')]
        blocked, _, _ = case.guard_processes(rows, self.exe)
        self.assertEqual([p['pid'] for p in blocked], [3, 4])

    def test_protected_paths_and_shared_target_cannot_be_run_roots(self):
        for private in [self.owner, self.owner / 'new', self.steam / 'new', self.game / 'new',
                        self.exe.parent / 'new', self.base, Path(__file__).resolve().parent / 'private']:
            with self.subTest(private=private):
                self.args.private_root = str(private)
                with self.assertRaises(ValueError):
                    case.validate(self.args)

    def test_failed_checks_and_manifest_hash_mismatch_refuse(self):
        self.args.sha256 = 'f' * 64
        with self.assertRaises(ValueError):
            case.validate(self.args)
        self.args.sha256 = case.digest(self.exe)
        self.checks.write_text(json.dumps([dict(head=HEAD, exit_code=1)]), encoding='utf-8')
        with self.assertRaises(ValueError):
            case.validate(self.args)

    def test_cs2_path_is_refused(self):
        cs2 = self.base / 'Counter-Strike Global Offensive' / 'game' / 'csgo'
        cs2.mkdir(parents=True)
        (cs2 / 'pak01_dir.vpk').write_bytes(b'not Legacy')
        self.args.game_dir = str(cs2)
        with self.assertRaises(ValueError):
            case.validate(self.args)

    def test_selected_cached_map_copy_and_missing_cache_refusal(self):
        paths, _, _ = case.validate(self.args)
        with self.assertRaises(ValueError):
            case.seed_data(paths, 'de_dust2_2017', self.base / 'seed')
        maps = self.owner / 'workshop' / 'maps'
        maps.mkdir(parents=True)
        (maps / 'de_dust2_2017.bsp').write_bytes(b'private fixture')
        (maps / 'de_nuke.bsp').write_bytes(b'unselected')
        copied = case.seed_data(paths, 'de_dust2_2017', self.base / 'seed2')
        self.assertEqual(len(copied), 2)
        self.assertFalse((self.base / 'seed2' / 'workshop' / 'maps' / 'de_nuke.bsp').exists())
        self.assertEqual((maps / 'de_dust2_2017.bsp').read_bytes(), b'private fixture')

    def test_environment_scrubs_all_csr_switches_and_command_is_fixed(self):
        with patch.dict(os.environ, {'CSR_FRAME_LOG': '1', 'CSR_AUTOFIRE': '1', 'CSR_NOCLIP': '1',
                                     'CSR_WATCH_BOT': '2', 'CSR_SCREENSHOT': 'evil'}):
            env = case.environment(self.base / 'private', self.game)
        self.assertEqual({k for k in env if k.startswith('CSR_')},
                         {'CSR_DATA_DIR', 'CSR_GAME_DIR', 'CSR_NO_UPDATE', 'CSR_PERF_REPORT', 'CSR_STEAM_DIR'})
        argv = case.command(self.exe, 'de_nuke', 3)
        self.assertEqual(argv[1:], ['play', 'de_nuke', '--direct', '--deathmatch', '--bots', '3', '--size', '1920', '1080'])
        for token in ('--screenshot', '--camera', '--noclip', '--autofire'):
            self.assertNotIn(token, argv)

    def test_report_rejects_bad_focus_menu_input_and_mixed_history(self):
        for old, new in [('focused: true', 'focused: false'), ('state: Match', 'state: Loading'),
                         ('page: None', 'page: Some(Pause)'), ('input_enabled: true', 'input_enabled: false'),
                         ('spectating: None', 'spectating: Some(2)'), ('noclip: false', 'noclip: true'),
                         ('mixed_context=false', 'mixed_context=true')]:
            reports = case.Reports('de_nuke')
            for line in report(40).replace(old, new).splitlines():
                reports.line(line)
            self.assertFalse(reports.ready, new)
            self.assertFalse(reports.records[0]['context_valid_without_camera_alive'], new)

    def test_missing_metadata_is_never_silently_admitted(self):
        reports = case.Reports('de_nuke')
        for sim in [29, 31, 32, 60, 61]:
            for line in report(sim).splitlines():
                reports.line(line)
        windows = reports.windows(False)
        self.assertEqual([r['sim'] for r in windows[0]['records']], [32, 60])
        self.assertFalse(windows[0]['admitted'])
        self.assertEqual(windows[0]['needed_metadata'], case.MISSING)

    def test_simulation_reset_contaminates_every_window(self):
        reports = case.Reports('de_nuke')
        for sim in [40, 41, 0, 40, 41]:
            for line in report(sim).splitlines():
                reports.line(line)
        self.assertTrue(all(w['contaminated'] and w['simulation_clock_regressed'] for w in reports.windows(False)))

    def test_changed_owner_hash_is_flagged_without_restoration_writes(self):
        adapter = FakeAdapter(self.exe)
        original = adapter.counters

        def counters(child):
            (self.owner / 'config' / 'user.cfg').write_text('external edit', encoding='utf-8')
            return original(child)

        adapter.counters = counters
        self.assertEqual(case.run(self.args, adapter, FakeClock()), 1)
        state = json.loads((Path(self.args.private_root) / 'status.json').read_text(encoding='utf-8'))
        self.assertFalse(state['protected_unchanged'])
        self.assertTrue(state['contaminated'])
        self.assertEqual(state['protected_changes'], [{'path': str(self.owner / 'config' / 'user.cfg'),
                         'before': case.hashlib.sha256(b'sensitivity=1.2').hexdigest(),
                         'after': case.hashlib.sha256(b'external edit').hexdigest()}])
        self.assertEqual((self.owner / 'config' / 'user.cfg').read_text(encoding='utf-8'), 'external edit')

    def test_short_mode_external_steam_localconfig_change_still_fails_without_restoration(self):
        self.args.experiment = 'hud-baseline'
        self.args.max_seconds = 180
        localconfig = self.steam / 'userdata' / '1' / 'config' / 'localconfig.vdf'
        localconfig.write_bytes(b'before')
        owner_before = case.snapshot([self.owner / 'config'])
        adapter = FakeAdapter(self.exe)
        original = adapter.counters

        def counters(child):
            localconfig.write_bytes(b'external steam change')
            return original(child)

        adapter.counters = counters
        self.assertEqual(case.run(self.args, adapter, FakeClock()), 1)
        root = Path(self.args.private_root)
        state = json.loads((root / 'status.json').read_text(encoding='utf-8'))
        self.assertEqual(case.snapshot([self.owner / 'config']), owner_before)
        self.assertEqual(state['close_reason'], 'failed')
        self.assertTrue(state['contaminated'])
        self.assertFalse(state['protected_unchanged'])
        changes = json.loads((root / 'protected-changes.json').read_text(encoding='utf-8'))
        self.assertEqual(changes, [{'path': str(localconfig), 'before': case.hashlib.sha256(b'before').hexdigest(),
                                   'after': case.hashlib.sha256(b'external steam change').hexdigest()}])
        self.assertEqual(state['protected_changes'], changes)
        self.assertFalse(state['protected_changes_truncated'])
        self.assertEqual(localconfig.read_bytes(), b'external steam change')
        windows = json.loads((root / 'windows.json').read_text(encoding='utf-8'))
        self.assertTrue(all(w['contaminated'] and not w['admitted'] for w in windows))

    def test_deadline_closes_owned_process(self):
        adapter = FakeAdapter(self.exe)
        clock = FakeClock()
        original = adapter.counters

        def counters(child):
            clock.now = self.args.max_seconds + 1
            return original(child)

        adapter.counters = counters
        self.assertEqual(case.run(self.args, adapter, clock), 1)
        self.assertTrue(adapter.child.closed)
        state = json.loads((Path(self.args.private_root) / 'status.json').read_text(encoding='utf-8'))
        self.assertIn('deadline', state['error'])

    def test_windows_backend_scripts_are_only_inspected_and_never_executed(self):
        adapter = case.WindowsAdapter.__new__(case.WindowsAdapter)
        scripts = []

        def collect(script):
            scripts.append(script)
            return [] if 'ForEach-Object' in script else {}

        adapter.powershell = collect
        adapter.processes()
        child = FakeChild(None, self.exe)
        adapter.counters(child)
        self.assertIn('Get-CimInstance Win32_Process', scripts[0])
        self.assertEqual(scripts[1].count('Counter process identity changed'), 2)
        self.assertIn('GPUPerformanceCounters_GPUEngine', scripts[1])
        self.assertIn('PrivateMemorySize64', scripts[1])
        for script in scripts:
            self.assertNotIn('Stop-Process', script)
            self.assertNotIn('Start-Process', script)
            self.assertNotIn('SendKeys', script)

    def test_relative_paths_and_symlink_sources_are_refused(self):
        with self.assertRaises(ValueError):
            case.absolute(Path('relative'))
        link = self.base / 'link'
        try:
            link.symlink_to(self.owner, target_is_directory=True)
        except OSError:
            self.skipTest('Windows symlink creation unavailable; junction guard checked by attribute branch')
        with self.assertRaises(ValueError):
            case.absolute(link / 'config')


if __name__ == '__main__':
    unittest.main()
