#!/usr/bin/python3
"""Synthetic metadata only. Never provisions, launches or samples a real app."""
from contextlib import ExitStack, contextmanager
import io
import json
import os
from pathlib import Path
import stat
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import collect
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
        self.assertNotEqual(app["HOME"], compositor["HOME"])
        self.assertNotIn("PRIVATE", json.dumps(app))

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
        for missing in (*required, None):
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
                    def poll(self):
                        return 0
                    def wait(self, **_kwargs):
                        return 0
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
                    self.assertEqual(kwargs["pass_fds"], (int(argv[-1]),))
                    # This FD really denotes the pre-workload fixture directory.
                    self.assertEqual((os.fstat(int(argv[-1])).st_dev, os.fstat(int(argv[-1])).st_ino),
                                     (fixture.stat().st_dev, fixture.stat().st_ino))
                    if argv[1].endswith("launch.py") and argv[3] == "app":
                        for name in required:
                            if name != missing:
                                (fixture / name).write_bytes(b'{"count":1}\n')
                        (fixture / "completed.marker").write_bytes(b"native-renderer-lane-only\n")
                    return Child()
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
                    self.assertTrue(kwargs["require_complete"])
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
                    stack.enter_context(patch.object(runner, "wait_for"))
                    stack.enter_context(patch.object(runner, "cleanup_identity", return_value=False))
                    stack.enter_context(patch.object(runner, "remove_group", side_effect=lambda directory, _inode: runner.shutil.rmtree(directory)))
                    result = runner.execute("smoke", binary, output)
                record = json.loads((output / "fixture-result.json").read_text())
                self.assertTrue(record["cleanupConfirmed"])
                self.assertEqual(record["passed"], missing is None)
                self.assertEqual(result, 0 if missing is None else 1)
                self.assertEqual(record["reason"], None if missing is None else "numeric_artifact_invalid")
                self.assertTrue(any(argv[:3] == ["/usr/sbin/ip", "netns", "delete"] for argv in calls))


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
