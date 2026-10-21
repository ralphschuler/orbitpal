#!/usr/bin/python3
"""Local harness tests only: no app, AT-SPI, portal, compositor or real capture."""
import ast
import base64
import io
import importlib.util
import json
from pathlib import Path
import socket
import sqlite3
import tempfile
import threading
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

from PIL import Image
import app
import provider

DIRECTORY = Path(__file__).resolve().parent
NONCE = "a0b1c2d3e4f56789"
_RUNTIME_SPEC = importlib.util.spec_from_file_location("app_runtime_diagnostic", DIRECTORY / "app-runtime-diagnostic.py")
runtime = importlib.util.module_from_spec(_RUNTIME_SPEC)
_RUNTIME_SPEC.loader.exec_module(runtime)


def jpeg(color="red", size=(1280, 800)):
    output = io.BytesIO()
    Image.new("RGB", size, (210, 30, 50) if color == "red" else (30, 50, 210)).save(output, format="JPEG")
    return "data:image/jpeg;base64," + base64.b64encode(output.getvalue()).decode()


def body(case="fresh_red", color="red"):
    return {"model": "orbitpal-wayland-fixture", "stream": True,
            "messages": [{"role": "user", "content": "[Quelle: operator] ORBITPAL_WAYLAND_" + case + "_" + NONCE},
                         {"role": "user", "content": [{"type": "text", "text": "current native screen"},
                                                         {"type": "image_url", "image_url": {"url": jpeg(color)}}]}]}


class Wire:
    def __init__(self, data, fail_send=False):
        self.data, self.sent, self.fail_send = data, bytearray(), fail_send

    def settimeout(self, value):
        self.timeout = value

    def recv(self, limit):
        chunk, self.data = self.data[:limit], self.data[limit:]
        return chunk

    def sendall(self, value):
        if self.fail_send:
            raise BrokenPipeError("synthetic cancelled connection")
        self.sent.extend(value)


def request(value, extra=b"", path=b"/v1/chat/completions"):
    encoded = json.dumps(value).encode()
    return b"POST " + path + b" HTTP/1.1\r\nContent-Type: application/json\r\nContent-Length: " + str(len(encoded)).encode() + b"\r\n" + extra + b"\r\n" + encoded


class ReceiverTests(unittest.TestCase):
    def test_actual_http_parser_preserves_body_without_echoing_headers(self):
        value = {"model": "synthetic", "messages": []}
        wire = Wire(request(value))
        self.assertEqual(provider.read_request(wire), value)
        self.assertGreater(wire.timeout, 0)
        self.assertLessEqual(wire.timeout, 10)

    def test_slow_partial_request_cannot_extend_its_total_deadline(self):
        with patch.object(provider.time, "monotonic", side_effect=[0, 1, 11]):
            class Slow(Wire):
                def recv(self, limit):
                    return b"P"
            with self.assertRaisesRegex(provider.Refusal, "request_deadline"):
                provider.read_request(Slow(b""))

    def test_http_refuses_auth_chunking_duplicate_headers_and_foreign_path(self):
        for extra in (b"Authorization: Bearer sentinel\r\n", b"x-api-key: sentinel\r\n",
                      b"Transfer-Encoding: chunked\r\n", b"Content-Length: 2\r\n", b"Expect: 100-continue\r\n"):
            with self.subTest(extra=extra.split(b":")[0]), self.assertRaises(provider.Refusal) as found:
                provider.read_request(Wire(request({}, extra)))
            self.assertNotIn("sentinel", found.exception.code)
        with self.assertRaises(provider.Refusal):
            provider.read_request(Wire(request({}, path=b"/another-account")))

    def test_header_body_limits_and_incomplete_request(self):
        variants = [b"x" * (provider.MAX_HEADERS + 4096), b"POST /v1/chat/completions HTTP/1.1\r\nContent-Length: 99999999\r\n\r\n",
                    b"POST /v1/chat/completions HTTP/1.1\r\nContent-Length: 5\r\n\r\n{}",
                    b"POST /v1/chat/completions HTTP/1.1\r\nContent-Length: -1\r\n\r\n",
                    b"POST /v1/chat/completions HTTP/1.1\r\nContent-Length: 1\r\n\r\nxx"]
        for raw in variants:
            with self.subTest(length=len(raw)), self.assertRaises(provider.Refusal):
                provider.read_request(Wire(raw))

    def test_two_jpegs_have_real_distinct_markers_and_bounded_metadata_only(self):
        red = provider.inspect_body(body(), "fresh_red", NONCE, "red")
        blue = provider.inspect_body(body("fresh_blue", "blue"), "fresh_blue", NONCE, "blue")
        self.assertNotEqual(red["digest"], blue["digest"])
        self.assertEqual((red["width"], red["height"]), (1280, 800))
        self.assertEqual(set(red), {"digest", "width", "height", "bytes"})
        self.assertLessEqual(red["bytes"], provider.MAX_IMAGE)

    def test_wrong_marker_nonjpeg_oversize_and_bad_base64_fail(self):
        variants = [(jpeg("blue"), "red"), (jpeg(size=(1281, 800)), "red"),
                    ("data:image/jpeg;base64,%%%", "red"), ("data:image/png;base64,AA==", "red"),
                    ("data:image/jpeg;base64," + "A" * (4 * ((provider.MAX_IMAGE + 2) // 3) + 1), "red")]
        for uri, color in variants:
            with self.subTest(size=len(uri)), self.assertRaises(provider.Refusal):
                provider.inspect_image(uri, color)

    def test_old_images_wrong_turn_and_enabled_tools_cannot_pass_freshness(self):
        for mutate in (lambda value: value.update(tools=[{"function": {"name": "capture"}}]),
                       lambda value: value["messages"].append(value["messages"][-1]),
                       lambda value: value["messages"][0].update(content="foreign turn"),
                       lambda value: value["messages"][-1].update(role="tool"),
                       lambda value: value.update(stream=False)):
            value = body()
            mutate(value)
            with self.assertRaises(provider.Refusal):
                provider.inspect_body(value, "fresh_red", NONCE, "red")

    def test_positive_reply_is_a_real_non_silent_sse_completion(self):
        state = provider.State(NONCE)
        state.arm("fresh_red", "red")
        wire = Wire(request(body()))
        provider.Handler(wire, ("127.0.0.1", 1), SimpleNamespace(state=state))
        output = wire.sent.decode()
        self.assertIn("Content-Type: text/event-stream", output)
        self.assertIn("ORBITPAL_REPLY_fresh_red_" + NONCE, output)
        self.assertIn('"finish_reason": "stop"', output)
        self.assertIn("data: [DONE]", output)
        self.assertNotIn("[silent]", output)
        self.assertNotIn("data:image", output)
        self.assertTrue(state.records["fresh_red"]["responseAttempted"])

    def test_held_response_requires_exact_release_after_real_request_receipt(self):
        state = provider.State(NONCE)
        state.arm("held_stop", "red", True)
        wire = Wire(request(body("held_stop")))
        thread = threading.Thread(target=lambda: provider.Handler(wire, ("127.0.0.1", 1), SimpleNamespace(state=state)))
        thread.start()
        try:
            state.wait_received("held_stop", 2)
            self.assertEqual(wire.sent, b"")
            with self.assertRaises(provider.Refusal):
                state.release("held_disable")
            state.release("held_stop")
            state.wait_finished("held_stop", 2)
        finally:
            state.close()
            thread.join(3)
        self.assertFalse(thread.is_alive())
        self.assertIn(b"ORBITPAL_REPLY_held_stop", wire.sent)

    def test_cancelled_transport_still_attempts_fixed_late_reply_without_echoing_error(self):
        state = provider.State(NONCE)
        state.arm("held_disable", "blue")
        wire = Wire(request(body("held_disable", "blue")), fail_send=True)
        provider.Handler(wire, ("127.0.0.1", 1), SimpleNamespace(state=state))
        self.assertIsNone(state.failure)
        self.assertTrue(state.records["held_disable"]["responseAttempted"])
        self.assertEqual(wire.sent, b"")

    def test_unsolicited_requests_and_receiver_failures_cannot_succeed(self):
        state = provider.State(NONCE)
        wire = Wire(request(body()))
        provider.Handler(wire, ("127.0.0.1", 1), SimpleNamespace(state=state))
        self.assertEqual(state.failure, "unsolicited_inference")
        self.assertEqual(wire.sent, b"")
        with self.assertRaises(provider.Refusal):
            state.wait_received("fresh_red", 0.1)

    def test_final_shutdown_handler_failure_is_checked_before_success(self):
        receiver = provider.Receiver.__new__(provider.Receiver)
        receiver.state = provider.State(NONCE)
        def shutdown():
            # Main's earlier failure check has passed; the last handler fails.
            receiver.state.fail("receiver_failed")
        receiver.server = SimpleNamespace(shutdown=shutdown, server_close=Mock())
        receiver.thread = SimpleNamespace(start=Mock(), join=Mock(), is_alive=lambda: False)
        with self.assertRaisesRegex(provider.Refusal, "receiver_failed"):
            with receiver:
                pass
        receiver.thread.join.assert_called_once_with(timeout=12)

    def test_shutdown_failures_cannot_replace_original_body_failure(self):
        receiver = provider.Receiver.__new__(provider.Receiver)
        receiver.state = provider.State(NONCE)
        receiver.server = SimpleNamespace(shutdown=Mock(side_effect=RuntimeError("untrusted cleanup detail")), server_close=Mock())
        receiver.thread = SimpleNamespace(join=Mock(), is_alive=lambda: False)
        with self.assertRaisesRegex(provider.Refusal, "original_body_failed"):
            # The fake receiver is already entered; only actual __exit__ runs.
            try:
                raise provider.Refusal("original_body_failed")
            except provider.Refusal as error:
                if not receiver.__exit__(type(error), error, None):
                    raise
        with self.assertRaisesRegex(provider.Refusal, "receiver_cleanup_failed"):
            receiver.__exit__(None, None, None)


class AppControlTests(unittest.TestCase):
    def test_harness_refuses_normal_environment_before_gui_or_process_inspection(self):
        with patch.dict(app.os.environ, {}, clear=True), patch.object(app, "process_parent") as process:
            with self.assertRaisesRegex(provider.Refusal, "not_disposable_app_job"):
                app.isolated_root()
            process.assert_not_called()

    def test_owned_lineage_is_bounded_and_never_accepts_a_foreign_sibling(self):
        with patch.object(app, "process_parent", side_effect=lambda pid: {10: 1, 11: 10, 12: 1}.get(pid, 1)):
            self.assertTrue(app.owned_process(11, 10))
            self.assertFalse(app.owned_process(12, 10))
        with patch.object(app, "process_parent", return_value=11) as parent:
            self.assertFalse(app.owned_process(11, 10))
            self.assertEqual(parent.call_count, 8)

    def test_tree_walk_never_uses_length_hint_or_unbounded_queue(self):
        class Infinite:
            def __iter__(self):
                return iter(lambda: "leaf", None)
            def __len__(self):
                raise AssertionError("length hint must not be used")
        self.assertEqual(len(list(app.walk(Infinite(), 7))), 7)

    def test_ambiguous_app_controls_are_hard_failure(self):
        studio = app.Studio.__new__(app.Studio)
        node = SimpleNamespace(name="Save", getRole=lambda: 1)
        studio.nodes = lambda: iter([node, node])
        with self.assertRaisesRegex(provider.Refusal, "ambiguous_app_control"):
            studio.find("Save", {1})

    def test_model_aliases_are_closed_and_ambiguity_still_fails(self):
        studio = app.Studio.__new__(app.Studio)
        node = SimpleNamespace(name="Model Load models", getRole=lambda: 1)
        studio.nodes = lambda: iter([node])
        self.assertIs(studio.find("Model", {1}), node)
        node.name = "Model explorer"
        self.assertIsNone(studio.find("Model", {1}))
        other = SimpleNamespace(name="Model", getRole=lambda: 1)
        node.name = "Model Load models"
        studio.nodes = lambda: iter([node, other])
        with self.assertRaisesRegex(provider.Refusal, "ambiguous_app_control"):
            studio.find("Model", {1})

    def test_reply_and_status_text_use_bounded_exact_owned_text_interface(self):
        studio = app.Studio.__new__(app.Studio)
        studio.a = SimpleNamespace(ROLE_PARAGRAPH=1)
        expected = "ORBITPAL_REPLY_fresh_blue_" + NONCE
        interface = SimpleNamespace(characterCount=len(expected), getText=Mock(return_value=expected))
        node = SimpleNamespace(name="", getRole=lambda: 1, queryText=lambda: interface)
        studio.nodes = lambda: iter([node])
        self.assertTrue(studio.text_visible(expected))
        interface.getText.assert_called_once_with(0, len(expected))
        interface.getText.return_value = expected + " extra"
        self.assertFalse(studio.text_visible(expected))
        for count in (True, -1, 0, 513):
            interface.characterCount = count
            interface.getText.reset_mock()
            self.assertFalse(studio.text_visible(expected))
            interface.getText.assert_not_called()
        self.assertFalse(studio.text_visible("x" * 513))

    def test_actual_window_metadata_cannot_authorize_a_covered_marker(self):
        studio = app.Studio.__new__(app.Studio)
        studio.a = SimpleNamespace(ROLE_FRAME=1, ROLE_WINDOW=2, DESKTOP_COORDS=0, STATE_ACTIVE="active")
        rect = SimpleNamespace(x=240, y=120, width=1440, height=960)
        node = SimpleNamespace(name="OrbitPal", getRole=lambda: 1, queryComponent=lambda: SimpleNamespace(getExtents=lambda _: rect), getState=lambda: SimpleNamespace(contains=lambda state: state == "active"))
        studio.nodes = lambda: iter([node])
        studio.clear_marker()
        rect.x, rect.width = 0, 1920
        with self.assertRaisesRegex(provider.Refusal, "synthetic_marker_covered"):
            studio.clear_marker()

    def test_profile_observer_reads_real_scoped_save_not_stale_selection_copy(self):
        with tempfile.TemporaryDirectory(prefix="orbitpal-app-observer-") as directory:
            studio = app.Studio.__new__(app.Studio)
            studio.database = Path(directory) / "cohost.sqlite3"
            with sqlite3.connect(studio.database) as db:
                db.executescript("CREATE TABLE settings(key TEXT PRIMARY KEY,value TEXT); CREATE TABLE bot_profiles(id TEXT PRIMARY KEY,profile TEXT);")
                db.execute("INSERT INTO settings VALUES('profile',?)", (json.dumps({"id": "nova", "screenContext": {"enabled": False}}),))
                db.execute("INSERT INTO bot_profiles VALUES('nova',?)", (json.dumps({"id": "nova", "screenContext": {"enabled": True}}),))
            self.assertTrue(studio.profile()["screenContext"]["enabled"])

    def test_postdraw_ack_cannot_ack_a_new_chooser_or_duplicate_draw(self):
        tree = ast.parse((DIRECTORY / "desktop.py").read_text())
        source = next(item for item in tree.body if isinstance(item, ast.ClassDef) and item.name == "Desktop")
        methods = [item for item in source.body if isinstance(item, ast.FunctionDef) and item.name in ("draw", "draw_ack")]
        klass = ast.ClassDef(name="Draw", bases=[], keywords=[], body=methods, decorator_list=[])
        module = ast.fix_missing_locations(ast.Module(body=[klass], type_ignores=[]))
        idle = Mock()
        namespace = {"APP_MODE": True, "GLib": SimpleNamespace(idle_add=idle)}
        exec(compile(module, "actual-desktop-draw-fixture", "exec"), namespace)
        controller = namespace["Draw"]()
        controller.color, controller.sequence, controller.pending = (1, 0, 0), 7, "red"
        acknowledged = []
        def ack():
            acknowledged.append(controller.sequence)
            controller.pending = None
        controller.ack = ack
        controller.draw(None, SimpleNamespace(set_source_rgb=Mock(), paint=Mock()))
        callback, sequence, color = idle.call_args.args
        controller.sequence, controller.pending = 8, "allow"
        self.assertFalse(callback(sequence, color))
        self.assertEqual(acknowledged, [])
        controller.sequence, controller.pending = 7, "red"
        callback(sequence, color)
        callback(sequence, color)
        self.assertEqual(acknowledged, [7])
        idle.reset_mock()
        namespace["APP_MODE"] = False
        controller.draw(None, SimpleNamespace(set_source_rgb=Mock(), paint=Mock()))
        idle.assert_not_called()


class ConsentScopeTests(unittest.TestCase):
    @staticmethod
    def actual_chooser(desktop, nodes, environment=None, app_mode=True):
        tree = ast.parse((DIRECTORY / "desktop.py").read_text())
        functions = [item for item in tree.body if isinstance(item, ast.FunctionDef)
                     and item.name in ("chooser", "chooser_title")]
        module = ast.fix_missing_locations(ast.Module(body=functions, type_ignores=[]))
        application = SimpleNamespace(name="OrbitPal", nodes=nodes)
        namespace = {
            "APP_MODE": app_mode, "KDE6": desktop == "kde", "DESKTOP": desktop,
            "TITLES": {"gnome": "Share Screen", "kde": "Screen Sharing"},
            "TRACE": SimpleNamespace(log=Mock()),
            "pyatspi": SimpleNamespace(ROLE_FRAME=1, ROLE_DIALOG=2,
                                       Registry=SimpleNamespace(getDesktop=lambda _: object())),
            "children": lambda _: [application],
            "walk": lambda node: iter((node, *node.nodes)),
            "visible": lambda node: True,
            "os": SimpleNamespace(environ={"ORBITPAL_ACCEPTANCE_PORTAL_PID": "42"}
                                  if environment is None else environment),
        }
        exec(compile(module, "actual-desktop-chooser-fixture", "exec"), namespace)
        return namespace["chooser"]

    @staticmethod
    def node(desktop, pid=42, role=1):
        name = "Share screen with OrbitPal" if desktop == "kde" else "Share Screen"
        return SimpleNamespace(name=name, getRole=lambda: role, get_process_id=lambda: pid)

    def test_app_chooser_accepts_only_backend_owned_frame_or_dialog(self):
        for desktop in ("gnome", "kde"):
            for role in (1, 2):
                with self.subTest(desktop=desktop, role=role):
                    owned = self.node(desktop, role=role)
                    self.assertIs(self.actual_chooser(desktop, [owned])(), owned)

    def test_app_chooser_rejects_studio_title_foreign_pid_and_wrong_role(self):
        for desktop in ("gnome", "kde"):
            studio_spoof = self.node(desktop, pid=100)
            foreign = self.node(desktop, pid=43, role=2)
            wrong_role = self.node(desktop, pid=42, role=3)
            with self.subTest(desktop=desktop):
                self.assertIsNone(self.actual_chooser(desktop, [studio_spoof, foreign, wrong_role])())
                owned = self.node(desktop)
                self.assertIs(self.actual_chooser(desktop, [studio_spoof, foreign, wrong_role, owned])(), owned)

    def test_app_chooser_cannot_authorize_without_trusted_backend_pid(self):
        for desktop in ("gnome", "kde"):
            with self.subTest(desktop=desktop), self.assertRaises(KeyError):
                self.actual_chooser(desktop, [self.node(desktop)], environment={})()

    def test_gnome_baseline_keeps_its_existing_private_desktop_behavior(self):
        node = self.node("gnome", pid=100, role=3)
        self.assertIs(self.actual_chooser("gnome", [node], environment={}, app_mode=False)(), node)


class DiagnosticTests(unittest.TestCase):
    def test_known_owned_frame_reports_only_fixed_basename_and_actual_line(self):
        with patch.dict(app.os.environ, {}, clear=True):
            try:
                app.isolated_root()
            except provider.Refusal as error:
                expected_line = error.__traceback__.tb_next.tb_lineno
                result = app.failure_diagnostic(error)
        self.assertEqual(result["source"], "app.py")
        self.assertEqual(result["line"], expected_line)
        self.assertEqual(result["exceptionType"], "Refusal")
        self.assertFalse(result["metadataTruncated"])
        self.assertEqual(set(result), {"stage", "exceptionType", "source", "line", "metadataFrames", "metadataTruncated"})

    def test_hostile_message_repr_cause_and_local_values_are_never_read_or_emitted(self):
        class Hostile(Exception):
            def __str__(self):
                raise AssertionError("exception string must not be used")
            def __repr__(self):
                raise AssertionError("exception repr must not be used")
            def __getattribute__(self, name):
                if name in ("args", "__cause__", "__context__", "__traceback__"):
                    raise AssertionError("exception values must not be read")
                return super().__getattribute__(name)
        private = "PRIVATE_TOKEN_https://secret.invalid/account_/Users/private/path"
        try:
            raise Hostile(private, {"secret": private}) from RuntimeError(private)
        except Hostile as error:
            result = app.failure_diagnostic(error)
        output = json.dumps(result)
        self.assertNotIn(private, output)
        self.assertEqual(result["exceptionType"], "other")
        self.assertIsNone(result["source"])
        self.assertIsNone(result["line"])

    def test_foreign_provider_basename_cannot_become_an_owned_source(self):
        namespace = {}
        exec(compile("def fail():\n    raise ValueError('sensitive_foreign_message')\n", "/private/account/provider.py", "exec"), namespace)
        try:
            namespace["fail"]()
        except ValueError as error:
            result = app.failure_diagnostic(error)
        self.assertEqual(result["exceptionType"], "ValueError")
        self.assertIsNone(result["source"])
        self.assertIsNone(result["line"])
        self.assertNotIn("sensitive_foreign_message", json.dumps(result))
        self.assertNotIn("/private/account", json.dumps(result))

    def test_traceback_metadata_has_a_fixed_scan_cap(self):
        def nested(depth):
            if depth:
                return nested(depth - 1)
            raise ValueError("not emitted")
        try:
            nested(app.TRACEBACK_METADATA_LIMIT + 10)
        except ValueError as error:
            result = app.failure_diagnostic(error)
        self.assertEqual(result["metadataFrames"], app.TRACEBACK_METADATA_LIMIT)
        self.assertTrue(result["metadataTruncated"])
        self.assertIsNone(result["source"])

    def test_finite_progress_is_emitted_once_and_unknown_stage_is_never_logged(self):
        output = io.StringIO()
        with patch.object(app, "_emitted_phases", set()), patch.object(app, "_current_phase", "controller_entry"), patch.object(app.sys, "stdout", output):
            for _ in range(100):
                app.phase("studio_construct")
            with self.assertRaises(provider.Refusal):
                app.phase("private_untrusted_stage")
            with patch.object(app, "_current_phase", "private_untrusted_stage"):
                result = app.failure_diagnostic(ValueError("private_message"))
        self.assertEqual(output.getvalue(), "app-phase: studio_construct\n")
        self.assertEqual(result["stage"], "controller_entry")
        self.assertNotIn("private", json.dumps(result))

    def test_main_does_not_catch_system_exit_or_claim_success(self):
        for failure in (SystemExit(7), KeyboardInterrupt()):
            output, errors = io.StringIO(), io.StringIO()
            with self.subTest(kind=type(failure).__name__), patch.object(app, "isolated_root", return_value=object()), patch.object(app, "run", side_effect=failure), patch.object(app, "_emitted_phases", set()), patch.object(app.sys, "stdout", output), patch.object(app.sys, "stderr", errors):
                with self.assertRaises(type(failure)) as found:
                    app.main()
            if isinstance(failure, SystemExit):
                self.assertEqual(found.exception.code, 7)
            self.assertEqual(errors.getvalue(), "")
            self.assertNotIn("complete", output.getvalue())

    def test_unknown_refusal_or_subclass_payload_cannot_escape_in_failure_footer(self):
        class Derived(provider.Refusal):
            pass
        private = "PRIVATE_REFUSAL_VALUE"
        for failure in (provider.Refusal(private), Derived(private), RuntimeError(private)):
            output, errors = io.StringIO(), io.StringIO()
            with self.subTest(kind=type(failure).__name__), patch.object(app, "isolated_root", return_value=object()), patch.object(app, "run", side_effect=failure), patch.object(app, "_emitted_phases", set()), patch.object(app.sys, "stdout", output), patch.object(app.sys, "stderr", errors):
                self.assertEqual(app.main(), 1)
            self.assertNotIn(private, errors.getvalue())
            self.assertNotIn(private, output.getvalue())
            self.assertIn("app-diagnostic: ", errors.getvalue())
        self.assertEqual(app.refusal_code(provider.Refusal("app_process_exited")), "app_process_exited")

    def test_missing_process_is_terminal_and_has_a_fixed_code(self):
        with patch.object(app.Path, "stat", side_effect=FileNotFoundError("sensitive proc path")), patch.object(app.time, "sleep") as sleep:
            with self.assertRaisesRegex(provider.Refusal, "^app_process_exited$"):
                app.process_parent(42)
            sleep.assert_not_called()

    def test_zombie_process_cannot_be_treated_as_ready(self):
        metadata = b"State:\tZ (zombie)\nPPid:\t1\n"
        with patch.object(app.Path, "stat", return_value=SimpleNamespace(st_uid=app.os.getuid())), patch.object(app.Path, "read_bytes", return_value=metadata):
            with self.assertRaisesRegex(provider.Refusal, "^app_process_exited$"):
                app.process_parent(42)

    def test_missing_pid_file_is_not_a_registry_or_capture_failure(self):
        output = io.StringIO()
        with patch.dict(app.sys.modules, {"pyatspi": SimpleNamespace()}), patch.object(app.Path, "read_text", side_effect=FileNotFoundError("sensitive path")), patch.object(app.sys, "stdout", output):
            with self.assertRaisesRegex(provider.Refusal, "^app_pid_not_ready$"):
                app.Studio(Path("/disposable"))

    def test_missing_executable_is_distinguished_from_terminal_exit_without_retry(self):
        for state in (1, provider.Refusal("app_process_exited")):
            expected = "app_executable_not_ready" if state == 1 else "app_process_exited"
            with self.subTest(expected=expected), patch.dict(app.sys.modules, {"pyatspi": SimpleNamespace()}), patch.object(app.Path, "read_text", return_value="42\n"), patch.object(app.Path, "resolve", side_effect=FileNotFoundError("sensitive executable path")), patch.object(app, "process_parent", side_effect=[1, state]) as process, patch.object(app.time, "sleep") as sleep, patch.object(app.sys, "stdout", io.StringIO()):
                with self.assertRaisesRegex(provider.Refusal, "^" + expected + "$"):
                    app.Studio(Path("/disposable"))
                self.assertEqual(process.call_count, 2)
                sleep.assert_not_called()

    def test_unexpected_executable_still_fails_closed(self):
        with patch.dict(app.sys.modules, {"pyatspi": SimpleNamespace()}), patch.object(app.Path, "read_text", return_value="42\n"), patch.object(app.Path, "resolve", return_value=Path("/foreign/binary")), patch.object(app, "process_parent", return_value=1), patch.object(app.sys, "stdout", io.StringIO()):
            with self.assertRaisesRegex(provider.Refusal, "^unexpected_app_executable$"):
                app.Studio(Path("/disposable"))


class DiagnosticClock:
    def __init__(self):
        self.now = 0.0

    def __call__(self):
        return self.now

    def sleep(self, value):
        self.now += value


class DiagnosticChild:
    """No OS process: model the exact signed Popen result and signal races."""
    def __init__(self, clock, returncode=None, exit_at=None, ignore_term=False, ignore_kill=False):
        self.clock = clock
        self.returncode = returncode
        self.exit_at = exit_at
        self.ignore_term, self.ignore_kill = ignore_term, ignore_kill
        self.terms = self.kills = self.waits = 0

    def poll(self):
        if self.returncode is None and self.exit_at is not None and self.clock() >= self.exit_at:
            self.returncode = 23
        return self.returncode

    def terminate(self):
        self.terms += 1
        if self.returncode is not None:
            raise ProcessLookupError("PRIVATE_PROCESS_VALUE")
        if not self.ignore_term:
            self.returncode = -15

    def kill(self):
        self.kills += 1
        if self.returncode is not None:
            raise ProcessLookupError("PRIVATE_PROCESS_VALUE")
        if not self.ignore_kill:
            self.returncode = -9

    def wait(self, timeout):
        self.waits += 1
        if self.returncode is None:
            raise TimeoutError("PRIVATE_WAIT_VALUE")
        return self.returncode


class RuntimeDiagnosticTests(unittest.TestCase):
    def test_fixed_categories_count_messages_without_retaining_payloads(self):
        filtered = runtime.RuntimeFilter()
        private = b"/PRIVATE_PATH KEY=PRIVATE_SECRET data:image/jpeg;base64,PRIVATE_FRAME"
        raw = (b"thread 'main' panicked at " + private + b"\n"
               b"(app:42): Gtk-WARNING **: cannot open display: " + private + b"\n"
               b"bwrap: Creating new namespace failed: Operation not permitted " + private + b"\n"
               b"WebKit sandbox failed " + private + b"\n"
               b"app: error while loading shared libraries: " + private + b"\n"
               b"Could not create EGL display: no supported platform available. Aborting... " + private + b"\n"
               b"memory allocation of 12345 bytes failed\n" + private + b"\n")
        for offset in range(0, len(raw), 19):
            filtered.feed(raw[offset:offset + 19])
        filtered.finish(True)
        summary = filtered.summary("eof")
        self.assertEqual(summary["observations"], dict.fromkeys(runtime.CATEGORIES, 1))
        self.assertEqual(summary["unclassifiedLines"], 1)
        self.assertEqual(summary["linesObserved"], 8)
        self.assertEqual(summary["bytesObserved"], len(raw))
        self.assertEqual(filtered.pending, b"")
        encoded = json.dumps(summary)
        for forbidden in ("PRIVATE", "base64", "/", "KEY=", "panicked at"):
            self.assertNotIn(forbidden, encoded)

    def test_final_unterminated_line_is_classified_only_on_actual_eof(self):
        for eof, expected in ((True, 1), (False, 0)):
            filtered = runtime.RuntimeFilter()
            filtered.feed(b"memory allocation of 128 bytes failed")
            filtered.finish(eof)
            filtered.finish(eof)
            self.assertEqual(filtered.counts["allocation_failure_message"], expected)
            self.assertEqual(filtered.lines, expected)
            self.assertEqual(filtered.pending, b"")

    def test_long_binary_lines_and_base64_are_discarded_in_bounded_memory(self):
        filtered = runtime.RuntimeFilter()
        filtered.feed(b"thread 'main' panicked at " + b"S" * (runtime.MAX_LINE + 1))
        self.assertLessEqual(len(filtered.pending), runtime.MAX_LINE)
        self.assertTrue(filtered.overlong)
        filtered.feed(b"data:image/jpeg;base64,PRIVATE\x00\xff\nunknown\r\n")
        filtered.finish(True)
        summary = filtered.summary("eof")
        self.assertEqual(summary["overlongLines"], 1)
        self.assertEqual(summary["unclassifiedLines"], 1)
        self.assertEqual(sum(summary["observations"].values()), 0)
        self.assertNotIn("PRIVATE", json.dumps(summary))

    def test_byte_and_line_caps_are_sticky_and_cannot_scan_trailing_messages(self):
        with patch.object(runtime, "MAX_BYTES", 12):
            filtered = runtime.RuntimeFilter()
            filtered.feed(b"x" * 100_000 + b"\nthread 'main' panicked at PRIVATE\n")
            self.assertTrue(filtered.limited)
            self.assertEqual(filtered.bytes, 12)
            filtered.feed(b"memory allocation of 1 bytes failed\n")
            self.assertEqual(sum(filtered.counts.values()), 0)
        with patch.object(runtime, "MAX_LINES", 3):
            filtered = runtime.RuntimeFilter()
            filtered.feed(b"x\nx\nx\nthread 'main' panicked at PRIVATE\n")
            self.assertTrue(filtered.limited)
            self.assertEqual(filtered.lines, 3)
            self.assertEqual(sum(filtered.counts.values()), 0)

    def test_exit_137_and_actual_signal_9_are_never_conflated(self):
        plain = runtime.exit_record(137, started=True, reason="none", already_exited=None, term_sent=False, kill_sent=False)
        killed = runtime.exit_record(-9, started=True, reason="none", already_exited=None, term_sent=False, kill_sent=False)
        self.assertEqual((plain["result"], plain["exitCode"], plain["signal"]), ("exited", 137, None))
        self.assertEqual((killed["result"], killed["exitCode"], killed["signal"]), ("signalled", None, 9))
        self.assertFalse(killed["limitInduced"])
        self.assertNotIn("oom", json.dumps(killed))

    def test_controller_success_cannot_mask_natural_exit_or_signal_at_cleanup_race(self):
        summary = {"status": "eof"}
        for code in (137, -9, -6):
            for reason, dead in (("none", None), ("cleanup_marker", True)):
                record = runtime.exit_record(code, started=True, reason=reason, already_exited=dead, term_sent=False, kill_sent=False)
                self.assertFalse(runtime.accepted_collection(summary, record))
        for code, term, kill in ((-15, True, False), (-9, True, True), (0, True, False)):
            record = runtime.exit_record(code, started=True, reason="cleanup_marker", already_exited=False, term_sent=term, kill_sent=kill)
            self.assertTrue(runtime.accepted_collection(summary, record))
        unexpected = runtime.exit_record(-6, started=True, reason="cleanup_marker", already_exited=False, term_sent=True, kill_sent=False)
        self.assertFalse(runtime.accepted_collection(summary, unexpected))

    def supervise(self, child, chunks, cleanup=lambda: None, hold_pipe=False):
        values = iter(chunks)
        def read_chunk(timeout):
            child.clock.sleep(timeout)
            return next(values, None if hold_pipe else b"")
        with patch.object(runtime.time, "sleep", side_effect=child.clock.sleep):
            return runtime.supervise(child, read_chunk, cleanup, child.clock)

    def test_natural_early_exit_drains_both_merged_stream_messages_then_waits(self):
        clock = DiagnosticClock()
        child = DiagnosticChild(clock, returncode=23)
        summary, result = self.supervise(child, [b"thread 'main' panicked at PRIVATE_STDOUT\n", b"Failed to initialize EGL PRIVATE_STDERR", b""])
        self.assertEqual(summary["status"], "eof")
        self.assertEqual(summary["observations"]["rust_panic_message"], 1)
        self.assertEqual(summary["observations"]["egl_init_message"], 1)
        self.assertEqual(result["exitCode"], 23)
        self.assertEqual(result["stopReason"], "none")
        self.assertEqual((child.terms, child.kills, child.waits), (0, 0, 1))
        self.assertNotIn("PRIVATE", json.dumps([summary, result]))

    def test_descendant_held_pipe_has_bounded_drain_and_never_claims_eof(self):
        clock = DiagnosticClock()
        child = DiagnosticChild(clock, returncode=7)
        summary, result = self.supervise(child, [b"PRIVATE_PARTIAL"], hold_pipe=True)
        self.assertEqual(summary["status"], "drain_timeout")
        self.assertEqual(summary["linesObserved"], 0)
        self.assertEqual(result["exitCode"], 7)
        self.assertGreaterEqual(clock(), runtime.DRAIN_SECONDS)
        self.assertLess(clock(), runtime.DRAIN_SECONDS + 0.2)

    def test_owned_cleanup_waits_signed_return_and_does_not_signal_a_dead_child(self):
        for initial, expected_terms in ((None, 1), (23, 0)):
            clock = DiagnosticClock()
            child = DiagnosticChild(clock, returncode=initial)
            summary, result = self.supervise(child, [], cleanup=lambda: "cleanup_marker")
            self.assertEqual(result["alreadyExitedBeforeCleanup"], initial is not None)
            self.assertEqual(result["stopReason"], "cleanup_marker")
            self.assertEqual(child.terms, expected_terms)
            self.assertEqual(result["signal"], 15 if initial is None else None)
            self.assertEqual(child.waits, 1)
            self.assertEqual(summary["status"], "eof")

    def test_natural_exit_between_poll_and_term_is_not_claimed_as_our_signal(self):
        clock = DiagnosticClock()
        child = DiagnosticChild(clock)
        def terminate():
            child.returncode = 137
            child.terms += 1
            raise ProcessLookupError("PRIVATE_RACE_VALUE")
        child.terminate = terminate
        summary, result = self.supervise(child, [], cleanup=lambda: "cleanup_marker")
        self.assertFalse(result["termSent"])
        self.assertEqual((result["result"], result["exitCode"], result["signal"]), ("exited", 137, None))
        self.assertFalse(runtime.accepted_collection(summary, result))
        self.assertNotIn("PRIVATE", json.dumps(result))

    def test_ignored_term_escalates_only_the_same_owned_child_with_bounded_wait(self):
        clock = DiagnosticClock()
        child = DiagnosticChild(clock, ignore_term=True)
        _, result = self.supervise(child, [], cleanup=lambda: "supervisor_signal")
        self.assertEqual((child.terms, child.kills, child.waits), (1, 1, 1))
        self.assertTrue(result["termSent"])
        self.assertTrue(result["killSent"])
        self.assertEqual(result["signal"], 9)
        self.assertEqual(result["stopReason"], "supervisor_signal")
        self.assertLess(clock(), runtime.TERM_SECONDS + 0.3)

    def test_limit_induced_termination_is_explicit_and_never_a_natural_cause(self):
        clock = DiagnosticClock()
        child = DiagnosticChild(clock)
        with patch.object(runtime, "MAX_BYTES", 8):
            summary, result = self.supervise(child, [b"PRIVATE_OUTPUT\n"])
        self.assertEqual(summary["status"], "limit")
        self.assertTrue(summary["limitReached"])
        self.assertEqual(result["stopReason"], "diagnostic_limit")
        self.assertTrue(result["limitInduced"])
        self.assertEqual(result["signal"], 15)
        self.assertEqual(child.terms, 1)
        self.assertNotIn("PRIVATE", json.dumps([summary, result]))

    def test_observed_limit_wins_over_a_simultaneous_later_cleanup_marker(self):
        clock = DiagnosticClock()
        child = DiagnosticChild(clock)
        checks = 0
        def cleanup():
            nonlocal checks
            checks += 1
            return None if checks == 1 else "cleanup_marker"
        with patch.object(runtime, "MAX_BYTES", 8):
            summary, result = self.supervise(child, [b"PRIVATE_OUTPUT"], cleanup)
        self.assertEqual(result["stopReason"], "diagnostic_limit")
        self.assertTrue(result["limitInduced"])
        self.assertFalse(result["alreadyExitedBeforeCleanup"])
        self.assertFalse(runtime.accepted_collection(summary, result))
        self.assertEqual(checks, 1)

    def test_deadline_and_failed_wait_are_bounded_without_exception_payload(self):
        clock = DiagnosticClock()
        child = DiagnosticChild(clock, ignore_term=True, ignore_kill=True)
        with patch.object(runtime, "LIFETIME_SECONDS", 0.1):
            _, result = self.supervise(child, [])
        self.assertEqual(result["result"], "wait_unavailable")
        self.assertEqual(result["stopReason"], "deadline")
        self.assertEqual((child.terms, child.kills, child.waits), (1, 1, 1))
        self.assertLess(clock(), 0.1 + runtime.TERM_SECONDS + runtime.KILL_SECONDS + 0.3)
        self.assertNotIn("PRIVATE", json.dumps(result))

    def test_raw_stream_error_and_hostile_repr_never_escape(self):
        class HostileError(Exception):
            def __str__(self):
                raise AssertionError("private exception must never be formatted")
            def __repr__(self):
                raise AssertionError("private exception must never be represented")
        clock = DiagnosticClock()
        child = DiagnosticChild(clock)
        def read_chunk(timeout):
            clock.sleep(timeout)
            raise HostileError("PRIVATE_MESSAGE", "/PRIVATE_PATH", b"PRIVATE_FRAME")
        with patch.object(runtime.time, "sleep", side_effect=clock.sleep):
            summary, result = runtime.supervise(child, read_chunk, lambda: None, clock)
        self.assertEqual(summary["status"], "read_failed")
        self.assertEqual(result["stopReason"], "diagnostic_failure")
        self.assertEqual(result["signal"], 15)
        self.assertNotIn("PRIVATE", json.dumps([summary, result]))

    def test_fixed_child_launch_merges_streams_without_arguments_or_env_replacement(self):
        clock = DiagnosticClock()
        child = DiagnosticChild(clock, returncode=0)
        child.pid = 42
        child.stdout = SimpleNamespace(fileno=lambda: 9, close=Mock())
        select = SimpleNamespace(register=Mock(), select=lambda timeout: [object()], close=Mock())
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            control = root / "control"
            control.mkdir()
            with patch.object(runtime, "isolated_root", return_value=root), patch.object(runtime, "private_control", return_value=(control, root / "acceptance-app")), patch.object(runtime.subprocess, "Popen", return_value=child) as popen, patch.object(runtime.os, "set_blocking"), patch.object(runtime.os, "read", return_value=b""), patch.object(runtime.selectors, "DefaultSelector", return_value=select), patch.object(runtime.signal, "signal"), patch.object(runtime.time, "sleep"), patch.object(runtime, "supervise", return_value=({"version": 1, "status": "eof"}, runtime.exit_record(0, started=True, reason="none", already_exited=None, term_sent=False, kill_sent=False))):
                self.assertEqual(runtime.main(), 0)
            popen.assert_called_once_with([str(root / "acceptance-app")], stdin=runtime.subprocess.DEVNULL, stdout=runtime.subprocess.PIPE, stderr=runtime.subprocess.STDOUT)
            self.assertEqual((control / "app-pid").read_text(), "42\n")
            self.assertEqual(json.loads((root / "app-exit.txt").read_text())["exitCode"], 0)
            self.assertEqual(json.loads((root / "app-runtime.txt").read_text())["status"], "eof")
            child.stdout.close.assert_called_once()
            select.close.assert_called_once()

    def test_setup_failure_has_only_fixed_summaries_and_no_raw_exception(self):
        private = "PRIVATE_STARTUP_ARGUMENT"
        output, errors = io.StringIO(), io.StringIO()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with patch.object(runtime, "isolated_root", return_value=root), patch.object(runtime, "private_control", side_effect=ValueError(private)), patch.object(app.sys, "stdout", output), patch.object(app.sys, "stderr", errors):
                self.assertEqual(runtime.main(), 1)
            records = (root / "app-exit.txt").read_text() + (root / "app-runtime.txt").read_text()
            self.assertNotIn(private, records + output.getvalue() + errors.getvalue())
            self.assertEqual(json.loads((root / "app-exit.txt").read_text())["result"], "not_started")
            self.assertEqual(json.loads((root / "app-runtime.txt").read_text())["status"], "not_opened")

    def test_only_private_control_and_actual_fixed_regular_executable_are_admitted(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            control = root / "control"
            control.mkdir(mode=0o700)
            executable = root / "acceptance-app"
            executable.write_bytes(b"synthetic; never executed")
            executable.chmod(0o700)
            self.assertEqual(runtime.private_control(root), (control, executable))
            control.chmod(0o755)
            with self.assertRaises(ValueError):
                runtime.private_control(root)
            control.chmod(0o700)
            executable.rename(root / "another-file")
            executable.symlink_to(root / "another-file")
            with self.assertRaises(ValueError):
                runtime.private_control(root)


class SwitchRevealTests(unittest.TestCase):
    """Exercise actual Studio walk/find/reveal/action with synthetic AT-SPI."""
    class Node:
        def __init__(self, name, role, states=(), pid=41, children=()):
            self.name, self.role, self.states, self.pid = name, role, set(states), pid
            self.children = list(children)
            self.scroll = Mock(return_value=True)
            self.press = Mock(side_effect=self.toggle)

        def __iter__(self):
            return iter(self.children)

        def get_process_id(self):
            return self.pid

        def getRole(self):
            return self.role

        def getState(self):
            return SimpleNamespace(contains=lambda state: state in self.states)

        def queryComponent(self):
            return SimpleNamespace(scrollTo=self.scroll)

        def queryAction(self):
            return SimpleNamespace(nActions=1, getName=lambda _: "toggle", doAction=self.press)

        def toggle(self, _):
            self.states.symmetric_difference_update({"checked"})
            return True

    def fixture(self, controls):
        studio = app.Studio.__new__(app.Studio)
        studio.pid, studio.root = 41, Path("/disposable")
        frame = self.Node("OrbitPal", 10, ("visible", "showing", "active"), children=controls)
        desktop = self.Node("private desktop", 0, pid=1, children=[frame])
        studio.a = SimpleNamespace(ROLE_FRAME=10, ROLE_WINDOW=11, ROLE_CHECK_BOX=3,
                                   ROLE_TOGGLE_BUTTON=4, ROLE_PUSH_BUTTON=5,
                                   STATE_VISIBLE="visible", STATE_SHOWING="showing", STATE_ACTIVE="active",
                                   STATE_CHECKED="checked", STATE_ENABLED="enabled", STATE_SENSITIVE="sensitive",
                                   SCROLL_ANYWHERE=0, Registry=SimpleNamespace(getDesktop=lambda _: desktop))
        return studio, frame, desktop

    def parents(self, pid):
        return {41: 1, 42: 41, 99: 1}.get(pid, 1)

    def test_real_switch_reveals_then_acts_only_on_fresh_showing_owned_control(self):
        old = self.Node("Enable speech output", 3, ("visible", "checked", "enabled", "sensitive"), pid=42)
        fresh = self.Node(old.name, 3, ("visible", "showing", "checked", "enabled", "sensitive"), pid=42)
        studio, frame, _ = self.fixture([old])
        def reveal(_):
            frame.children = [fresh]
            return True
        old.scroll.side_effect = reveal
        with patch.object(app, "process_parent", side_effect=self.parents), patch.object(app.Path, "resolve", return_value=studio.root / "acceptance-app"), patch.object(app, "phase"):
            studio.switch("Enable speech output", False)
        old.scroll.assert_called_once_with(0)
        old.press.assert_not_called()
        fresh.scroll.assert_not_called()  # No second scroll/stale target before activation.
        fresh.press.assert_called_once_with(0)
        self.assertNotIn("checked", fresh.states)

    def test_hidden_foreign_wrong_role_and_outside_studio_candidates_cannot_be_revealed(self):
        for kind in ("hidden", "foreign", "wrong_role", "other_window"):
            node = self.Node("Enable speech output", 5 if kind == "wrong_role" else 3,
                             () if kind == "hidden" else ("visible",), pid=99 if kind == "foreign" else 42)
            studio, frame, desktop = self.fixture([] if kind == "other_window" else [node])
            if kind == "other_window":
                desktop.children.append(self.Node("avatar-nova", 10, ("visible", "showing"), children=[node]))
            with self.subTest(kind=kind), patch.object(app, "process_parent", side_effect=self.parents), patch.object(app.Path, "resolve", return_value=studio.root / "acceptance-app"), patch.object(app, "phase"):
                self.assertIsNone(studio.switch_control(node.name, {3, 4})())
            node.scroll.assert_not_called()
            node.press.assert_not_called()

    def test_visible_duplicate_remains_ambiguous_even_if_one_is_already_showing(self):
        a = self.Node("Enable speech output", 3, ("visible", "showing"))
        b = self.Node(a.name, 3, ("visible",))
        studio, _, _ = self.fixture([a, b])
        with patch.object(app, "process_parent", side_effect=self.parents), patch.object(app.Path, "resolve", return_value=studio.root / "acceptance-app"), patch.object(app, "phase"), self.assertRaisesRegex(provider.Refusal, "ambiguous_app_control"):
            studio.switch_control(a.name, {3, 4})()
        a.press.assert_not_called()
        b.scroll.assert_not_called()

    def test_failed_scroll_and_success_without_showing_never_activate(self):
        for succeeds in (False, True):
            node = self.Node("Enable speech output", 3, ("visible",))
            studio, _, _ = self.fixture([node])
            node.scroll.return_value = succeeds
            with self.subTest(scroll=succeeds), patch.object(app, "process_parent", side_effect=self.parents), patch.object(app.Path, "resolve", return_value=studio.root / "acceptance-app"), patch.object(app, "phase"):
                locate = studio.switch_control(node.name, {3, 4})
                if succeeds:
                    for _ in range(5):
                        self.assertIsNone(locate())
                    self.assertEqual(node.scroll.call_count, 2)
                else:
                    with self.assertRaisesRegex(provider.Refusal, "app_action_refused"):
                        locate()
            node.press.assert_not_called()

    def test_executable_or_active_frame_change_after_scroll_blocks_reacquisition(self):
        for change in ("executable", "inactive", "duplicate_frame"):
            node = self.Node("Enable speech output", 3, ("visible",))
            studio, frame, desktop = self.fixture([node])
            executable = [studio.root / "acceptance-app"]
            def reveal(_):
                node.states.add("showing")
                if change == "executable":
                    executable[0] = Path("/disposable/foreign-app")
                elif change == "inactive":
                    frame.states.discard("active")
                else:
                    desktop.children.append(self.Node("OrbitPal", 10, ("visible", "showing", "active")))
                return True
            node.scroll.side_effect = reveal
            expected = {"executable": "unexpected_app_executable", "inactive": "app_window_not_active", "duplicate_frame": "ambiguous_app_window"}[change]
            with self.subTest(change=change), patch.object(app, "process_parent", side_effect=self.parents), patch.object(app.Path, "resolve", side_effect=lambda **_: executable[0]), patch.object(app, "phase"), self.assertRaisesRegex(provider.Refusal, expected):
                studio.switch_control(node.name, {3, 4})()
            node.press.assert_not_called()

    def test_switch_retries_only_inactive_observation_then_reacquires_before_action(self):
        node = self.Node("Enable speech output", 3, ("visible", "showing", "checked", "enabled", "sensitive"), pid=42)
        studio, frame, _ = self.fixture([node])
        frame.states.discard("active")
        clock = [0.0]
        def resume(delay):
            node.press.assert_not_called()
            node.scroll.assert_not_called()
            clock[0] += delay
            frame.states.add("active")
        with patch.object(app, "process_parent", side_effect=self.parents), patch.object(app.Path, "resolve", return_value=studio.root / "acceptance-app") as executable, patch.object(app, "phase"), patch.object(app.time, "monotonic", side_effect=lambda: clock[0]), patch.object(app.time, "sleep", side_effect=resume) as sleep:
            studio.switch(node.name, False)
        sleep.assert_called_once_with(0.1)
        self.assertGreaterEqual(executable.call_count, 3)
        node.press.assert_called_once_with(0)
        self.assertNotIn("checked", node.states)

    def test_inactive_after_scroll_reacquires_fresh_showing_node_without_repeating_action(self):
        old = self.Node("Enable speech output", 3, ("visible", "checked", "enabled", "sensitive"), pid=42)
        fresh = self.Node(old.name, 3, ("visible", "showing", "checked", "enabled", "sensitive"), pid=42)
        studio, frame, _ = self.fixture([old])
        clock = [0.0]
        def reveal(_):
            frame.children = [fresh]
            frame.states.discard("active")
            return True
        def resume(delay):
            old.press.assert_not_called()
            fresh.press.assert_not_called()
            clock[0] += delay
            frame.states.add("active")
        old.scroll.side_effect = reveal
        with patch.object(app, "process_parent", side_effect=self.parents), patch.object(app.Path, "resolve", return_value=studio.root / "acceptance-app"), patch.object(app, "phase"), patch.object(app.time, "monotonic", side_effect=lambda: clock[0]), patch.object(app.time, "sleep", side_effect=resume):
            studio.switch(old.name, False)
        old.scroll.assert_called_once_with(0)
        old.press.assert_not_called()
        fresh.scroll.assert_not_called()
        fresh.press.assert_called_once_with(0)

    def test_permanently_inactive_frame_refuses_within_original_deadline_without_action(self):
        node = self.Node("Enable speech output", 3, ("visible", "showing", "checked", "enabled", "sensitive"))
        studio, frame, _ = self.fixture([node])
        frame.states.discard("active")
        clock = [0.0]
        with patch.object(app, "process_parent", side_effect=self.parents), patch.object(app.Path, "resolve", return_value=studio.root / "acceptance-app"), patch.object(app, "phase"), patch.object(app.time, "monotonic", side_effect=lambda: clock[0]), patch.object(app.time, "sleep", side_effect=lambda delay: clock.__setitem__(0, clock[0] + delay)), self.assertRaisesRegex(provider.Refusal, "^app_window_not_active$"):
            studio.wait(studio.switch_control(node.name, {3, 4}), "app_control_missing", seconds=0.25, retry_inactive=True)
        self.assertGreaterEqual(clock[0], 0.25)
        self.assertLess(clock[0], 0.4)
        node.scroll.assert_not_called()
        node.press.assert_not_called()

    def test_reacquisition_does_not_retry_ownership_replacement_or_ambiguity(self):
        for change, expected in (("process", "app_process_exited"), ("executable", "unexpected_app_executable"), ("frame", "ambiguous_app_window")):
            node = self.Node("Enable speech output", 3, ("visible", "showing", "checked", "enabled", "sensitive"))
            studio, frame, desktop = self.fixture([node])
            frame.states.discard("active")
            clock, replaced = [0.0], [False]
            def parent(pid):
                if replaced[0] and change == "process":
                    raise provider.Refusal("app_process_exited")
                return self.parents(pid)
            def resume(delay):
                clock[0] += delay
                replaced[0] = True
                frame.states.add("active")
                if change == "frame":
                    desktop.children.append(self.Node("OrbitPal", 10, ("visible", "showing", "active")))
            def executable(**_):
                return Path("/disposable/foreign-app") if replaced[0] and change == "executable" else studio.root / "acceptance-app"
            with self.subTest(change=change), patch.object(app, "process_parent", side_effect=parent), patch.object(app.Path, "resolve", side_effect=executable), patch.object(app, "phase"), patch.object(app.time, "monotonic", side_effect=lambda: clock[0]), patch.object(app.time, "sleep", side_effect=resume) as sleep, self.assertRaisesRegex(provider.Refusal, "^" + expected + "$"):
                studio.switch(node.name, False)
            sleep.assert_called_once_with(0.1)
            node.scroll.assert_not_called()
            node.press.assert_not_called()

    def test_inactive_retries_do_not_reset_two_scroll_budget_or_extend_deadline(self):
        node = self.Node("Enable speech output", 3, ("visible",))
        studio, frame, _ = self.fixture([node])
        clock = [0.0]
        def reveal(_):
            frame.states.discard("active")
            return True
        def resume(delay):
            clock[0] += delay
            frame.states.add("active")
        node.scroll.side_effect = reveal
        with patch.object(app, "process_parent", side_effect=self.parents), patch.object(app.Path, "resolve", return_value=studio.root / "acceptance-app"), patch.object(app, "phase"), patch.object(app.time, "monotonic", side_effect=lambda: clock[0]), patch.object(app.time, "sleep", side_effect=resume), self.assertRaisesRegex(provider.Refusal, "^app_control_missing$"):
            studio.wait(studio.switch_control(node.name, {3, 4}), "app_control_missing", seconds=0.35, retry_inactive=True)
        self.assertEqual(node.scroll.call_count, 2)
        self.assertLess(clock[0], 0.5)
        node.press.assert_not_called()

    def test_inactive_does_not_change_other_waits_or_retry_other_refusals(self):
        studio, _, _ = self.fixture([])
        for retry, failure in ((False, provider.Refusal("app_window_not_active")), (True, provider.Refusal("app_action_unavailable")), (True, provider.Refusal("ambiguous_app_control")), (True, provider.Refusal("foreign_process"))):
            predicate = Mock(side_effect=failure)
            with self.subTest(retry=retry, code=failure.code), patch.object(app, "process_parent", side_effect=self.parents), patch.object(app.time, "monotonic", return_value=0), patch.object(app.time, "sleep") as sleep, self.assertRaises(provider.Refusal) as caught:
                studio.wait(predicate, "app_control_missing", retry_inactive=retry)
            self.assertIs(caught.exception, failure)
            predicate.assert_called_once_with()
            sleep.assert_not_called()

    def test_expired_inactive_wait_does_not_poll_or_act_after_focus_returns(self):
        node = self.Node("Enable speech output", 3, ("visible", "showing", "checked", "enabled", "sensitive"))
        studio, frame, _ = self.fixture([node])
        frame.states.discard("active")
        clock = [0.0]
        def late_resume(_):
            clock[0] = 1.0
            frame.states.add("active")
        with patch.object(app, "process_parent", side_effect=self.parents), patch.object(app.Path, "resolve", return_value=studio.root / "acceptance-app") as executable, patch.object(app, "phase"), patch.object(app.time, "monotonic", side_effect=lambda: clock[0]), patch.object(app.time, "sleep", side_effect=late_resume), self.assertRaisesRegex(provider.Refusal, "^app_window_not_active$"):
            studio.wait(studio.switch_control(node.name, {3, 4}), "app_control_missing", seconds=0.25, retry_inactive=True)
        executable.assert_called_once_with(strict=True)
        node.scroll.assert_not_called()
        node.press.assert_not_called()

    def test_reveal_never_guesses_names_or_roles(self):
        node = self.Node("Enable speech output extra", 3, ("visible",))
        studio, _, _ = self.fixture([node])
        with patch.object(app, "process_parent", side_effect=self.parents), patch.object(app.Path, "resolve", return_value=studio.root / "acceptance-app"), patch.object(app, "phase"):
            self.assertIsNone(studio.switch_control("Enable speech output", {3, 4})())
            with self.assertRaisesRegex(provider.Refusal, "app_control_missing"):
                studio.switch_control(node.name, {3, 4})
        node.scroll.assert_not_called()
        node.press.assert_not_called()


class ActionCapabilityDiagnosticTests(unittest.TestCase):
    """Production action() with local AX doubles; no GUI/input/capture executes."""
    def fixture(self, names=(), offered=None, succeeds=True):
        studio = app.Studio.__new__(app.Studio)
        studio.enabled = Mock(return_value=True)
        interface = SimpleNamespace(nActions=len(names) if offered is None else offered,
                                    getName=Mock(side_effect=lambda index: names[index]),
                                    doAction=Mock(return_value=succeeds))
        node = SimpleNamespace(queryAction=Mock(return_value=interface))
        return studio, node, interface

    def refused(self, studio, node):
        errors = io.StringIO()
        with patch.object(app.sys, "stderr", errors), self.assertRaises(provider.Refusal) as raised:
            studio.action(node, scroll=False)
        self.assertEqual(raised.exception.code, "app_action_unavailable")
        lines = errors.getvalue().splitlines()
        self.assertEqual(len(lines), 1)
        self.assertTrue(lines[0].startswith("app-action-capabilities: "))
        self.assertLess(len(lines[0]), 384)
        result = json.loads(lines[0].removeprefix("app-action-capabilities: "))
        self.assertEqual(set(result), {"version", "offeredActionCountValid", "offeredActionCount",
                                      "offeredActionCountCapped", "examinedActionCount", "activateCount",
                                      "checkCount", "uncheckCount", "otherCount"})
        self.assertTrue(all(type(value) in (int, bool) for value in result.values()))
        return result, errors.getvalue()

    def test_zero_actions_is_distinct_from_an_unrecognized_offered_action(self):
        studio, node, interface = self.fixture()
        result, _ = self.refused(studio, node)
        self.assertEqual(result["offeredActionCount"], 0)
        self.assertEqual(result["examinedActionCount"], 0)
        self.assertTrue(result["offeredActionCountValid"])
        self.assertFalse(result["offeredActionCountCapped"])
        interface.getName.assert_not_called()
        interface.doAction.assert_not_called()
        studio, node, interface = self.fixture(["PRIVATE_AX_URL_AND_PATH"])
        result, output = self.refused(studio, node)
        self.assertEqual(result["offeredActionCount"], 1)
        self.assertEqual(result["otherCount"], 1)
        self.assertNotIn("PRIVATE_AX", output)
        interface.getName.assert_called_once_with(0)
        interface.doAction.assert_not_called()

    def test_authored_alias_buckets_are_diagnostic_only_and_never_input(self):
        studio, node, interface = self.fixture(["Activate", "CHECK", "uncheck", "PRIVATE_CONTROL_NAME"])
        result, output = self.refused(studio, node)
        self.assertEqual([result[key] for key in ("activateCount", "checkCount", "uncheckCount", "otherCount")], [1, 1, 1, 1])
        self.assertEqual(result["examinedActionCount"], 4)
        self.assertNotIn("PRIVATE_CONTROL_NAME", output)
        self.assertNotIn('"Activate"', output)
        self.assertEqual(interface.getName.call_count, 4)
        interface.doAction.assert_not_called()

    def test_large_offered_count_does_not_query_or_activate_beyond_original_eight(self):
        studio, node, interface = self.fixture(["private"] * 8 + ["press"], offered=10000)
        result, _ = self.refused(studio, node)
        self.assertEqual(result["offeredActionCount"], 8)
        self.assertTrue(result["offeredActionCountCapped"])
        self.assertEqual(result["examinedActionCount"], 8)
        self.assertEqual(result["otherCount"], 8)
        self.assertEqual([call.args for call in interface.getName.call_args_list], [(i,) for i in range(8)])
        interface.doAction.assert_not_called()

    def test_existing_mixed_case_execution_names_keep_their_original_behavior(self):
        for name in ("CLICK", "Press", "Toggle", "SELECT"):
            studio, node, interface = self.fixture(["private", name])
            errors = io.StringIO()
            with self.subTest(name=name), patch.object(app.sys, "stderr", errors):
                studio.action(node, scroll=False)
            interface.doAction.assert_called_once_with(1)
            self.assertEqual(interface.getName.call_count, 2)
            self.assertEqual(errors.getvalue(), "")

    def test_rejected_existing_action_keeps_its_refusal_and_no_capability_footer(self):
        studio, node, interface = self.fixture(["press"], succeeds=False)
        errors = io.StringIO()
        with patch.object(app.sys, "stderr", errors), self.assertRaisesRegex(provider.Refusal, "^app_action_refused$"):
            studio.action(node, scroll=False)
        interface.doAction.assert_called_once_with(0)
        self.assertEqual(errors.getvalue(), "")

    def test_accessor_failures_never_print_raw_values_or_invent_zero_actions(self):
        for broken in ("count", "name"):
            studio, node, interface = self.fixture(["private"])
            if broken == "count":
                class BrokenCount:
                    @property
                    def nActions(self):
                        raise RuntimeError("PRIVATE_AX_COUNT_VALUE")
                node.queryAction.return_value = BrokenCount()
            else:
                interface.getName.side_effect = RuntimeError("PRIVATE_AX_NAME_VALUE")
            output, errors = io.StringIO(), io.StringIO()
            with self.subTest(broken=broken), patch.object(app, "isolated_root", return_value=object()), patch.object(app, "run", side_effect=lambda _: studio.action(node, scroll=False)), patch.object(app, "_emitted_phases", set()), patch.object(app.sys, "stdout", output), patch.object(app.sys, "stderr", errors):
                self.assertEqual(app.main(), 1)
            self.assertNotIn("PRIVATE_AX", output.getvalue() + errors.getvalue())
            self.assertNotIn("app-action-capabilities", errors.getvalue())
            self.assertIn("app-diagnostic: ", errors.getvalue())
            self.assertIn("controller_failed", errors.getvalue())
            interface.doAction.assert_not_called()

    def test_disabled_control_keeps_existing_guard_without_any_query_or_diagnostic(self):
        studio, node, interface = self.fixture(["press"])
        studio.enabled.return_value = False
        errors = io.StringIO()
        with patch.object(app.sys, "stderr", errors), self.assertRaisesRegex(provider.Refusal, "^app_control_disabled$"):
            studio.action(node, scroll=False)
        node.queryAction.assert_not_called()
        interface.doAction.assert_not_called()
        self.assertEqual(errors.getvalue(), "")

    def test_each_terminal_observation_is_local_and_malformed_negative_count_is_flagged(self):
        studio, node, _ = self.fixture(["private", "private"])
        result, _ = self.refused(studio, node)
        self.assertEqual(result["otherCount"], 2)
        studio, node, _ = self.fixture()
        result, _ = self.refused(studio, node)
        self.assertEqual(result["otherCount"], 0)
        studio, node, interface = self.fixture(offered=-1)
        result, _ = self.refused(studio, node)
        self.assertFalse(result["offeredActionCountValid"])
        self.assertEqual(result["offeredActionCount"], 0)
        self.assertEqual(result["examinedActionCount"], 0)
        interface.getName.assert_not_called()
        interface.doAction.assert_not_called()



class StudioFocusTests(unittest.TestCase):
    """Controlled AT-SPI focus attempts; no native app, desktop or focus is used."""
    Node = SwitchRevealTests.Node
    fixture = SwitchRevealTests.fixture
    parents = SwitchRevealTests.parents

    def control(self):
        return self.Node("Enable speech output", 3,
                         ("visible", "showing", "checked", "enabled", "sensitive"), pid=42)

    def focus(self, frame, effect=None):
        request = Mock(return_value=True, side_effect=effect)
        frame.queryComponent = Mock(return_value=SimpleNamespace(grabFocus=request))
        return request

    def test_one_native_request_requires_fresh_active_readback_before_real_switch_action(self):
        old, fresh = self.control(), self.control()
        studio, frame, _ = self.fixture([old])
        frame.states.discard("active")
        def activate():
            old.press.assert_not_called()
            fresh.press.assert_not_called()
            frame.states.add("active")
            frame.children = [fresh]
            return True
        request = self.focus(frame, activate)
        with patch.object(app, "process_parent", side_effect=self.parents), patch.object(app.Path, "resolve", return_value=studio.root / "acceptance-app") as executable, patch.object(app, "phase"):
            studio.switch(old.name, False, focus=True)
        request.assert_called_once_with()
        self.assertGreaterEqual(executable.call_count, 3)
        old.press.assert_not_called()
        fresh.press.assert_called_once_with(0)
        old.scroll.assert_not_called()
        fresh.scroll.assert_not_called()

    def test_true_native_return_without_active_readback_never_authorizes_input_or_retries_focus(self):
        node = self.control()
        studio, frame, _ = self.fixture([node])
        frame.states.discard("active")
        request = self.focus(frame)
        clock = [0.0]
        with patch.object(app, "process_parent", side_effect=self.parents), patch.object(app.Path, "resolve", return_value=studio.root / "acceptance-app"), patch.object(app, "phase"), patch.object(app.time, "monotonic", side_effect=lambda: clock[0]), patch.object(app.time, "sleep", side_effect=lambda delay: clock.__setitem__(0, clock[0] + delay)), self.assertRaisesRegex(provider.Refusal, "^app_window_not_active$"):
            locate = studio.switch_control(node.name, {3, 4})
            studio.wait(studio.focus_once(locate, seconds=0.25), "app_control_missing", seconds=0.25, retry_inactive=True)
        request.assert_called_once_with()
        self.assertGreaterEqual(clock[0], 0.25)
        self.assertLess(clock[0], 0.4)
        node.scroll.assert_not_called()
        node.press.assert_not_called()

    def test_refused_or_unsupported_native_focus_is_terminal_without_input(self):
        for response, expected in ((False, "app_action_refused"), (RuntimeError("private"), "app_action_unavailable"), (AttributeError("private"), "app_action_unavailable"), (NotImplementedError("private"), "app_action_unavailable")):
            node = self.control()
            studio, frame, _ = self.fixture([node])
            request = self.focus(frame)
            if isinstance(response, BaseException):
                request.side_effect = response
            else:
                request.return_value = response
            with self.subTest(response=type(response).__name__), patch.object(app, "process_parent", side_effect=self.parents), patch.object(app.Path, "resolve", return_value=studio.root / "acceptance-app"), patch.object(app, "phase"), patch.object(app.time, "sleep") as sleep, self.assertRaisesRegex(provider.Refusal, "^" + expected + "$"):
                studio.switch(node.name, False, focus=True)
            request.assert_called_once_with()
            sleep.assert_not_called()
            node.press.assert_not_called()
            node.scroll.assert_not_called()

    def test_focus_target_must_be_the_unique_owned_visible_showing_fixed_studio_frame(self):
        for change in ("hidden", "offscreen", "foreign", "wrong_name", "wrong_role", "duplicate"):
            node = self.control()
            studio, frame, desktop = self.fixture([node])
            request = self.focus(frame)
            if change == "hidden": frame.states.discard("visible")
            elif change == "offscreen": frame.states.discard("showing")
            elif change == "foreign": frame.pid = 99
            elif change == "wrong_name": frame.name = "other"
            elif change == "wrong_role": frame.role = 5
            else: desktop.children.append(self.Node("OrbitPal", 10, ("visible", "showing", "active")))
            with self.subTest(change=change), patch.object(app, "process_parent", side_effect=self.parents), patch.object(app.Path, "resolve", return_value=studio.root / "acceptance-app"), patch.object(app, "phase"), self.assertRaisesRegex(provider.Refusal, "^ambiguous_app_window$"):
                studio.switch(node.name, False, focus=True)
            request.assert_not_called()
            node.press.assert_not_called()

    def test_process_and_executable_guards_precede_the_focus_request(self):
        for change in ("process", "executable"):
            node = self.control()
            studio, frame, _ = self.fixture([node])
            request = self.focus(frame)
            expected = "app_process_exited" if change == "process" else "unexpected_app_executable"
            parent = Mock(side_effect=provider.Refusal(expected)) if change == "process" else self.parents
            executable = Path("/disposable/foreign-app") if change == "executable" else studio.root / "acceptance-app"
            with self.subTest(change=change), patch.object(app, "process_parent", side_effect=parent), patch.object(app.Path, "resolve", return_value=executable), patch.object(app, "phase"), self.assertRaisesRegex(provider.Refusal, "^" + expected + "$"):
                studio.switch(node.name, False, focus=True)
            request.assert_not_called()
            node.press.assert_not_called()

    def test_replacement_or_ambiguity_after_native_request_is_not_focus_success(self):
        for change, expected in (("process", "app_process_exited"), ("executable", "unexpected_app_executable"), ("duplicate", "ambiguous_app_window")):
            node = self.control()
            studio, frame, desktop = self.fixture([node])
            requested = [False]
            def activate():
                requested[0] = True
                frame.states.add("active")
                if change == "duplicate": desktop.children.append(self.Node("OrbitPal", 10, ("visible", "showing", "active")))
                return True
            def parent(pid):
                if requested[0] and change == "process": raise provider.Refusal("app_process_exited")
                return self.parents(pid)
            def executable(**_):
                return Path("/disposable/foreign-app") if requested[0] and change == "executable" else studio.root / "acceptance-app"
            request = self.focus(frame, activate)
            with self.subTest(change=change), patch.object(app, "process_parent", side_effect=parent), patch.object(app.Path, "resolve", side_effect=executable), patch.object(app, "phase"), self.assertRaisesRegex(provider.Refusal, "^" + expected + "$"):
                studio.switch(node.name, False, focus=True)
            request.assert_called_once_with()
            node.press.assert_not_called()
            node.scroll.assert_not_called()

    def test_late_native_return_cannot_extend_original_wait_or_authorize_input(self):
        node = self.control()
        studio, frame, _ = self.fixture([node])
        clock = [0.0]
        def late_activate():
            clock[0] = 12.1
            frame.states.add("active")
            return True
        request = self.focus(frame, late_activate)
        with patch.object(app, "process_parent", side_effect=self.parents), patch.object(app.Path, "resolve", return_value=studio.root / "acceptance-app"), patch.object(app, "phase"), patch.object(app.time, "monotonic", side_effect=lambda: clock[0]), patch.object(app.time, "sleep") as sleep, self.assertRaisesRegex(provider.Refusal, "^app_focus_not_confirmed$"):
            studio.switch(node.name, False, focus=True)
        request.assert_called_once_with()
        sleep.assert_not_called()
        node.press.assert_not_called()
        node.scroll.assert_not_called()

    def test_late_frame_guard_cannot_issue_a_focus_request_after_deadline(self):
        node = self.control()
        studio, frame, _ = self.fixture([node])
        clock = [0.0]
        original = studio.studio_frame
        def late_guard(**kwargs):
            result = original(**kwargs)
            clock[0] = 12.1
            return result
        request = self.focus(frame)
        with patch.object(app, "process_parent", side_effect=self.parents), patch.object(app.Path, "resolve", return_value=studio.root / "acceptance-app"), patch.object(app, "phase"), patch.object(studio, "studio_frame", side_effect=late_guard), patch.object(app.time, "monotonic", side_effect=lambda: clock[0]), patch.object(app.time, "sleep") as sleep, self.assertRaisesRegex(provider.Refusal, "^app_focus_not_confirmed$"):
            studio.switch(node.name, False, focus=True)
        request.assert_not_called()
        sleep.assert_not_called()
        node.press.assert_not_called()
        node.scroll.assert_not_called()

    def test_default_switch_does_not_request_focus(self):
        node = self.control()
        studio, frame, _ = self.fixture([node])
        request = self.focus(frame)
        with patch.object(app, "process_parent", side_effect=self.parents), patch.object(app.Path, "resolve", return_value=studio.root / "acceptance-app"), patch.object(app, "phase"):
            studio.switch(node.name, False)
        request.assert_not_called()
        node.press.assert_called_once_with(0)

    def test_only_first_voice_configuration_explicitly_opts_into_focus(self):
        tree = ast.parse((DIRECTORY / "app.py").read_text())
        run = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == "run")
        requests = [node for node in ast.walk(run) if isinstance(node, ast.Call) and isinstance(node.func, ast.Attribute) and node.func.attr == "switch" and any(keyword.arg == "focus" for keyword in node.keywords)]
        self.assertEqual(len(requests), 1)
        self.assertEqual([ast.literal_eval(value) for value in requests[0].args], ["Enable speech output", False])
        self.assertEqual([(value.arg, ast.literal_eval(value.value)) for value in requests[0].keywords], [("focus", True)])

class ObservedSwitchActionTests(unittest.TestCase):
    """Real switch/action paths with authored AX data; no desktop/input runs."""
    def fixture(self, checked=True, names=("uncheck",), changes=True):
        source = SwitchRevealTests()
        states = ["visible", "showing", "enabled", "sensitive"] + (["checked"] if checked else [])
        node = source.Node("Enable speech output", 3, states, pid=42)
        studio, frame, desktop = source.fixture([node])
        interface = SimpleNamespace(nActions=len(names), getName=Mock(side_effect=lambda index: names[index]),
                                    doAction=Mock(side_effect=node.toggle if changes else lambda _: True))
        node.queryAction = Mock(return_value=interface)
        return source, studio, node, frame, desktop, interface

    def test_observed_uncheck_runs_only_for_an_explicit_off_switch_and_has_readback(self):
        source, studio, node, _, _, interface = self.fixture()
        errors = io.StringIO()
        with patch.object(app, "process_parent", side_effect=source.parents), patch.object(app.Path, "resolve", return_value=studio.root / "acceptance-app") as executable, patch.object(app, "phase"), patch.object(app.sys, "stderr", errors):
            studio.switch(node.name, False)
        interface.doAction.assert_called_once_with(0)
        self.assertNotIn("checked", node.states)
        self.assertGreaterEqual(executable.call_count, 3)  # Acquire, refresh, readback.
        self.assertEqual(errors.getvalue(), "")

    def test_default_and_on_requests_do_not_accept_uncheck_or_other_diagnostic_aliases(self):
        for names in (("uncheck",), ("check",), ("activate",)):
            for target in (None, True):
                source, studio, node, _, _, interface = self.fixture(names=names)
                errors = io.StringIO()
                with self.subTest(names=names, target=target), patch.object(app.sys, "stderr", errors), self.assertRaisesRegex(provider.Refusal, "^app_action_unavailable$"):
                    studio.action(node, scroll=False, switch_target=target)
                interface.doAction.assert_not_called()
        source, studio, node, _, _, interface = self.fixture(checked=False)
        errors = io.StringIO()
        with patch.object(app, "process_parent", side_effect=source.parents), patch.object(app.Path, "resolve", return_value=studio.root / "acceptance-app"), patch.object(app, "phase"), patch.object(app.sys, "stderr", errors), self.assertRaisesRegex(provider.Refusal, "^app_action_unavailable$"):
            studio.switch(node.name, True)
        interface.doAction.assert_not_called()

    def test_fresh_already_off_node_requires_no_action_or_new_input(self):
        source, studio, old, frame, _, interface = self.fixture()
        fresh = source.Node(old.name, 3, ("visible", "showing", "enabled", "sensitive"), pid=42)
        fresh.queryAction = Mock(return_value=interface)
        with patch.object(app, "process_parent", side_effect=source.parents):
            studio.action(old, scroll=False, switch_target=False, switch_current=Mock(return_value=fresh))
        fresh.queryAction.assert_not_called()
        interface.doAction.assert_not_called()
        old.scroll.assert_not_called()

    def test_wrong_name_role_state_or_foreign_node_never_receives_directional_input(self):
        for kind in ("name", "role", "hidden", "not_showing", "disabled", "foreign"):
            source, studio, old, _, _, interface = self.fixture()
            states = {"visible", "showing", "enabled", "sensitive", "checked"}
            if kind == "hidden": states.remove("visible")
            if kind == "not_showing": states.remove("showing")
            if kind == "disabled": states.remove("enabled")
            fresh = source.Node("PRIVATE_WRONG_NAME" if kind == "name" else old.name, 5 if kind == "role" else 3, states, pid=99 if kind == "foreign" else 42)
            fresh.queryAction = Mock(return_value=interface)
            with self.subTest(kind=kind), patch.object(app, "process_parent", side_effect=source.parents), self.assertRaises(provider.Refusal):
                studio.action(old, scroll=False, switch_target=False, switch_current=Mock(return_value=fresh))
            fresh.queryAction.assert_not_called()
            interface.doAction.assert_not_called()

    def test_stale_process_executable_or_inactive_frame_is_rejected_on_refresh(self):
        for kind, code in (("process", "app_process_exited"), ("executable", "unexpected_app_executable"), ("inactive", "app_window_not_active"), ("duplicate", "ambiguous_app_window")):
            source, studio, node, frame, desktop, interface = self.fixture()
            retired = [False]
            original = studio.action
            def retire(*args, **kwargs):
                retired[0] = True
                if kind == "inactive": frame.states.discard("active")
                if kind == "duplicate": desktop.children.append(source.Node("OrbitPal", 10, ("visible", "showing", "active")))
                return original(*args, **kwargs)
            def parent(pid):
                if retired[0] and kind == "process": raise provider.Refusal("app_process_exited")
                return source.parents(pid)
            def executable(**_):
                return Path("/disposable/foreign-app") if retired[0] and kind == "executable" else studio.root / "acceptance-app"
            with self.subTest(kind=kind), patch.object(studio, "action", side_effect=retire), patch.object(app, "process_parent", side_effect=parent), patch.object(app.Path, "resolve", side_effect=executable), patch.object(app, "phase"), self.assertRaisesRegex(provider.Refusal, "^" + code + "$"):
                studio.switch(node.name, False)
            interface.doAction.assert_not_called()

    def test_directional_action_refusal_or_missing_readback_cannot_count_as_success(self):
        source, studio, node, _, _, interface = self.fixture()
        interface.doAction.side_effect = None
        interface.doAction.return_value = False
        with patch.object(app, "process_parent", side_effect=source.parents), patch.object(app.Path, "resolve", return_value=studio.root / "acceptance-app"), patch.object(app, "phase"), self.assertRaisesRegex(provider.Refusal, "^app_action_refused$"):
            studio.switch(node.name, False)
        interface.doAction.assert_called_once_with(0)
        source, studio, node, _, _, interface = self.fixture(changes=False)
        clock = [0.0]
        with patch.object(app, "process_parent", side_effect=source.parents), patch.object(app.Path, "resolve", return_value=studio.root / "acceptance-app"), patch.object(app, "phase"), patch.object(app.time, "monotonic", side_effect=lambda: clock[0]), patch.object(app.time, "sleep", side_effect=lambda delay: clock.__setitem__(0, clock[0] + delay)), self.assertRaisesRegex(provider.Refusal, "^app_switch_not_changed$"):
            studio.switch(node.name, False)
        interface.doAction.assert_called_once_with(0)
        self.assertIn("checked", node.states)

    def test_explicit_off_path_requires_its_fresh_locator_and_never_adds_scroll_or_ninth_action(self):
        source, studio, node, _, _, interface = self.fixture(names=("private",) * 8 + ("uncheck",))
        for kwargs in ({"switch_target": False}, {"switch_target": False, "switch_current": 1}, {"switch_target": False, "switch_current": lambda: node, "scroll": True}):
            with self.subTest(kwargs=tuple(kwargs)), self.assertRaisesRegex(provider.Refusal, "^app_action_unavailable$"):
                studio.action(node, **({"scroll": False} | kwargs))
        interface.getName.assert_not_called()
        errors = io.StringIO()
        with patch.object(app, "process_parent", side_effect=source.parents), patch.object(app.sys, "stderr", errors), self.assertRaisesRegex(provider.Refusal, "^app_action_unavailable$"):
            studio.action(node, scroll=False, switch_target=False, switch_current=lambda: node)
        self.assertEqual(interface.getName.call_count, 8)
        interface.doAction.assert_not_called()
        node.scroll.assert_not_called()

    def test_off_path_does_not_accept_unobserved_check_or_activate_actions(self):
        for name in ("check", "activate"):
            source, studio, node, _, _, interface = self.fixture(names=(name,))
            errors = io.StringIO()
            with self.subTest(name=name), patch.object(app, "process_parent", side_effect=source.parents), patch.object(app.sys, "stderr", errors), self.assertRaisesRegex(provider.Refusal, "^app_action_unavailable$"):
                studio.action(node, scroll=False, switch_target=False, switch_current=lambda: node)
            interface.doAction.assert_not_called()



if __name__ == "__main__":
    unittest.main()
