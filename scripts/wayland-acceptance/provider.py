#!/usr/bin/python3
"""Bounded loopback SSE receiver. JPEGs are decoded in memory, never recorded.

This substitutes only the model's HTTP transport in an actual-app acceptance
job. Portal selection, capture, grants, request construction and cancellation
remain the real application paths. All published failures are fixed codes.
"""
import base64
import hashlib
import io
import json
import re
import socket
import socketserver
import threading
import time

MAX_HEADERS = 8192
MAX_BODY = 20 * 1024 * 1024
MAX_IMAGE = 5 * 1024 * 1024
CASES = ("fresh_red", "fresh_blue", "held_stop", "held_images_off", "held_disable")


class Refusal(Exception):
    def __init__(self, code):
        super().__init__(code)
        self.code = code


def read_request(connection):
    """No chunked bodies, arbitrary request paths or unbounded HTTP parsing."""
    deadline = time.monotonic() + 10
    def receive(limit):
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise Refusal("request_deadline")
        connection.settimeout(remaining)
        return connection.recv(limit)
    data = bytearray()
    while b"\r\n\r\n" not in data:
        chunk = receive(2048)
        if not chunk:
            raise Refusal("request_incomplete")
        data.extend(chunk)
        if b"\r\n\r\n" not in data and len(data) > MAX_HEADERS:
            raise Refusal("header_limit")
    header, body = bytes(data).split(b"\r\n\r\n", 1)
    if len(header) > MAX_HEADERS:
        raise Refusal("header_limit")
    lines = header.split(b"\r\n")
    if not lines or lines[0] != b"POST /v1/chat/completions HTTP/1.1" or len(lines) > 48:
        raise Refusal("request_scope")
    fields = {}
    for line in lines[1:]:
        if b":" not in line:
            raise Refusal("invalid_headers")
        name, value = line.split(b":", 1)
        name = name.strip().lower()
        if not re.fullmatch(rb"[a-z0-9-]{1,80}", name) or name in fields:
            raise Refusal("invalid_headers")
        fields[name] = value.strip()
    if any(key in fields for key in (b"authorization", b"x-api-key", b"xi-api-key", b"transfer-encoding", b"expect")):
        raise Refusal("unexpected_auth_or_transport")
    if fields.get(b"content-type") != b"application/json":
        raise Refusal("unexpected_content_type")
    raw_length = fields.get(b"content-length", b"")
    if not re.fullmatch(rb"[0-9]{1,8}", raw_length):
        raise Refusal("invalid_length")
    length = int(raw_length)
    if not 0 < length <= MAX_BODY or len(body) > length:
        raise Refusal("body_limit")
    payload = bytearray(body)
    while len(payload) < length:
        chunk = receive(min(65536, length - len(payload)))
        if not chunk:
            raise Refusal("request_incomplete")
        payload.extend(chunk)
    try:
        return json.loads(payload)
    except (UnicodeError, ValueError, RecursionError):
        raise Refusal("invalid_json") from None


def inspect_image(value, color):
    from PIL import Image
    if not isinstance(value, str) or not value.startswith("data:image/jpeg;base64,"):
        raise Refusal("not_production_jpeg")
    encoded = value[len("data:image/jpeg;base64,"):]
    if len(encoded) > 4 * ((MAX_IMAGE + 2) // 3):
        raise Refusal("image_limit")
    try:
        raw = base64.b64decode(encoded, validate=True)
    except (ValueError, UnicodeError):
        raise Refusal("invalid_base64") from None
    if not 0 < len(raw) <= MAX_IMAGE:
        raise Refusal("image_limit")
    try:
        with Image.open(io.BytesIO(raw)) as image:
            width, height = image.size
            if image.format != "JPEG" or not 0 < width <= 1280 or not 0 < height <= 1280:
                raise Refusal("jpeg_dimensions")
            image.load()
            # The app window must separately be proven clear of this edge band.
            rgb = image.convert("RGB")
            x, y = int(width * 0.97), int(height * 0.5)
            pixels = [rgb.getpixel((x + dx, y + dy)) for dx in (-1, 0, 1) for dy in (-1, 0, 1)]
            if any(not (pixel[0] > 160 and pixel[2] < 110) if color == "red"
                   else not (pixel[2] > 160 and pixel[0] < 110) for pixel in pixels):
                raise Refusal("fresh_marker_mismatch")
    except Refusal:
        raise
    except Exception:
        raise Refusal("jpeg_decode_failed") from None
    # Only a digest and bounded numeric metadata survive the local call; pixels
    # and Base64 are never retained in State or a request/result file.
    return {"digest": hashlib.sha256(raw).digest(), "width": width, "height": height, "bytes": len(raw)}


def inspect_body(body, case, nonce, color):
    if not isinstance(body, dict) or body.get("model") != "orbitpal-wayland-fixture" or body.get("stream") is not True:
        raise Refusal("wrong_model_or_stream")
    if body.get("tools"):
        raise Refusal("tools_must_be_disabled")
    messages = body.get("messages")
    if not isinstance(messages, list) or not 1 <= len(messages) <= 64:
        raise Refusal("message_limit")
    wanted = f"ORBITPAL_WAYLAND_{case}_{nonce}"
    user_texts, images = [], []
    for message in messages:
        if not isinstance(message, dict):
            raise Refusal("invalid_message")
        content = message.get("content")
        if message.get("role") == "user" and isinstance(content, str):
            user_texts.append(content)
        if isinstance(content, list):
            for block in content:
                if not isinstance(block, dict):
                    raise Refusal("invalid_block")
                if block.get("type") == "image_url":
                    if message.get("role") != "user" or not isinstance(block.get("image_url"), dict):
                        raise Refusal("invalid_image_role")
                    images.append(block["image_url"].get("url"))
    if not user_texts or wanted not in user_texts[-1] or len(images) != 1:
        raise Refusal("wrong_turn_or_image_count")
    return inspect_image(images[0], color)


class State:
    def __init__(self, nonce):
        if not re.fullmatch(r"[0-9a-f]{16}", nonce):
            raise Refusal("invalid_fixture_nonce")
        self.nonce = nonce
        self.condition = threading.Condition()
        self.plan = None
        self.records = {}
        self.failure = None
        self.closed = False

    def arm(self, case, color, held=False):
        with self.condition:
            if case not in CASES or color not in ("red", "blue") or case in self.records or self.plan is not None:
                raise Refusal("invalid_fixture_plan")
            self.plan = {"case": case, "color": color, "held": held, "released": not held}

    def accept(self, body):
        with self.condition:
            if self.plan is None or len(self.records) >= len(CASES):
                raise Refusal("unsolicited_inference")
            plan = self.plan
        metadata = inspect_body(body, plan["case"], self.nonce, plan["color"])
        with self.condition:
            self.records[plan["case"]] = metadata
            self.condition.notify_all()
            deadline = time.monotonic() + 30
            while not plan["released"] and not self.closed:
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise Refusal("held_response_deadline")
                self.condition.wait(min(remaining, 0.1))
            if self.closed:
                raise Refusal("receiver_closed")
            self.plan = None
        return plan["case"], "ORBITPAL_REPLY_" + plan["case"] + "_" + self.nonce

    def wait_received(self, case, timeout=15):
        with self.condition:
            deadline = time.monotonic() + timeout
            while case not in self.records:
                if self.failure:
                    raise Refusal(self.failure)
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise Refusal("inference_not_received")
                self.condition.wait(min(remaining, 0.1))
            return dict(self.records[case])

    def release(self, case):
        with self.condition:
            if self.plan is None or self.plan["case"] != case or case not in self.records:
                raise Refusal("wrong_release")
            self.plan["released"] = True
            self.condition.notify_all()

    def finished(self, case):
        with self.condition:
            self.records[case]["responseAttempted"] = True
            self.condition.notify_all()

    def wait_finished(self, case, timeout=5):
        with self.condition:
            deadline = time.monotonic() + timeout
            while not self.records.get(case, {}).get("responseAttempted"):
                if self.failure:
                    raise Refusal(self.failure)
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise Refusal("response_not_released")
                self.condition.wait(min(remaining, 0.1))

    def fail(self, code):
        with self.condition:
            self.failure = code
            self.condition.notify_all()

    def close(self):
        with self.condition:
            self.closed = True
            self.condition.notify_all()


class Handler(socketserver.BaseRequestHandler):
    def handle(self):
        try:
            body = read_request(self.request)
            case, text = self.server.state.accept(body)
            del body
            event = {"id": "synthetic-completion", "choices": [{"index": 0, "delta": {"content": text}, "finish_reason": None}]}
            final = {"choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}]}
            payload = ("data: " + json.dumps(event) + "\n\ndata: " + json.dumps(final) + "\n\ndata: [DONE]\n\n").encode()
            headers = ("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\nContent-Length: " + str(len(payload)) + "\r\n\r\n").encode()
            try:
                self.request.sendall(headers + payload)
            except (BrokenPipeError, ConnectionResetError):
                # Expected for a response released after actual Stop/revocation.
                pass
            self.server.state.finished(case)
        except Refusal as error:
            self.server.state.fail(error.code)
        except Exception:
            self.server.state.fail("receiver_failed")


class SerialServer(socketserver.TCPServer):
    request_queue_size = 1


class Receiver:
    def __init__(self, nonce):
        self.state = State(nonce)
        # Serial, bounded backlog; the app serializes its own inference turns.
        self.server = SerialServer(("127.0.0.1", 0), Handler, bind_and_activate=True)
        self.server.state = self.state
        self.thread = threading.Thread(target=self.server.serve_forever, kwargs={"poll_interval": 0.1}, daemon=True)

    @property
    def base_url(self):
        return "http://127.0.0.1:" + str(self.server.server_address[1]) + "/v1"

    def __enter__(self):
        self.thread.start()
        return self

    def __exit__(self, exception_type, _exception, _traceback):
        cleanup_failed = False
        try:
            self.state.close()
            self.server.shutdown()
        except Exception:
            cleanup_failed = True
        finally:
            try:
                self.server.server_close()
                self.thread.join(timeout=12)
                cleanup_failed = cleanup_failed or self.thread.is_alive()
            except Exception:
                cleanup_failed = True
        # A last handler may fail between the caller's final check and shutdown.
        # No success receipt is possible until join AND this final check. Keep
        # an original body failure instead of replacing it with a cleanup error.
        if exception_type is None:
            if cleanup_failed:
                raise Refusal("receiver_cleanup_failed")
            if self.state.failure:
                raise Refusal(self.state.failure)
        return False
