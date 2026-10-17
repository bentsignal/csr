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

HEAD = 'a' * 40


def row(pid, name, parent=0, path=None, created='stable'):
    return dict(pid=pid, name=name, parent=parent, path=path, created=created)


def report(sim, extra=''):
    return ('perf interval: elapsed_real_s=1 report_wall_s=1 frames=100 mixed_context=false\n'
            'perf context: endpoint_only=true map=de_nuke mixed_context=false observed=Some(FrameContext { '
            'state: Match, page: None, in_match: true, window: Some(WindowContext { size: (1920, 1080), '
            'focused: true }), screenshot: false, input_enabled: true, local: Some(LocalContext { '
            'spectating: None, noclip: false }) })' + extra + '\n'
            f'perf simulation census: sim_seconds={sim:.3f} players=6 bots=5 alive=6\n')


class FakeClock:
    def __init__(self):
        self.now = 0

    def monotonic(self):
        return self.now

    def sleep(self, seconds):
        self.now += seconds


class FakeChild:
    def __init__(self, log, exe):
        self.identity = (42, 'stable', str(exe))
        self.closed = False
        self.log = log
        self.sim = -1

    def poll(self):
        self.sim += 1
        self.log.write(report(self.sim).encode())
        self.log.flush()
        return None

    def close_owned(self):
        self.closed = True


class FakeAdapter:
    def __init__(self, exe, additional=None, ram=20 * 1024**3, cancel=None):
        self.exe, self.additional, self.ram = exe, additional or [], ram
        self.child, self.launches, self.samples, self.cancel = None, [], [], cancel

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
        self.child = FakeChild(log, self.exe)
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
        self.assertEqual((self.owner / 'config' / 'user.cfg').read_text(encoding='utf-8'), 'external edit')

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
