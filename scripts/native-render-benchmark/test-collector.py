#!/usr/bin/python3
"""Synthetic metadata only. Never provisions, launches or samples a real app."""
from contextlib import ExitStack, contextmanager
import ast
import io
import json
import os
from pathlib import Path
import re
import runpy
import secrets
import shutil
import signal
import stat
import subprocess
import sys
import tempfile
import time
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import collect
import compositor_diagnostic
import launch
import runner
import qualification


NAME = "orbitpal-render-0123456789abcdef"
NET = "net:[12345]"

# The Linux production writer rejects every symlink ancestor. Keep synthetic
# temp roots canonical as well (macOS commonly spells TMPDIR through /var).
tempfile.tempdir = str(Path(tempfile.gettempdir()).resolve(strict=True))


def process_stat(*, birth=42, user=7, system=3, rss=9):
    fields = ["0"] * 22
    fields[0] = "S"
    for index, value in ((11, user), (12, system), (19, birth), (21, rss)):
        fields[index] = str(value)
    # A sensitive/awkward comm must be discarded even when it contains ') '.
    return b"123 (PRIVATE ) (COMM)) " + " ".join(fields).encode() + b"\n"


class CollectorTests(unittest.TestCase):
    def test_stat_uses_documented_fields_and_discards_comm(self):
        result = collect.parse_stat(process_stat())
        self.assertEqual(result, {"cpuTicks": 10, "startTicks": 42, "rssPages": 9, "exited": False})
        self.assertNotIn("PRIVATE", json.dumps(result))

    def test_stat_rejects_incomplete_non_numeric_negative_and_oversize(self):
        for raw in (b"invalid", b"1 (x) S 0\n", process_stat(rss=-1), process_stat(birth=2**64), b"x" * 8193):
            with self.assertRaises(collect.Refusal):
                collect.parse_stat(raw)

    def test_pid_lists_are_bounded_unique_positive_numbers_only(self):
        self.assertEqual(collect.numeric_pids(b"12\n42\n"), [12, 42])
        for raw in (b"12\n12\n", b"0\n", b"-1\n", b"2147483648\n", b"2 PRIVATE\n", b"1\n" * 257):
            with self.assertRaises(collect.Refusal):
                collect.numeric_pids(raw)

    def test_scope_parsing_rejects_prefix_spoof_multiline_and_traversal(self):
        self.assertEqual(collect.membership(("0::/" + NAME + "/child\n").encode()), NAME + "/child")
        self.assertTrue(collect.in_domain(NAME + "/child", NAME))
        self.assertFalse(collect.in_domain(NAME + "-other", NAME))
        self.assertFalse(collect.in_domain(NAME + "-compositor", NAME))
        for raw in (b"0::/../other\n", b"0::/x//y\n", b"0::/x\n0::/other\n", b"0::/x", b"1:cpu:/x\n", b"0::/a/b/c/d/e/f\n"):
            with self.assertRaises(collect.Refusal):
                collect.membership(raw)

    def test_fresh_uid_outside_both_domains_is_never_silently_omitted(self):
        for group in ("host_session", NAME + "-other", NAME + "-compositor-other"):
            with patch.object(collect, "bounded", return_value=("0::/" + group + "\n").encode()), patch.object(collect, "sample_process") as sample:
                with self.assertRaisesRegex(collect.Refusal, "^unattributed_fresh_account_process$"):
                    collect.inventory_samples({"123": 42}, set(), 1001, NAME, NAME + "-compositor", NET, "net:[1]")
                sample.assert_not_called()

    def test_late_fresh_uid_app_process_is_added_but_compositor_is_only_validated(self):
        for is_app in (True, False):
            group = NAME if is_app else NAME + "-compositor"
            value = {"pid": 123, "uid": 1001, "startTicks": 42}
            with patch.object(collect, "bounded", return_value=("0::/" + group + "\n").encode()), patch.object(collect, "sample_process", return_value=value) as sample:
                result = collect.inventory_samples({"123": 42}, set(), 1001, NAME, NAME + "-compositor", NET, "net:[1]")
                self.assertEqual(result, [value] if is_app else [])
                self.assertEqual(sample.call_args.args[-1], NET if is_app else "net:[1]")

    def test_sample_accepts_other_helper_uids_in_owned_group_without_filtering(self):
        def read(path, _limit):
            return process_stat() if path.name == "stat" else ("0::/" + NAME + "\n").encode()
        with patch.object(collect, "bounded", side_effect=read), patch.object(collect.Path, "stat", return_value=SimpleNamespace(st_uid=0)), patch.object(collect.Path, "readlink", return_value=Path(NET)):
            result = collect.sample_process(123, None, NAME, NET)
        self.assertEqual(result["uid"], 0)
        self.assertEqual(result["startTicks"], 42)

    def test_sample_rejects_pid_reuse_before_returning_numeric_row(self):
        stats = iter((process_stat(), process_stat(), process_stat(birth=99)))
        def read(path, _limit):
            return next(stats) if path.name == "stat" else ("0::/" + NAME + "\n").encode()
        with patch.object(collect, "bounded", side_effect=read), patch.object(collect.Path, "stat", return_value=SimpleNamespace(st_uid=1001)), patch.object(collect.Path, "readlink", return_value=Path(NET)):
            with self.assertRaisesRegex(collect.Refusal, "^process_identity_changed$"):
                collect.sample_process(123, 1001, NAME, NET)

    def test_sample_rechecks_namespace_and_membership_after_numeric_reads(self):
        for mutate_scope in (False, True):
            scopes = iter((NAME, NAME + "/moved" if mutate_scope else NAME))
            def read(path, _limit):
                return process_stat() if path.name == "stat" else ("0::/" + next(scopes) + "\n").encode()
            links = [Path(NET), Path(NET if mutate_scope else "net:[54321]")]
            with patch.object(collect, "bounded", side_effect=read), patch.object(collect.Path, "stat", return_value=SimpleNamespace(st_uid=1001)), patch.object(collect.Path, "readlink", side_effect=links):
                with self.assertRaises(collect.Refusal):
                    collect.sample_process(123, 1001, NAME, NET)

    def test_sample_disappearance_is_not_a_fabricated_zero_row(self):
        with patch.object(collect.Path, "stat", side_effect=FileNotFoundError("PRIVATE_PATH")):
            self.assertIsNone(collect.sample_process(123, None, NAME, NET))

    def test_zombie_cannot_establish_live_app_or_renderer_readiness(self):
        raw = process_stat().replace(b" S ", b" Z ", 1)
        with patch.object(collect, "bounded", return_value=raw), patch.object(collect.Path, "stat", return_value=SimpleNamespace(st_uid=1001)):
            self.assertTrue(collect.parse_stat(raw)["exited"])
            self.assertIsNone(collect.sample_process(123, 1001, NAME, NET))

    def test_sample_expected_uid_cannot_match_another_process(self):
        with patch.object(collect.Path, "stat", return_value=SimpleNamespace(st_uid=1002)):
            with self.assertRaisesRegex(collect.Refusal, "^unexpected_helper_uid$"):
                collect.sample_process(123, 1001, NAME, NET)

    def test_recursive_scope_counts_children_but_never_neighbor_compositor(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            app = root / NAME
            app.mkdir()
            (app / "cgroup.procs").write_bytes(b"1\n")
            child = app / "child"
            child.mkdir()
            (child / "cgroup.procs").write_bytes(b"2\n3\n")
            other = root / (NAME + "-compositor")
            other.mkdir()
            (other / "cgroup.procs").write_bytes(b"4\n")
            self.assertEqual(collect.scoped_pids(app), {1: NAME, 2: NAME + "/child", 3: NAME + "/child"})

    def test_recursive_scope_has_real_group_and_duplicate_pid_caps(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "cgroup.procs").write_bytes(b"1\n")
            child = root / "child"
            child.mkdir()
            (child / "cgroup.procs").write_bytes(b"1\n")
            with self.assertRaisesRegex(collect.Refusal, "^process_scope_sample_raced$"):
                collect.scoped_pids(root)
            (child / "cgroup.procs").write_bytes(b"")
            for number in range(16):
                item = root / ("node" + str(number))
                item.mkdir()
                (item / "cgroup.procs").write_bytes(b"")
            with self.assertRaisesRegex(collect.Refusal, "^cgroup_tree_limit$"):
                collect.scoped_pids(root)

    def test_symlinked_cgroup_tree_is_not_followed_as_owned(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "cgroup.procs").write_bytes(b"")
            target = root / "actual"
            target.mkdir()
            (target / "cgroup.procs").write_bytes(b"2\n")
            (root / "linked").symlink_to(target, target_is_directory=True)
            with self.assertRaisesRegex(collect.Refusal, "^cgroup_tree_limit$"):
                collect.scoped_pids(root)

    def test_group_cpu_and_charged_memory_are_separate_and_duplicates_fail(self):
        values = {"cpu.stat": b"usage_usec 123\nuser_usec 80\nsystem_usec 43\n", "memory.current": b"456\n"}
        with patch.object(collect, "bounded", side_effect=lambda path, _limit: values[path.name]):
            self.assertEqual(collect.group_numbers(Path("/synthetic")), (123, 456))
            values["cpu.stat"] = b"usage_usec 123\nusage_usec 1\n"
            with self.assertRaisesRegex(collect.Refusal, "^invalid_group_cpu$"):
                collect.group_numbers(Path("/synthetic"))

    def test_first_interval_is_unknown_and_one_core_is_100_percent(self):
        self.assertEqual(collect.cpu_delta(None, 4, 999999), (None, None))
        self.assertEqual(collect.cpu_delta((1, 100), 2, 1000100), (100.0, 1))
        for previous, now, micros in (((2, 100), 1, 200), ((1, 200), 2, 100), ((1, 100), 1, 200)):
            with self.assertRaisesRegex(collect.Refusal, "^group_clock_regressed$"):
                collect.cpu_delta(previous, now, micros)

    def test_scope_ready_marker_is_exact_readable_by_native_uid_and_single_use(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with patch.object(collect.os, "fchown") as chown:
                collect.write_ready(root, 12345)
            self.assertEqual(chown.call_args.args[1:], (12345, -1))
            self.assertEqual((root / "collector.ready").read_bytes(), b"numeric-process-scope-ready\n")
            self.assertEqual(stat.S_IMODE((root / "collector.ready").stat().st_mode), 0o600)
            with self.assertRaises(FileExistsError):
                collect.write_ready(root, 12345)

    def test_metadata_reader_rejects_symlinks_and_real_byte_overflow(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            path = root / "data"
            path.write_bytes(b"12345")
            with self.assertRaisesRegex(collect.Refusal, "^metadata_limit$"):
                collect.bounded(path, 4)
            (root / "link").symlink_to(path)
            with self.assertRaises(OSError):
                collect.bounded(root / "link", 100)

    def test_failure_json_cannot_export_hostile_exception_payloads(self):
        class Hostile(Exception):
            def __str__(self):
                raise AssertionError("exception must not be formatted")
        for error in (Hostile("PRIVATE_SECRET", b"PRIVATE_IMAGE"), collect.Refusal("PRIVATE_PATH")):
            self.assertEqual(collect.failure_summary(error), {"version": 1, "passed": False, "reason": "metadata_unavailable"})
        self.assertEqual(collect.failure_summary(collect.Refusal("process_scope_changed"))["reason"], "process_scope_changed")


class CgroupCpuSchemaTests(unittest.TestCase):
    BASE = b"usage_usec 123\nuser_usec 80\nsystem_usec 43\n"
    FULL = (BASE + b"nice_usec 17\ncore_sched.force_idle_usec 29\nnr_periods 31\n"
            b"nr_throttled 7\nthrottled_usec 41\nnr_bursts 2\nburst_usec 53\n")

    def numbers(self, cpu, memory=b"456\n"):
        def value(path, _limit):
            return cpu if path.name == "cpu.stat" else memory
        with patch.object(collect, "bounded", side_effect=value) as read:
            result = collect.group_numbers(Path("/synthetic"))
            self.assertEqual([call.args for call in read.call_args_list],
                             [(Path("/synthetic/cpu.stat"), 8192), (Path("/synthetic/memory.current"), 64)])
        return result

    def cpu_refusal(self, cpu, reason):
        with patch.object(collect, "bounded", return_value=cpu) as read:
            with self.assertRaisesRegex(collect.Refusal, "^" + reason + "$"):
                collect.group_numbers(Path("/synthetic"))
            self.assertEqual([call.args for call in read.call_args_list], [(Path("/synthetic/cpu.stat"), 8192)])

    def test_base_full_kernel_forms_and_reordered_fields_keep_usage_separate(self):
        reordered = b"\n".join(reversed(self.FULL.splitlines())) + b"\n"
        for cpu in (self.BASE, self.FULL, reordered):
            self.assertEqual(self.numbers(cpu), (123, 456))
        # Neither core-forced-idle/nice time nor CFS counters are added to usage.
        self.assertEqual(self.numbers(self.FULL.replace(b"usage_usec 123", b"usage_usec 0")), (0, 456))

    def test_every_known_value_is_u64_bounded_and_usage_retains_its_signed_limit(self):
        for field in collect.CPU_STAT_FIELDS:
            prefix = b"usage_usec 123\n" if field != b"usage_usec" else b""
            valid = prefix + field + b" " + str(2**64 - 1).encode("ascii") + b"\n"
            if field == b"usage_usec":
                with self.assertRaisesRegex(collect.Refusal, "^group_metadata_limit$"):
                    self.numbers(valid)
            else:
                self.assertEqual(self.numbers(valid), (123, 456))
            for too_large in (str(2**64).encode("ascii"), b"0" * 21):
                self.cpu_refusal(prefix + field + b" " + too_large + b"\n", "group_metadata_limit")

    def test_each_duplicate_key_including_core_is_rejected_before_memory_read(self):
        for field in collect.CPU_STAT_FIELDS:
            prefix = b"usage_usec 123\n" if field != b"usage_usec" else b""
            self.cpu_refusal(prefix + field + b" 1\n" + field + b" 2\n", "invalid_group_cpu")

    def test_unknown_names_and_dotted_spoofs_remain_closed_refusals(self):
        for field in (b"unknown_field", b"PRIVATE_PATH", b"core_sched.force_idle_usecs",
                      b"core_sched.force_idle_usec.extra", b"core-sched.force_idle_usec", b"/private/field"):
            self.cpu_refusal(self.BASE + field + b" 5\n", "invalid_group_cpu")

    def test_missing_usage_never_becomes_an_invented_zero(self):
        for cpu in (b"", b"user_usec 80\nsystem_usec 43\n", self.FULL.split(b"\n", 1)[1]):
            self.cpu_refusal(cpu, "group_cpu_unavailable")

    def test_cpu_rows_require_exact_unsigned_decimal_pairs(self):
        for row in (b"core_sched.force_idle_usec", b"core_sched.force_idle_usec 1 extra",
                    b"core_sched.force_idle_usec -1", b"core_sched.force_idle_usec +1",
                    b"core_sched.force_idle_usec nan", b"core_sched.force_idle_usec inf",
                    b"core_sched.force_idle_usec 1.5", b"core_sched.force_idle_usec 1e2",
                    b"core_sched.force_idle_usec \xff", b""):
            self.cpu_refusal(self.BASE + row + b"\n", "invalid_group_cpu")

    def test_existing_signed_usage_memory_limits_and_read_scope_remain(self):
        largest = str(2**63 - 1).encode("ascii")
        self.assertEqual(self.numbers(b"usage_usec " + largest + b"\n", largest + b"\n"), (2**63 - 1, 2**63 - 1))
        with self.assertRaisesRegex(collect.Refusal, "^group_metadata_limit$"):
            self.numbers(b"usage_usec " + str(2**63).encode("ascii") + b"\n")
        with self.assertRaisesRegex(collect.Refusal, "^group_metadata_limit$"):
            self.numbers(self.BASE, str(2**63).encode("ascii") + b"\n")
        with self.assertRaisesRegex(collect.Refusal, "^group_memory_unavailable$"):
            self.numbers(self.BASE, b"max\n")

    def test_real_cpu_stat_read_keeps_the_existing_8192_byte_limit(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "cpu.stat").write_bytes(self.BASE + b" " * 8192)
            with patch.object(collect, "bounded", wraps=collect.bounded) as read:
                with self.assertRaisesRegex(collect.Refusal, "^metadata_limit$"):
                    collect.group_numbers(root)
                self.assertEqual([call.args for call in read.call_args_list], [(root / "cpu.stat", 8192)])


class CollectorExitDiagnosticsTests(unittest.TestCase):
    def test_exit_manifest_is_literal_unique_nonzero_and_only_authored_refusals(self):
        source = ast.parse(Path(collect.__file__).read_text(encoding="utf-8"))
        entries = [node.value for node in source.body if isinstance(node, ast.Assign)
                   and any(isinstance(target, ast.Name) and target.id == "FAILURE_EXIT_CODES" for target in node.targets)]
        self.assertEqual(len(entries), 1)
        self.assertIsInstance(entries[0], ast.Dict)
        manifest = ast.literal_eval(entries[0])
        authored = {node.args[0].value for node in ast.walk(source) if isinstance(node, ast.Call)
                    and isinstance(node.func, ast.Name) and node.func.id == "refuse" and len(node.args) == 1
                    and isinstance(node.args[0], ast.Constant) and type(node.args[0].value) is str}
        self.assertEqual(len(manifest), 39)
        self.assertEqual(len(entries[0].keys), len(manifest))  # No shadowed literal key.
        self.assertTrue(set(manifest) <= authored)
        self.assertEqual(set(manifest.values()), set(range(64, 103)))
        self.assertTrue(all(type(value) is int for value in manifest.values()))
        self.assertEqual(manifest, collect.FAILURE_EXIT_CODES)
        self.assertEqual(set(manifest), collect.FAILURE_CODES)

    def test_documented_exit_manifest_matches_actual_closed_classification(self):
        document = Path(collect.__file__).with_name("README.md").read_text(encoding="utf-8")
        rows = [tuple(column.strip().strip("`") for column in line.split("|")[1:-1])
                for line in document.splitlines() if line.startswith("|")]
        manifest = {reason: int(code) for code, reason in rows if code.isascii() and code.isdigit() and 64 <= int(code) <= 102}
        self.assertEqual(manifest, collect.FAILURE_EXIT_CODES)
        for reason, code in manifest.items():
            error = collect.Refusal(reason)
            self.assertEqual(collect.failure_summary(error)["reason"], reason)
            self.assertEqual(collect.failure_exit_code(error), code)

    def test_private_unknown_or_forged_errors_never_choose_a_diagnostic_code(self):
        class PrivateError(Exception):
            def __str__(self):
                raise AssertionError("private exception must not be formatted")
        class ForgedRefusal(collect.Refusal):
            pass
        errors = (PrivateError("PRIVATE_SECRET", b"PRIVATE_IMAGE"), OSError("PRIVATE_PATH"),
                  collect.Refusal("PRIVATE_PATH"), collect.Refusal(75), collect.Refusal(True),
                  collect.Refusal("process_scope_changed", "PRIVATE_SECRET"),
                  ForgedRefusal("process_scope_changed"), RuntimeError("process_scope_changed"))
        for error in errors:
            self.assertEqual(collect.failure_exit_code(error), 1)
            self.assertEqual(collect.failure_summary(error)["reason"], "metadata_unavailable")

    def test_actual_main_boundary_classifies_preflight_without_new_file_reads(self):
        hosted = {"GITHUB_ACTIONS": "true", "RUNNER_OS": "Linux", "RUNNER_ENVIRONMENT": "github-hosted"}
        cases = (({}, ["collect.py"], 100), (hosted, ["collect.py"], 101),
                 (hosted, ["collect.py", "--collect", "/not-opened", "1000", "3"], 102))
        for environment, arguments, code in cases:
            with self.subTest(code=code), patch.dict(os.environ, environment, clear=True), patch.object(sys, "argv", arguments), \
                    patch.object(os, "open") as opening, patch.object(sys, "stderr", io.StringIO()) as stderr, \
                    patch.object(sys, "stdout", io.StringIO()) as stdout:
                with self.assertRaises(SystemExit) as stopped:
                    runpy.run_path(collect.__file__, run_name="__main__")
                self.assertEqual(stopped.exception.code, code)
                self.assertEqual(stderr.getvalue(), "benchmark-collector-failed\n")
                self.assertEqual(stdout.getvalue(), "")
                opening.assert_not_called()

    def test_actual_main_boundary_keeps_private_errors_generic_and_base_exceptions_intact(self):
        class PrivateError(Exception):
            def __str__(self):
                raise AssertionError("private exception must not be formatted")
        for error in (PrivateError("PRIVATE_SECRET"), KeyboardInterrupt(), SystemExit(23)):
            def failed_guard(_name):
                raise error
            with patch.object(os, "environ", SimpleNamespace(get=failed_guard)), \
                    patch.object(sys, "stderr", io.StringIO()) as stderr, patch.object(sys, "stdout", io.StringIO()) as stdout:
                with self.assertRaises(SystemExit if type(error) is PrivateError else type(error)) as stopped:
                    runpy.run_path(collect.__file__, run_name="__main__")
                if type(error) is PrivateError:
                    self.assertEqual(stopped.exception.code, 1)
                    self.assertEqual(stderr.getvalue(), "benchmark-collector-failed\n")
                else:
                    self.assertIs(stopped.exception, error)
                    self.assertEqual(stderr.getvalue(), "")
                self.assertEqual(stdout.getvalue(), "")

    @unittest.skipUnless(os.name == "posix", "Linux collector uses POSIX child signal statuses")
    def test_actual_main_boundary_preserves_real_sigint_and_sigterm_child_statuses(self):
        # Only this owned tiny Python child receives a real signal. The real
        # collector boundary runs, but its first guard cannot reach host reads.
        child = """import os, runpy, sys
class InterruptedEnvironment:
    def get(self, _name):
        os.kill(os.getpid(), int(sys.argv[2]))
        raise AssertionError('signal did not interrupt')
os.environ = InterruptedEnvironment()
runpy.run_path(sys.argv[1], run_name='__main__')
"""
        for signum in (signal.SIGINT, signal.SIGTERM):
            with self.subTest(signal=int(signum)):
                result = subprocess.run([sys.executable, "-I", "-c", child, str(Path(collect.__file__).resolve()), str(int(signum))],
                                        stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                        env={}, timeout=10, check=False)
                self.assertEqual(result.returncode, -int(signum))
                self.assertEqual(result.stdout, b"")
                self.assertNotIn(b"benchmark-collector-failed", result.stderr)


class HarnessSafetyTests(unittest.TestCase):
    def test_host_guard_refuses_mac_and_nonhosted_before_provisioning(self):
        for system, hosted in (("darwin", True), ("linux", False)):
            with patch.object(runner.sys, "platform", system), patch.object(runner.os, "geteuid", return_value=0), patch.dict(runner.os.environ, {"GITHUB_ACTIONS": "true" if hosted else "false", "RUNNER_ENVIRONMENT": "github-hosted", "RUNNER_OS": "Linux"}), patch.object(runner, "command") as command:
                with self.assertRaisesRegex(collect.Refusal, "^disposable_hosted_linux_required$"):
                    runner.hosted_root()
                command.assert_not_called()

    def test_app_environment_is_closed_and_does_not_inherit_host_or_secrets(self):
        root = Path("/tmp") / NAME
        with patch.dict(os.environ, {"PRIVATE_SECRET": "PRIVATE_VALUE", "DISPLAY": ":99", "TAURI_CONFIG": "PRIVATE_PATH", "DBUS_SESSION_BUS_ADDRESS": "unix:path=/host/bus"}):
            app = launch.environment(root, "opr0123456789abcdef", "app", "smoke", NET)
            compositor = launch.environment(root, "opr0123456789abcdef", "compositor", "smoke", NET)
        for forbidden in ("DISPLAY", "TAURI_CONFIG", "PRIVATE_SECRET", "SSH_AUTH_SOCK", "XAUTHORITY", "AT_SPI_BUS_ADDRESS"):
            self.assertNotIn(forbidden, app)
        self.assertEqual(app["HOME"], str(root / "home"))
        self.assertEqual(app["XDG_RUNTIME_DIR"], str(root / "runtime"))
        self.assertEqual(app["WAYLAND_DISPLAY"], "orbitpal-benchmark")
        self.assertEqual(app["DBUS_SESSION_BUS_ADDRESS"], "unix:path=" + str(root / "runtime/session-bus"))
        self.assertNotEqual(app["DBUS_SESSION_BUS_ADDRESS"], compositor["DBUS_SESSION_BUS_ADDRESS"])
        self.assertEqual(app["DBUS_SYSTEM_BUS_ADDRESS"], "unix:path=" + str(root / "runtime/nonexistent-system-bus"))
        self.assertEqual(compositor["DBUS_SYSTEM_BUS_ADDRESS"], "unix:path=" + str(root / "compositor/runtime/system-bus"))
        self.assertNotEqual(compositor["DBUS_SYSTEM_BUS_ADDRESS"], compositor["DBUS_SESSION_BUS_ADDRESS"])
        self.assertNotEqual(app["HOME"], compositor["HOME"])
        self.assertNotIn("PRIVATE", json.dumps(app))

    def test_compositor_system_transport_is_empty_private_and_cannot_activate_host_services(self):
        import xml.etree.ElementTree as ET
        address = Path("/tmp") / NAME / "compositor/runtime/system-bus"
        value = ET.fromstring(runner.compositor_system_bus(address))
        self.assertEqual(value.tag, "busconfig")
        self.assertEqual([child.tag for child in value], ["type", "listen", "auth", "policy"])
        self.assertEqual(value.findtext("type"), "system")
        self.assertEqual(value.findtext("listen"), "unix:path=" + str(address))
        self.assertEqual(value.findtext("auth"), "EXTERNAL")
        # No include, standard servicedir, activator, user/host transport or
        # logind implementation. The actual service absence is preserved.
        self.assertEqual(value.findall(".//servicedir"), [])
        self.assertEqual(value.findall(".//include"), [])
        self.assertEqual(value.findall(".//user"), [])
        self.assertNotIn("login1", runner.compositor_system_bus(address))
        for invalid in (Path("/run/dbus/system_bus_socket"), address.parent / "../system-bus",
                        Path("/tmp") / NAME / "runtime/system-bus", Path("/tmp/other/compositor/runtime/system-bus")):
            with self.subTest(path=str(invalid)), self.assertRaisesRegex(collect.Refusal, "^invalid_compositor_bus_address$"):
                runner.compositor_system_bus(invalid)

    @unittest.skipUnless(sys.platform == "linux" and shutil.which("dbus-daemon") and shutil.which("gdbus"),
                         "isolated real D-Bus transport requires Linux packaged helpers")
    def test_real_empty_compositor_bus_has_no_activatable_logind_or_host_service(self):
        # A small unprivileged transport fixture only: no compositor, host bus,
        # accounts, cgroups or system configuration. The production config is
        # exercised with a fresh protected socket and no inherited environment.
        root = Path("/tmp") / ("orbitpal-render-" + secrets.token_hex(8))
        root.mkdir(mode=0o700)
        child = None
        try:
            (root / "compositor").mkdir(mode=0o700)
            runtime = root / "compositor/runtime"
            runtime.mkdir(mode=0o700)
            address = runtime / "system-bus"
            config = root / "bus.conf"
            config.write_text(runner.compositor_system_bus(address), encoding="ascii")
            child = subprocess.Popen([shutil.which("dbus-daemon"), "--nofork", "--config-file=" + str(config)],
                                     stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                                     stderr=subprocess.DEVNULL, env={"PATH": "/usr/bin:/bin"})
            deadline = time.monotonic() + 5
            while not address.is_socket() and time.monotonic() < deadline:
                self.assertIsNone(child.poll())
                time.sleep(.02)
            self.assertTrue(address.is_socket())
            def call(method, *args):
                return subprocess.run([shutil.which("gdbus"), "call", "--address", "unix:path=" + str(address),
                                       "--dest", "org.freedesktop.DBus", "--object-path", "/org/freedesktop/DBus",
                                       "--method", "org.freedesktop.DBus." + method, *args],
                                      stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                      env={"PATH": "/usr/bin:/bin"}, timeout=3, check=False)
            self.assertEqual(call("ListNames").returncode, 0)
            activatable = call("ListActivatableNames")
            self.assertEqual(activatable.returncode, 0)
            # Some D-Bus versions list their own built-in daemon name. No
            # external service may be advertised by this empty configuration.
            self.assertLessEqual(set(re.findall(rb"'([^']+)'", activatable.stdout)), {b"org.freedesktop.DBus"})
            start = call("StartServiceByName", "org.freedesktop.login1", "0")
            self.assertNotEqual(start.returncode, 0)
            self.assertIn(b"ServiceUnknown", start.stderr)
        finally:
            if child is not None and child.poll() is None:
                child.terminate()
                try:
                    child.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait(timeout=3)
            shutil.rmtree(root)

    def test_only_closed_1_3_5_figure_plans_can_enter_the_launcher_or_driver(self):
        root = Path("/tmp") / NAME
        for mode, plan in (("smoke", (1, 30)), ("pilot", (1, 30)), ("pilot3", (3, 30)), ("pilot5", (5, 20))):
            self.assertEqual(collect.measurement_mode(mode), plan)
            self.assertEqual(launch.environment(root, "opr0123456789abcdef", "app", mode, NET)["ORBITPAL_BENCH_MODE"], mode)
        for mode in (None, True, 5, "pilot2", "pilot50", "pilot5 ", "pilot/../../outside", "--command=PRIVATE"):
            with self.subTest(mode=mode), patch.object(runner.secrets, "token_hex") as nonce:
                with self.assertRaisesRegex(collect.Refusal, "^invalid_benchmark_mode$"):
                    runner.execute(mode, Path("/synthetic/native-bench"), Path("/synthetic/output"))
                nonce.assert_not_called()
                with self.assertRaisesRegex(collect.Refusal, "^invalid_benchmark_mode$"):
                    launch.environment(root, "opr0123456789abcdef", "app", mode, NET)

    def test_pilot_requires_successful_manual_smoke_for_same_exact_source_and_repository(self):
        sha, run_id = "a" * 40, "123456789"
        run = {"id": int(run_id), "event": "workflow_dispatch", "status": "completed", "conclusion": "success",
               "head_sha": sha, "path": qualification.SMOKE_PATH,
               "repository": {"full_name": qualification.REPOSITORY}, "head_repository": {"full_name": qualification.REPOSITORY}}
        self.assertTrue(qualification.qualified_smoke(run, sha, run_id))
        for key, value in (("id", True), ("id", run_id), ("id", 123456788), ("event", "push"),
                           ("status", "in_progress"), ("conclusion", "failure"), ("head_sha", "b" * 40),
                           ("path", ".github/workflows/ci.yml"), ("repository", {"full_name": "other/orbitpal"}),
                           ("head_repository", {"full_name": "other/orbitpal"})):
            with self.subTest(field=key):
                self.assertFalse(qualification.qualified_smoke({**run, key: value}, sha, run_id))
        for wrong in (None, [], True, "PRIVATE"):
            self.assertFalse(qualification.qualified_smoke(wrong, sha, run_id))
        for wrong_sha, wrong_id in (("A" * 40, run_id), (sha, "01"), (sha, "123/../../other")):
            self.assertFalse(qualification.qualified_smoke(run, wrong_sha, wrong_id))

    def test_bus_entry_is_only_exact_packaged_a11y_or_dconf_without_delegation(self):
        raw = b"[D-BUS Service]\nName=org.a11y.Bus\nExec=/usr/libexec/at-spi-bus-launcher\nSystemdService=PRIVATE.service\nUser=root\n"
        executable, result = runner.service_entry("org.a11y.Bus", raw)
        self.assertEqual(executable, "/usr/libexec/at-spi-bus-launcher")
        self.assertEqual(result, "[D-BUS Service]\nName=org.a11y.Bus\nExec=/usr/libexec/at-spi-bus-launcher\n")
        self.assertNotIn("PRIVATE", result)
        for service, value in (("org.freedesktop.secrets", raw),
                               ("org.a11y.Bus", raw.replace(b"Exec=/usr/libexec/at-spi-bus-launcher", b"Exec=/bin/sh -c PRIVATE")),
                               ("org.a11y.Bus", raw.replace(b"Name=org.a11y.Bus", b"Name=another.service"))):
            with self.assertRaises(collect.Refusal):
                runner.service_entry(service, value)

    def test_bus_refusal_classifies_only_the_exact_service_metadata_without_reading_it(self):
        cases = [(stat.S_IFLNK | 0o777, 0), (stat.S_IFREG | 0o666, 0), (stat.S_IFREG | 0o644, 1001)]
        for service in ("org.a11y.Bus",):
            for mode, uid in cases:
                with patch.object(runner.Path, "lstat", return_value=SimpleNamespace(st_mode=mode, st_uid=uid)), patch.object(runner, "bounded") as read:
                    with self.assertRaises(runner.BusServiceRefusal) as caught:
                        runner.checked_service(service)
                    read.assert_not_called()
                report = caught.exception.diagnostic
                self.assertEqual(set(report), {"serviceCode", "regularFile", "symbolicLink", "rootOwned", "writeBits"})
                self.assertEqual(report["serviceCode"], list(runner.SERVICES).index(service) + 1)
                self.assertEqual(report["writeBits"], mode & 0o022)
                self.assertEqual(str(caught.exception), "untrusted_bus_service")

    def test_bus_check_retains_exact_exec_and_native_file_trust_guards(self):
        service = "org.a11y.Bus"
        raw = b"[D-BUS Service]\nName=org.a11y.Bus\nExec=/usr/libexec/at-spi-bus-launcher\n"
        regular = SimpleNamespace(st_mode=stat.S_IFREG | 0o644, st_uid=0)
        with patch.object(runner.Path, "lstat", return_value=regular), patch.object(runner, "bounded", return_value=raw), patch.object(runner.Path, "stat", return_value=regular):
            self.assertEqual(runner.checked_service(service), raw.decode("ascii"))
        with patch.object(runner.Path, "lstat") as lookup:
            with self.assertRaises(collect.Refusal):
                runner.checked_service("unknown.PRIVATE")
            lookup.assert_not_called()

    def test_service_preflight_is_read_only_and_refuses_before_any_workload(self):
        info = SimpleNamespace(st_mode=stat.S_IFLNK | 0o777, st_uid=0)
        failure = runner.BusServiceRefusal("ca.desrt.dconf", info)
        with patch.object(runner.sys, "argv", ["runner.py", "--check-services"]), patch.object(runner, "hosted_root") as hosted, patch.object(runner.os, "umask"), patch.object(runner, "trusted_sources") as trusted, patch.object(runner, "checked_service", side_effect=failure), patch.object(runner, "execute") as execute, patch.object(runner.subprocess, "run") as process, patch("sys.stdout", new_callable=io.StringIO) as out:
            self.assertEqual(runner.main(), 1)
            hosted.assert_called_once(); trusted.assert_called_once()
            execute.assert_not_called(); process.assert_not_called()
        value = json.loads(out.getvalue())
        self.assertFalse(value["passed"])
        self.assertEqual(value["busService"], failure.diagnostic)
        self.assertNotIn("ca.desrt", out.getvalue())

    def test_service_preflight_checks_both_fixed_services_without_creating_an_app(self):
        with patch.object(runner.sys, "argv", ["runner.py", "--check-services"]), patch.object(runner, "hosted_root"), patch.object(runner.os, "umask"), patch.object(runner, "trusted_sources"), patch.object(runner, "checked_service", return_value="fixed") as check, patch.object(runner, "execute") as execute, patch("sys.stdout", new_callable=io.StringIO) as out:
            self.assertEqual(runner.main(), 0)
            self.assertEqual([call.args[0] for call in check.call_args_list], list(runner.SERVICES))
            execute.assert_not_called()
        self.assertEqual(json.loads(out.getvalue()), {"passed": True, "servicesChecked": 2})

    def test_numeric_export_accepts_only_metadata_and_keeps_all_numeric_rows(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "fixture"
            out = Path(temporary) / "output"
            root.mkdir()
            out.mkdir()
            rows = [{"nativeMs": 1, "event": {"kind": "batch", "histogram": [0] * 257}},
                    {"nativeMs": 2, "event": {"kind": "completed", "live": 0}}]
            raw = "".join(json.dumps(row) + "\n" for row in rows).encode()
            (root / "native.ndjson").write_bytes(raw)
            (root / "app-output.txt").write_bytes(b"PRIVATE_RAW_MESSAGE")
            with collect.PinnedDirectory(root) as source:
                runner.export_numeric(source, out, native_uid=os.getuid(), collector_uid=os.getuid(), writers_stopped=True)
            self.assertEqual((out / "native.ndjson").read_bytes(), raw)
            self.assertEqual([entry.name for entry in out.iterdir()], ["native.ndjson"])

    def test_numeric_export_rejects_paths_raw_messages_and_nonfinite_or_deep_data(self):
        for value in ({"path": "/PRIVATE/HOME"}, {"message": "PRIVATE raw exception"}, {"value": float("nan")}, {"x": float("inf")}, [0] * 513):
            with self.assertRaises(collect.Refusal):
                runner.numeric_tree(value)
        nested = 1
        for _ in range(14):
            nested = [nested]
        with self.assertRaisesRegex(collect.Refusal, "^artifact_depth_limit$"):
            runner.numeric_tree(nested)
        runner.numeric_tree({"cpuDefinition": "100_percent_is_one_logical_cpu", "value": None})

    def test_cleanup_uses_owned_inode_and_never_touches_a_replacement_group(self):
        fake = SimpleNamespace(lstat=lambda: SimpleNamespace(st_ino=99, st_uid=0))
        with patch.object(runner, "wait_for") as wait:
            with self.assertRaisesRegex(collect.Refusal, "^cleanup_scope_changed$"):
                runner.remove_group(fake, 42)
            wait.assert_not_called()

    def test_cleanup_wait_is_bounded_but_not_abandoned_by_original_signal(self):
        clock = SimpleNamespace(now=0.0)
        def sleep(seconds):
            clock.now += seconds
        calls = 0
        def done():
            nonlocal calls
            calls += 1
            return calls >= 3
        with patch.object(runner, "_interrupted", True), patch.object(runner.time, "monotonic", side_effect=lambda: clock.now), patch.object(runner.time, "sleep", side_effect=sleep):
            runner.wait_for(done, 1, honor_interrupt=False)
            with self.assertRaisesRegex(collect.Refusal, "^fixture_wait_expired$"):
                runner.wait_for(lambda: False, 1)
        self.assertEqual(calls, 3)


class PrivilegedBoundaryTests(unittest.TestCase):
    def test_exclusive_summary_rejects_symlink_and_regular_file_without_overwrite(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            sentinel = root / "sentinel"
            sentinel.write_text("unchanged")
            summary = root / "collector-summary.json"
            with collect.PinnedDirectory(root) as directory:
                summary.symlink_to(sentinel)
                with self.assertRaises(FileExistsError):
                    directory.write_new(summary.name, "hostile")
                self.assertEqual(sentinel.read_text(), "unchanged")
                summary.unlink()
                summary.write_text("previous")
                with self.assertRaises(FileExistsError):
                    directory.write_new(summary.name, "hostile")
                self.assertEqual(summary.read_text(), "previous")

    def test_directory_symlink_and_traversing_leaf_never_create_external_file(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            external = root / "external"
            external.mkdir()
            linked = root / "linked"
            linked.symlink_to(external, target_is_directory=True)
            with self.assertRaises(OSError):
                collect.PinnedDirectory(linked)
            with collect.PinnedDirectory(root) as directory:
                for leaf in ("../sentinel", "external/sentinel", "/sentinel"):
                    with self.assertRaisesRegex(collect.Refusal, "^invalid_write_leaf$"):
                        directory.write_new(leaf, "hostile")
            self.assertEqual(list(external.iterdir()), [])

    def test_replaced_parent_refuses_without_reopening_new_directory(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            parent = root / "parent"
            parent.mkdir()
            child = parent / "child"
            child.mkdir()
            with collect.PinnedDirectory(child) as directory:
                parent.rename(root / "old-parent")
                parent.mkdir()
                (parent / "child").mkdir()
                with self.assertRaisesRegex(collect.Refusal, "^write_directory_changed$"):
                    directory.write_new("collector-summary.json", "hostile")
            self.assertFalse((parent / "child/collector-summary.json").exists())
            self.assertFalse((root / "old-parent/child/collector-summary.json").exists())

    def test_parent_swap_between_check_and_open_remains_bound_to_original_fd(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            parent, external = root / "parent", root / "external"
            parent.mkdir()
            external.mkdir()
            sentinel = external / "collector-summary.json"
            sentinel.write_text("unchanged")
            real_open = collect.os.open
            with collect.PinnedDirectory(parent) as directory:
                def swap(leaf, flags, *args, **kwargs):
                    parent.rename(root / "retired")
                    parent.symlink_to(external, target_is_directory=True)
                    return real_open(leaf, flags, *args, **kwargs)
                with patch.object(collect.os, "open", side_effect=swap):
                    with self.assertRaisesRegex(collect.Refusal, "^write_directory_changed$"):
                        directory.write_new("collector-summary.json", "hostile")
            self.assertEqual(sentinel.read_text(), "unchanged")
            self.assertEqual((root / "retired/collector-summary.json").read_bytes(), b"")

    def test_replaced_leaf_after_open_cannot_redirect_descriptor_write(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            sentinel = root / "sentinel"
            sentinel.write_text("unchanged")
            with collect.PinnedDirectory(root) as directory:
                with directory.open_new("collector-summary.json") as output:
                    (root / "collector-summary.json").unlink()
                    (root / "collector-summary.json").symlink_to(sentinel)
                    output.write("bound_to_unlinked_original")
                    output.flush()
                    with self.assertRaisesRegex(collect.Refusal, "^write_leaf_changed$"):
                        directory.linked_file("collector-summary.json", output.fileno())
            self.assertEqual(sentinel.read_text(), "unchanged")

    def test_inherited_directory_identity_cannot_be_replaced_by_same_named_root(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            actual = root / "fixture"
            actual.mkdir()
            with collect.PinnedDirectory(actual) as before:
                actual.rename(root / "retired")
                actual.mkdir()
                with self.assertRaisesRegex(collect.Refusal, "^write_directory_changed$"):
                    collect.PinnedDirectory(actual, before.fd)
            self.assertEqual(list(actual.iterdir()), [])

    @staticmethod
    def model():
        name, uid, gid, home = "opr0123456789abcdef", 23456, 23457, "/tmp/fixture/home"
        account = SimpleNamespace(pw_name=name, pw_uid=uid, pw_gid=gid, pw_dir=home, pw_shell="/usr/sbin/nologin")
        group = SimpleNamespace(gr_name=name, gr_gid=gid, gr_mem=[])
        state = SimpleNamespace(account=account, group=group)
        def lookup(kind):
            def get(_key):
                value = getattr(state, kind)
                if value is None:
                    raise KeyError(_key)
                return value
            return get
        return name, home, state, lookup

    def test_occupied_numeric_uid_revokes_before_any_workload_or_cleanup_authority(self):
        name, home, state, lookup = self.model()
        with patch.object(runner.pwd, "getpwnam", side_effect=lookup("account")), patch.object(runner.pwd, "getpwuid", side_effect=lookup("account")), patch.object(runner.grp, "getgrnam", side_effect=lookup("group")), patch.object(runner.grp, "getgrgid", side_effect=lookup("group")), patch.object(runner, "uid_inventory", return_value={"88": 42}), patch.object(runner, "command") as command, patch.object(runner.subprocess, "run") as kill:
            identity = runner.FixtureIdentity(name, home)
            with self.assertRaisesRegex(collect.Refusal, "^fixture_uid_already_live$"):
                identity.prove_vacant()
            self.assertFalse(identity.vacant)
            self.assertTrue(identity.revoked)
            with self.assertRaises(collect.Refusal):
                runner.cleanup_identity(identity)
            command.assert_not_called()
            kill.assert_not_called()

    def test_missing_or_failed_vacancy_proof_never_allows_uid_or_name_cleanup(self):
        name, home, state, lookup = self.model()
        with patch.object(runner.pwd, "getpwnam", side_effect=lookup("account")), patch.object(runner.pwd, "getpwuid", side_effect=lookup("account")), patch.object(runner.grp, "getgrnam", side_effect=lookup("group")), patch.object(runner.grp, "getgrgid", side_effect=lookup("group")), patch.object(runner, "uid_inventory", side_effect=PermissionError("synthetic")), patch.object(runner, "command") as command, patch.object(runner.subprocess, "run") as kill:
            for prove in (False, True):
                identity = runner.FixtureIdentity(name, home)
                if prove:
                    with self.assertRaises(PermissionError):
                        identity.prove_vacant()
                with self.assertRaises(collect.Refusal):
                    runner.cleanup_identity(identity)
            with self.assertRaises(collect.Refusal):
                runner.cleanup_identity(None)
            command.assert_not_called()
            kill.assert_not_called()

    def test_changed_account_group_home_or_numeric_alias_never_kills_or_deletes(self):
        for field, value in (("pw_uid", 34567), ("pw_gid", 34568), ("pw_dir", "/replacement/home"), ("pw_shell", "/bin/sh"), ("gr_gid", 34568), ("gr_mem", ["foreign"])):
            name, home, state, lookup = self.model()
            with patch.object(runner.pwd, "getpwnam", side_effect=lookup("account")), patch.object(runner.pwd, "getpwuid", side_effect=lookup("account")), patch.object(runner.grp, "getgrnam", side_effect=lookup("group")), patch.object(runner.grp, "getgrgid", side_effect=lookup("group")), patch.object(runner, "uid_inventory", return_value={}), patch.object(runner, "command") as command, patch.object(runner.subprocess, "run") as kill:
                identity = runner.FixtureIdentity(name, home)
                identity.prove_vacant()
                target = state.account if field.startswith("pw_") else state.group
                before = getattr(target, field)
                setattr(target, field, value)
                with self.assertRaises(collect.Refusal):
                    runner.cleanup_identity(identity)
                setattr(target, field, before)
                with self.assertRaises(collect.Refusal):
                    runner.cleanup_identity(identity)  # Refusal remains sticky after ABA.
                command.assert_not_called()
                kill.assert_not_called()
        name, home, state, lookup = self.model()
        with patch.object(runner.pwd, "getpwnam", side_effect=lookup("account")), patch.object(runner.pwd, "getpwuid", side_effect=lookup("account")), patch.object(runner.grp, "getgrnam", side_effect=lookup("group")), patch.object(runner.grp, "getgrgid", side_effect=lookup("group")), patch.object(runner, "uid_inventory", return_value={}), patch.object(runner, "command") as command, patch.object(runner.subprocess, "run") as kill:
            identity = runner.FixtureIdentity(name, home)
            identity.prove_vacant()
            with patch.object(runner.pwd, "getpwuid", return_value=SimpleNamespace(pw_name="foreign")):
                with self.assertRaises((AttributeError, collect.Refusal)):
                    runner.cleanup_identity(identity)
            command.assert_not_called()
            kill.assert_not_called()

    def test_replacement_during_inventory_is_rechecked_before_uid_wide_kill(self):
        name, home, state, lookup = self.model()
        with patch.object(runner.pwd, "getpwnam", side_effect=lookup("account")), patch.object(runner.pwd, "getpwuid", side_effect=lookup("account")), patch.object(runner.grp, "getgrnam", side_effect=lookup("group")), patch.object(runner.grp, "getgrgid", side_effect=lookup("group")), patch.object(runner, "uid_inventory", return_value={}) as inventory, patch.object(runner, "command") as command, patch.object(runner.subprocess, "run") as kill:
            identity = runner.FixtureIdentity(name, home)
            identity.prove_vacant()
            def changed(_uid):
                state.account.pw_uid = 34567
                return {"88": 42}
            inventory.side_effect = changed
            with self.assertRaisesRegex(collect.Refusal, "^cleanup_identity_changed$"):
                runner.cleanup_identity(identity)
            command.assert_not_called()
            kill.assert_not_called()

    def test_replacement_after_empty_drain_never_reaches_userdel(self):
        name, home, state, lookup = self.model()
        with patch.object(runner.pwd, "getpwnam", side_effect=lookup("account")), patch.object(runner.pwd, "getpwuid", side_effect=lookup("account")), patch.object(runner.grp, "getgrnam", side_effect=lookup("group")), patch.object(runner.grp, "getgrgid", side_effect=lookup("group")), patch.object(runner, "uid_inventory", return_value={}) as inventory, patch.object(runner, "command") as command, patch.object(runner.subprocess, "run") as kill:
            identity = runner.FixtureIdentity(name, home)
            identity.prove_vacant()
            calls = 0
            def changed(_uid):
                nonlocal calls
                calls += 1
                if calls == 2:
                    state.account.pw_dir = "/replacement/home"
                return {}
            inventory.side_effect = changed
            with self.assertRaisesRegex(collect.Refusal, "^cleanup_identity_changed$"):
                runner.cleanup_identity(identity)
            command.assert_not_called()
            kill.assert_not_called()

    def test_successful_exact_cleanup_and_group_replacement_after_userdel(self):
        for replace_group, recreate_account, automatic_group in ((False, False, False), (True, False, False), (False, True, False), (False, False, True)):
            name, home, state, lookup = self.model()
            with patch.object(runner.pwd, "getpwnam", side_effect=lookup("account")), patch.object(runner.pwd, "getpwuid", side_effect=lookup("account")), patch.object(runner.grp, "getgrnam", side_effect=lookup("group")), patch.object(runner.grp, "getgrgid", side_effect=lookup("group")), patch.object(runner, "uid_inventory", return_value={}), patch.object(runner, "command") as command, patch.object(runner.subprocess, "run") as kill:
                identity = runner.FixtureIdentity(name, home)
                identity.prove_vacant()
                original = state.account
                def remove(argv):
                    if argv[0] == "/usr/sbin/userdel":
                        state.account = None
                        if replace_group:
                            state.group.gr_gid += 1
                        if recreate_account:
                            state.account = original
                        if automatic_group:
                            state.group = None
                    else:
                        self.assertEqual(argv, ["/usr/sbin/groupdel", name])
                        state.group = None
                command.side_effect = remove
                if replace_group or recreate_account:
                    with self.assertRaisesRegex(collect.Refusal, "^cleanup_identity_changed$"):
                        runner.cleanup_identity(identity)
                    self.assertEqual(command.call_args_list, [unittest.mock.call(["/usr/sbin/userdel", name])])
                else:
                    self.assertFalse(runner.cleanup_identity(identity))
                    self.assertEqual(command.call_count, 1 if automatic_group else 2)
                kill.assert_not_called()

    def test_occupied_uid_aborts_execute_before_chown_or_first_workload(self):
        # Exercise execute's real ordering with all external operations replaced.
        # Even on the first provisioning failure, no real user/path/process is used.
        events = []
        name, home, state, lookup = self.model()
        nonce = name.removeprefix("opr")
        state.account.pw_dir = "/tmp/orbitpal-render-" + nonce + "/home"
        def inventory(_uid):
            events.append("vacancy")
            return {"88": 42}
        with tempfile.TemporaryDirectory() as temporary:
            out = Path(temporary).resolve()
            with patch.object(runner.secrets, "token_hex", return_value=nonce), patch.object(runner.Path, "mkdir"), patch.object(runner.pwd, "getpwnam", side_effect=[KeyError(name)] + [state.account] * 20), patch.object(runner.pwd, "getpwuid", return_value=state.account), patch.object(runner.grp, "getgrnam", side_effect=[KeyError(name)] + [state.group] * 20), patch.object(runner.grp, "getgrgid", return_value=state.group), patch.object(runner, "uid_inventory", side_effect=inventory), patch.object(runner.os, "chown") as chown, patch.object(runner, "command") as command, patch.object(runner.subprocess, "Popen") as workload, patch.object(runner.subprocess, "run") as kill, patch.object(runner, "export_numeric"), patch.object(runner.shutil, "rmtree"):
                self.assertEqual(runner.execute("smoke", Path("/synthetic/native-bench"), out), 1)
                self.assertEqual(events, ["vacancy"])
                self.assertEqual(command.call_args_list, [unittest.mock.call(["/usr/sbin/useradd", "--user-group", "--no-create-home", "--home-dir", state.account.pw_dir, "--shell", "/usr/sbin/nologin", name])])
                workload.assert_not_called()
                kill.assert_not_called()
                chown.assert_called()  # Only output ownership transfer after refusal.
                self.assertFalse(any(str(call.args[0]).startswith("/tmp/orbitpal-render-") for call in chown.call_args_list))


    def test_partial_useradd_is_not_adopted_for_any_destructive_cleanup(self):
        name, home, state, lookup = self.model()
        nonce = name.removeprefix("opr")
        with tempfile.TemporaryDirectory() as temporary:
            out = Path(temporary).resolve()
            with patch.object(runner.secrets, "token_hex", return_value=nonce), patch.object(runner.Path, "mkdir"), patch.object(runner.pwd, "getpwnam", side_effect=KeyError(name)) as account, patch.object(runner.grp, "getgrnam", side_effect=KeyError(name)) as group, patch.object(runner, "uid_inventory") as inventory, patch.object(runner.os, "chown"), patch.object(runner, "command", side_effect=collect.Refusal("fixture_command_failed")) as command, patch.object(runner.subprocess, "Popen") as workload, patch.object(runner.subprocess, "run") as kill, patch.object(runner, "export_numeric"), patch.object(runner.shutil, "rmtree"):
                self.assertEqual(runner.execute("smoke", Path("/synthetic/native-bench"), out), 1)
                self.assertEqual(account.call_count, 1)
                self.assertEqual(group.call_count, 1)
                self.assertEqual(command.call_count, 1)
                self.assertEqual(command.call_args.args[0][0], "/usr/sbin/useradd")
                inventory.assert_not_called()
                workload.assert_not_called()
                kill.assert_not_called()
                result = json.loads((out / "fixture-result.json").read_text())
                self.assertFalse(result["cleanupConfirmed"])

    def test_proven_exact_uid_kill_drains_then_deletes_only_the_retained_identity(self):
        name, home, state, lookup = self.model()
        with patch.object(runner.pwd, "getpwnam", side_effect=lookup("account")), patch.object(runner.pwd, "getpwuid", side_effect=lookup("account")), patch.object(runner.grp, "getgrnam", side_effect=lookup("group")), patch.object(runner.grp, "getgrgid", side_effect=lookup("group")), patch.object(runner, "uid_inventory", side_effect=[{}, {"88": 42}, {}, {}]), patch.object(runner, "command") as command, patch.object(runner.subprocess, "run") as kill:
            identity = runner.FixtureIdentity(name, home)
            identity.prove_vacant()
            def remove(argv):
                self.assertEqual(argv, ["/usr/sbin/userdel", name])
                state.account = state.group = None  # Real userdel may remove both.
            command.side_effect = remove
            self.assertTrue(runner.cleanup_identity(identity))
            self.assertEqual(kill.call_count, 1)
            self.assertEqual(kill.call_args.args[0], ["/usr/bin/pkill", "-KILL", "-u", "23456"])
            command.assert_called_once_with(["/usr/sbin/userdel", name])

    def test_group_is_rechecked_again_immediately_before_groupdel(self):
        name, home, state, lookup = self.model()
        with patch.object(runner.pwd, "getpwnam", side_effect=lookup("account")), patch.object(runner.pwd, "getpwuid", side_effect=lookup("account")), patch.object(runner.grp, "getgrnam", side_effect=lookup("group")), patch.object(runner.grp, "getgrgid", side_effect=lookup("group")) as group_id, patch.object(runner, "uid_inventory", return_value={}), patch.object(runner, "command") as command, patch.object(runner.subprocess, "run") as kill:
            identity = runner.FixtureIdentity(name, home)
            identity.prove_vacant()
            def remove(argv):
                self.assertEqual(argv, ["/usr/sbin/userdel", name])
                state.account = None
                def changed(key):
                    old = state.group
                    state.group = SimpleNamespace(gr_name="replacement", gr_gid=34567, gr_mem=[])
                    return old
                group_id.side_effect = changed
            command.side_effect = remove
            with self.assertRaisesRegex(collect.Refusal, "^cleanup_identity_changed$"):
                runner.cleanup_identity(identity)
            command.assert_called_once_with(["/usr/sbin/userdel", name])
            kill.assert_not_called()


class ArtifactReadBoundaryTests(unittest.TestCase):
    @staticmethod
    def exporter(source, output, **kwargs):
        return runner.export_numeric(source, output, native_uid=os.getuid(),
                                     collector_uid=os.getuid(), writers_stopped=True, **kwargs)

    @staticmethod
    def mutate_after_read(real_fdopen, mutation):
        @contextmanager
        def opened(*args, **kwargs):
            with real_fdopen(*args, **kwargs) as stream:
                def read(limit):
                    value = stream.read(limit)
                    mutation()
                    return value
                yield SimpleNamespace(read=read)
        return opened

    def test_unpinned_path_and_unconfirmed_writers_never_open_artifacts(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            output = root / "output"
            output.mkdir()
            with collect.PinnedDirectory(root) as source:
                with patch.object(collect.os, "open") as opened:
                    with self.assertRaisesRegex(collect.Refusal, "^pinned_artifact_source_required$"):
                        self.exporter(root, output)
                    for stopped in (False, None, 1):
                        with self.assertRaisesRegex(collect.Refusal, "^artifact_writers_not_stopped$"):
                            runner.export_numeric(source, output, native_uid=os.getuid(), writers_stopped=stopped)
                    opened.assert_not_called()
            self.assertEqual(list(output.iterdir()), [])

    def test_private_json_ancestor_substitution_cannot_export_accepted_token_shape(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            actual, private, output = root / "fixture", root / "private", root / "output"
            for directory in (actual, private, output):
                directory.mkdir()
            raw = json.dumps({"token": "ab" * 32}).encode("ascii")
            runner.numeric_tree(json.loads(raw))  # Shape checking is not secret filtering.
            sentinel = private / "collector-summary.json"
            sentinel.write_bytes(raw)
            with collect.PinnedDirectory(actual) as source:
                actual.rename(root / "retired")
                actual.symlink_to(private, target_is_directory=True)
                with patch.object(collect.os, "fdopen") as read:
                    with self.assertRaisesRegex(collect.Refusal, "^write_directory_changed$"):
                        self.exporter(source, output)
                    read.assert_not_called()
            self.assertEqual(sentinel.read_bytes(), raw)
            self.assertEqual(list(output.iterdir()), [])

    def test_ancestor_swap_between_validation_and_open_cannot_read_private_target(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            actual, private, output = root / "fixture", root / "private", root / "output"
            for directory in (actual, private, output):
                directory.mkdir()
            (actual / "native.ndjson").write_bytes(b'{"count":1}\n')
            sentinel = private / "native.ndjson"
            sentinel.write_bytes(b'{"token":"' + b'a' * 64 + b'"}\n')
            original = sentinel.read_bytes()
            real_open = collect.os.open
            with collect.PinnedDirectory(actual) as source:
                def swapped(name, flags, *args, **kwargs):
                    self.assertEqual(name, "native.ndjson")
                    self.assertEqual(kwargs["dir_fd"], source.fd)
                    self.assertTrue(flags & os.O_NOFOLLOW)
                    self.assertTrue(flags & os.O_NONBLOCK)
                    actual.rename(root / "retired")
                    actual.symlink_to(private, target_is_directory=True)
                    return real_open(name, flags, *args, **kwargs)
                with patch.object(collect.os, "open", side_effect=swapped), patch.object(collect.os, "fdopen") as read:
                    with self.assertRaisesRegex(collect.Refusal, "^write_directory_changed$"):
                        self.exporter(source, output)
                    read.assert_not_called()
            self.assertEqual(sentinel.read_bytes(), original)
            self.assertEqual(list(output.iterdir()), [])

    def test_leaf_symlink_and_hardlink_private_sentinels_are_never_read(self):
        for kind in ("symlink", "hardlink"):
            with tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary).resolve()
                actual, output = root / "fixture", root / "output"
                actual.mkdir()
                output.mkdir()
                sentinel = root / "private"
                raw = b'{"token":"' + b'b' * 64 + b'"}\n'
                sentinel.write_bytes(raw)
                leaf = actual / "native.ndjson"
                if kind == "symlink":
                    leaf.symlink_to(sentinel)
                else:
                    os.link(sentinel, leaf)
                with collect.PinnedDirectory(actual) as source:
                    with patch.object(collect.os, "fdopen") as read:
                        with self.assertRaises((OSError, collect.Refusal)):
                            self.exporter(source, output)
                        read.assert_not_called()
                self.assertEqual(sentinel.read_bytes(), raw)
                self.assertEqual(list(output.iterdir()), [])

    def test_expected_native_and_collector_owners_are_distinct_and_checked_before_export(self):
        for native_wrong in (False, True):
            with tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary).resolve()
                actual, output = root / "fixture", root / "output"
                actual.mkdir()
                output.mkdir()
                (actual / "native.ndjson").write_bytes(b'{"count":1}\n')
                (actual / "process.ndjson").write_bytes(b'{"count":2}\n')
                with collect.PinnedDirectory(actual) as source:
                    with self.assertRaisesRegex(collect.Refusal, "^artifact_read_identity$"):
                        runner.export_numeric(source, output, native_uid=os.getuid() + int(native_wrong),
                                              collector_uid=os.getuid() + int(not native_wrong), writers_stopped=True)
                # Even a preceding valid native artifact is only staged, not copied.
                self.assertEqual(list(output.iterdir()), [])

    def test_regular_file_size_bound_is_checked_before_any_content_read(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            (root / "native.ndjson").write_bytes(b'12345')
            with collect.PinnedDirectory(root) as source:
                with patch.object(collect.os, "fdopen") as read:
                    with self.assertRaisesRegex(collect.Refusal, "^artifact_read_size$"):
                        source.read_artifact("native.ndjson", os.getuid(), 4)
                    read.assert_not_called()

    def test_nonregular_fifo_is_nonblocking_and_never_read(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            os.mkfifo(root / "native.ndjson", 0o600)
            with collect.PinnedDirectory(root) as source:
                with patch.object(collect.os, "fdopen") as read:
                    with self.assertRaisesRegex(collect.Refusal, "^artifact_read_identity$"):
                        source.read_artifact("native.ndjson", os.getuid(), 256)
                    read.assert_not_called()

    def test_size_or_leaf_replacement_during_read_exports_no_original_bytes(self):
        for replace in (False, True):
            with tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary).resolve()
                actual, output = root / "fixture", root / "output"
                actual.mkdir()
                output.mkdir()
                leaf = actual / "native.ndjson"
                leaf.write_bytes(b'{"count":1}\n')
                sentinel = root / "private"
                raw = b'{"token":"' + b'c' * 64 + b'"}\n'
                sentinel.write_bytes(raw)
                def changed():
                    if replace:
                        leaf.unlink()
                        leaf.symlink_to(sentinel)
                    else:
                        with leaf.open("ab") as changed_file:
                            changed_file.write(b' ')
                real_fdopen = collect.os.fdopen
                with collect.PinnedDirectory(actual) as source:
                    with patch.object(collect.os, "fdopen", side_effect=self.mutate_after_read(real_fdopen, changed)):
                        with self.assertRaisesRegex(collect.Refusal, "^artifact_read_identity$" if replace else "^artifact_read_changed$"):
                            self.exporter(source, output)
                self.assertEqual(sentinel.read_bytes(), raw)
                self.assertEqual(list(output.iterdir()), [])

    def test_parent_replacement_during_read_is_not_reopened_for_export(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            parent, private, output = root / "parent", root / "private", root / "output"
            for directory in (parent, private, output):
                directory.mkdir()
            actual = parent / "fixture"
            actual.mkdir()
            (actual / "native.ndjson").write_bytes(b'{"count":1}\n')
            target = private / "fixture"
            target.mkdir()
            sentinel = target / "native.ndjson"
            raw = b'{"token":"' + b'd' * 64 + b'"}\n'
            sentinel.write_bytes(raw)
            def changed():
                parent.rename(root / "retired")
                parent.symlink_to(private, target_is_directory=True)
            real_fdopen = collect.os.fdopen
            with collect.PinnedDirectory(actual) as source:
                with patch.object(collect.os, "fdopen", side_effect=self.mutate_after_read(real_fdopen, changed)):
                    with self.assertRaisesRegex(collect.Refusal, "^write_directory_changed$"):
                        self.exporter(source, output)
            self.assertEqual(sentinel.read_bytes(), raw)
            self.assertEqual(list(output.iterdir()), [])

    def test_invalid_later_artifact_prevents_copying_earlier_valid_artifact(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            actual, output = root / "fixture", root / "output"
            actual.mkdir()
            output.mkdir()
            (actual / "native.ndjson").write_bytes(b'{"count":1}\n')
            (actual / "process.ndjson").write_bytes(b'{"path":"/private/home"}\n')
            with collect.PinnedDirectory(actual) as source:
                with self.assertRaisesRegex(collect.Refusal, "^artifact_text_not_metadata$"):
                    self.exporter(source, output)
            self.assertEqual(list(output.iterdir()), [])

    def test_bootstrap_failure_without_pinned_source_never_falls_back_to_path_reads(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary).resolve()
            with patch.object(runner.Path, "mkdir"), patch.object(runner.pwd, "getpwnam", side_effect=KeyError("synthetic")), patch.object(runner.grp, "getgrnam", side_effect=KeyError("synthetic")), patch.object(runner, "command", side_effect=collect.Refusal("fixture_command_failed")), patch.object(runner, "export_numeric") as export, patch.object(runner, "bounded") as read, patch.object(runner.os, "chown"), patch.object(runner.shutil, "rmtree"):
                self.assertEqual(runner.execute("smoke", Path("/synthetic/native-bench"), output), 1)
                export.assert_not_called()
                read.assert_not_called()
                self.assertFalse(json.loads((output / "fixture-result.json").read_text())["cleanupConfirmed"])


class FixedMetadataBoundaryTests(unittest.TestCase):
    VALUES = {"fixture.marker": b"0123456789abcdef", "host-netns.txt": b"net:[12345]",
              "app.pid": b"123", "completed.marker": b"native-renderer-lane-only\n"}

    def test_all_fixed_metadata_reads_use_bound_producer_and_exact_size_limits(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            for name, raw in self.VALUES.items():
                (root / name).write_bytes(raw)
            with collect.PinnedDirectory(root) as files:
                for name, raw in self.VALUES.items():
                    self.assertEqual(files.read_metadata(name, os.getuid()), raw)
                    with self.assertRaisesRegex(collect.Refusal, "^artifact_read_identity$"):
                        files.read_metadata(name, os.getuid() + 1)
                    limit = 16 if name in ("fixture.marker", "app.pid") else 64
                    (root / name).write_bytes(b'x' * (limit + 1))
                    with self.assertRaisesRegex(collect.Refusal, "^artifact_read_size$"):
                        files.read_metadata(name, os.getuid())
            with self.assertRaisesRegex(collect.Refusal, "^invalid_metadata_read$"):
                files.read_metadata("unrelated", os.getuid())

    def test_matching_private_metadata_behind_swapped_ancestor_is_never_read(self):
        for name, raw in self.VALUES.items():
            with tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary).resolve()
                actual, private = root / "fixture", root / "private"
                actual.mkdir()
                private.mkdir()
                (private / name).write_bytes(raw)
                with collect.PinnedDirectory(actual) as files:
                    actual.rename(root / "retired")
                    actual.symlink_to(private, target_is_directory=True)
                    with patch.object(collect.os, "fdopen") as read:
                        with self.assertRaisesRegex(collect.Refusal, "^write_directory_changed$"):
                            files.read_metadata(name, os.getuid())
                        read.assert_not_called()
                self.assertEqual((private / name).read_bytes(), raw)

    def test_fixed_metadata_leaf_symlinks_and_hardlinks_are_not_read(self):
        for name, raw in self.VALUES.items():
            for linked in (False, True):
                with tempfile.TemporaryDirectory() as temporary:
                    root = Path(temporary).resolve()
                    actual = root / "fixture"
                    actual.mkdir()
                    sentinel = root / "private"
                    sentinel.write_bytes(raw)
                    if linked:
                        os.link(sentinel, actual / name)
                    else:
                        (actual / name).symlink_to(sentinel)
                    with collect.PinnedDirectory(actual) as files:
                        with patch.object(collect.os, "fdopen") as read:
                            with self.assertRaises((OSError, collect.Refusal)):
                                files.read_metadata(name, os.getuid())
                            read.assert_not_called()
                    self.assertEqual(sentinel.read_bytes(), raw)

    def test_launcher_and_collector_reject_new_same_named_root_before_marker_read(self):
        for module in (launch, collect):
            with tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary).resolve()
                actual = root / NAME
                actual.mkdir()
                with collect.PinnedDirectory(actual) as original:
                    witness = os.dup(original.fd)
                    actual.rename(root / "retired")
                    actual.mkdir()
                    for name, raw in self.VALUES.items():
                        (actual / name).write_bytes(raw)
                    argv = (["launch.py", str(actual), "app", "23456", "23457", "smoke", str(witness)]
                            if module is launch else ["collect.py", "--collect", str(actual), "23456", str(witness)])
                    try:
                        with patch.object(module.sys, "argv", argv), patch.object(module.sys, "platform", "linux"), patch.object(module.os, "geteuid", return_value=0), patch.dict(os.environ, {"GITHUB_ACTIONS": "true", "RUNNER_ENVIRONMENT": "github-hosted", "RUNNER_OS": "Linux"}), patch.object(module, "checked_root") as checked, patch.object(collect.os, "fdopen") as read:
                            with self.assertRaisesRegex(collect.Refusal, "^write_directory_changed$"):
                                module.main()
                            checked.assert_not_called()
                            read.assert_not_called()
                    finally:
                        os.close(witness)  # Failed child init would close it on exit.

    def test_missing_required_artifacts_cannot_export_as_success_but_failed_partial_is_allowed(self):
        required = ("native.ndjson", "process.ndjson", "collector-summary.json")
        for missing in (*required, None):
            with tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary).resolve()
                actual, output = root / "fixture", root / "output"
                actual.mkdir()
                output.mkdir()
                for name in required:
                    if name != missing:
                        (actual / name).write_bytes(b'{"count":1}\n')
                with collect.PinnedDirectory(actual) as files:
                    if missing is None:
                        runner.export_numeric(files, output, native_uid=os.getuid(), collector_uid=os.getuid(), writers_stopped=True, require_complete=True)
                        self.assertEqual(sorted(path.name for path in output.iterdir()), sorted(required))
                    else:
                        with self.assertRaisesRegex(collect.Refusal, "^required_artifact_missing$"):
                            runner.export_numeric(files, output, native_uid=os.getuid(), collector_uid=os.getuid(), writers_stopped=True, require_complete=True)
                        self.assertEqual(list(output.iterdir()), [])
                        runner.export_numeric(files, output, native_uid=os.getuid(), collector_uid=os.getuid(), writers_stopped=True, require_complete=False)
                        self.assertEqual(sorted(path.name for path in output.iterdir()), sorted(set(required) - {missing}))

    def test_execute_success_requires_all_three_artifacts_with_other_kernel_work_mocked(self):
        # Actual execute/export/result path; all accounts, commands, children,
        # /proc and cgroups are synthetic. This is no native/OS qualification.
        required = ("native.ndjson", "process.ndjson", "collector-summary.json")
        for missing in (*required, None, "collector_exit", "compositor_exit", "collector_exit_empty"):
            with tempfile.TemporaryDirectory() as temporary:
                base = Path(temporary).resolve()
                roots, groups, namespaces, output = (base / name for name in ("roots", "groups", "netns", "output"))
                for directory in (roots, groups, namespaces, output):
                    directory.mkdir()
                binary = base / "synthetic-binary"
                binary.write_bytes(b"never_executed")
                nonce, uid = "0123456789abcdef", os.getuid()
                fixture = roots / NAME
                real_path, real_mkdir, real_bounded, real_export = Path, Path.mkdir, runner.bounded, runner.export_numeric
                calls = []
                class Identity:
                    gid = uid
                    def __init__(self, *_args):
                        self.uid = uid
                    def prove_vacant(self):
                        pass
                    def verify(self):
                        pass
                class Child:
                    def __init__(self, domain):
                        self.status = 0
                        if missing in ("collector_exit", "collector_exit_empty", "compositor_exit"):
                            self.status = (1 if domain == "collector" else None) if missing.startswith("collector") else (-6 if domain == "compositor" else None)
                    def poll(self):
                        return self.status
                    def wait(self, **_kwargs):
                        return self.status
                    def terminate(self):
                        self.status = -15
                def path(value, *parts):
                    if value == "/tmp":
                        return roots.joinpath(*parts)
                    if value == "/run/netns":
                        return namespaces.joinpath(*parts)
                    if value == "/proc/self/ns/net":
                        return SimpleNamespace(readlink=lambda: real_path(NET))
                    return real_path(value, *parts)
                def mkdir(directory, *args, **kwargs):
                    result = real_mkdir(directory, *args, **kwargs)
                    if directory.parent == groups:
                        for name in ("cpu.stat", "memory.current", "cgroup.kill", "cgroup.procs"):
                            (directory / name).write_bytes(b"")
                    return result
                def command(argv):
                    calls.append(argv)
                    if argv[:3] == ["/usr/sbin/ip", "netns", "add"]:
                        (namespaces / argv[3]).write_bytes(b"synthetic")
                    elif argv[:3] == ["/usr/sbin/ip", "netns", "delete"]:
                        (namespaces / argv[3]).unlink()
                def popen(argv, **kwargs):
                    is_launcher = argv[1].endswith("launch.py")
                    is_compositor = is_launcher and argv[3] == "compositor"
                    witness = int(argv[7] if is_launcher else argv[-1])
                    self.assertEqual(kwargs["pass_fds"], (witness, int(argv[8])) if is_compositor else (witness,))
                    if is_compositor:
                        self.assertTrue(stat.S_ISFIFO(os.fstat(int(argv[8])).st_mode))
                    # This FD really denotes the pre-workload fixture directory.
                    self.assertEqual((os.fstat(witness).st_dev, os.fstat(witness).st_ino),
                                     (fixture.stat().st_dev, fixture.stat().st_ino))
                    if argv[1].endswith("launch.py") and argv[3] == "app":
                        for name in required:
                            if name != missing:
                                (fixture / name).write_bytes(b'' if missing == "collector_exit_empty" and name == "native.ndjson" else b'{"count":1}\n')
                        (fixture / "completed.marker").write_bytes(b"native-renderer-lane-only\n")
                    return Child(argv[3] if argv[1].endswith("launch.py") else "collector")
                def bounded(file, limit):
                    if file == groups / "cgroup.subtree_control":
                        return b"cpu memory"
                    return real_bounded(file, limit)
                def checked(raw, expected_uid, files):
                    self.assertEqual(raw, str(fixture))
                    self.assertEqual(expected_uid, uid)
                    self.assertEqual(files.read_metadata("fixture.marker", uid), nonce.encode("ascii"))
                    return files.path  # Only literal/tmp prefix is synthetic here.
                def export(files, destination, **kwargs):
                    self.assertEqual(kwargs["require_complete"], missing not in ("collector_exit", "collector_exit_empty", "compositor_exit"))
                    # Current test UID stands in for the real root collector.
                    return real_export(files, destination, collector_uid=uid, **kwargs)
                with ExitStack() as stack:
                    for target, name, value in ((runner, "Path", path), (runner, "CGROUP", groups),
                                                (runner, "FixtureIdentity", Identity), (runner, "command", command),
                                                (runner, "checked_root", checked), (runner, "bounded", bounded),
                                                (runner, "export_numeric", export), (runner.subprocess, "Popen", popen),
                                                (real_path, "mkdir", mkdir)):
                        stack.enter_context(patch.object(target, name, value))
                    stack.enter_context(patch.object(runner.secrets, "token_hex", return_value=nonce))
                    stack.enter_context(patch.object(runner.pwd, "getpwnam", side_effect=KeyError("synthetic")))
                    stack.enter_context(patch.object(runner.grp, "getgrnam", side_effect=KeyError("synthetic")))
                    stack.enter_context(patch.object(runner.os, "chown"))
                    stack.enter_context(patch.object(runner, "install_bus", return_value="synthetic_private_bus"))
                    # This test remaps literal /tmp to its synthetic root. The
                    # exact real transport/address policy has its own test.
                    stack.enter_context(patch.object(runner, "compositor_system_bus", return_value="synthetic_empty_bus"))
                    stack.enter_context(patch.object(runner, "wait_for"))
                    stack.enter_context(patch.object(runner, "cleanup_identity", return_value=False))
                    stack.enter_context(patch.object(runner, "remove_group", side_effect=lambda directory, _inode: runner.shutil.rmtree(directory)))
                    result = runner.execute("smoke", binary, output)
                record = json.loads((output / "fixture-result.json").read_text())
                self.assertTrue(record["cleanupConfirmed"])
                self.assertEqual(record["passed"], missing is None)
                self.assertEqual(result, 0 if missing is None else 1)
                scope_failed = missing in ("collector_exit", "collector_exit_empty", "compositor_exit")
                self.assertEqual(record["reason"], "fixture_scope_or_compositor_failed" if scope_failed else None if missing is None else "numeric_artifact_invalid")
                self.assertEqual(record["bootstrap"]["phase"], 5 if scope_failed else 8)
                self.assertEqual(record["bootstrap"]["artifactExportRejected"], missing in required or missing == "collector_exit_empty")
                if scope_failed:
                    # These are the original statuses, not the later -15 from
                    # our controlled cleanup. Primary refusal is never masked.
                    self.assertIsNone(record["bootstrap"]["appExit"])
                    self.assertEqual(record["bootstrap"]["collectorExit"], 1 if missing.startswith("collector") else None)
                    self.assertEqual(record["bootstrap"]["compositorExit"], -6 if missing == "compositor_exit" else None)
                if missing == "collector_exit_empty":
                    self.assertFalse((output / "fixture-manifest.json").exists())
                self.assertTrue(any(argv[:3] == ["/usr/sbin/ip", "netns", "delete"] for argv in calls))


class CompositorDiagnosticTests(unittest.TestCase):
    def capture(self, raw):
        value = compositor_diagnostic.CompositorDiagnostic(time.monotonic() + 5)
        writer = os.dup(value.writer)  # Only a synthetic producer, never GNOME.
        value.close_writer()
        try:
            for start in range(0, len(raw), 4096):
                chunk = raw[start:start + 4096]
                self.assertEqual(os.write(writer, chunk), len(chunk))
        finally:
            os.close(writer)
            value.finish()
        self.assertTrue(value.finished)
        self.assertTrue(value.eof)
        return value

    def test_real_pipe_keeps_fatal_tail_and_bounded_utf8_export_after_drain(self):
        value = self.capture(b"startup warning\n" * 15000 + "fatal: \u00e9chec final\n".encode("utf-8"))
        self.assertEqual(len(value.tail), compositor_diagnostic.LIMIT)
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            value.export_failed(output, failed=True, writers_stopped=True, redactions=())
            text = (output / compositor_diagnostic.TEXT_NAME).read_bytes()
            metadata = json.loads((output / compositor_diagnostic.META_NAME).read_text())
            self.assertLessEqual(len(text), 65536)
            self.assertTrue(text.decode("utf-8").endswith("fatal: \u00e9chec final\n"))
            self.assertTrue(metadata["truncated"])
            self.assertTrue(metadata["captureComplete"])
            self.assertFalse(metadata["measurementQualified"])
            self.assertEqual(metadata["tailBytes"], len(text))
            self.assertEqual(value.tail, bytearray())

    def test_sanitizer_redacts_known_paths_controls_ansi_and_utf8_expansion(self):
        raw = b"\x1b[31m/tmp/fixed/private\x1b[0m /checkout/a /home/runner/cache\x00\x07\x7f" + "\u202efatal\r\n".encode("utf-8")
        text, truncated = compositor_diagnostic.sanitize_tail(raw, (("/tmp/fixed", "<fixture>"),
                                                                 ("/checkout", "<checkout>"), ("/home/runner", "<runner-home>")))
        self.assertEqual(text, b"<fixture>/private <checkout>/a <runner-home>/cachefatal\n")
        self.assertFalse(truncated)
        text, truncated = compositor_diagnostic.sanitize_tail(b"\xff" * 65530 + b"FATAL!", ())
        self.assertLessEqual(len(text), 65536)
        self.assertTrue(text.decode("utf-8").endswith("FATAL!"))
        self.assertTrue(truncated)

    def test_actual_child_inherits_only_write_end_and_other_child_cannot_write_it(self):
        value = compositor_diagnostic.CompositorDiagnostic(time.monotonic() + 5)
        self.assertFalse(os.get_inheritable(value.reader))
        self.assertFalse(os.get_inheritable(value.writer))
        child = """import os, sys
w, r = map(int, sys.argv[1:])
try: os.fstat(r)
except OSError: pass
else: raise SystemExit(9)
os.dup2(w, 2); os.close(w)
os.write(1, b'NOT_CAPTURED_STDOUT')
os.write(2, b'fixed synthetic stderr')
"""
        try:
            result = subprocess.run([sys.executable, "-I", "-c", child, str(value.writer), str(value.reader)],
                                    pass_fds=(value.writer,), stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                                    stderr=subprocess.DEVNULL, env={}, timeout=3, check=False)
            self.assertEqual(result.returncode, 0)
            unrelated = "import os,sys\ntry: os.write(int(sys.argv[1]), b'NOT_CAPTURED_APP')\nexcept OSError: pass\nelse: raise SystemExit(9)"
            result = subprocess.run([sys.executable, "-I", "-c", unrelated, str(value.writer)],
                                    stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                    env={}, timeout=3, check=False)
            self.assertEqual(result.returncode, 0)
        finally:
            value.finish()
        self.assertEqual(bytes(value.tail), b"fixed synthetic stderr")
        self.assertTrue(value.eof)

    def test_actual_production_shell_redirection_routes_only_fixed_command_stderr(self):
        version = subprocess.run(["bash", "-c", 'printf "%s" "${BASH_VERSINFO[0]}"'],
                                 stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                                 timeout=3, check=False)
        if version.returncode != 0 or not version.stdout.isdigit() or int(version.stdout) < 4:
            self.skipTest("Actual redirection requires the guarded hosted Linux Bash4+; local macOS Bash3.2 cannot qualify it")
        source = Path(launch.__file__).with_name("session.sh").read_text()
        # Execute the actual redirection/parent-close block with only a shell
        # function producer; no system service, UID guard or app is executed.
        block = source.split("  # Same real headless GNOME path", 1)[1]
        block = block[block.index('  if [[ -n "$diagnostic_fd" ]]'):block.index("  workload_status=0")]
        value = compositor_diagnostic.CompositorDiagnostic(time.monotonic() + 5)
        script = 'diagnostic_fd="$1"\ngnome-shell() { printf NOT_CAPTURED_STDOUT; printf fixed_shell_stderr >&2; }\n' + block + '\nwait "$workload_pid"\n'
        try:
            result = subprocess.run(["bash", "-c", script, "synthetic", str(value.writer)], pass_fds=(value.writer,),
                                    stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                    timeout=3, check=False)
            self.assertEqual(result.returncode, 0)
        finally:
            value.finish()
        self.assertEqual(bytes(value.tail), b"fixed_shell_stderr")
        self.assertTrue(value.eof)

    def test_pipe_validator_rejects_regular_foreign_open_modes_and_invalid_ids(self):
        good = SimpleNamespace(st_mode=stat.S_IFIFO | 0o600, st_uid=0)
        with patch.object(compositor_diagnostic.os, "fstat", return_value=good), \
                patch.object(compositor_diagnostic.fcntl, "fcntl", return_value=os.O_WRONLY):
            compositor_diagnostic.checked_write_pipe(9)
        for bad, flags in ((SimpleNamespace(st_mode=stat.S_IFREG | 0o600, st_uid=0), os.O_WRONLY),
                           (SimpleNamespace(st_mode=stat.S_IFIFO | 0o600, st_uid=1001), os.O_WRONLY),
                           (SimpleNamespace(st_mode=stat.S_IFIFO | 0o620, st_uid=0), os.O_WRONLY), (good, os.O_RDONLY)):
            with patch.object(compositor_diagnostic.os, "fstat", return_value=bad), \
                    patch.object(compositor_diagnostic.fcntl, "fcntl", return_value=flags), self.assertRaises(collect.Refusal):
                compositor_diagnostic.checked_write_pipe(9)
        for descriptor in (True, 0, 1, 2, -1, 2**31, "PRIVATE"):
            with patch.object(compositor_diagnostic.os, "fstat") as lookup, self.assertRaises(collect.Refusal):
                compositor_diagnostic.checked_write_pipe(descriptor)
            lookup.assert_not_called()

    def test_app_launcher_rejects_diagnostic_descriptor_before_any_pipe_or_root_lookup(self):
        arguments = ["launch.py", "/not-opened", "app", "23456", "23457", "smoke", "8", "9"]
        with patch.object(launch.sys, "argv", arguments), patch.object(launch.sys, "platform", "linux"), \
                patch.object(launch.os, "geteuid", return_value=0), patch.dict(os.environ, {"GITHUB_ACTIONS": "true", "RUNNER_ENVIRONMENT": "github-hosted", "RUNNER_OS": "Linux"}), \
                patch.object(launch, "checked_write_pipe") as pipe, patch.object(launch, "PinnedDirectory") as directory:
            with self.assertRaisesRegex(collect.Refusal, "^invalid_launcher_arguments$"):
                launch.main()
            pipe.assert_not_called()
            directory.assert_not_called()

    def test_failed_and_confirmed_writer_drain_are_both_required_before_artifact_write(self):
        for failed, drained in ((False, True), (True, False), (True, None)):
            value = self.capture(b"PRIVATE_STARTUP")
            with patch.object(compositor_diagnostic, "PinnedDirectory") as directory:
                value.export_failed(Path("/not-opened"), failed=failed, writers_stopped=drained, redactions=())
                directory.assert_not_called()
            self.assertEqual(value.tail, bytearray())

    def test_original_deadline_and_unobserved_eof_never_become_complete_capture(self):
        expired = time.monotonic() - 1
        first = compositor_diagnostic.CompositorDiagnostic(expired)
        first.finish()
        self.assertEqual(first.deadline, expired)
        self.assertTrue(first.deadline_reached)
        second = compositor_diagnostic.CompositorDiagnostic(time.monotonic() + 5)
        writer = os.dup(second.writer)  # An unclosed synthetic writer is not EOF.
        try:
            second.finish()
            self.assertFalse(second.eof)
            for value in (first, second):
                with tempfile.TemporaryDirectory() as temporary:
                    output = Path(temporary)
                    value.export_failed(output, failed=True, writers_stopped=True, redactions=())
                    self.assertFalse((output / compositor_diagnostic.TEXT_NAME).exists())
                    self.assertFalse(json.loads((output / compositor_diagnostic.META_NAME).read_text())["captureComplete"])
        finally:
            os.close(writer)

    def test_diagnostic_never_makes_rejected_numeric_evidence_valid(self):
        value = self.capture(b"fixed fatal")
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source, output = root / "source", root / "output"
            source.mkdir(); output.mkdir()
            (source / "native.ndjson").write_bytes(b"")
            with collect.PinnedDirectory(source) as files:
                with self.assertRaisesRegex(collect.Refusal, "^empty_numeric_artifact$"):
                    runner.export_numeric(files, output, native_uid=os.getuid(), writers_stopped=True)
            value.export_failed(output, failed=True, writers_stopped=True, redactions=())
            self.assertEqual(sorted(path.name for path in output.iterdir()),
                             sorted((compositor_diagnostic.META_NAME, compositor_diagnostic.TEXT_NAME)))

    def test_private_reader_errors_are_not_formatted_or_exported(self):
        class PrivateError(Exception):
            def __str__(self):
                raise AssertionError("private exception must not be formatted")
        with patch.object(compositor_diagnostic.select, "select", side_effect=PrivateError("PRIVATE_NATIVE_PAYLOAD")):
            value = compositor_diagnostic.CompositorDiagnostic(time.monotonic() + 5)
            value.finish()
        self.assertTrue(value.read_failed)
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            value.export_failed(output, failed=True, writers_stopped=True, redactions=())
            metadata = (output / compositor_diagnostic.META_NAME).read_text()
            self.assertNotIn("PRIVATE", metadata)
            self.assertFalse(json.loads(metadata)["captureComplete"])
            self.assertFalse((output / compositor_diagnostic.TEXT_NAME).exists())


class BootstrapDiagnosticTests(unittest.TestCase):
    def test_owned_child_exit_status_is_closed_without_reading_any_child_payload(self):
        class Child:
            def __init__(self, code): self.code = code
            def poll(self): return self.code
        value = runner.bootstrap_diagnostic(5, Child(None), Child(1), Child(None))
        self.assertEqual(value, {"phase": 5, "compositorExit": None, "collectorExit": 1,
                                 "appExit": None, "artifactExportRejected": False})
        self.assertEqual(runner.bootstrap_diagnostic(5, Child(-6))["compositorExit"], -6)
        for code in (True, 999, "PRIVATE", {"raw": "PRIVATE"}):
            value = runner.bootstrap_diagnostic(5, Child(code))
            self.assertEqual(value["compositorExit"], 256)
            self.assertNotIn("PRIVATE", json.dumps(value))

    def test_unknown_child_probe_is_not_fabricated_as_success_and_cannot_change_phase(self):
        class Child:
            def poll(self): raise OSError("PRIVATE")
        self.assertEqual(runner.bootstrap_diagnostic(2, Child())["compositorExit"], 256)
        for phase in (True, -1, 9, "5"):
            with self.assertRaisesRegex(collect.Refusal, "^invalid_bootstrap_phase$"):
                runner.bootstrap_diagnostic(phase)
        self.assertEqual(runner.bootstrap_diagnostic(0), {"phase": 0, "compositorExit": None,
            "collectorExit": None, "appExit": None, "artifactExportRejected": False})


class DconfActivatorTests(unittest.TestCase):
    def metadata(self, *, mode=stat.S_IFREG | 0o755, uid=0, inode=42, size=100):
        return SimpleNamespace(st_mode=mode, st_uid=uid, st_ino=inode, st_dev=1,
                               st_nlink=1, st_size=size, st_mtime_ns=2, st_ctime_ns=3)

    @contextmanager
    def native(self, *, before=None, after=None, named=None, header=b"\x7fELF", directories=None):
        before = before or self.metadata()
        directories = directories or [self.metadata(mode=stat.S_IFDIR | 0o755)] * 3
        with ExitStack() as stack:
            opening = stack.enter_context(patch.object(runner.os, "open", side_effect=[10, 11, 12, 13]))
            stack.enter_context(patch.object(runner.os, "fstat", side_effect=directories + [before, after or before]))
            read = stack.enter_context(patch.object(runner.os, "read", return_value=header))
            lookup = stack.enter_context(patch.object(runner.os, "stat", return_value=named or before))
            close = stack.enter_context(patch.object(runner.os, "close"))
            yield opening, read, lookup, close

    def test_declared_dconf_never_reads_or_repairs_the_writable_package_descriptor(self):
        # The observed package descriptor can contain arbitrary text. It is not
        # a source at all: only repo constants and the independently vetted ELF.
        with (patch.object(runner, "bounded") as read, patch.object(runner.Path, "lstat") as descriptor,
              patch.object(runner, "checked_dconf_executable") as vetted, patch.object(runner.os, "chmod") as chmod):
            value = runner.checked_service("ca.desrt.dconf")
        self.assertEqual(value, "[D-BUS Service]\nName=ca.desrt.dconf\nExec=/usr/libexec/dconf-service\n")
        vetted.assert_called_once_with("/usr/libexec/dconf-service")
        read.assert_not_called(); descriptor.assert_not_called(); chmod.assert_not_called()
        self.assertNotIn("SystemdService", value); self.assertNotIn("User=", value)

    def test_fallback_is_only_for_an_absent_fixed_candidate_not_an_untrusted_one(self):
        with patch.object(runner, "checked_dconf_executable", side_effect=[FileNotFoundError(), None]) as vet:
            self.assertEqual(runner.declared_dconf_service(), "[D-BUS Service]\nName=ca.desrt.dconf\nExec=/usr/lib/dconf/dconf-service\n")
            self.assertEqual([call.args[0] for call in vet.call_args_list], list(runner.SERVICES["ca.desrt.dconf"]))
        for error in (collect.Refusal("untrusted_bus_activator"), PermissionError(), OSError("PRIVATE")):
            with patch.object(runner, "checked_dconf_executable", side_effect=error) as vet:
                with self.assertRaises(type(error)):
                    runner.declared_dconf_service()
                self.assertEqual(vet.call_count, 1)
        with patch.object(runner, "checked_dconf_executable", side_effect=FileNotFoundError()):
            with self.assertRaisesRegex(collect.Refusal, "^missing_bus_activator$"):
                runner.declared_dconf_service()

    def test_native_elf_is_opened_through_retained_nofollow_parents_and_closed(self):
        executable = runner.SERVICES["ca.desrt.dconf"][0]
        with self.native() as (opening, read, lookup, close):
            runner.checked_dconf_executable(executable)
        self.assertEqual([call.args[0] for call in opening.call_args_list], ["/", "usr", "libexec", "dconf-service"])
        for call in opening.call_args_list:
            self.assertTrue(call.args[1] & os.O_NOFOLLOW)
            self.assertTrue(call.args[1] & os.O_CLOEXEC)
        self.assertEqual([call.kwargs.get("dir_fd") for call in opening.call_args_list], [None, 10, 11, 12])
        self.assertTrue(opening.call_args_list[-1].args[1] & os.O_NONBLOCK)
        read.assert_called_once_with(13, 4)
        lookup.assert_called_once_with(executable, follow_symlinks=False)
        self.assertEqual([call.args[0] for call in close.call_args_list], [13, 12, 11, 10])

    def test_untrusted_parent_directory_cannot_reach_the_binary_read(self):
        good = self.metadata(mode=stat.S_IFDIR | 0o755)
        for index in range(3):
            for bad in (self.metadata(mode=stat.S_IFDIR | 0o777), self.metadata(mode=stat.S_IFDIR | 0o700), self.metadata(mode=stat.S_IFDIR | 0o755, uid=1001), self.metadata(mode=stat.S_IFLNK | 0o777)):
                directories = [good] * 3; directories[index] = bad
                with self.subTest(index=index, mode=bad.st_mode, uid=bad.st_uid), self.native(directories=directories) as (opening, read, _, close):
                    with self.assertRaisesRegex(collect.Refusal, "^untrusted_bus_activator$"):
                        runner.checked_dconf_executable(runner.SERVICES["ca.desrt.dconf"][0])
                    read.assert_not_called()
                    self.assertEqual(close.call_count, opening.call_count)

    def test_native_file_rejects_writable_foreign_nonregular_nonexecutable_and_setid(self):
        cases = [self.metadata(mode=stat.S_IFREG | mode) for mode in (0o775, 0o757, 0o644, 0o4755, 0o2755)]
        cases += [self.metadata(uid=1001), self.metadata(mode=stat.S_IFIFO | 0o755), self.metadata(mode=stat.S_IFLNK | 0o755), self.metadata(size=3)]
        for bad in cases:
            with self.subTest(mode=bad.st_mode, uid=bad.st_uid, size=bad.st_size), self.native(before=bad) as (_, read, lookup, close):
                with self.assertRaisesRegex(collect.Refusal, "^untrusted_bus_activator$"):
                    runner.checked_dconf_executable(runner.SERVICES["ca.desrt.dconf"][0])
                read.assert_not_called(); lookup.assert_not_called(); self.assertEqual(close.call_count, 4)

    def test_absent_or_symlink_parent_open_closes_every_retained_ancestor(self):
        good = self.metadata(mode=stat.S_IFDIR | 0o755)
        for error in (FileNotFoundError(), OSError("closed symlink refusal")):
            with patch.object(runner.os, "open", side_effect=[10, 11, error]), patch.object(runner.os, "fstat", return_value=good), patch.object(runner.os, "close") as close, patch.object(runner.os, "read") as read:
                with self.assertRaises(type(error)):
                    runner.checked_dconf_executable(runner.SERVICES["ca.desrt.dconf"][0])
                self.assertEqual([call.args[0] for call in close.call_args_list], [11, 10])
                read.assert_not_called()

    def test_no_script_substitution_or_replacement_during_read_can_pass(self):
        for patching in ({"header": b"#!/b"}, {"after": self.metadata(inode=99)}, {"named": self.metadata(inode=99)}, {"named": self.metadata(mode=stat.S_IFLNK | 0o755)}):
            with self.subTest(patching=patching), self.native(**patching) as (_, _, _, close):
                with self.assertRaisesRegex(collect.Refusal, "^untrusted_bus_activator$"):
                    runner.checked_dconf_executable(runner.SERVICES["ca.desrt.dconf"][0])
                self.assertEqual(close.call_count, 4)
        with self.native() as (_, _, lookup, close):
            lookup.side_effect = FileNotFoundError()
            with self.assertRaisesRegex(collect.Refusal, "^untrusted_bus_activator$"):
                runner.checked_dconf_executable(runner.SERVICES["ca.desrt.dconf"][0])
            self.assertEqual(close.call_count, 4)

    def test_unknown_executable_or_arguments_are_rejected_before_any_filesystem_lookup(self):
        for executable in ("/bin/sh", "/usr/libexec/dconf-service --session", "/tmp/dconf-service", "dconf-service"):
            with self.subTest(executable=executable), patch.object(runner.os, "open") as opening:
                with self.assertRaisesRegex(collect.Refusal, "^unsupported_bus_activator$"):
                    runner.checked_dconf_executable(executable)
                opening.assert_not_called()


if __name__ == "__main__":
    unittest.main()
