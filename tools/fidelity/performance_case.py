"""Private, opt-in ordinary-input performance evidence. Never drives the UI.

Use performance-case.ps1 for execution; tests inject a process adapter and clock.
No third-party modules. All output stays under a new private run directory.
"""
import argparse
import ctypes
from ctypes import wintypes
import hashlib
import json
import math
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time
from datetime import datetime, timezone

# Share the exact old/new text parsers with offline analysis. No process or I/O
# happens on import; the analyzer is standard-library-only.
sys.dont_write_bytecode = True  # All helper output belongs in the private run, never __pycache__.
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import perf_report as perf

WINDOWS = ((30, 60), (300, 330), (600, 630))
MISSING = ['whole_interval_camera_pose', 'independent_window_coverage_and_focus_input_validation']
REQUESTED_SIZE = (1920, 1080)
BUILD_NAMES = {'cargo.exe', 'rustc.exe', 'rust-lld.exe', 'link.exe', 'cl.exe',
               'cmake.exe', 'ninja.exe', 'msbuild.exe', 'build-script-build.exe'}
GAME_NAMES = {'csr.exe', 'csgo.exe', 'cs2.exe', 'hl2.exe', 'iw4sp.exe', 'iw4mp.exe',
              'javaw.exe', 'minecraft.exe', 'valorant-win64-shipping.exe'}


def utc():
    return datetime.now(timezone.utc).isoformat()


def absolute(value):
    path = Path(value)
    if not path.is_absolute():
        raise ValueError(f'Absolute path required: {value}')
    # Resolve only AFTER rejecting junctions, including nonexistent descendants.
    for part in (path, *path.parents):
        try:
            attributes = part.lstat()
        except FileNotFoundError:
            continue
        if part.is_symlink() or getattr(attributes, 'st_file_attributes', 0) & 0x400:
            raise ValueError(f'Reparse path refused: {part}')
    return path.resolve()


def beneath(path, root):
    return path == root or root in path.parents


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def files_under(root):
    absolute(root)
    if not root.exists():
        return []
    if root.is_file():
        return [root]
    result = []
    for directory, dirs, files in os.walk(root, followlinks=False):
        for name in dirs + files:
            path = absolute(Path(directory) / name)
            if path.is_file():
                result.append(path)
    return sorted(result)


def snapshot(roots):
    # Explicit missing entries detect later creation; folder entries detect loss.
    result = {}
    for root in roots:
        result[str(root)] = 'directory' if root.is_dir() else (
            digest(root) if root.is_file() else 'missing')
        for path in files_under(root):
            result[str(path)] = digest(path)
    return result


def protection_roots(owner, steam):
    # No restoration writes: preserve hashes, flag any external change. Snapshot
    # all accounts, including newly-created files, not just the active account.
    return [owner / 'config', steam / 'config', steam / 'userdata',
            steam / 'steamapps' / 'libraryfolders.vdf']


def write_json(path, value):
    absolute(path)
    temporary = path.with_suffix(path.suffix + '.tmp')
    absolute(temporary)
    temporary.write_text(json.dumps(value, indent=2, allow_nan=False), encoding='utf-8')
    temporary.replace(path)


def guard_processes(processes, executable, owned=None):
    """CI is recorded, never killed. PID alone is never an ownership token."""
    ci = {p['pid'] for p in processes if p['name'].lower() in
          {'runner.worker.exe', 'runner.listener.exe'}}
    # A listener alone is idle; only a Worker and its descendants imply work.
    active_ci = {p['pid'] for p in processes if p['name'].lower() == 'runner.worker.exe'}
    for _ in range(len(processes)):
        new = {p['pid'] for p in processes if p.get('parent') in active_ci}
        if new <= active_ci:
            break
        active_ci |= new
    blocked = []
    for p in processes:
        if owned and (p['pid'], p.get('created'), p.get('path')) == owned:
            continue
        name = p['name'].lower()
        path = (p.get('path') or '').lower().replace('\\', '/')
        # Steam also distributes desktop utilities. Do not classify the known
        # Wallpaper Engine background executable as a second game, or stop it.
        # Other unknown programs under common remain conservatively blocked.
        wallpaper_utility = (name in {'wallpaper32.exe', 'wallpaper64.exe'} and
                             path.endswith('/steamapps/common/wallpaper_engine/' + name))
        game = (name in GAME_NAMES or name == executable.name.lower() or
                ('/steamapps/common/' in path and not wallpaper_utility))
        if game or (name in BUILD_NAMES and p['pid'] not in active_ci):
            blocked.append({'pid': p['pid'], 'name': p['name'], 'path': p.get('path'),
                            'reason': 'other_game' if game else 'non_ci_build'})
    return blocked, sorted(active_ci), sorted(ci)


def command(executable, map_name, bots):
    if map_name not in {'de_dust2_2017', 'de_nuke'} or bots not in {0, 3}:
        raise ValueError('Unsupported case')
    return [str(executable), 'play', map_name, '--direct', '--deathmatch',
            '--bots', str(bots), '--size', *map(str, REQUESTED_SIZE)]


def environment(data, game):
    env = {k: v for k, v in os.environ.items() if not k.upper().startswith('CSR_')}
    env.update(CSR_DATA_DIR=str(data), CSR_GAME_DIR=str(game), CSR_NO_UPDATE='1',
               CSR_PERF_REPORT='1', CSR_STEAM_DIR=str(data / 'no-steam'))
    env['RUST_LOG'] = 'info'
    # No CSR_FRAME_LOG, screenshot, camera, autofire, watch-bot or noclip flags.
    return env


def validate(args):
    expected = getattr(args, 'expected_resolution', None)
    if expected is not None and (len(expected) != 2 or any(type(v) is not int or not 0 < v <= 16384 for v in expected)):
        raise ValueError('Expected physical resolution needs two positive dimensions <= 16384')
    paths = {name: absolute(getattr(args, name)) for name in
             ('executable', 'manifest', 'private_root', 'owner_data', 'steam_root', 'game_dir')}
    exe, private = paths['executable'], paths['private_root']
    if not re.fullmatch(r'[0-9a-fA-F]{64}', args.sha256) or not re.fullmatch(r'[0-9a-fA-F]{40}', args.head):
        raise ValueError('Full expected SHA256 and source commit required')
    if not 660 <= args.max_seconds <= 1200 or not 12 <= args.min_free_gib <= 128:
        raise ValueError('Deadline or free-RAM floor outside bounded range')
    if private.exists():
        raise ValueError('PrivateRoot must be new; existing evidence is never reused')
    repo = Path(__file__).resolve().parents[2]
    # Do not write to any Git checkout, Steam folder, owner folder or binary
    # bundle; reject ancestors too (e.g. C:\ as a private root).
    protected = [repo, paths['owner_data'], paths['steam_root'], paths['game_dir'],
                 exe.parent, paths['manifest'].parent]
    if paths['owner_data'].name.lower() == 'userdata':
        protected.append(paths['owner_data'].parent)  # frozen player bundle
    if any(beneath(private, p) or beneath(p, private) for p in protected):
        raise ValueError('PrivateRoot overlaps protected source/binary/repository')
    if any((p / '.git').exists() for p in (private, *private.parents)):
        raise ValueError('PrivateRoot cannot be inside a Git checkout')
    if any(p.lower() in {'target', 'debug'} for p in exe.parts):
        raise ValueError('Use an immutable copied release executable, never shared target')
    if exe.name.lower() != 'csr.exe' or digest(exe) != args.sha256.lower():
        raise ValueError('Immutable csr.exe SHA256 mismatch')
    manifest = json.loads(paths['manifest'].read_text(encoding='utf-8-sig'))
    if (manifest.get('head') != args.head.lower() or manifest.get('sha256', '').lower() != args.sha256.lower()
            or absolute(manifest['exe']) != exe or manifest.get('profile') != 'release'
            or manifest.get('source_clean') is not True or manifest.get('owner_config_unchanged') is not True):
        raise ValueError('Build manifest does not match expected verified immutable release')
    checks_path = absolute(manifest['checks'])
    checks = json.loads(checks_path.read_text(encoding='utf-8-sig'))
    if not isinstance(checks, list) or not checks or any(
            c.get('exit_code') != 0 or c.get('head') != args.head.lower() for c in checks):
        raise ValueError('Build source checks are absent, failed or from another HEAD')
    game = paths['game_dir']
    if (game.name.lower() != 'csgo' or game.parent.name.lower() not in
            {'csgo legacy', 'counter-strike global offensive'} or not (game / 'pak01_dir.vpk').is_file()):
        raise ValueError('An explicit supported CS:GO Legacy csgo directory is required')
    if not (paths['owner_data'] / 'config').is_dir() or not paths['steam_root'].is_dir():
        raise ValueError('Owner config and Steam audit sources must exist')
    # An installed Velopack layout could offer Steam integration; refuse it.
    if any((p / 'sq.version').exists() or (p / 'Update.exe').exists() for p in (exe.parent, exe.parent.parent)):
        raise ValueError('Installed layout refused; copy csr.exe into a private immutable folder')
    return paths, manifest, checks_path


def seed_data(paths, map_name, data):
    owner = paths['owner_data']
    copied = []
    data.mkdir(parents=True, exist_ok=False)
    # All selected .cfg preferences, but no owner locks, logs, packs or temp files.
    for source in files_under(owner / 'config'):
        if source.suffix.lower() == '.cfg':
            target = data / 'config' / source.relative_to(owner / 'config')
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, target)
            copied.append({'source': str(source), 'destination': str(target), 'sha256': digest(target)})
    if map_name == 'de_dust2_2017':
        source = owner / 'workshop' / 'maps' / (map_name + '.bsp')
        if not source.is_file():
            raise ValueError('Old Dust II must already be cached; no fetch/download fallback')
        for suffix in ('.bsp', '.nav'):
            source = absolute(source.with_suffix(suffix))
            if source.is_file():
                target = data / 'workshop' / 'maps' / source.name
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(source, target)
                copied.append({'source': str(source), 'destination': str(target), 'sha256': digest(target)})
    return copied


class Reports:
    """Retain whole cadence records only; never reconstruct 30s quantiles."""
    def __init__(self, map_name, expected_resolution=None):
        self.map = map_name
        self.expected_resolution = list(expected_resolution) if expected_resolution is not None else None
        self.offset = 0
        self.partial = b''
        self.pending = None
        self.records = []
        self.sim = None
        self.ready = False
        self.sim_regressed = False
        self.observed_resolutions = []
        self.reports_without_simulation = []

    def read(self, path):
        with path.open('rb') as stream:
            stream.seek(self.offset)
            data = stream.read()
            self.offset = stream.tell()
        lines = (self.partial + data).split(b'\n')
        self.partial = lines.pop()
        for line in lines:
            self.line(re.sub(r'\x1b\[[0-9;]*m', '', line.decode('utf-8', errors='replace')))

    def line(self, line):
        if 'perf interval:' in line:
            if self.pending:
                # Startup can legitimately lack SimWorld: preserve that absence
                # explicitly without assigning it invented simulation bounds.
                self.pending['errors'].append('no simulation census before next interval; unbounded report')
                self.reports_without_simulation.append(self.pending)
            self.pending = {'interval': line, 'context': None, 'sim': None, 'observed_utc': utc(),
                            'parsed': {}, 'errors': []}
            try:
                self.pending['parsed']['interval'] = perf.parse_interval(line.split('perf interval:', 1)[1].strip())
            except (ValueError, KeyError) as error:
                self.pending['errors'].append(f'invalid interval: {error}')
        elif self.pending and 'perf context:' in line:
            if self.pending['context'] is not None:
                self.pending['errors'].append('duplicate context')
            self.pending['context'] = line
            try:
                self.pending['parsed']['context'] = perf.parse_context(line.split('perf context:', 1)[1].strip())
            except (ValueError, KeyError) as error:
                self.pending['errors'].append(f'invalid context: {error}')
        elif self.pending and any(marker in line for marker in (
                'perf player endpoint:', 'perf view intent endpoint:', 'perf world camera endpoint:')):
            for marker, kind in perf.MARKERS.items():
                if kind in perf.ENDPOINT_KEYS and marker in line:
                    try:
                        endpoint = perf.parse_endpoint(kind, line.split(marker, 1)[1].strip())
                        parsed = self.pending['parsed']
                        if kind == 'player_endpoint' and endpoint.get('local_resource') is not False:
                            endpoints = parsed.setdefault('player_endpoints', {})
                            if endpoint['role'] in endpoints or 'player_resource_endpoint' in parsed:
                                raise ValueError('duplicate/conflicting player endpoint')
                            endpoints[endpoint['role']] = endpoint
                        else:
                            key = 'player_resource_endpoint' if kind == 'player_endpoint' else kind
                            if key in parsed or (key == 'player_resource_endpoint' and parsed.get('player_endpoints')):
                                raise ValueError('duplicate/conflicting endpoint')
                            parsed[key] = endpoint
                    except (ValueError, KeyError) as error:
                        self.pending['errors'].append(f'invalid {kind}: {error}')
                    break
        elif self.pending and 'perf simulation census:' in line:
            try:
                census = perf.parse_census('simulation', line.split('perf simulation census:', 1)[1].strip())
                sim = census['sim_seconds']
            except (ValueError, KeyError) as error:
                self.pending['errors'].append(f'invalid simulation census: {error}')
                return
            if self.sim is not None and sim < self.sim:
                self.sim_regressed = True
            self.sim = sim
            self.pending.update(sim=sim, census=line)
            parsed = self.pending['parsed']
            context, interval = parsed.get('context', {}), parsed.get('interval', {})
            resolution = context.get('resolution')
            if resolution and resolution not in self.observed_resolutions:
                self.observed_resolutions.append(resolution)
            errors = self.pending['errors']
            errors.extend(perf.endpoint_exclusions(parsed))
            necessary = dict(map=self.map, state='Match', page='None', in_match=True,
                             focused=True, screenshot=False, input_enabled=True,
                             spectating='None', noclip=False, mixed_context=False)
            if not all(context.get(key) == value for key, value in necessary.items()):
                errors.append('missing/nonordinary/mixed Match context')
            if not interval or interval.get('mixed_context') or interval.get('rejected'):
                errors.append('missing/mixed/rejected interval samples')
            if not resolution or min(resolution) <= 0:
                errors.append('missing/invalid actual physical resolution')
            if self.expected_resolution is not None and resolution != self.expected_resolution:
                errors.append('actual physical resolution differs from expected physical resolution')
            if not census['tick_rate'] or abs(census['tick'] / max(census['tick_rate'], 1) - sim) > .002:
                errors.append('tick/tick_rate disagrees with actual sim_seconds')
            if census['bots'] > census['players'] or census['alive'] > census['players']:
                errors.append('invalid simulation counts')
            self.pending.update(actual_physical_resolution=resolution,
                                requested_cli_resolution=list(REQUESTED_SIZE),
                                expected_physical_resolution=self.expected_resolution,
                                endpoint_metadata=perf.endpoint_metadata(parsed))
            self.pending['ordinary_context_endpoint'] = not errors
            # Retain the old JSON key for existing root consumers. This is an
            # endpoint startup classification, never whole-interval admission.
            self.pending['context_valid_without_camera_alive'] = not errors
            self.ready |= self.pending['context_valid_without_camera_alive']
            self.records.append(self.pending)
            self.pending = None

    def windows(self, contaminated):
        windows = []
        previous = None
        for start, end in WINDOWS:
            rows = []
            previous = None
            # The prior census bounds the whole reported interval conservatively;
            # boundary-straddling frames cannot be assigned to an entire window.
            for record in self.records:
                if previous is not None and start <= previous <= record['sim'] <= end:
                    rows.append(record)
                previous = record['sim']
            windows.append({'sim_seconds': [start, end], 'records': rows, 'admitted': False,
                            'contaminated': contaminated or self.sim_regressed,
                            'unbounded_report_count': len(self.reports_without_simulation),
                            'simulation_clock_regressed': self.sim_regressed,
                            'needed_metadata': MISSING,
                            'observed_physical_resolutions': [list(size) for size in sorted({tuple(r['actual_physical_resolution']) for r in rows if r['actual_physical_resolution']})],
                            'missing_endpoint_fields': sorted({field for r in rows for field in r['endpoint_metadata']['missing_fields']}),
                            'invalid_context_records': sum(not r['context_valid_without_camera_alive'] for r in rows),
                            'reason': 'Whole-interval pose, external validation and coverage review required; never auto-admitted'})
        return windows


def run(args, adapter, clock=time):
    paths, manifest, checks_path = validate(args)
    root = paths['private_root']
    roots = protection_roots(paths['owner_data'], paths['steam_root'])
    if args.map == 'de_dust2_2017':
        roots += [paths['owner_data'] / 'workshop' / 'maps' / (args.map + suffix) for suffix in ('.bsp', '.nav')]
    before = snapshot(roots)
    root.mkdir(parents=True, exist_ok=False)
    state = {'schema': 1, 'phase': 'started', 'utc': utc(), 'admitted': False,
             'contaminated': False, 'contamination': [], 'needed_metadata': MISSING,
             'cancel_marker': str(root / 'CANCEL'), 'command': command(paths['executable'], args.map, args.bots),
             'bots_argument': args.bots, 'expected_bot_total': 0 if args.bots == 0 else 5,
             'head': args.head, 'sha256': args.sha256, 'manifest': manifest,
             'manifest_sha256': digest(paths['manifest']), 'checks_sha256': digest(checks_path)}
    expected = getattr(args, 'expected_resolution', None)
    state.update(requested_cli_resolution=list(REQUESTED_SIZE), expected_physical_resolution=expected,
                 resolution_policy='explicit_expected_physical' if expected else 'observe_actual_physical')
    reports = Reports(args.map, expected)
    child = None
    reason = 'error'
    deadline = clock.monotonic() + args.max_seconds

    def status(phase):
        state.update(phase=phase, utc=utc(), sim_seconds=reports.sim,
                     observed_physical_resolutions=reports.observed_resolutions.copy(),
                     reports_without_simulation=reports.reports_without_simulation)
        write_json(root / 'status.json', state)
        with (root / 'phases.jsonl').open('a', encoding='utf-8') as stream:
            stream.write(json.dumps({'phase': phase, 'utc': state['utc'], 'sim_seconds': reports.sim}) + '\n')

    status('started')
    write_json(root / 'protected-before.json', before)
    try:
        state['copied'] = seed_data(paths, args.map, root / 'userdata')
        state['environment'] = {k: v for k, v in environment(root / 'userdata', paths['game_dir']).items()
                                if k.startswith('CSR_') or k == 'RUST_LOG'}
        last_guard = -math.inf
        last_counters = -math.inf
        with (root / 'game.log').open('wb') as log, (root / 'telemetry.jsonl').open('a', encoding='utf-8') as telemetry:
            while True:
                now = clock.monotonic()
                if (root / 'CANCEL').exists():
                    reason = 'cancelled'
                    break
                if now >= deadline:
                    raise RuntimeError('Wall-clock deadline exceeded')
                if now - last_guard >= 2:
                    last_guard = now
                    query_started = clock.monotonic()
                    if adapter.free_ram() < args.min_free_gib * 1024**3:
                        raise RuntimeError('Free-RAM floor crossed')
                    blocked, ci, _ = guard_processes(adapter.processes(), paths['executable'], child.identity if child else None)
                    if ci:
                        state['contaminated'] = True
                        if not state['contamination'] or state['contamination'][-1]['pids'] != ci:
                            state['contamination'].append({'utc': utc(), 'pids': ci, 'reason': 'CI activity; no CI process stopped'})
                    if blocked:
                        state['blocked_processes'] = blocked
                        raise RuntimeError('Other game or non-CI build guard')
                    state['last_guard_duration_s'] = clock.monotonic() - query_started
                if child is None:
                    # Recheck hash immediately before launch; no shared-target use.
                    if digest(paths['executable']) != args.sha256.lower():
                        raise RuntimeError('Executable changed before launch')
                    if (digest(paths['manifest']) != state['manifest_sha256'] or
                            digest(checks_path) != state['checks_sha256']):
                        raise RuntimeError('Build verification receipts changed before launch')
                    child = adapter.launch(state['command'], root / 'userdata', environment(root / 'userdata', paths['game_dir']), log)
                    state['process_identity'] = child.identity
                    state['launched_utc'] = utc()
                if child.poll() is not None:
                    state['exit_code'] = child.poll()
                    raise RuntimeError('Game exited before requested windows completed')
                reports.read(root / 'game.log')
                if reports.ready:
                    if state['phase'] == 'started':
                        status('ready')
                        status('warmup')
                    sim = reports.sim
                    phase = 'measurement' if any(a <= sim <= b for a, b in WINDOWS) else 'warmup'
                    if phase != state['phase']:
                        status(phase)
                if now - last_counters >= 5:
                    last_counters = now
                    query_started = clock.monotonic()
                    sample = adapter.counters(child)
                    sample.update(utc=utc(), monotonic_s=now, sim_seconds=reports.sim,
                                  query_duration_s=clock.monotonic() - query_started,
                                  process_identity=child.identity, last_guard_duration_s=state['last_guard_duration_s'])
                    telemetry.write(json.dumps(sample, allow_nan=False) + '\n')
                    telemetry.flush()
                    write_json(root / 'windows.json', reports.windows(state['contaminated']))
                    write_json(root / 'status.json', {**state, 'utc': utc(), 'sim_seconds': reports.sim,
                               'observed_physical_resolutions': reports.observed_resolutions})
                if reports.sim is not None and reports.sim > WINDOWS[-1][1]:
                    reason = 'windows_recorded_unadmitted'
                    break
                clock.sleep(0.5)
    except BaseException as error:
        state['error'] = f'{type(error).__name__}: {error}'
        reason = 'failed'
    finally:
        try:
            if child is not None:
                # A retained OS handle, never taskkill /T, name, or reopened PID.
                child.close_owned()
            after = snapshot(roots)
            state['protected_unchanged'] = before == after
            write_json(root / 'protected-after.json', after)
            if before != after:
                state['contaminated'] = True
                state['preservation_error'] = 'Protected hashes changed externally; no restoration writes attempted'
                reason = 'failed'
            state['binary_unchanged'] = digest(paths['executable']) == args.sha256.lower()
            if not state['binary_unchanged']:
                reason = 'failed'
        except BaseException as error:
            state['cleanup_error'] = f'{type(error).__name__}: {error}'
            reason = 'failed'
        state['close_reason'] = reason
        write_json(root / 'windows.json', reports.windows(state['contaminated']))
        status('closed')
    return 0 if reason == 'windows_recorded_unadmitted' else (2 if reason == 'cancelled' else 1)


class WindowsAdapter:
    """Read-only CIM counters, owned Popen handle. No OS UI implementation."""
    def __init__(self):
        if os.name != 'nt':
            raise RuntimeError('Execution requires Windows')

    @staticmethod
    def powershell(script):
        result = subprocess.run(['powershell.exe', '-NoProfile', '-NonInteractive', '-Command', script],
                                capture_output=True, text=True, timeout=8,
                                creationflags=subprocess.CREATE_NO_WINDOW)
        if result.returncode:
            raise RuntimeError('Read-only telemetry query failed: ' + result.stderr[-1000:])
        return json.loads(result.stdout.lstrip('\ufeff'))

    def processes(self):
        rows = self.powershell("$ErrorActionPreference='Stop'; @(Get-CimInstance Win32_Process | ForEach-Object {"
                               "@{pid=[int]$_.ProcessId;parent=[int]$_.ParentProcessId;name=$_.Name;"
                               "path=$_.ExecutablePath;created=if($_.CreationDate){$_.CreationDate.ToUniversalTime().ToString('o')}else{$null}}"
                               "}) | ConvertTo-Json -Compress")
        return rows if isinstance(rows, list) else [rows]

    def free_ram(self):
        class Memory(ctypes.Structure):
            _fields_ = [('length', wintypes.DWORD), ('load', wintypes.DWORD)] + [
                (name, ctypes.c_ulonglong) for name in ('total', 'available', 'page_total', 'page_available', 'virtual_total', 'virtual_available', 'extended')]
        memory = Memory()
        memory.length = ctypes.sizeof(memory)
        if not ctypes.windll.kernel32.GlobalMemoryStatusEx(ctypes.byref(memory)):
            raise ctypes.WinError()
        return memory.available

    def launch(self, argv, cwd, env, log):
        proc = subprocess.Popen(argv, cwd=cwd, env=env, stdout=log, stderr=subprocess.STDOUT,
                                creationflags=subprocess.CREATE_NEW_PROCESS_GROUP)
        try:
            rows = [p for p in self.processes() if p['pid'] == proc.pid]
            if len(rows) != 1 or not rows[0]['created'] or not rows[0]['path'] or absolute(rows[0]['path']) != Path(argv[0]):
                raise RuntimeError('Owned process identity unavailable/mismatched')
            return OwnedProcess(proc, (proc.pid, rows[0]['created'], rows[0]['path']))
        except BaseException:
            if proc.poll() is None:
                proc.terminate()
                proc.wait(timeout=8)
            raise

    def counters(self, child):
        # CIM Process counters include cumulative CPU seconds and byte sizes;
        # GPU counters are per-engine utilization, not summed "GPU busy" or VRAM.
        pid = int(child.identity[0])
        quoted_path = "'" + child.identity[2].replace("'", "''") + "'"
        quoted_start = "'" + child.identity[1].replace("'", "''") + "'"
        identity_check = ("$i=Get-CimInstance Win32_Process -Filter 'ProcessId=" + str(pid) + "';"
                          "if(!$i -or $i.CreationDate.ToUniversalTime().ToString('o') -cne " + quoted_start +
                          " -or ![string]::Equals($i.ExecutablePath," + quoted_path +
                          ",[StringComparison]::OrdinalIgnoreCase)){throw 'Counter process identity changed'};")
        script = ("$ErrorActionPreference='Stop';" + identity_check + "$p=Get-Process -Id " + str(pid) + ";"
                  "$r=@{private_bytes=[long]$p.PrivateMemorySize64;working_bytes=[long]$p.WorkingSet64;"
                  "cpu_seconds=$p.TotalProcessorTime.TotalSeconds;counter_utc=[DateTime]::UtcNow.ToString('o')};"
                  "try {$r.gpu_engines=@(Get-CimInstance Win32_PerfFormattedData_GPUPerformanceCounters_GPUEngine | "
                  "Where-Object {$_.Name -like 'pid_" + str(pid) + "_*'} | Select-Object Name,UtilizationPercentage);"
                  "$r.gpu_memory=@(Get-CimInstance Win32_PerfFormattedData_GPUPerformanceCounters_GPUProcessMemory | "
                  "Where-Object {$_.Name -like 'pid_" + str(pid) + "_*'} | Select-Object Name,DedicatedUsage,SharedUsage)"
                  "}catch{$r.gpu_error=$_.Exception.Message};" + identity_check + " $r | ConvertTo-Json -Depth 5 -Compress")
        try:
            return self.powershell(script)
        except (RuntimeError, subprocess.TimeoutExpired, ValueError) as error:
            return {'counter_error': str(error), 'private_bytes': None, 'working_bytes': None,
                    'cpu_seconds': None, 'gpu_engines': None}


class OwnedProcess:
    def __init__(self, proc, identity):
        self.proc, self.identity = proc, identity

    def poll(self):
        return self.proc.poll()

    def close_owned(self):
        if self.proc.poll() is None:
            self.proc.terminate()  # Popen retains the original Windows process handle.
        self.proc.wait(timeout=8)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('executable', 'sha256', 'head', 'manifest', 'private-root', 'owner-data', 'steam-root', 'game-dir'):
        parser.add_argument('--' + name, required=True)
    parser.add_argument('--map', required=True, choices=('de_dust2_2017', 'de_nuke'))
    parser.add_argument('--bots', type=int, required=True, choices=(0, 3))
    parser.add_argument('--max-seconds', type=int, default=900)
    parser.add_argument('--min-free-gib', type=int, default=12)
    parser.add_argument('--expected-resolution', type=int, nargs=2, metavar=('WIDTH', 'HEIGHT'),
                        help='optional actual physical WindowContext size contract; CLI request remains 1920x1080')
    parser.add_argument('--execute', action='store_true')
    parser.add_argument('--native-slot-granted', action='store_true')
    args = parser.parse_args()
    if not args.execute or not args.native_slot_granted:
        parser.error('No launch without --execute and --native-slot-granted; use performance-case.ps1')
    if os.environ.get('CSR_PERF_CASE_SLOT_PARENT_PID') != str(os.getppid()):
        parser.error('Native slot must be held by the parent performance-case.ps1 wrapper')
    return run(args, WindowsAdapter())


if __name__ == '__main__':
    raise SystemExit(main())
