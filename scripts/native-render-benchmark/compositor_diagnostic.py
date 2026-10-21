"""Failed-run-only bounded tail of the fixed private GNOME stderr pipe.

No path/log reader, app stream, pass criterion or measurement authority.
"""
import fcntl
import json
import os
import re
import select
import stat
import threading
import time
import unicodedata

from collect import PinnedDirectory, refuse

LIMIT = 64 * 1024
TEXT_NAME = "compositor-startup-diagnostic.txt"
META_NAME = "compositor-startup-diagnostic.json"


def checked_write_pipe(descriptor):
    """Validate the inherited root-created channel before the UID transition."""
    if type(descriptor) is not int or not 3 <= descriptor <= 2**31 - 1:
        refuse("invalid_compositor_diagnostic_pipe")
    value = os.fstat(descriptor)
    if (not stat.S_ISFIFO(value.st_mode) or value.st_uid != 0 or value.st_mode & 0o077
            or fcntl.fcntl(descriptor, fcntl.F_GETFL) & os.O_ACCMODE != os.O_WRONLY):
        refuse("invalid_compositor_diagnostic_pipe")


def sanitize_tail(raw, redactions):
    """Decode at most64KiB, redact known paths, and return a bounded UTF-8 tail."""
    if type(raw) is not bytes or len(raw) > LIMIT:
        refuse("invalid_compositor_diagnostic_tail")
    text = raw.decode("utf-8", errors="replace")
    # Remove complete ANSI CSI/OSC escapes before discarding every remaining
    # Unicode control/format character except the diagnostic's newlines/tabs.
    text = re.sub(r"\x1b(?:\[[0-?]*[ -/]*[@-~]|\][^\x07\x1b]*(?:\x07|\x1b\\))", "", text)
    text = text.replace("\r\n", "\n").replace("\r", "\n")
    text = "".join(char for char in text if char in "\n\t" or not unicodedata.category(char).startswith("C"))
    for path, label in sorted(redactions, key=lambda pair: len(pair[0]), reverse=True):
        if type(path) is not str or not path.startswith("/") or label not in (
                "<fixture>", "<checkout>", "<runner-temp>", "<runner-home>"):
            refuse("invalid_compositor_diagnostic_redaction")
        text = text.replace(path, label)
    encoded = text.encode("utf-8")
    truncated = len(encoded) > LIMIT
    # A suffix may start inside a code point after replacement expansion.
    return encoded[-LIMIT:].decode("utf-8", errors="ignore").encode("utf-8"), truncated


class CompositorDiagnostic:
    def __init__(self, deadline):
        self.deadline = deadline  # Original runner deadline, never renewed.
        self.tail = bytearray()
        self.truncated = self.eof = self.read_failed = self.deadline_reached = False
        self.finished = False
        self.stop = threading.Event()
        self.reader, self.writer = os.pipe()
        try:
            os.set_inheritable(self.reader, False)
            os.set_inheritable(self.writer, False)
            os.set_blocking(self.reader, False)
            self.thread = threading.Thread(target=self._read, name="fixed-compositor-stderr", daemon=True)
            self.thread.start()
        except BaseException:
            os.close(self.reader)
            os.close(self.writer)
            raise

    def _read(self):
        try:
            while not self.stop.is_set():
                remaining = self.deadline - time.monotonic()
                if remaining <= 0:
                    self.deadline_reached = True
                    break
                ready, _, _ = select.select([self.reader], [], [], min(0.1, remaining))
                if self.stop.is_set():
                    break
                if not ready:
                    continue
                try:
                    chunk = os.read(self.reader, 4096)
                except BlockingIOError:
                    continue
                if not chunk:
                    self.eof = True
                    break
                overflow = max(0, len(self.tail) + len(chunk) - LIMIT)
                self.truncated = self.truncated or overflow > 0
                if overflow:
                    del self.tail[:overflow]
                self.tail.extend(chunk)
        except Exception:
            self.read_failed = True  # Never retain/format an OS exception.
        finally:
            try:
                os.close(self.reader)
            except Exception:
                self.read_failed = True

    def close_writer(self):
        if self.writer is not None:
            os.close(self.writer)
            self.writer = None

    def finish(self):
        self.close_writer()
        self.thread.join(timeout=0.25)  # Bounded own-reader cleanup, no native deadline extension.
        self.stop.set()
        self.thread.join(timeout=0.25)
        self.finished = not self.thread.is_alive()
        # The reader exclusively closes its FD. Never reuse that FD number
        # while an unfinished thread could still be returning from select.

    def export_failed(self, output, *, failed, writers_stopped, redactions):
        if failed is not True or writers_stopped is not True or self.finished is not True:
            self.tail.clear()
            return
        complete = self.eof and not self.read_failed and not self.deadline_reached
        text, expanded = sanitize_tail(bytes(self.tail), redactions) if complete else (b"", False)
        self.tail.clear()
        metadata = {"version": 1, "failedDiagnostic": True, "source": "fixed_gnome_shell_stderr",
                    "tailBytes": len(text), "truncated": self.truncated or expanded,
                    "eofObserved": self.eof, "deadlineReached": self.deadline_reached,
                    "captureComplete": complete, "measurementQualified": False}
        with PinnedDirectory(output) as destination:
            destination.write_new(META_NAME, json.dumps(metadata, sort_keys=True) + "\n")
            if complete:
                with destination.open_new(TEXT_NAME) as stream:
                    stream.buffer.write(text)
                    stream.flush()
                    destination.linked_file(TEXT_NAME, stream.fileno())
