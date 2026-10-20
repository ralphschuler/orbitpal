#!/usr/bin/python3
"""Supervise only the fresh fixture executable; never persist its raw output.

This is diagnostic collection for a disposable CI account, not an app launcher
API. Pattern counts are observations of messages, never attribution of a cause.
"""
import json
import os
import re
import selectors
import signal
import stat
import subprocess
import time

from app import isolated_root


MAX_LINE = 4096
MAX_BYTES = 1024 * 1024
MAX_LINES = 8192
LIFETIME_SECONDS = 240
TERM_SECONDS = 2
KILL_SECONDS = 2
DRAIN_SECONDS = 2
POLL_SECONDS = 0.05
CATEGORIES = (
    "rust_panic_message", "gtk_display_message", "bwrap_namespace_message",
    "webkit_sandbox_message", "dynamic_loader_message", "egl_init_message",
    "allocation_failure_message",
)
STOP_REASONS = frozenset(("none", "cleanup_marker", "supervisor_signal", "deadline",
                          "diagnostic_limit", "diagnostic_failure"))
STREAM_STATUSES = frozenset(("eof", "limit", "drain_timeout", "read_failed", "not_opened"))
_RUST_PANIC = re.compile(rb"^thread '[^'\r\n]{1,128}'(?: \([0-9]{1,10}\))? panicked at[ :]")
_ALLOCATION = re.compile(rb"^memory allocation of [0-9]{1,20} bytes failed$")


class RuntimeFilter:
    """At most one bounded byte line lives here; no text/values escape summary()."""

    def __init__(self):
        self.pending = bytearray()
        self.overlong = False
        self.bytes = 0
        self.lines = 0
        self.long_lines = 0
        self.unclassified = 0
        self.counts = dict.fromkeys(CATEGORIES, 0)
        self.limited = False
        self.finished = False

    def _line(self):
        self.lines += 1
        if self.overlong:
            self.long_lines += 1
        else:
            raw = bytes(self.pending).rstrip(b"\r")
            lower = raw.lower()
            matched = []
            if _RUST_PANIC.search(raw):
                matched.append("rust_panic_message")
            if b"gtk" in lower and (b"cannot open display" in lower or b"gtk_init_check() failed" in lower):
                matched.append("gtk_display_message")
            if b"bwrap:" in lower and b"namespace" in lower and any(
                    text in lower for text in (b"operation not permitted", b"permission denied", b"no permissions", b"creating new namespace failed")):
                matched.append("bwrap_namespace_message")
            if b"webkit" in lower and b"sandbox" in lower and any(text in lower for text in (b"failed", b"error", b"denied")):
                matched.append("webkit_sandbox_message")
            if any(text in lower for text in (b"error while loading shared libraries:", b"symbol lookup error:", b"error relocating ")):
                matched.append("dynamic_loader_message")
            if b"could not create egl display:" in lower or b"egl_not_initialized" in lower or b"failed to initialize egl" in lower:
                matched.append("egl_init_message")
            if _ALLOCATION.fullmatch(raw):
                matched.append("allocation_failure_message")
            if not matched:
                self.unclassified += 1
            for category in matched:
                self.counts[category] += 1
        self.pending.clear()
        self.overlong = False
        if self.lines >= MAX_LINES:
            self.limited = True

    def feed(self, chunk):
        if self.finished or self.limited:
            return
        # The production os.read is also capped. Never copy an arbitrary chunk.
        for value in memoryview(chunk)[:MAX_BYTES - self.bytes]:
            self.bytes += 1
            if value == 10:
                self._line()
            elif not self.overlong:
                if len(self.pending) < MAX_LINE:
                    self.pending.append(value)
                else:
                    self.pending.clear()
                    self.overlong = True
            if self.limited or self.bytes >= MAX_BYTES:
                self.limited = True
                break

    def finish(self, eof):
        if not self.finished:
            if eof and not self.limited and (self.pending or self.overlong):
                self._line()
            self.pending.clear()
            self.overlong = False
            self.finished = True

    def summary(self, stream_status):
        if stream_status not in STREAM_STATUSES:
            stream_status = "read_failed"
        return {"version": 1, "status": stream_status, "bytesObserved": self.bytes,
                "linesObserved": self.lines, "overlongLines": self.long_lines,
                "unclassifiedLines": self.unclassified, "limitReached": self.limited,
                "observations": dict(self.counts)}


def exit_record(returncode, *, started, reason, already_exited, term_sent, kill_sent):
    """Popen's signed returncode distinguishes actual exit(137) from SIGKILL."""
    if reason not in STOP_REASONS:
        reason = "diagnostic_failure"
    valid = type(returncode) is int and -255 <= returncode <= 255
    return {"version": 1, "started": bool(started),
            "result": ("signalled" if returncode < 0 else "exited") if valid else ("wait_unavailable" if started else "not_started"),
            "exitCode": returncode if valid and returncode >= 0 else None,
            "signal": -returncode if valid and returncode < 0 else None,
            "stopReason": reason, "limitInduced": reason == "diagnostic_limit",
            "alreadyExitedBeforeCleanup": already_exited if type(already_exited) is bool else None,
            "termSent": bool(term_sent), "killSent": bool(kill_sent)}


def accepted_collection(runtime, result):
    """Collection success cannot turn a natural crash into app acceptance."""
    if runtime["status"] != "eof" or result["stopReason"] not in ("none", "cleanup_marker", "supervisor_signal"):
        return False
    if result["result"] == "exited":
        return result["exitCode"] == 0
    if result["result"] != "signalled" or result["alreadyExitedBeforeCleanup"] is not False:
        return False
    return (result["signal"] == 15 and result["termSent"] or
            result["signal"] == 9 and result["killSent"])


def supervise(child, read_chunk, cleanup_requested, clock=time.monotonic):
    """Owned Popen + bounded read seam. Tests use a fake child, never the app.

    read_chunk(timeout) returns bytes, None for timeout, b'' for EOF. It must
    never return more than MAX_LINE. No raw exception is formatted or returned.
    """
    filtered = RuntimeFilter()
    start = clock()
    reason, already_exited = "none", None
    term_at = kill_at = exited_at = None
    term_sent = kill_sent = False
    stream_status = None
    while True:
        now = clock()
        code = child.poll()
        if code is not None and exited_at is None:
            exited_at = now
        if reason == "none":
            if filtered.limited:
                reason, already_exited = "diagnostic_limit", code is not None
            else:
                try:
                    requested = cleanup_requested()
                except Exception:
                    requested = "diagnostic_failure"
                if requested in ("cleanup_marker", "supervisor_signal", "diagnostic_failure"):
                    reason, already_exited = requested, code is not None
                elif now - start >= LIFETIME_SECONDS:
                    reason, already_exited = "deadline", code is not None
        if reason != "none" and code is None and term_at is None:
            term_at = now
            try:
                child.terminate()
                term_sent = True
            except ProcessLookupError:
                pass
            except Exception:
                reason = "diagnostic_failure"
        if code is None and term_at is not None and now - term_at >= TERM_SECONDS and kill_at is None:
            kill_at = now
            try:
                child.kill()
                kill_sent = True
            except ProcessLookupError:
                pass
            except Exception:
                reason = "diagnostic_failure"
        if code is None and kill_at is not None and now - kill_at >= KILL_SECONDS:
            stream_status = stream_status or "read_failed"
            break
        if stream_status is None:
            try:
                chunk = read_chunk(POLL_SECONDS)
                if chunk is not None:
                    if type(chunk) is not bytes or len(chunk) > MAX_LINE:
                        raise ValueError()
                    if not chunk:
                        stream_status = "eof"
                    else:
                        filtered.feed(chunk)
                        if filtered.limited:
                            stream_status = "limit"
                            # An already-observed limit wins over any later
                            # cleanup request; do not relabel it normal cleanup.
                            reason, already_exited = "diagnostic_limit", child.poll() is not None
            except Exception:
                stream_status = "read_failed"
                if reason == "none":
                    reason, already_exited = "diagnostic_failure", code is not None
        else:
            # No pipe read after EOF/limit; poll without a busy loop.
            time.sleep(POLL_SECONDS)
        if code is not None:
            if stream_status is not None:
                break
            if now - exited_at >= DRAIN_SECONDS:
                stream_status = "drain_timeout"
                break
    # wait() owns/reaps the exact child, even if poll() already reaped it.
    try:
        returncode = child.wait(timeout=KILL_SECONDS)
    except Exception:
        returncode = None
    filtered.finish(stream_status == "eof")
    if filtered.limited and reason == "none":
        reason, already_exited = "diagnostic_limit", returncode is not None
    return (filtered.summary(stream_status), exit_record(returncode, started=True, reason=reason,
            already_exited=already_exited, term_sent=term_sent, kill_sent=kill_sent))


def write_summary(root, name, record):
    # name is selected only by our two fixed call sites, never app output.
    if name not in ("app-runtime", "app-exit"):
        raise ValueError()
    path = root / (name + ".txt")
    with path.open("x", encoding="ascii") as output:
        output.write(json.dumps(record, sort_keys=True, separators=(",", ":")) + "\n")


def private_control(root):
    control = root / "control"
    info = control.lstat()
    if not stat.S_ISDIR(info.st_mode) or info.st_uid != os.getuid() or info.st_mode & 0o077:
        raise ValueError()
    executable = root / "acceptance-app"
    info = executable.lstat()
    if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid() or not os.access(executable, os.X_OK):
        raise ValueError()
    return control, executable


def main():
    root = child = selector = pipe = None
    interrupted = False

    def interrupted_handler(_signum, _frame):
        nonlocal interrupted
        interrupted = True

    try:
        root = isolated_root()
        control, executable = private_control(root)
        if any((control / name).exists() for name in ("app-pid", "app-pid.tmp", "app-cleanup")):
            raise ValueError()
        signal.signal(signal.SIGTERM, interrupted_handler)
        signal.signal(signal.SIGINT, interrupted_handler)
        # Exact fixed path, inherited unchanged env, no arguments or shell.
        child = subprocess.Popen([str(executable)], stdin=subprocess.DEVNULL,
                                 stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        pipe = child.stdout
        os.set_blocking(pipe.fileno(), False)
        selector = selectors.DefaultSelector()
        selector.register(pipe, selectors.EVENT_READ)
        with (control / "app-pid.tmp").open("x", encoding="ascii") as output:
            output.write(str(child.pid) + "\n")
        (control / "app-pid.tmp").rename(control / "app-pid")

        def read_chunk(timeout):
            if not selector.select(timeout):
                return None
            try:
                return os.read(pipe.fileno(), MAX_LINE)
            except BlockingIOError:
                return None

        def cleanup_requested():
            if interrupted:
                return "supervisor_signal"
            marker = control / "app-cleanup"
            try:
                info = marker.lstat()
            except FileNotFoundError:
                return None
            if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid() or info.st_size != 8:
                return "diagnostic_failure"
            with marker.open("rb") as source:
                return "cleanup_marker" if source.read(9) == b"cleanup\n" else "diagnostic_failure"

        runtime, result = supervise(child, read_chunk, cleanup_requested)
        write_summary(root, "app-runtime", runtime)
        write_summary(root, "app-exit", result)
        return 0 if accepted_collection(runtime, result) else 1
    except Exception:
        # No exception message/type/args/traceback from the raw stream path.
        code, term_sent, kill_sent, already_exited = None, False, False, None
        if child is not None:
            try:
                already_exited = child.poll() is not None
            except Exception:
                pass
            try:
                child.terminate()
                term_sent = True
            except Exception:
                pass
            try:
                code = child.wait(timeout=TERM_SECONDS)
            except Exception:
                try:
                    child.kill()
                    kill_sent = True
                    code = child.wait(timeout=KILL_SECONDS)
                except Exception:
                    pass
        if root is not None:
            try:
                write_summary(root, "app-runtime", RuntimeFilter().summary("not_opened"))
                write_summary(root, "app-exit", exit_record(code, started=child is not None,
                              reason="diagnostic_failure", already_exited=already_exited, term_sent=term_sent, kill_sent=kill_sent))
            except Exception:
                pass
        return 1
    finally:
        if selector is not None:
            try:
                selector.close()
            except Exception:
                pass
        if pipe is not None:
            try:
                pipe.close()
            except Exception:
                pass


if __name__ == "__main__":
    raise SystemExit(main())
