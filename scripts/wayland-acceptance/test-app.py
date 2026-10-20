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


if __name__ == "__main__":
    unittest.main()
