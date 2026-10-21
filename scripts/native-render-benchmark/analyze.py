#!/usr/bin/python3
"""Offline bounded numeric artifact analysis. No app, host probes or network.

Percentiles are histogram upper bounds, not GPU/present timing. Native phase
and process samples correlate only through an explicit CLOCK_MONOTONIC stamp.
"""
import hashlib
import json
import os
from pathlib import Path
import stat
import sys

LIMIT = 32 * 1024 * 1024
SAFE = 9_007_199_254_740_991
PHASES = ("warmup", "control_shown_dom_in", "control_shown_dom_out", "control_hidden_dom_out", "control_hidden_dom_in")
MODES = {"smoke": (1, 30), "pilot": (1, 30), "pilot3": (3, 30), "pilot5": (5, 20)}


def integer(value, maximum=SAFE):
    if type(value) is not int or not 0 <= value <= maximum:
        raise ValueError()
    return value


def p95_upper(bins):
    if type(bins) is not list or len(bins) != 257:
        raise ValueError()
    total = sum(integer(count, 10_000_000) for count in bins)
    if total > 10_000_000:
        raise ValueError()
    if not total:
        return None
    threshold = (total * 95 + 99) // 100
    count = 0
    for index, value in enumerate(bins):
        count += value
        if count >= threshold:
            # The overflow bin includes all durations >=256ms. Its upper bound
            # is unknown, and must never be reported as a256ms percentile.
            return index + 1 if index < 256 else None
    raise ValueError()


def frame_delta(first, last):
    if integer(first["epoch"]) != integer(last["epoch"]) or integer(last["sequence"]) <= integer(first["sequence"]):
        raise ValueError()
    counters = {}
    for key in ("created", "disposed", "raf", "submitted", "contextLost", "failures"):
        before = integer(first["counters"][key], 10_000_000)
        after = integer(last["counters"][key], 10_000_000)
        if after < before:
            raise ValueError()
        counters[key] = after - before
    if first["telemetryInvalid"] is not False or last["telemetryInvalid"] is not False:
        raise ValueError()
    if first["counters"]["created"] != 1 or first["counters"]["disposed"] or first["counters"]["contextLost"] or first["counters"]["failures"] or counters["created"] or counters["contextLost"] or counters["failures"] or counters["disposed"]:
        raise ValueError()
    histograms = {}
    for channel in ("raf", "submission", "cpu"):
        before, after = first["histograms"][channel], last["histograms"][channel]
        if type(before) is not list or type(after) is not list or len(before) != 257 or len(after) != 257:
            raise ValueError()
        delta = [integer(b, 10_000_000) - integer(a, 10_000_000) for a, b in zip(before, after)]
        if any(value < 0 for value in delta):
            raise ValueError()
        histograms[channel] = delta
    if (sum(histograms["cpu"]) != counters["submitted"] or sum(histograms["submission"]) != counters["submitted"]
            or sum(histograms["raf"]) != counters["raf"]):
        raise ValueError()
    return {"rafCallbacks": counters["raf"], "renderSubmissions": counters["submitted"],
            "submissionP95UpperMs": p95_upper(histograms["submission"]),
            "cpuSubmissionP95UpperMs": p95_upper(histograms["cpu"])}


def process_cost(rows, start, end):
    if start is None:
        return {"available": False, "reason": "clock_alignment_unavailable"}
    chosen = [row for row in rows if start <= row.get("monotonicUsec", -1) <= end]
    if len(chosen) < 2:
        return {"available": False, "reason": "insufficient_samples"}
    times = [integer(row["monotonicUsec"]) for row in chosen]
    cpu = [integer(row["cpuUsecCumulative"]) for row in chosen]
    if any(b <= a for a, b in zip(times, times[1:])) or any(b < a for a, b in zip(cpu, cpu[1:])):
        raise ValueError()
    span = times[-1] - times[0]
    return {"available": True, "samples": len(chosen), "sampledSpanMs": span / 1000,
            "maximumSampleGapMs": max(b - a for a, b in zip(times, times[1:])) / 1000,
            "cpuPercentOneCore": (cpu[-1] - cpu[0]) * 100 / span,
            "peakSummedRssBytesMayDoubleCountSharedPages": max(integer(row["summedRssBytesMayDoubleCountSharedPages"]) for row in chosen),
            "peakCgroupChargedMemoryBytesNotRss": max(integer(row["cgroupChargedMemoryBytesNotRss"]) for row in chosen)}


def analyze(native, processes, result, manifest):
    mode = manifest.get("mode")
    if mode not in MODES:
        raise ValueError()
    actors, fps = MODES[mode]
    if result.get("passed") is not True or result.get("cleanupConfirmed") is not True:
        raise ValueError()
    scopes = [row["event"] for row in native if row["event"].get("kind") == "scope"]
    completed = [row["event"] for row in native if row["event"].get("kind") == "completed"]
    if len(scopes) != 1 or len(completed) != 1:
        raise ValueError()
    scope, done = scopes[0], completed[0]
    for key in ("actors", "simultaneousStages", "nativeWebviews"):
        integer(scope.get(key), 6)
    for key in ("activeFixtureStages", "activeFigureWindows"):
        integer(done.get(key), 5)
    if (scope.get("mode") != mode or scope.get("actors") != actors or scope.get("simultaneousStages") != actors
            or scope.get("nativeWebviews") != actors + 1 or scope.get("normalAppBootstrap") is not False
            or integer(scope.get("framesPerSecond"), 30) != fps
            or scope.get("fullIssue66Acceptance") is not False or scope.get("softwareGpuQualificationOnly") is not True
            or done.get("activeFixtureStages") != 0 or done.get("activeFigureWindows") != 0):
        raise ValueError()
    previous = -1
    previous_absolute = -1
    stamped = 0
    for row in native:
        stamp = integer(row["nativeMs"])
        if stamp < previous:
            raise ValueError()
        previous = stamp
        if "monotonicUsec" in row:
            absolute = integer(row["monotonicUsec"])
            if absolute < previous_absolute:
                raise ValueError()
            previous_absolute = absolute
            stamped += 1
    if stamped not in (0, len(native)):
        raise ValueError()
    previous_absolute = -1
    for row in processes:
        if "monotonicUsec" in row:
            absolute = integer(row["monotonicUsec"])
            if absolute <= previous_absolute:
                raise ValueError()
            previous_absolute = absolute
    phases = []
    for name in PHASES:
        starts = [row for row in native if row["event"].get("kind") == "phase_start" and row["event"].get("phase") == name]
        if len(starts) != 1:
            raise ValueError()
        start = starts[0]
        duration = integer(start["event"]["durationMs"], 3_600_000)
        end = start["nativeMs"] + duration
        batches = [row for row in native if row["event"].get("kind") == "batch" and row["event"].get("phase") == name
                   and row["event"].get("stablePhase") is True and start["nativeMs"] <= row["nativeMs"] <= end]
        owners = {}
        for row in batches:
            packet = row["event"]["packet"]
            epoch = integer(packet["epoch"])
            if not 1 <= epoch <= actors:
                raise ValueError()
            owners.setdefault(epoch, []).append(row)
        if len(owners) > actors:
            raise ValueError()
        figures = []
        for epoch in range(1, actors + 1):
            samples = owners.get(epoch, [])
            if len(samples) < 2:
                figures.append({"epoch": epoch, "available": False, "reason": "insufficient_samples"})
                continue
            first, last = samples[0], samples[-1]
            span = last["nativeMs"] - first["nativeMs"]
            if span <= 0:
                raise ValueError()
            frame = frame_delta(first["event"]["packet"], last["event"]["packet"])
            maximum_gap = max(b["nativeMs"] - a["nativeMs"] for a, b in zip(samples, samples[1:]))
            if maximum_gap <= 0:
                raise ValueError()
            figures.append({"epoch": epoch, "available": True, "sampledSpanMs": span, **frame,
                            "maximumSampleGapMs": maximum_gap,
                            "coverageSufficient": span >= max(duration * 0.9, duration - 2500) and maximum_gap <= 2500,
                            "submissionRatePerSecond": frame["renderSubmissions"] * 1000 / span})
        invisible = name.endswith("dom_out")
        complete_frames = all(figure["available"] and figure["coverageSufficient"] for figure in figures)
        stamp = start.get("monotonicUsec")
        phases.append({"phase": name, "requestedDurationMs": duration, "figures": figures,
                       "domOutSubmissionsSuppressed": all(figure["renderSubmissions"] == 0 for figure in figures) if invisible and complete_frames else None,
                       "processCost": process_cost(processes, stamp, stamp + duration * 1000 if stamp is not None else None)})
    cycles = 3 if mode == "smoke" else 20
    destroyed = [row["event"].get("epoch") for row in native if row["event"].get("kind") == "native_destroyed_after_actual_dispose"]
    expected = {cycle * 5 + ordinal + 1 for cycle in range(cycles + 1) for ordinal in range(actors)}
    return {"version": 1, "mode": mode, "actors": actors, "requestedFramesPerSecond": fps,
            "softwareCompositorOnly": True, "phases": phases,
            "orderlyWindowEpochsObserved": len(destroyed) == len(expected) and set(destroyed) == expected,
            "gpuDuration": None, "gpuUtilization": None, "displayPresentTiming": None,
            "instrumentationOverhead": None, "hardwarePerformanceBaseline": False,
            "productionStudioOrObsExposureQualified": False, "fullIssue66Acceptance": False}


def load(root_fd, name, lines=False):
    # The caller must verify the downloaded GitHub artifact digest before
    # extraction. This reader adds local no-follow/type/size/change checks.
    if name not in ("native.ndjson", "process.ndjson", "fixture-result.json", "fixture-manifest.json"):
        raise ValueError()
    descriptor = os.open(name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=root_fd)
    try:
        before = os.fstat(descriptor)
        if not stat.S_ISREG(before.st_mode) or before.st_nlink != 1 or not 0 < before.st_size <= LIMIT:
            raise ValueError()
        with os.fdopen(descriptor, "rb", closefd=False) as stream:
            raw = stream.read(LIMIT + 1)
        after = os.fstat(descriptor)
        linked = os.stat(name, dir_fd=root_fd, follow_symlinks=False)
        stamp = lambda value: (value.st_dev, value.st_ino, value.st_size, value.st_mtime_ns, value.st_ctime_ns)
        if len(raw) != before.st_size or stamp(before) != stamp(after) or stamp(after) != stamp(linked):
            raise ValueError()
        chunks = raw.splitlines() if lines else [raw]
        if len(chunks) > 100_000 or any(len(chunk) > (256 * 1024 if lines else LIMIT) for chunk in chunks):
            raise ValueError()
        return [json.loads(chunk) for chunk in chunks] if lines else json.loads(raw), hashlib.sha256(raw).hexdigest()
    finally:
        os.close(descriptor)


def main():
    if len(sys.argv) != 2:
        raise ValueError()
    root_fd = os.open(Path(sys.argv[1]), os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        native, native_hash = load(root_fd, "native.ndjson", True)
        processes, process_hash = load(root_fd, "process.ndjson", True)
        result, result_hash = load(root_fd, "fixture-result.json")
        manifest, manifest_hash = load(root_fd, "fixture-manifest.json")
    finally:
        os.close(root_fd)
    output = analyze(native, processes, result, manifest)
    output["sourceSha256"] = {"native": native_hash, "process": process_hash, "result": result_hash, "manifest": manifest_hash}
    print(json.dumps(output, sort_keys=True, allow_nan=False))


if __name__ == "__main__":
    try:
        main()
    except Exception:
        raise SystemExit("Native benchmark analysis unavailable.") from None
