#!/usr/bin/python3
"""Controlled pure seams only: no actual process, system bus, desktop or app."""
import importlib.util
import io
import json
from pathlib import Path
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

SPEC = importlib.util.spec_from_file_location("app_session_availability", Path(__file__).with_name("app-session-availability.py"))
probe = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(probe)


class Bus:
    def __init__(self):
        self.calls = []
        self.owner_value = ":1.42"
        self.path = "/org/freedesktop/login1/session/c42"
        self.user = (88, "/org/freedesktop/login1/user/_88")
        self.values = {"LockedHint": False, "Active": True, "State": "active", "PreparingForSleep": False}
        self.no_session = False

    def owner(self):
        self.calls.append(("owner",))
        return self.owner_value

    def session(self, owner, pid):
        self.calls.append(("session", owner, pid))
        if self.no_session:
            raise probe.NoSession()
        return self.path

    def property(self, owner, path, interface, name, signature):
        self.calls.append(("property", owner, path, interface, name, signature))
        return self.user if name == "User" else self.values[name]


class Observations(unittest.TestCase):
    def setUp(self):
        self.target = SimpleNamespace(child=4242, check=Mock())
        self.bus = Bus()
        self.uid = patch.object(probe.os, "getuid", return_value=88)
        self.uid.start()
        self.addCleanup(self.uid.stop)

    def run_probe(self, clock=lambda: 0):
        return probe.observe(self.target, self.bus, 4, clock)

    def test_eligible_requires_two_real_scoped_samples_and_readbacks(self):
        result = self.run_probe()
        self.assertEqual((result["status"], result["samples"], result["knownMask"]), (1, 2, 31))
        self.assertEqual((result["active"], result["locked"], result["preparing"], result["state"]), (2, 1, 1, 1))
        self.assertEqual(self.target.check.call_count, 4)
        self.assertEqual([call[2] for call in self.bus.calls if call[0] == "session"], [4242] * 4)
        self.assertEqual([call[1] for call in self.bus.calls if call[0] == "property"], [":1.42"] * 12)
        self.assertEqual({call[4] for call in self.bus.calls if call[0] == "property"}, {"User", "LockedHint", "Active", "State", "PreparingForSleep"})

    def test_locked_inactive_sleeping_online_closing_unknown_are_not_ready(self):
        for key, value in (("LockedHint", True), ("Active", False), ("PreparingForSleep", True), ("State", "online"), ("State", "closing"), ("State", "other")):
            with self.subTest(key=key, value=value):
                self.bus = Bus()
                self.bus.values[key] = value
                self.assertEqual(self.run_probe()["status"], 2)

    def test_mistyped_properties_do_not_produce_partial_availability(self):
        for key, value in (("LockedHint", 0), ("Active", 1), ("PreparingForSleep", "false"), ("State", b"active"), ("State", "x" * 17)):
            with self.subTest(key=key):
                self.bus = Bus()
                self.bus.values[key] = value
                result = self.run_probe()
                self.assertEqual((result["status"], result["knownMask"], result["samples"]), (0, 0, 0))

    def test_session_user_must_be_typed_and_match_private_uid(self):
        self.bus.user = (99, "/org/freedesktop/login1/user/_99")
        result = self.run_probe()
        self.assertEqual((result["status"], result["sessionUserMatches"]), (4, 0))
        for value in ([88, "/org/freedesktop/login1/user/_88"], (True, "/org/freedesktop/login1/user/_88"), (88, "/foreign/path")):
            self.bus = Bus()
            self.bus.user = value
            self.assertEqual(self.run_probe()["status"], 0)

    def test_actual_no_session_is_an_observation_not_eligible(self):
        self.bus.no_session = True
        result = self.run_probe()
        self.assertEqual((result["status"], result["samples"], result["knownMask"]), (3, 1, 0))
        self.assertEqual(self.target.check.call_count, 2)

    def test_no_session_after_one_sample_discards_previous_fields(self):
        self.bus.session = Mock(side_effect=[self.bus.path, self.bus.path, probe.NoSession()])
        result = self.run_probe()
        self.assertEqual((result["status"], result["samples"], result["knownMask"], result["active"]), (3, 1, 0, 0))

    def test_owner_change_during_or_between_samples_discards_all_fields(self):
        for sequence in ((":1.42", ":1.43"), (":1.42", ":1.42", ":1.43", ":1.43")):
            self.bus = Bus()
            self.bus.owner = Mock(side_effect=sequence)
            result = self.run_probe()
            self.assertEqual((result["status"], result["knownMask"]), (5, 0))

    def test_session_path_change_during_or_between_samples_is_not_accepted(self):
        for sequence in (("/a", "/b"), ("/a", "/a", "/b", "/b")):
            self.bus = Bus()
            self.bus.session = Mock(side_effect=sequence)
            self.assertEqual(self.run_probe()["status"], 5)

    def test_session_user_identity_change_is_not_accepted(self):
        original = self.bus.property
        count = [0]
        def changing(*arguments):
            value = original(*arguments)
            if arguments[3] == "User":
                count[0] += 1
                return value if count[0] < 4 else (88, "/org/freedesktop/login1/user/replaced")
            return value
        self.bus.property = changing
        self.assertEqual(self.run_probe()["status"], 5)

    def test_target_replacement_or_exit_prevents_old_samples_surviving(self):
        for code in (5, 7):
            self.target.check = Mock(side_effect=[None, None, probe.ObservationError(code)])
            result = self.run_probe()
            self.assertEqual((result["status"], result["samples"], result["targetVerified"]), (code, 0, 0))

    def test_expired_deadline_queries_nothing_and_does_not_extend(self):
        result = self.run_probe(lambda: 4)
        self.assertEqual(result["status"], 6)
        self.assertEqual(self.bus.calls, [])
        self.target.check.assert_not_called()
        clock = Mock(side_effect=[0, 0, 4, 4])
        self.assertEqual(self.run_probe(clock)["status"], 6)

    def test_transport_or_unknown_error_contains_no_payload(self):
        self.bus.property = Mock(side_effect=RuntimeError("secret raw path /foreign and uid 99"))
        encoded = probe.encode_record(self.run_probe())
        for text in (b"secret", b"foreign", b"RuntimeError", b":1.42", b"4242"):
            self.assertNotIn(text, encoded)
        self.assertEqual(json.loads(encoded)["status"], 0)


class ProcessIdentity(unittest.TestCase):
    def target(self):
        target = probe.Target.__new__(probe.Target)
        target.paths = Mock()
        target.wrapper, target.controller, target.supervisor, target.child = 10, 11, 12, 13
        target.wrapper_command = b"fixed-wrapper"
        target.root, target.executable, target.initial = Path("/private"), Path("/private/acceptance-app"), None
        target.process = Mock(side_effect=lambda pid, *_: {10: (11, 100, 1, 1), 11: (1, 101, 1, 2), 12: (11, 102, 1, 3), 13: (12, 103, 1, 4)}[pid])
        return target

    def test_real_guard_requires_wrapper_and_supervisor_siblings_then_exact_child(self):
        target = self.target()
        with patch.object(probe.os, "getppid", return_value=10):
            target.check()
            target.check()
        self.assertEqual(target.process.call_count, 8)
        self.assertEqual(target.process.call_args_list[0].args, (10, Path("/usr/bin/timeout"), b"fixed-wrapper"))
        self.assertIn(b"app-runtime-diagnostic.py\0", target.process.call_args_list[1].args[2])
        self.assertEqual(target.process.call_args_list[3].args[1], Path("/usr/bin/bash"))

    def test_start_identity_or_parent_replacement_fails(self):
        for changed in ((12, 999, 1, 4), (99, 103, 1, 4)):
            target = self.target()
            original = target.process.side_effect
            with patch.object(probe.os, "getppid", return_value=10):
                target.check()
                target.process.side_effect = lambda pid, *args: changed if pid == 13 else original(pid, *args)
                with self.assertRaises(probe.ObservationError) as found:
                    target.check()
            self.assertEqual(found.exception.code, 5)

    def test_proc_stat_uses_last_comm_delimiter_and_actual_start_field(self):
        raw = b"13 (odd ) app name) S 12 " + b"0 " * 17 + b"103 0 0\n"
        self.assertEqual(probe.proc_fields(raw, 13), (12, 103))
        for changed in (raw.replace(b") S ", b") Z "), b"wrong", raw + b"x" * 8192):
            with self.assertRaises(probe.ObservationError):
                probe.proc_fields(changed, 13)

    def test_executable_uid_root_and_cmdline_are_rechecked_by_process(self):
        raw = b"13 (app) S 12 " + b"0 " * 17 + b"103 0\n"
        metadata = {"/proc/13/stat": raw, "/proc/13/status": b"Uid:\t88\t88\t88\t88\n", "/proc/13/cmdline": b"exact\0"}
        identities = {"/proc/13": (88, 1, 1), "/proc/13/exe": (88, 1, 4),
                      "/private/app": (88, 1, 4), "/proc/13/root": (0, 1, 2), "/": (0, 1, 2)}
        class Node:
            def __init__(self, value):
                self.value = str(value)
            def __truediv__(self, name):
                return Node(self.value.rstrip("/") + "/" + name)
            def stat(self):
                uid, device, inode = identities[self.value]
                return SimpleNamespace(st_uid=uid, st_dev=device, st_ino=inode)
            def open(self, _mode):
                return io.BytesIO(metadata[self.value])
        target = probe.Target.__new__(probe.Target)
        with patch.object(probe, "Path", Node), patch.object(probe.os, "getuid", return_value=88):
            self.assertEqual(target.process(13, Node("/private/app"), b"exact\0"), (12, 103, 1, 4))
            for key, replacement in (("/proc/13/exe", (88, 1, 99)), ("/proc/13/root", (0, 1, 99)), ("/proc/13", (99, 1, 1))):
                previous = identities[key]
                identities[key] = replacement
                with self.assertRaises(probe.ObservationError):
                    target.process(13, Node("/private/app"), b"exact\0")
                identities[key] = previous
            metadata["/proc/13/status"] = b"Uid:\t88\t99\t88\t88\n"
            with self.assertRaises(probe.ObservationError):
                target.process(13, Node("/private/app"), b"exact\0")
            metadata["/proc/13/status"] = b"Uid:\t88\t88\t88\t88\n"
            with self.assertRaises(probe.ObservationError):
                target.process(13, Node("/private/app"), b"different\0")


class ClosedOutputAndTransport(unittest.TestCase):
    def test_real_adapter_queries_fixed_owner_and_exact_child_without_raw_output(self):
        bus = probe.SystemBus.__new__(probe.SystemBus)
        bus.GLib = SimpleNamespace(Variant=lambda signature, values: (signature, values))
        bus.call = Mock(return_value=SimpleNamespace(unpack=lambda: (":1.42",)))
        self.assertEqual(bus.owner(), ":1.42")
        self.assertEqual(bus.call.call_args.args, ("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus", "GetNameOwner", ("(s)", ("org.freedesktop.login1",)), "(s)"))
        bus.call.return_value = SimpleNamespace(unpack=lambda: ("/org/freedesktop/login1/session/c42",))
        self.assertEqual(bus.session(":1.42", 4242), "/org/freedesktop/login1/session/c42")
        self.assertEqual(bus.call.call_args.args, (":1.42", "/org/freedesktop/login1", "org.freedesktop.login1.Manager", "GetSessionByPID", ("(u)", (4242,)), "(o)"))

    def test_only_exact_remote_no_session_error_is_classified_as_absence(self):
        class Error(Exception):
            pass
        bus = probe.SystemBus.__new__(probe.SystemBus)
        bus.GLib = SimpleNamespace(Variant=lambda *_: None, Error=Error)
        remote = Mock(return_value="org.freedesktop.login1.NoSessionForPID")
        bus.Gio = SimpleNamespace(DBusError=SimpleNamespace(get_remote_error=remote))
        bus.call = Mock(side_effect=Error("raw-sensitive-payload"))
        with self.assertRaises(probe.NoSession):
            bus.session(":1.42", 4242)
        for name in ("org.freedesktop.login1.NoSuchUser", "org.freedesktop.DBus.Error.AccessDenied", None):
            remote.return_value = name
            with self.assertRaises(Error):
                bus.session(":1.42", 4242)

    def test_record_schema_is_numeric_bounded_and_cannot_serialize_extra_data(self):
        good = probe.record()
        self.assertLess(len(probe.encode_record(good)), 2048)
        self.assertTrue(all(type(value) is int for value in json.loads(probe.encode_record(good)).values()))
        for key, value in (("status", "raw-error"), ("elapsedMs", 5001), ("active", True), ("samples", 3)):
            bad = dict(good, **{key: value})
            with self.assertRaises(ValueError):
                probe.encode_record(bad)
        with self.assertRaises(ValueError):
            probe.encode_record(dict(good, path="/secret"))

    def test_override_is_refused_before_root_or_system_bus(self):
        # Target imports the existing pure guard, but must not call it or DBus.
        import sys
        for name in probe.OVERRIDES:
            guard = Mock()
            with patch.dict(sys.modules, {"app": SimpleNamespace(isolated_root=guard)}), patch.dict(probe.os.environ, {name: ""}, clear=True), patch.object(probe.os, "getuid", return_value=88):
                with self.assertRaises(probe.ObservationError) as found:
                    probe.Target(12)
                self.assertEqual(found.exception.code, 8)
                guard.assert_not_called()

    def test_call_uses_no_autostart_exact_type_and_remaining_deadline(self):
        bus = probe.SystemBus.__new__(probe.SystemBus)
        bus.deadline, bus.clock, bus.cancel = 4, lambda: 3.25, object()
        bus.Gio = SimpleNamespace(DBusCallFlags=SimpleNamespace(NO_AUTO_START=32))
        bus.GLib = SimpleNamespace(VariantType=SimpleNamespace(new=lambda value: ("type", value)))
        bus.bus = SimpleNamespace(call_sync=Mock(return_value="fixed-result"))
        self.assertEqual(bus.call(":1.42", "/fixed", "fixed.Interface", "Get", None, "(v)"), "fixed-result")
        self.assertEqual(bus.bus.call_sync.call_args.args, (":1.42", "/fixed", "fixed.Interface", "Get", None, ("type", "(v)"), 32, 750, bus.cancel))
        bus.clock = lambda: 4
        with self.assertRaises(probe.ObservationError):
            bus.call(":1.42", "/fixed", "fixed.Interface", "Get", None, "(v)")
        self.assertEqual(bus.bus.call_sync.call_count, 1)

    def test_variant_inner_type_is_checked_before_unpack(self):
        bus = probe.SystemBus.__new__(probe.SystemBus)
        bus.GLib = SimpleNamespace(Variant=lambda *_: None)
        inner = SimpleNamespace(get_type_string=lambda: "s", unpack=Mock(return_value="raw-sensitive"))
        bus.call = Mock(return_value=SimpleNamespace(get_child_value=lambda _: SimpleNamespace(get_variant=lambda: inner)))
        with self.assertRaises(probe.ObservationError):
            bus.property(":1.42", "/fixed", "fixed.Interface", "Active", "b")
        inner.unpack.assert_not_called()


if __name__ == "__main__":
    unittest.main()
