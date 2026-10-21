#!/usr/bin/env python3
"""Pure guards/packaging/protocol tests. Never macOS UI or TCC access."""
import importlib.util
import json
import os
from pathlib import Path
import plistlib
import tempfile
import unittest
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
        return {key: 1 if key in ('version', 'ax', 'appleEvents', 'gui') else 0 for key in runner.FIELDS}

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
                    {**self.record(), 'code': 10}, {**self.record(), 'windows': 17}, {**self.record(), 'acted': -1}]
        for value in variants:
            with self.subTest(value=value), self.assertRaises(runner.Refused):
                runner.ui_record(json.dumps(value).encode())
        with self.assertRaises(runner.Refused): runner.ui_record(b'{"version":1,"version":1}')
        with self.assertRaises(runner.Refused): runner.ui_record(b'x' * 2049)

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
        self.assertIn('target.aeDesc, 0x2a2a2a2a, 0x2a2a2a2a, false', source)
        self.assertIn('$.AXIsProcessTrusted()', source)


if __name__ == '__main__':
    unittest.main()
