#!/usr/bin/python3
"""Synthetic artifact math only; no actual performance or host sampling."""
import copy
import json
import os
from pathlib import Path
import tempfile
import unittest

import analyze


def packet(sequence, submitted, epoch=1):
    def bins(count, index):
        result = [0] * 257
        result[index] = count
        return result
    return {"epoch": epoch, "sequence": sequence, "telemetryInvalid": False,
            "counters": {"created": 1, "disposed": 0, "raf": submitted, "submitted": submitted, "contextLost": 0, "failures": 0},
            "histograms": {"raf": bins(submitted - 1, 33), "submission": bins(submitted - 1, 33), "cpu": bins(submitted, 2)}}


def fixture(actors=3):
    mode = {1: "pilot", 3: "pilot3", 5: "pilot5"}[actors]
    native, processes = [], []
    def row(at, event):
        return {"nativeMs": at, "monotonicUsec": 100_000_000 + at * 1000, "event": event}
    native.append(row(0, {"kind": "scope", "mode": mode, "actors": actors, "simultaneousStages": actors,
                          "nativeWebviews": actors + 1, "framesPerSecond": 20 if actors == 5 else 30,
                          "normalAppBootstrap": False, "fullIssue66Acceptance": False, "softwareGpuQualificationOnly": True}))
    for ordinal, name in enumerate(analyze.PHASES):
        start = ordinal * 1100 + 100
        native.append(row(start, {"kind": "phase_start", "phase": name, "durationMs": 1000}))
        for sequence, at in ((1, start + 50), (2, start + 950)):
            for epoch in range(1, actors + 1):
                count = 1 if sequence == 1 or name.endswith("dom_out") else 28
                native.append(row(at, {"kind": "batch", "phase": name, "stablePhase": True, "packet": packet(sequence, count, epoch)}))
        for offset in (100, 500, 900):
            stamp = 100_000_000 + (start + offset) * 1000
            processes.append({"monotonicUsec": stamp, "cpuUsecCumulative": stamp // 2,
                              "summedRssBytesMayDoubleCountSharedPages": 1024, "cgroupChargedMemoryBytesNotRss": 512})
    for cycle in range(21):
        for ordinal in range(actors):
            native.append(row(6000 + cycle, {"kind": "native_destroyed_after_actual_dispose", "epoch": cycle * 5 + ordinal + 1}))
    native.append(row(6100, {"kind": "completed", "activeFixtureStages": 0, "activeFigureWindows": 0}))
    return native, processes, {"passed": True, "cleanupConfirmed": True}, {"mode": mode}


class Analysis(unittest.TestCase):
    def test_percentiles_are_upper_bounds_and_empty_or_overflow_is_unknown(self):
        bins = [0] * 257
        self.assertIsNone(analyze.p95_upper(bins))
        bins[49] = 95; bins[200] = 5
        self.assertEqual(analyze.p95_upper(bins), 50)
        bins = [0] * 257; bins[256] = 1
        self.assertIsNone(analyze.p95_upper(bins))
        for value in ([0] * 256, [True] + [0] * 256, [-1] + [0] * 256, [10_000_001] + [0] * 256):
            with self.assertRaises(ValueError): analyze.p95_upper(value)

    def test_frame_delta_rejects_regression_foreign_epochs_and_inconsistent_histograms(self):
        first, last = packet(1, 1), packet(2, 28)
        self.assertEqual(analyze.frame_delta(first, last)["renderSubmissions"], 27)
        for change in ("epoch", "sequence", "counter", "histogram", "failure", "invalid"):
            bad = copy.deepcopy(last)
            if change == "epoch": bad["epoch"] = 2
            elif change == "sequence": bad["sequence"] = 1
            elif change == "counter": bad["counters"]["submitted"] = 0
            elif change == "histogram": bad["histograms"]["cpu"][2] = 27
            elif change == "failure": bad["counters"]["failures"] = 1
            else: bad["telemetryInvalid"] = True
            with self.subTest(change=change), self.assertRaises(ValueError): analyze.frame_delta(first, bad)

    def test_all_current_owners_report_independently_and_no_hardware_acceptance_is_inferred(self):
        for actors in (1, 3, 5):
            output = analyze.analyze(*fixture(actors))
            self.assertEqual(len(output["phases"]), 5)
            self.assertTrue(output["orderlyWindowEpochsObserved"])
            for phase in output["phases"]:
                self.assertEqual(len(phase["figures"]), actors)
                self.assertTrue(all(figure["available"] for figure in phase["figures"]))
                self.assertEqual(phase["processCost"]["cpuPercentOneCore"], 50)
                if phase["phase"].endswith("dom_out"):
                    self.assertIs(phase["domOutSubmissionsSuppressed"], True)
            self.assertFalse(output["hardwarePerformanceBaseline"])
            self.assertFalse(output["productionStudioOrObsExposureQualified"])
            self.assertFalse(output["fullIssue66Acceptance"])
            self.assertIsNone(output["displayPresentTiming"])

    def test_missing_owner_or_sparse_window_never_becomes_zero_render_success(self):
        args = fixture()
        native = args[0]
        native[:] = [row for row in native if not (row["event"].get("kind") == "batch" and row["event"]["packet"]["epoch"] == 3)]
        output = analyze.analyze(*args)
        phase = output["phases"][2]
        self.assertFalse(phase["figures"][2]["available"])
        self.assertIsNone(phase["domOutSubmissionsSuppressed"])
        args = fixture()
        for row in args[0]:
            if row["event"].get("kind") == "phase_start": row["event"]["durationMs"] = 450_000
        self.assertIsNone(analyze.analyze(*args)["phases"][2]["domOutSubmissionsSuppressed"])

    def test_offscreen_submissions_and_replayed_destroyed_epoch_remain_visible(self):
        args = fixture()
        for row in args[0]:
            event = row["event"]
            if event.get("kind") == "batch" and event["phase"].endswith("dom_out") and event["packet"]["sequence"] == 2:
                event["packet"] = packet(2, 2, event["packet"]["epoch"])
        output = analyze.analyze(*args)
        self.assertIs(output["phases"][2]["domOutSubmissionsSuppressed"], False)
        destroyed = [row for row in args[0] if row["event"].get("kind") == "native_destroyed_after_actual_dispose"]
        destroyed[1]["event"]["epoch"] = 1
        self.assertFalse(analyze.analyze(*args)["orderlyWindowEpochsObserved"])

    def test_relative_clocks_are_never_guessed_equal_and_missing_cpu_is_not_zero(self):
        args = fixture()
        for row in args[0]: del row["monotonicUsec"]
        for phase in analyze.analyze(*args)["phases"]:
            self.assertEqual(phase["processCost"], {"available": False, "reason": "clock_alignment_unavailable"})
        self.assertEqual(analyze.process_cost([], 100, 200), {"available": False, "reason": "insufficient_samples"})
        args = fixture(); args[1][1]["monotonicUsec"] = args[1][0]["monotonicUsec"]
        with self.assertRaises(ValueError): analyze.analyze(*args)

    def test_failed_cleanup_or_foreign_frame_cannot_produce_a_report(self):
        args = fixture(); args[2]["cleanupConfirmed"] = False
        with self.assertRaises(ValueError): analyze.analyze(*args)
        args = fixture()
        next(row["event"]["packet"] for row in args[0] if row["event"].get("kind") == "batch")["epoch"] = 6
        with self.assertRaises(ValueError): analyze.analyze(*args)

    def test_reader_refuses_links_nonmetadata_names_and_oversized_numeric_artifacts(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "fixture-result.json").write_text(json.dumps({"passed": True}))
            (root / "native.ndjson").symlink_to(root / "fixture-result.json")
            descriptor = os.open(root, os.O_RDONLY | os.O_DIRECTORY)
            try:
                result, digest = analyze.load(descriptor, "fixture-result.json")
                self.assertEqual(result, {"passed": True}); self.assertEqual(len(digest), 64)
                with self.assertRaises(OSError): analyze.load(descriptor, "native.ndjson", True)
                with self.assertRaises(ValueError): analyze.load(descriptor, "private.key")
                os.link(root / "fixture-result.json", root / "fixture-manifest.json")
                with self.assertRaises(ValueError): analyze.load(descriptor, "fixture-manifest.json")
                (root / "process.ndjson").write_text("1" * (256 * 1024 + 1))
                with self.assertRaises(ValueError): analyze.load(descriptor, "process.ndjson", True)
            finally: os.close(descriptor)


if __name__ == "__main__": unittest.main()
