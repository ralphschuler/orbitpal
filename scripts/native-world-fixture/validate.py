#!/usr/bin/env python3
"""Closed numeric/enum-only validation. No app launch, IPC or authority.

Reads exactly one selected result.json, at most16KiB, regular owned single-link
no-follow file. It never prints its path, body or an exception. A missing real
observation is qualification_incomplete; ordinary exit/metadata cannot pass.
"""
import json
import os
import re
import stat
import sys

TOP = frozenset(('version','scope','status','duration_ms','credential_attempts','outbound_attempts','denied_commands','diagnostics','main_getter_drained','actors','remaining'))
ACTOR = frozenset(('actor','constructor_observed','prepared_epochs','current','best_live','best_hidden','destroyed','invalid'))
EPOCH = frozenset(('epoch','normal_warm_sequence','accepted_frames','first_admitted_ms','last_admitted_ms','last_frame_sequence','live_span_ms','max_frame_gap_ms','max_frame_age_ms','stopped','cleanup'))
SPAN = frozenset(('epoch','accepted_frames','first_admitted_ms','last_admitted_ms','span_ms','last_frame_sequence','max_frame_gap_ms','max_frame_age_ms'))
PHASES = ('register_entered', 'register_authenticated', 'register_member', 'register_awaiting_layout', 'register_awaiting_calibration', 'register_source', 'register_registry_accepted', 'register_awaiting_measurement', 'register_ready', 'register_unavailable', 'register_reply_pinned', 'measurement_entered', 'measurement_authenticated', 'measurement_valid', 'measurement_owned', 'measurement_source_matches', 'measurement_source', 'measurement_registry_accepted', 'measurement_promotion_accepted', 'measurement_accepted')
DENIED_KINDS = ('main_only', 'get_document', 'get_knowledge', 'get_emote_sound', 'get_speech_audio', 'list_models', 'check_cli_provider', 'list_cli_models', 'overlay_control', 'plugin', 'other')
HOST_MAX = 9_007_199_254_740_991
REMAINING = ['native_action_arrival','speech_handoff','obs_cef','mixed_dpi_lock_suspend','visible_pixel_attestation']
LIMIT = 10_000_000

class Refusal(Exception):
    pass

def closed(pairs):
    value = {}
    for key, item in pairs:
        if key in value:
            raise Refusal()
        value[key] = item
    return value

def integer(value, maximum=LIMIT):
    return type(value) is int and 0 <= value <= maximum

def span_record(value, duration):
    if type(value) is not dict or set(value) != SPAN:
        raise Refusal()
    if not integer(value['epoch'], HOST_MAX) or value['epoch'] == 0:
        raise Refusal()
    if any(not integer(value[key]) for key in SPAN - {'epoch'}):
        raise Refusal()
    first, last = value['first_admitted_ms'], value['last_admitted_ms']
    if first > last or last > duration or last - first != value['span_ms'] or value['accepted_frames'] < 2 or value['accepted_frames'] > value['last_frame_sequence'] or value['span_ms'] > (value['accepted_frames'] - 1) * value['max_frame_gap_ms']:
        raise Refusal()
    return value

def epoch_record(value, duration):
    if type(value) is not dict or set(value) != EPOCH:
        raise Refusal()
    if not integer(value['epoch'], HOST_MAX) or value['epoch'] == 0:
        raise Refusal()
    if any(not integer(value[key]) for key in ('normal_warm_sequence','accepted_frames','last_frame_sequence','live_span_ms','max_frame_gap_ms','max_frame_age_ms')) or value['normal_warm_sequence'] == 0:
        raise Refusal()
    if type(value['stopped']) is not bool or value['cleanup'] not in (None,'restored','retired','without_geometry','guarded'):
        raise Refusal()
    first, last = value['first_admitted_ms'], value['last_admitted_ms']
    if value['accepted_frames'] == 0:
        if first is not None or last is not None or any(value[key] != 0 for key in ('last_frame_sequence','live_span_ms','max_frame_gap_ms','max_frame_age_ms')):
            raise Refusal()
    elif not integer(first) or not integer(last) or first > last or last > duration or last - first != value['live_span_ms'] or value['accepted_frames'] > value['last_frame_sequence'] or value['live_span_ms'] > (value['accepted_frames'] - 1) * value['max_frame_gap_ms']:
        raise Refusal()
    return value

def validate(value):
    if type(value) is not dict or set(value) != TOP:
        raise Refusal()
    if value['version'] != 3 or type(value['version']) is not int or value['scope'] != 'actual_wry_native_render_lifecycle':
        raise Refusal()
    if value['status'] not in ('passed','qualification_incomplete','invalid') or value['remaining'] != REMAINING:
        raise Refusal()
    if any(not integer(value[key]) for key in ('duration_ms','credential_attempts','outbound_attempts','denied_commands')):
        raise Refusal()
    diagnostics = value['diagnostics']
    if type(diagnostics) is not dict or set(diagnostics) != {'ipc_phases','denied_kinds'}:
        raise Refusal()
    for name, keys in [('ipc_phases',PHASES),('denied_kinds',DENIED_KINDS)]:
        counts = diagnostics[name]
        if type(counts) is not dict or set(counts) != set(keys) or any(not integer(count) for count in counts.values()):
            raise Refusal()
    # Debug observations never participate in lifecycle qualification.
    if type(value['main_getter_drained']) is not bool or type(value['actors']) is not list or len(value['actors']) != 2:
        raise Refusal()
    good = value['credential_attempts'] == value['outbound_attempts'] == 0 and value['duration_ms'] <= 185_000 and value['main_getter_drained']
    for index, actor in enumerate(value['actors']):
        if type(actor) is not dict or set(actor) != ACTOR:
            raise Refusal()
        if any(type(actor[key]) is not bool for key in ('constructor_observed','destroyed','invalid')):
            raise Refusal()
        if not integer(actor['actor']) or actor['actor'] != index or not integer(actor['prepared_epochs']):
            raise Refusal()
        current = None if actor['current'] is None else epoch_record(actor['current'], value['duration_ms'])
        if (current is None) != (actor['prepared_epochs'] == 0):
            raise Refusal()
        spans = []
        for name in ('best_live','best_hidden'):
            item = actor[name]
            if item is not None:
                item = span_record(item, value['duration_ms'])
                if current is None or item['epoch'] > current['epoch'] or actor['prepared_epochs'] == 1 and item['epoch'] != current['epoch']:
                    raise Refusal()
                if item['epoch'] == current['epoch'] and (item['accepted_frames'] > current['accepted_frames'] or item['last_frame_sequence'] > current['last_frame_sequence'] or item['last_admitted_ms'] > current['last_admitted_ms'] or item['max_frame_gap_ms'] > current['max_frame_gap_ms'] or item['max_frame_age_ms'] > current['max_frame_age_ms']):
                    raise Refusal()
            spans.append(item)
        live, hidden = spans
        good = good and actor['constructor_observed'] and not actor['invalid'] and actor['destroyed'] and current is not None and current['stopped'] and current['cleanup'] in ('restored','retired') and current['max_frame_gap_ms'] <= 5_000 and current['max_frame_age_ms'] <= 250 and live is not None and live['accepted_frames'] >= 10 and live['span_ms'] >= 35_000 and live['max_frame_gap_ms'] <= 5_000 and live['max_frame_age_ms'] <= 250 and hidden is not None and hidden['accepted_frames'] >= 2 and hidden['span_ms'] >= 10_000 and hidden['max_frame_gap_ms'] <= 5_000 and hidden['max_frame_age_ms'] <= 250
    if value['status'] == 'passed' and not good:
        raise Refusal()
    return value['status'] == 'passed' and bool(good)

def load(path):
    parent = os.path.dirname(os.path.abspath(path))
    if not re.fullmatch(r'world-native-macos-actual-run-[0-9a-f]{32}', os.path.basename(parent)):
        raise Refusal()
    directory = os.open(parent, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC)
    descriptor = None
    try:
        root = os.fstat(directory)
        if not stat.S_ISDIR(root.st_mode) or root.st_uid != os.geteuid() or root.st_mode & 0o077:
            raise Refusal()
        descriptor = os.open('result.json', os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC, dir_fd=directory)
        before = os.fstat(descriptor)
        if not stat.S_ISREG(before.st_mode) or before.st_uid != os.geteuid() or before.st_nlink != 1 or not 0 < before.st_size <= 16_384:
            raise Refusal()
        data = os.read(descriptor, 16_385)
        after = os.fstat(descriptor)
        identity = lambda meta: (meta.st_dev,meta.st_ino,meta.st_uid,meta.st_nlink,meta.st_size,meta.st_mtime_ns,meta.st_ctime_ns)
        if len(data) != before.st_size or identity(before) != identity(after):
            raise Refusal()
        return json.loads(data, object_pairs_hook=closed, parse_constant=lambda _: (_ for _ in ()).throw(Refusal()))
    finally:
        if descriptor is not None:
            os.close(descriptor)
        os.close(directory)

def main():
    try:
        if len(sys.argv) != 2 or os.path.basename(sys.argv[1]) != 'result.json':
            raise Refusal()
        value = load(sys.argv[1])
        passed = validate(value)
        if value['status'] == 'invalid':
            print('native_world_fixture_invalid')
            return 2
    except Exception:
        print('native_world_fixture_invalid')
        return 2
    print('native_world_fixture_passed' if passed else 'native_world_fixture_qualification_incomplete')
    return 0 if passed else 32

if __name__ == '__main__':
    raise SystemExit(main())
