#!/usr/bin/python3
"""Local harness tests only: no app, AT-SPI, portal, compositor or real capture."""
import ast
import base64
import io
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


if __name__ == "__main__":
    unittest.main()
