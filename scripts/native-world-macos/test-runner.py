#!/usr/bin/env python3
"""Pure guards/packaging/protocol tests. Never macOS UI or TCC access."""
import importlib.util
import json
import os
from pathlib import Path
import plistlib
import tempfile
import unittest
from types import SimpleNamespace
from unittest.mock import Mock, patch

DIRECTORY = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('macos_world_runner', DIRECTORY / 'runner.py')
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


class Guards(unittest.TestCase):
    def environment(self):
        return {'GITHUB_ACTIONS': 'true', 'RUNNER_ENVIRONMENT': 'github-hosted', 'RUNNER_OS': 'macOS',
                'GITHUB_REPOSITORY': 'ralphschuler/orbitpal', 'GITHUB_EVENT_NAME': 'workflow_dispatch',
                'GITHUB_REF': 'refs/heads/main', 'REVIEWED_SHA': 'a' * 40, 'DISPATCH_SHA': 'a' * 40}

    def record(self):
        return {key: 6 if key == 'probePhase' else 1 if key == 'lockState' else 2 if key in runner.SESSION_FIELDS else 1 if key in ('version', 'ax', 'appleEvents', 'gui', 'aeFunction', 'targetRunning') else 0 for key in runner.FIELDS}

    def test_only_reviewed_explicit_hosted_mac_source_can_reach_probe(self):
        runner.environment('darwin', self.environment())
        for key, bad in (('GITHUB_ACTIONS', 'false'), ('RUNNER_ENVIRONMENT', 'self-hosted'),
                         ('RUNNER_OS', 'Linux'), ('GITHUB_REPOSITORY', 'foreign/repo'),
                         ('GITHUB_EVENT_NAME', 'pull_request'), ('GITHUB_REF', 'refs/heads/arbitrary'),
                         ('REVIEWED_SHA', 'a' * 39), ('DISPATCH_SHA', 'b' * 40), ('DYLD_INSERT_LIBRARIES', '/foreign')):
            with self.subTest(key=key), self.assertRaises(runner.Refused):
                runner.environment('darwin', {**self.environment(), key: bad})
        with self.assertRaises(runner.Refused):
            runner.environment('linux', self.environment())

    def test_numeric_protocol_rejects_private_extra_duplicate_bool_and_out_of_range(self):
        self.assertEqual(runner.ui_record(json.dumps(self.record()).encode()), self.record())
        variants = [{**self.record(), 'private': 'PRIVATE'}, {**self.record(), 'ax': True},
                    {**self.record(), 'code': 10}, {**self.record(), 'windows': 17}, {**self.record(), 'acted': -1},
                    {**self.record(), 'probePhase': 8}, {**self.record(), 'probePhase': True},
                    {**self.record(), 'aeFunction': 2}, {**self.record(), 'aeFunction': True},
                    {**self.record(), 'probePhase': 4}, {**self.record(), 'aeFunction': 0},
                    {**self.record(), 'aeStatusKind': -1}, {**self.record(), 'aeStatusKind': 5},
                    {**self.record(), 'aeStatusKind': True}, {**self.record(), 'aeStatusKind': 1},
                    {**self.record(), 'code': 3, 'probePhase': 4},
                    {**self.record(), 'targetRunning': 2}, {**self.record(), 'targetRunning': True},
                    {**self.record(), 'targetRunning': 0}]
        for value in variants:
            with self.subTest(value=value), self.assertRaises(runner.Refused):
                runner.ui_record(json.dumps(value).encode())
        with self.assertRaises(runner.Refused): runner.ui_record(b'{"version":1,"version":1}')
        with self.assertRaises(runner.Refused): runner.ui_record(b'x' * 2049)

    def test_closed_probe_failure_phases_are_retained_without_becoming_success(self):
        for code, phase, present in ((3, 4, 1), (8, 1, 0), (8, 2, 1), (8, 3, 1), (8, 4, 1), (8, 5, 1)):
            value = {**self.record(), 'code': code, 'probePhase': phase, 'aeFunction': present,
                     'appleEvents': int(phase == 5), 'gui': 0, 'aeStatusKind': 0 if phase == 5 else 2 if code == 3 else 4}
            with self.subTest(code=code, phase=phase):
                self.assertEqual(runner.ui_record(json.dumps(value).encode()), value)
                self.assertNotEqual(value['code'], 0)

    def test_completed_permission_failure_keeps_each_closed_kind(self):
        for kind in (1, 2, 3, 4):
            value = {**self.record(), 'code': 3, 'probePhase': 4, 'aeStatusKind': kind, 'appleEvents': 0, 'gui': 0}
            self.assertEqual(runner.ui_record(json.dumps(value).encode()), value)
            self.assertNotIn('status', value)

    def test_session_diagnostics_are_closed_tristates_not_raw_values(self):
        for field in runner.SESSION_FIELDS:
            for state in (0, 1, 2):
                record = {**self.record(), 'code': 4, 'probePhase': 5, 'gui': 0, field: state}
                self.assertEqual(runner.ui_record(json.dumps(record).encode()), record)
            for invalid in (-1, 3, True, 'PRIVATE_SESSION_VALUE', None):
                with self.subTest(field=field, invalid=invalid), self.assertRaises(runner.Refused):
                    runner.ui_record(json.dumps({**self.record(), field: invalid}).encode())
        with self.assertRaises(runner.Refused):
            runner.ui_record(json.dumps({**self.record(), 'rawSession': {'PRIVATE': 1}}).encode())

    def system_source(self, identifier='com.apple.systemevents'):
        """All paths/stat/FDs/processes below are doubles, never system access."""
        metadata = {}
        class SourcePath:
            def __init__(self, value): self.value = value
            def __truediv__(self, value): return SourcePath(self.value + '/' + value)
            def __str__(self): return self.value
            def resolve(self, strict):
                assert strict is True
                return self
            def __eq__(self, other): return isinstance(other, SourcePath) and self.value == other.value
            def lstat(self): return metadata[self.value]
        bundle = SourcePath('/System/Library/CoreServices/System Events.app')
        executable = bundle / 'Contents/MacOS/System Events'
        raw = plistlib.dumps({'CFBundleIdentifier': identifier, 'CFBundleExecutable': 'System Events'})
        def meta(mode, size, inode):
            return SimpleNamespace(st_mode=mode, st_uid=0, st_size=size, st_dev=1, st_ino=inode, st_mtime_ns=1, st_ctime_ns=1)
        for index, path in enumerate((bundle, bundle / 'Contents', bundle / 'Contents/MacOS')):
            metadata[str(path)] = meta(runner.stat.S_IFDIR | 0o755, 0, index)
        metadata[str(bundle / 'Contents/Info.plist')] = meta(runner.stat.S_IFREG | 0o644, len(raw), 10)
        metadata[str(executable)] = meta(runner.stat.S_IFREG | 0o755, 4, 11)
        paths = {31: str(bundle / 'Contents/Info.plist'), 32: str(executable)}
        fake_os = SimpleNamespace(environ=self.environment(), O_RDONLY=os.O_RDONLY, O_NOFOLLOW=os.O_NOFOLLOW,
            O_NONBLOCK=os.O_NONBLOCK, open=Mock(side_effect=lambda path, flags: next(fd for fd, name in paths.items() if name == str(path))),
            fstat=Mock(side_effect=lambda fd: metadata[paths[fd]]), read=Mock(return_value=raw), close=Mock())
        process = SimpleNamespace(DEVNULL=-3, run=Mock(return_value=SimpleNamespace(returncode=0)))
        return bundle, executable, metadata, fake_os, process

    def test_fixed_apple_source_is_verified_before_one_background_launch(self):
        bundle, executable, _, fake_os, process = self.system_source()
        with patch.object(runner, 'SYSTEM_EVENTS', bundle), patch.object(runner, 'SYSTEM_EVENTS_EXECUTABLE', executable), \
             patch.object(runner, 'os', fake_os), patch.object(runner, 'sys', SimpleNamespace(platform='darwin')), \
             patch.object(runner, 'time', SimpleNamespace(monotonic=lambda: 0)), patch.object(runner, 'subprocess', process):
            runner.start_system_events(5)
        self.assertEqual([call.args[0] for call in process.run.call_args_list], [
            ['/usr/bin/codesign', '--verify', '--strict', '-R', '=anchor apple and identifier = "com.apple.systemevents"', str(bundle)],
            ['/usr/bin/open', '-g', '-j', '-a', str(bundle)],
        ])
        for call in process.run.call_args_list:
            self.assertEqual(call.kwargs, {'stdin': -3, 'stdout': -3, 'stderr': -3, 'timeout': 2, 'check': False})
        self.assertTrue(all(call.args[1] & os.O_NOFOLLOW for call in fake_os.open.call_args_list))
        self.assertEqual(fake_os.close.call_count, 2)

    def test_foreign_bundle_signature_change_and_expiry_never_reach_background_launch(self):
        for reason in ('bundle', 'owner', 'signature', 'replacement', 'deadline'):
            bundle, executable, metadata, fake_os, process = self.system_source('foreign' if reason == 'bundle' else 'com.apple.systemevents')
            if reason == 'owner': metadata[str(bundle)].st_uid = 501
            if reason == 'signature': process.run.return_value.returncode = 1
            if reason == 'replacement':
                def replace(*args, **kwargs):
                    metadata[str(executable)].st_ino += 1
                    return SimpleNamespace(returncode=0)
                process.run.side_effect = replace
            with patch.object(runner, 'SYSTEM_EVENTS', bundle), patch.object(runner, 'SYSTEM_EVENTS_EXECUTABLE', executable), \
                 patch.object(runner, 'os', fake_os), patch.object(runner, 'sys', SimpleNamespace(platform='darwin')), \
                 patch.object(runner, 'time', SimpleNamespace(monotonic=lambda: 0)), patch.object(runner, 'subprocess', process), \
                 self.subTest(reason=reason), self.assertRaises(runner.Refused):
                runner.start_system_events(0 if reason == 'deadline' else 5)
            self.assertFalse(any(call.args[0][0] == '/usr/bin/open' for call in process.run.call_args_list))

    def test_non_native_context_and_invalid_reviewed_source_cannot_start_system_target(self):
        with patch.object(runner, 'sys', SimpleNamespace(platform='linux')), \
             patch.object(runner, 'os', SimpleNamespace(environ=self.environment())), \
             patch.object(runner, 'SYSTEM_EVENTS') as path, patch.object(runner, 'subprocess') as process, self.assertRaises(runner.Refused):
            runner.start_system_events(5)
        path.resolve.assert_not_called(); process.run.assert_not_called()
        for bad_head, dirty in ((True, False), (False, True)):
            parser = Mock(); parser.parse_args.return_value = SimpleNamespace(evidence=Path('/owned/evidence'), binary=None)
            environment = {**self.environment(), 'RUNNER_TEMP': '/owned'}
            process = SimpleNamespace(check_output=Mock(return_value='b' * 40 if bad_head else 'a' * 40),
                                      run=Mock(return_value=SimpleNamespace(returncode=int(dirty))))
            with patch.object(runner, 'argparse', SimpleNamespace(ArgumentParser=Mock(return_value=parser))), \
                 patch.object(runner, 'sys', SimpleNamespace(platform='darwin')), \
                 patch.object(runner, 'os', SimpleNamespace(environ=environment, umask=Mock())), \
                 patch.object(runner, 'directory', return_value=Path('/owned')), patch.object(runner, 'subprocess', process), \
                 patch.object(runner, 'start_system_events') as start, patch.object(runner, 'ui') as ui, self.assertRaises(runner.Refused):
                runner.main()
            start.assert_not_called(); ui.assert_not_called()

    def test_codesign_and_background_launch_share_the_original_deadline(self):
        bundle, executable, _, fake_os, process = self.system_source()
        clock = Mock(side_effect=[0, 1, 4.75, 5])
        with patch.object(runner, 'SYSTEM_EVENTS', bundle), patch.object(runner, 'SYSTEM_EVENTS_EXECUTABLE', executable), \
             patch.object(runner, 'os', fake_os), patch.object(runner, 'sys', SimpleNamespace(platform='darwin')), \
             patch.object(runner, 'time', SimpleNamespace(monotonic=clock)), patch.object(runner, 'subprocess', process), \
             self.assertRaises(runner.Refused):
            runner.start_system_events(5)
        self.assertEqual([call.kwargs['timeout'] for call in process.run.call_args_list], [2, 0.25])

    def test_failed_target_preparation_retains_only_a_negative_record_before_any_probe(self):
        parser = Mock(); parser.parse_args.return_value = SimpleNamespace(evidence=Path('/owned/evidence'), binary=None)
        environment = {**self.environment(), 'RUNNER_TEMP': '/owned'}
        process = SimpleNamespace(check_output=Mock(return_value='a' * 40), run=Mock(return_value=SimpleNamespace(returncode=0)))
        with patch.object(runner, 'argparse', SimpleNamespace(ArgumentParser=Mock(return_value=parser))), \
             patch.object(runner, 'sys', SimpleNamespace(platform='darwin')), \
             patch.object(runner, 'os', SimpleNamespace(environ=environment, umask=Mock())), \
             patch.object(runner, 'directory', return_value=Path('/owned')), patch.object(runner, 'subprocess', process), \
             patch.object(runner, 'time', SimpleNamespace(monotonic=lambda: 0)), \
             patch.object(runner, 'start_system_events', side_effect=runner.Refused(5)) as start, \
             patch.object(runner, 'write') as write, patch.object(runner, 'ui') as ui, self.assertRaises(runner.Refused):
            runner.main()
        start.assert_called_once_with(5); ui.assert_not_called(); write.assert_called_once()
        value = write.call_args.args[1]
        self.assertEqual(runner.ui_record(json.dumps(value).encode()), value)
        self.assertEqual((value['code'], value['probePhase'], value['aeStatusKind']), (5, 7, 4))
        self.assertTrue(all(value[field] == 0 for field in ('ax', 'appleEvents', 'aeFunction', 'gui', 'targetRunning', 'acted')))

    def test_ui_uses_retained_child_fixed_osascript_and_withheld_stderr(self):
        child = Mock(pid=42)
        child.poll.return_value = None
        result = Mock(returncode=0, stdout=json.dumps(self.record()).encode())
        with patch.object(runner.time, 'monotonic', return_value=0), patch.object(runner.subprocess, 'run', return_value=result) as call:
            runner.ui('inspect', 4, child, Path('/owned/Fixture.app'))
        args, keywords = call.call_args
        self.assertEqual(args[0][:3], ['/usr/bin/osascript', '-l', 'JavaScript'])
        self.assertEqual(args[0][-4:], ['inspect', '42', '/owned/Fixture.app', '/owned/Fixture.app/Contents/MacOS/native_world_fixture'])
        self.assertEqual(keywords['timeout'], 4)
        self.assertEqual(keywords['stdin'], runner.subprocess.DEVNULL)
        self.assertEqual(keywords['stderr'], runner.subprocess.DEVNULL)

    def test_dead_or_expired_owned_child_cannot_start_a_native_client(self):
        child = Mock()
        child.poll.return_value = 0
        with patch.object(runner.time, 'monotonic', return_value=0), patch.object(runner.subprocess, 'run') as call:
            with self.assertRaises(runner.Refused): runner.ui('inspect', 2, child, Path('/owned/Fixture.app'))
        call.assert_not_called()
        with patch.object(runner.time, 'monotonic', return_value=2), patch.object(runner.subprocess, 'run') as call:
            with self.assertRaises(runner.Refused): runner.ui('probe', 2)
        call.assert_not_called()

    def test_native_client_reply_after_deadline_and_malformed_stdout_are_failures(self):
        result = Mock(returncode=0, stdout=json.dumps(self.record()).encode())
        with patch.object(runner.time, 'monotonic', side_effect=[0, 2]), patch.object(runner.subprocess, 'run', return_value=result):
            with self.assertRaises(runner.Refused): runner.ui('probe', 2)
        result.stdout = b'PRIVATE_NATIVE_ERROR'
        with patch.object(runner.time, 'monotonic', return_value=0), patch.object(runner.subprocess, 'run', return_value=result):
            with self.assertRaises((runner.Refused, ValueError)): runner.ui('probe', 2)

    def test_packaging_copies_exact_owned_macho_without_signing_or_extra_resources(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            binary = root / 'binary'
            contents = b'\xcf\xfa\xed\xfe' + b'controlled numeric test payload'
            binary.write_bytes(contents); binary.chmod(0o700)
            evidence = root / 'evidence'; evidence.mkdir(mode=0o700)
            app, digest = runner.package(binary, evidence)
            self.assertEqual((app / 'Contents/MacOS/native_world_fixture').read_bytes(), contents)
            self.assertEqual(digest, runner.hashlib.sha256(contents).hexdigest())
            with (app / 'Contents/Info.plist').open('rb') as stream:
                info = plistlib.load(stream)
            self.assertEqual(info['CFBundleIdentifier'], runner.APP_ID)
            self.assertTrue(info['NSHighResolutionCapable'])
            self.assertNotIn('LSUIElement', info)
            self.assertFalse((app / 'Contents/Resources').exists())

    def test_symlink_non_macho_and_writable_binary_refuse_without_subprocess(self):
        for kind in ('symlink', 'not_macho', 'writable'):
            with self.subTest(kind=kind), tempfile.TemporaryDirectory() as raw:
                root = Path(raw); binary = root / 'binary'; evidence = root / 'evidence'; evidence.mkdir()
                binary.write_bytes(b'\xcf\xfa\xed\xfe' if kind != 'not_macho' else b'not an executable')
                binary.chmod(0o722 if kind == 'writable' else 0o700)
                if kind == 'symlink':
                    binary.rename(root / 'target'); binary.symlink_to(root / 'target')
                with patch.object(runner.subprocess, 'Popen') as launch, self.assertRaises((runner.Refused, OSError)):
                    runner.package(binary, evidence)
                launch.assert_not_called()

    def test_receipt_is_exclusive_private_and_bounded(self):
        with tempfile.TemporaryDirectory() as raw:
            path = Path(raw) / 'result.json'
            runner.write(path, self.record())
            self.assertEqual(path.stat().st_mode & 0o777, 0o600)
            with self.assertRaises(FileExistsError): runner.write(path, self.record())
            with self.assertRaises(runner.Refused): runner.write(Path(raw) / 'oversize.json', {'x': 'x' * 2048})

    def test_failed_ui_forces_only_retained_child_cleanup_and_never_calls_validator(self):
        with tempfile.TemporaryDirectory() as raw:
            evidence = Path(raw)
            binary = evidence / 'binary'; binary.write_bytes(b'\xcf\xfa\xed\xfe'); binary.chmod(0o700)
            child = Mock(returncode=None)
            child.poll.side_effect = lambda: child.returncode
            child.terminate.side_effect = lambda: setattr(child, 'returncode', -15)
            child.wait.return_value = -15
            with patch.dict(runner.os.environ, {'REVIEWED_SHA': 'a' * 40}), patch.object(runner.subprocess, 'Popen', return_value=child), patch.object(runner, 'ui', side_effect=runner.Refused(7)), patch.object(runner, 'validated_report') as validate:
                self.assertEqual(runner.execute(binary, evidence), 1)
            record = json.loads((evidence / 'controller.json').read_text())
            self.assertEqual((record['forcedCleanup'], record['normalQuit'], record['fixturePassed']), (1, 0, 0))
            child.terminate.assert_called_once_with()
            child.kill.assert_not_called()
            validate.assert_not_called()
            self.assertFalse((evidence / 'OrbitPalNativeWorldFixture.app').exists())

    def test_validated_incomplete_fixture_report_is_retained_but_cannot_pass(self):
        with tempfile.TemporaryDirectory() as raw:
            evidence, clock = Path(raw).resolve(), [0.0]
            binary = evidence / 'binary'; binary.write_bytes(b'\xcf\xfa\xed\xfe'); binary.chmod(0o700)
            child = Mock(returncode=None)
            child.poll.side_effect = lambda: child.returncode
            child.wait.return_value = 0
            def ui(operation, *args):
                if operation == 'quit':
                    child.returncode = 0
                    folder = evidence / ('world-native-macos-actual-run-' + 'a' * 32)
                    folder.mkdir(mode=0o700)
                    (folder / 'result.json').write_text('{}')
                return self.record()
            value = {'version': 3, 'status': 'qualification_incomplete'}
            with patch.dict(runner.os.environ, {'REVIEWED_SHA': 'a' * 40}), patch.object(runner.subprocess, 'Popen', return_value=child), patch.object(runner, 'ui', side_effect=ui), patch.object(runner.time, 'monotonic', side_effect=lambda: clock[0]), patch.object(runner.time, 'sleep', side_effect=lambda delay: clock.__setitem__(0, clock[0]+delay)), patch.object(runner, 'validated_report', return_value=(value, False)):
                self.assertEqual(runner.execute(binary, evidence), 1)
            record = json.loads((evidence / 'controller.json').read_text())
            self.assertEqual((record['forcedCleanup'], record['normalQuit'], record['fixturePassed'], record['cleanupUnconfirmed']), (0, 1, 0, 0))
            self.assertEqual(json.loads((evidence / 'fixture-result.json').read_text()), value)
            child.terminate.assert_not_called()
            self.assertFalse(list(evidence.glob('world-native-macos-actual-run-*')))

    def test_native_source_has_no_app_script_or_permission_mutator(self):
        source = (DIRECTORY / 'ui.js').read_text()
        for forbidden in ('executeScript(', '.eval(', 'invoke(', 'tccutil', 'CGEventPost', 'keyCode(', 'keystroke('):
            self.assertNotIn(forbidden, source)
        self.assertIn('address, 0x2a2a2a2a, 0x2a2a2a2a, false', source)
        self.assertIn('$.AXIsProcessTrusted()', source)


if __name__ == '__main__':
    unittest.main()
