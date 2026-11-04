"""Scope gate rejects incomplete/duplicated evidence even when marked PASSED."""
import copy
import hashlib
from pathlib import Path
import sys
import unittest

ROOT = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(ROOT/'integration/tools/acceptance'))
from managed_v2_acceptance import BANK_SHA256, FAULTS, validate_process


def report():
    native_rows = [dict(case=case, actual_amount_in='100', quoted_amount_in='100', actual_amount_out='99',
                        quoted_amount_out='99', complete_original_poststate_matches=True,
                        before_getters_checked=80, after_getters_checked=80)
                   for case in ('two:swap-forward', 'two:swap-reverse') for _ in range(2)]
    return dict(passed=True, state='PASSED', source_binding={'source': 'bound'}, input_hashes={'input': 'bound'},
                inputs_unchanged=True, verified=False, production_ready=False, all_requested_features_complete=False,
                original_bank_sha256=BANK_SHA256, source_frame_count=15, source_frames_checked=[dict(header=str(i)) for i in range(15)],
                quote_checks=[dict(actor=actor, case=case) for actor in ('router1', 'router2')
                              for case in ('two:swap-forward', 'two:swap-reverse', 'swap-after-vault-buffer', 'recovery-token-swap')], fault_count=len(FAULTS),
                checks=['scope-withdraw-and-new-generation:'+fault for fault in FAULTS]
                       + ['router-restart-restores-complete-native-managed-state','verified-and-build-remain-disabled',
                          'selected-api-single-hops-reexecuted-by-original-vault-with-real-balances',
                          'original-vault-pause-keeps-canonical-state-but-blocks-swap-until-own-buffer-expires',
                          'original-recovery-effective-zero-fee-swap-zero-collection-and-normal-exit'],
                recovery_source_frames=[dict(header=str(i),effective_fee='0') for i in range(5)],
                recovery_exit_source=dict(header='14',recovery_mode=False,effective_fee='10000000000000000'),
                paused_quote_rejections=[dict(actor=a,header=str(h),status=404) for a in ('router1','router2') for h in range(3)],
                paused_branch_sha256=hashlib.sha256((ROOT/'integration/test/contracts/managed-v2-aum-recovery.json').read_bytes()).hexdigest(),
                future_original_cases_excluded=['future-reference-case'], independent_native_plan_replay=dict(
                    passed=True, vm_started=True, verified=False, original_bank_sha256=BANK_SHA256,
                    selected_route_count=4, native_transaction_count=28, routes=native_rows))


class ManagedV2AcceptanceTests(unittest.TestCase):
    def test_complete_local_scope_shape_passes_without_promoting_verified(self):
        value = report()
        validate_process(value, {'source':'bound'}, {'input':'bound'})
        self.assertFalse(value['verified'])
        self.assertFalse(value['all_requested_features_complete'])

    def test_passed_flag_cannot_replace_complete_matching_evidence(self):
        changes = [lambda v: v.update(verified=True), lambda v: v.update(source_binding={}),
                   lambda v: v['source_frames_checked'].__setitem__(1,v['source_frames_checked'][0]),
                   lambda v: v['quote_checks'].__setitem__(1,v['quote_checks'][0]),
                   lambda v: v['checks'].pop(0), lambda v: v.update(future_original_cases_excluded=[]),
                   lambda v: v['paused_quote_rejections'].pop(), lambda v: v['recovery_source_frames'].pop(),
                   lambda v: v['recovery_exit_source'].update(effective_fee='0'), lambda v: v.update(paused_branch_sha256='0'*64),
                   lambda v: v['independent_native_plan_replay'].update(passed=False),
                   lambda v: v['independent_native_plan_replay']['routes'][0].pop('actual_amount_in'),
                   lambda v: v['independent_native_plan_replay']['routes'][0].update(actual_amount_out='100'),
                   lambda v: v['independent_native_plan_replay']['routes'][0].update(actual_amount_out='099'),
                   lambda v: v['independent_native_plan_replay']['routes'][0].update(after_getters_checked=0),
                   lambda v: v['independent_native_plan_replay']['routes'].__setitem__(3,v['independent_native_plan_replay']['routes'][0])]
        for index, change in enumerate(changes):
            value = copy.deepcopy(report())
            change(value)
            with self.subTest(index=index), self.assertRaises(ValueError):
                validate_process(value, {'source':'bound'}, {'input':'bound'})


class ManagedV2ReviewTests(unittest.TestCase):
    def test_reviews_require_two_distinct_reviewers_and_exact_frozen_inputs(self):
        import managed_v2_acceptance as gate
        runtime = {'source_tree_sha256':'source','binaries':{'go':'checked'}}
        inputs = {'reference':'fixed'}
        good = dict(passed=True,authority='independent-agent-source-review',scope='managed-v2-implemented-local-scope',
                    runtime=runtime,input_hashes=inputs,verification_sha256='verification',
                    spec=dict(passed=True,reviewer='spec-agent',unresolved=[]),
                    quality=dict(passed=True,reviewer='quality-agent',unresolved=[]))
        validate = getattr(gate,'validate_review',None)
        self.assertIsNotNone(validate,'missing independent frozen two-stage review gate')
        validate(good,runtime,inputs,'verification')
        edits = [lambda v:v.pop('quality'),lambda v:v['spec'].update(passed=False),
                 lambda v:v['quality'].update(reviewer='spec-agent'),lambda v:v['quality'].update(unresolved=['P1']),
                 lambda v:v.update(runtime={}),lambda v:v.update(input_hashes={}),
                 lambda v:v.update(verification_sha256='stale')]
        for edit in edits:
            value = copy.deepcopy(good);edit(value)
            with self.assertRaises(ValueError):validate(value,runtime,inputs,'verification')


if __name__ == '__main__':
    unittest.main()
