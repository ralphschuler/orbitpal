#!/usr/bin/env python3
"""Opt-in hosted-Mac native UI controller. No app IPC, TCC or profile writes."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import plistlib
import re
import shutil
import stat
import subprocess
import sys
import time

APP_ID = 'studio.orbitpal.native-world-fixture'
FIELDS = frozenset(('version', 'code', 'ax', 'appleEvents', 'gui', 'matched', 'acted', 'mainVisible', 'windows'))
REFS = ('refs/heads/main', 'refs/heads/feat/parallel-backlog', 'refs/heads/test/native-world-macos-acceptance')
DIRECTORY = Path(__file__).resolve().parent


class Refused(Exception):
    def __init__(self, code):
        super().__init__()
        self.code = code


def require(value, code=1):
    if not value:
        raise Refused(code)


def environment(platform, values):
    require(platform == 'darwin' and values.get('GITHUB_ACTIONS') == 'true'
            and values.get('RUNNER_ENVIRONMENT') == 'github-hosted' and values.get('RUNNER_OS') == 'macOS'
            and values.get('GITHUB_REPOSITORY') == 'ralphschuler/orbitpal'
            and values.get('GITHUB_EVENT_NAME') == 'workflow_dispatch' and values.get('GITHUB_REF') in REFS)
    reviewed = values.get('REVIEWED_SHA', '')
    require(re.fullmatch('[0-9a-f]{40}', reviewed) is not None and reviewed == values.get('DISPATCH_SHA'))
    require(not any(key.startswith('DYLD_') for key in values))


def directory(path, parent=None):
    require(path.is_absolute() and path.resolve(strict=True) == path)
    meta = path.lstat()
    require(stat.S_ISDIR(meta.st_mode) and meta.st_uid == os.geteuid() and not meta.st_mode & 0o022)
    if parent is not None:
        require(path.parent == parent and re.fullmatch('native-world-macos\\.[A-Za-z0-9]{8}', path.name) is not None)
    return path


def write(path, value):
    data = (json.dumps(value, sort_keys=True, separators=(',', ':')) + '\n').encode()
    require(len(data) <= 2048)
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, 'wb') as stream:
        stream.write(data)


def ui_record(raw):
    require(len(raw) <= 2048, 2)
    def pairs(items):
        result = {}
        for key, value in items:
            require(key not in result, 2)
            result[key] = value
        return result
    value = json.loads(raw, object_pairs_hook=pairs)
    require(type(value) is dict and set(value) == FIELDS and all(type(v) is int for v in value.values()), 2)
    require(value['version'] == 1 and 0 <= value['code'] <= 9 and 0 <= value['windows'] <= 16, 2)
    require(all(value[k] in (0, 1) for k in FIELDS - {'version', 'code', 'windows'}), 2)
    return value


def ui(operation, deadline, child=None, app=None):
    remaining = deadline - time.monotonic()
    require(remaining > 0, 3)
    if child is not None:
        require(child.poll() is None, 4) # Retained actual child: no PID reuse.
    args = [operation, str(child.pid) if child else '0', str(app) if app else '/', str(app / 'Contents/MacOS/native_world_fixture') if app else '/']
    result = subprocess.run(['/usr/bin/osascript', '-l', 'JavaScript', str(DIRECTORY / 'ui.js'), *args],
                            stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                            timeout=min(5, remaining), check=False)
    require(result.returncode == 0 and time.monotonic() < deadline, 3)
    return ui_record(result.stdout)


def package(binary, evidence):
    # Retained O_NOFOLLOW FD and repeated identity checks; no certificate changes.
    fd = os.open(binary, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    try:
        before = os.fstat(fd)
        require(stat.S_ISREG(before.st_mode) and before.st_uid == os.geteuid() and before.st_nlink == 1
                and before.st_mode & 0o111 and not before.st_mode & 0o022 and 0 < before.st_size <= 1024**3, 5)
        require(os.read(fd, 4) == b'\xcf\xfa\xed\xfe', 5) # Actual64-bit little-endian Mach-O.
        os.lseek(fd, 0, os.SEEK_SET)
        app = evidence / 'OrbitPalNativeWorldFixture.app'
        (app / 'Contents/MacOS').mkdir(parents=True, mode=0o700)
        destination = app / 'Contents/MacOS/native_world_fixture'
        digest = hashlib.sha256()
        output = os.open(destination, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o700)
        with os.fdopen(output, 'wb') as stream:
            copied = 0
            while data := os.read(fd, 1024 * 1024):
                copied += len(data)
                require(copied <= before.st_size, 5)
                digest.update(data); stream.write(data)
            require(copied == before.st_size, 5)
        after = os.fstat(fd)
        identity = lambda meta: (meta.st_dev, meta.st_ino, meta.st_size, meta.st_mtime_ns, meta.st_ctime_ns)
        require(identity(before) == identity(after) == identity(binary.lstat()), 5)
        info = {'CFBundleExecutable': 'native_world_fixture', 'CFBundleIdentifier': APP_ID,
                'CFBundleName': 'OrbitPal Native World Fixture', 'CFBundlePackageType': 'APPL',
                'CFBundleShortVersionString': '0.0.0', 'CFBundleVersion': '1', 'NSHighResolutionCapable': True}
        with (app / 'Contents/Info.plist').open('xb') as stream:
            plistlib.dump(info, stream)
        return app, digest.hexdigest()
    finally:
        os.close(fd)


def validated_report(path):
    """Existing exact fixture reader/schema; invalid schema is never exported."""
    source = DIRECTORY.parent / 'native-world-fixture/validate.py'
    spec = importlib.util.spec_from_file_location('owned_native_world_validator', source)
    validator = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(validator)
    value = validator.load(str(path))
    passed = validator.validate(value)
    return value, passed


def execute(binary, evidence):
    child, app, success, cleanup = None, None, False, 0
    status = {'version': 1, 'code': 0, 'stage': 0, 'normalQuit': 0, 'forcedCleanup': 0, 'fixturePassed': 0}
    deadline = time.monotonic() + 150 # Fixture itself retains its original180s.
    try:
        require(not list(evidence.glob('world-native-macos-actual-run-*')), 6)
        candidate = evidence / 'OrbitPalNativeWorldFixture.app'
        require(not os.path.lexists(candidate), 5)
        app = candidate # Own this initially absent path even if packaging fails midway.
        app, digest = package(binary, evidence)
        write(evidence / 'source.json', {'version': 1, 'reviewedSha': os.environ['REVIEWED_SHA'], 'binarySha256': digest})
        child = subprocess.Popen([str(app / 'Contents/MacOS/native_world_fixture')], stdin=subprocess.DEVNULL,
                                 stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        def perform(operation, stage):
            status['stage'] = stage
            # Reacquisition only for a not-yet-present actual frame/control.
            end = min(deadline, time.monotonic() + 12)
            while time.monotonic() < end:
                record = ui(operation, end, child, app)
                if record['code'] == 0:
                    return
                require(record['code'] in (6, 9) and record['acted'] == 0, 7)
                time.sleep(min(.25, max(0, end - time.monotonic())))
            raise Refused(7)
        perform('inspect', 1)
        for number, operation in enumerate(('autopilot', 'setup', 'nova', 'blue', 'start'), 2):
            perform(operation, number)
        # Wall waits are opportunities only. The existing real-frame report is
        # the only qualification oracle; elapsed waiting never fabricates Live.
        for operation, stage, seconds in ((None, 7, 45), ('hide', 8, 12)):
            status['stage'] = stage
            if operation:
                perform(operation, stage)
            end = time.monotonic() + seconds
            require(end < deadline, 3)
            while time.monotonic() < end:
                require(child.poll() is None, 4)
                time.sleep(min(.25, max(0, end - time.monotonic())))
        perform('show', 9)
        perform('stop', 10)
        perform('quit', 11)
        require(child.wait(timeout=min(10, max(.01, deadline-time.monotonic()))) == 0, 8)
        status['normalQuit'] = 1
        reports = list(evidence.glob('world-native-macos-actual-run-*/result.json'))
        require(len(reports) == 1, 9)
        require(time.monotonic() < deadline, 3)
        value, passed = validated_report(reports[0])
        require(time.monotonic() < deadline, 3)
        raw = (json.dumps(value, sort_keys=True, separators=(',', ':')) + '\n').encode()
        require(0 < len(raw) <= 16384, 9)
        # Retain genuine schema-valid incomplete/invalid numeric diagnostics too.
        with (evidence / 'fixture-result.json').open('xb') as stream:
            stream.write(raw)
        require(passed and value['status'] == 'passed', 9)
        status['fixturePassed'] = 1
        success = True
    except Refused as error:
        status['code'] = error.code
    except Exception:
        status['code'] = 10
    finally:
        if child is not None and child.poll() is None:
            status['forcedCleanup'] = 1
            child.terminate()
            try:
                child.wait(timeout=5)
            except subprocess.TimeoutExpired:
                child.kill()
                try: child.wait(timeout=3)
                except subprocess.TimeoutExpired: cleanup = 1
        try:
            if child is None or child.poll() is not None:
                for path in evidence.glob('world-native-macos-actual-run-*'):
                    require(re.fullmatch('world-native-macos-actual-run-[0-9a-f]{32}', path.name) is not None, 12)
                    directory(path)
                    shutil.rmtree(path) # Only this fresh fixture's private data, never uploaded.
                if app is not None:
                    shutil.rmtree(app)
            else:
                cleanup = 1
        except Exception:
            cleanup = 1
        status['cleanupUnconfirmed'] = cleanup
        write(evidence / 'controller.json', status)
    return 0 if success and not cleanup and not status['forcedCleanup'] else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--evidence', required=True, type=Path)
    parser.add_argument('--binary', type=Path)
    args = parser.parse_args()
    environment(sys.platform, os.environ)
    os.umask(0o077)
    temporary = directory(Path(os.environ['RUNNER_TEMP']))
    evidence = directory(args.evidence, temporary)
    head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=DIRECTORY, text=True).strip()
    require(head == os.environ['REVIEWED_SHA'])
    require(subprocess.run(['git', 'diff', '--quiet'], cwd=DIRECTORY, check=False).returncode == 0)
    probe = ui('probe', time.monotonic() + 5)
    write(evidence / ('capability.json' if args.binary is None else 'run-capability.json'), probe)
    require(probe['code'] == 0 and all(probe[key] == 1 for key in ('ax', 'appleEvents', 'gui')), 11)
    if args.binary is None:
        return 0
    target = Path(os.environ['CARGO_TARGET_DIR']).resolve(strict=True) / 'debug/examples/native_world_fixture'
    require(args.binary == target and args.binary.resolve(strict=True) == target, 5)
    return execute(target, evidence)


if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except Exception:
        print('native_world_macos_refused', file=sys.stderr)
        raise SystemExit(1)
