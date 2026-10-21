#!/usr/bin/env python3
"""UNRUN source-only validator regressions. Synthetic numeric records are not
actual Wry, native renderer, frame, action, lifecycle or performance evidence."""
import importlib.util
import json
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('fixture_validate', Path(__file__).with_name('validate.py'))
validator = importlib.util.module_from_spec(spec)
spec.loader.exec_module(validator)

def record():
    live = dict(epoch=1,accepted_frames=13,first_admitted_ms=100,last_admitted_ms=36_100,
                span_ms=36_000,last_frame_sequence=80,max_frame_gap_ms=3_100,max_frame_age_ms=20)
    hidden = dict(epoch=4,accepted_frames=5,first_admitted_ms=50_100,last_admitted_ms=62_100,
                  span_ms=12_000,last_frame_sequence=100,max_frame_gap_ms=3_100,max_frame_age_ms=20)
    current = dict(epoch=9,normal_warm_sequence=110,accepted_frames=0,first_admitted_ms=None,
                   last_admitted_ms=None,last_frame_sequence=0,live_span_ms=0,
                   max_frame_gap_ms=0,max_frame_age_ms=0,stopped=True,cleanup='restored')
    def actor(index):
        return dict(actor=index,constructor_observed=True,prepared_epochs=3,current=dict(current),
                    best_live=dict(live),best_hidden=dict(hidden),destroyed=True,invalid=False)
    return dict(version=3,scope='actual_wry_native_render_lifecycle',status='passed',
                duration_ms=70_000,credential_attempts=0,outbound_attempts=0,
                denied_commands=0,diagnostics=dict(ipc_phases=dict.fromkeys(validator.PHASES,0),denied_kinds=dict.fromkeys(validator.DENIED_KINDS,0)),main_getter_drained=True,actors=[actor(0),actor(1)],
                remaining=list(validator.REMAINING))

class ValidatorTests(unittest.TestCase):
    def test_complete_synthetic_record_checks_schema_only(self):
        self.assertTrue(validator.validate(record()))

    def test_no_runtime_observations_cannot_pass(self):
        value = record(); value['status'] = 'qualification_incomplete'
        for actor in value['actors']:
            actor.update(constructor_observed=False,prepared_epochs=0,current=None,
                         best_live=None,best_hidden=None,destroyed=False)
        self.assertFalse(validator.validate(value))
        value['status'] = 'passed'
        with self.assertRaises(validator.Refusal):
            validator.validate(value)

    def test_each_required_actual_lifecycle_fact_is_required(self):
        for section, key, bad in [(None,'constructor_observed',False),(None,'destroyed',False),
                                  (None,'invalid',True),('current','normal_warm_sequence',0),
                                  ('current','stopped',False),('current','cleanup','without_geometry'),
                                  ('best_live','accepted_frames',9),('best_live','max_frame_gap_ms',5_001),
                                  ('best_live','max_frame_age_ms',251),('best_hidden','max_frame_age_ms',251)]:
            value = record(); target = value['actors'][1]
            if section: target = target[section]
            target[key] = bad
            with self.subTest(section=section,key=key), self.assertRaises(validator.Refusal):
                validator.validate(value)

    def test_credential_outbound_attempt_or_pending_main_getter_invalidates_pass(self):
        for key, bad in [('credential_attempts',1),('outbound_attempts',1),('main_getter_drained',False),('duration_ms',185_001)]:
            value = record(); value[key] = bad
            with self.subTest(key=key), self.assertRaises(validator.Refusal):
                validator.validate(value)

    def test_closed_top_actor_keys_and_enum_only_gaps(self):
        for mutate in [lambda v:v.update(extra='untrusted'),lambda v:v['actors'][0].update(privateId='forbidden'),lambda v:v['actors'][0]['current'].update(routeId='forbidden'),lambda v:v.update(remaining=['full_72_passed'])]:
            value = record(); mutate(value)
            with self.assertRaises(validator.Refusal):
                validator.validate(value)

    def test_bool_float_negative_and_overflow_are_not_numeric_evidence(self):
        for bad in [True,1.0,-1,10_000_001,float('nan'),float('inf')]:
            value = record(); value['actors'][0]['best_live']['accepted_frames'] = bad
            with self.subTest(bad=type(bad).__name__), self.assertRaises(validator.Refusal):
                validator.validate(value)

    def test_duplicate_json_keys_are_rejected_before_validation(self):
        with self.assertRaises(validator.Refusal):
            json.loads('{"version":3,"version":3}', object_pairs_hook=validator.closed)

    def test_actor_identity_correlation_and_impossible_spans_are_rejected(self):
        for mutate in [lambda a:a.update(actor=0),lambda a:a['best_live'].update(span_ms=99_000),
                       lambda a:a['best_hidden'].update(last_admitted_ms=80_000),
                       lambda a:a['current'].update(last_frame_sequence=12)]:
            value = record(); mutate(value['actors'][1])
            with self.assertRaises(validator.Refusal):
                validator.validate(value)

    def test_incomplete_or_invalid_status_never_becomes_pass_from_numbers(self):
        for status in ['qualification_incomplete','invalid']:
            value = record(); value['status'] = status
            self.assertFalse(validator.validate(value))

    def test_independent_qualified_owner_spans_do_not_require_the_final_owner_to_replay_them(self):
        value = record(); actor = value['actors'][0]
        self.assertNotEqual(actor['best_live']['epoch'],actor['best_hidden']['epoch'])
        self.assertNotEqual(actor['current']['epoch'],actor['best_live']['epoch'])
        self.assertEqual(actor['current']['accepted_frames'],0)
        self.assertTrue(validator.validate(value))

    def test_short_owner_spans_cannot_be_added_to_reach_thirty_five_seconds(self):
        value = record(); live = value['actors'][0]['best_live']
        live.update(accepted_frames=10,last_admitted_ms=18_100,span_ms=18_000,max_frame_gap_ms=2_000)
        with self.assertRaises(validator.Refusal):
            validator.validate(value)

    def test_future_epoch_and_unobserved_single_owner_history_are_refused(self):
        for mutate in [lambda a:a['best_live'].update(epoch=10),lambda a:a.update(prepared_epochs=1),
                       lambda a:a['current'].update(epoch=0),lambda a:a['best_hidden'].update(epoch=validator.HOST_MAX+1)]:
            value = record(); mutate(value['actors'][0])
            with self.assertRaises(validator.Refusal):
                validator.validate(value)

    def test_guarded_current_cleanup_and_old_record_version_never_pass(self):
        for mutate in [lambda v:v['actors'][0]['current'].update(cleanup='guarded'),lambda v:v.update(version=1)]:
            value = record(); mutate(value)
            with self.assertRaises(validator.Refusal):
                validator.validate(value)

    def test_diagnostics_are_closed_bounded_and_do_not_supply_lifecycle_evidence(self):
        for mutate in [lambda d:d.update(error='forbidden'),
                       lambda d:d['ipc_phases'].update(capability='forbidden'),
                       lambda d:d['denied_kinds'].update(command='forbidden'),
                       lambda d:d['ipc_phases'].update(register_entered=True),
                       lambda d:d['ipc_phases'].update(register_entered=validator.LIMIT+1),
                       lambda d:d['denied_kinds'].update(other=-1)]:
            value = record(); mutate(value['diagnostics'])
            with self.assertRaises(validator.Refusal):
                validator.validate(value)
        value = record(); value['status'] = 'qualification_incomplete'
        value['diagnostics']['ipc_phases'] = dict.fromkeys(validator.PHASES,validator.LIMIT)
        self.assertFalse(validator.validate(value))
        value['status'] = 'passed'; value['actors'][0]['best_live'] = None
        with self.assertRaises(validator.Refusal):
            validator.validate(value)
        value = record(); value['version'] = 2
        with self.assertRaises(validator.Refusal):
            validator.validate(value)

if __name__ == '__main__':
    unittest.main()
