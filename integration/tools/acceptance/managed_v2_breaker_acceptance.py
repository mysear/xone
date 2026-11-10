#!/usr/bin/env python3
"""Close the finite Managed V2 breaker batch against one exact reviewed freeze."""
from pathlib import Path
import hashlib
import json
import re
import sys
import time
from weighted_evidence import runtime_fingerprint, snapshot_inputs, write_result
from managed_v2_multi_acceptance import validate_verification
from managed_v2_breaker_smoke import validate_rejected_case
ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / 'integration/artifacts/local'
TARGET = OUT / 'managed-v2-breaker-local-acceptance.json'
COMMANDS = (
 ('verification', ('python3', 'tools/acceptance/verify.py')),
 ('reference', ('make', 'managed-v2-breaker-reference-check', 'managed-v2-breaker-wire-check')),
 ('helpers', ('make', 'managed-v2-breaker-helper-test')),
 ('worker', ('python3', 'tools/acceptance/evm_execution_smoke.py')),
 ('bpt-process', ('python3', 'tools/acceptance/managed_v2_breaker_smoke.py')),
)
STAGE_REPORTS = {'verification': ('verification.json',), 'worker': ('evm-execution-three-service-smoke.json',),
                 'bpt-process': ('managed-v2-breaker-three-service-smoke.json',)}
def require(value, message):
 if not value: raise ValueError(message)
def sha(path): return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def load(name): return json.loads((OUT / name).read_text())
REQUIRED_REVIEW_SCOPES = ('CB1 original source and exact scenario/transaction/account ownership',
 'CB2 raw42/getter/profile/wire/clone/unknown',
 'CB3 exact power/cache/scaled callback math and full state',
 'CB4/5 source config/linked pure library and actual plan evidence tools',
 'CI complete race inventory sharding',
 'CB5 final evidence closure and progress qualification')
REQUIRED_REVIEW_FILES = ('common/protoc/proto/dex/v1/state.proto',
 'chain-parser-go/internal/domain/managed_v2_circuit_state.go',
 'chain-parser-rust/crates/domain/src/managed_v2_circuit_state.rs',
 'dex-router/crates/domain/src/managed_v2_circuit_state.rs',
 'chain-parser-go/internal/chain/evm/managed_v2_circuit.go',
 'chain-parser-go/internal/chain/evm/managed_v2.go',
 'chain-parser-go/internal/config/managed_v2.go',
 'dex-router/crates/config/src/managed_v2.rs',
 'dex-router/crates/protocol-math/src/managed_v2_breaker.rs',
 'dex-router/crates/protocol-math/src/managed_v2_bpt.rs',
 'dex-router/crates/protocol-math/src/managed_v2_swap.rs',
 'dex-router/crates/protocol-math/src/managed_v2_pool.rs',
 'dex-router/crates/routing/src/lib.rs',
 'integration/tools/reference/balancer-managed-v2-breaker/inputs.cjs',
 'integration/tools/reference/balancer-managed-v2-breaker/runtime.cjs',
 'integration/tools/reference/balancer-managed-v2-breaker/generate.cjs',
 'integration/tools/reference/balancer-managed-v2-breaker/checker.cjs',
 'integration/tools/reference/balancer-managed-v2-breaker/scenarios.cjs',
 'integration/tools/acceptance/managed_v2_breaker_projection.py',
 'integration/tools/acceptance/managed_v2_breaker_smoke.py',
 'integration/tools/ci/managed-v2-breaker-plan-replay.cjs',
 'chain-parser-go/Makefile',
 'chain-parser-go/test/tools/go_unit_shards.py',
 'chain-parser-go/test/tools/test_go_unit_shards.py',
 'integration/tools/acceptance/managed_v2_breaker_acceptance.py',
 'integration/tools/acceptance/managed_v2_breaker_final.py',
 'integration/tools/acceptance/tests/test_managed_v2_breaker_acceptance.py',
 'integration/tools/progress_document.py',
 'integration/tools/ci/tests/test_progress_document.py',
 'integration/tools/acceptance/weighted_evidence.py',
 'integration/Makefile',
 'integration/tools/ci/progress_bindings.py')

PROGRESS_PLAN_ANCHOR = 'plan-2026-11-06-managed-v2-breaker'
REQUIRED_PROGRESS_TASKS = {
    'CB1': ['WP03', 'WP11', 'WP25'],
    'CB2': ['WP02', 'WP04', 'WP05', 'WP11', 'WP25'],
    'CB3': ['WP11', 'WP13', 'WP25'],
    'CB4': ['WP04', 'WP05', 'WP09', 'WP11', 'WP12', 'WP13', 'WP14'],
    'CB5': ['WP03', 'WP04', 'WP05', 'WP09', 'WP11', 'WP12', 'WP13', 'WP14', 'WP16', 'WP25'],
}
REQUIRED_PROGRESS_DESIGNS = {
    '11.3.managed-v2-breaker-state': ['WP02', 'WP11', 'WP25'],
    '7.3.managed-v2-breaker-source': ['WP03', 'WP04', 'WP05', 'WP11', 'WP25'],
    '12.2.managed-v2-breaker-transition': ['WP11', 'WP13', 'WP25'],
    '12.9.managed-v2-breaker-routing': ['WP11', 'WP12', 'WP13', 'WP14'],
    '12.14.managed-v2-breaker-reference': ['WP03', 'WP11', 'WP13', 'WP25'],
    '23.managed-v2-breaker-process': ['WP04', 'WP05', 'WP09', 'WP11', 'WP12', 'WP13', 'WP14', 'WP16', 'WP25'],
}
HELPER_CASES_PER_MODE = 44


def validate_progress_document(document):
    """Validate this batch's complete inventory and return only frozen structure."""
    require(type(document) is dict, 'progress document must be an object')
    batch = document.get('current_batch')
    require(type(batch) is dict and batch.get('plan_anchor') == PROGRESS_PLAN_ANCHOR,
            'progress current batch must be the breaker plan')

    def inventory(rows, identity, required, bindings=False):
        require(type(rows) is list and len(rows) == len(required)
                and all(type(row) is dict and type(row.get(identity)) is str for row in rows),
                'progress inventory is incomplete: ' + identity)
        require({row[identity] for row in rows} == set(required),
                'progress inventory is missing or duplicates an identity: ' + identity)
        normalized = []
        for row in sorted(rows, key=lambda item: item[identity]):
            key = row[identity]
            packages = row.get('work_packages')
            require(type(packages) is list and all(type(value) is str for value in packages)
                    and sorted(packages) == sorted(required[key]),
                    'progress work packages differ: ' + key)
            entry = {identity: key, 'work_packages': sorted(packages)}
            if bindings:
                for name in ('paths', 'tests'):
                    values = row.get(name)
                    require(type(values) is list and bool(values)
                            and all(type(value) is str and bool(value.strip()) for value in values)
                            and len(set(values)) == len(values), 'progress binding is incomplete: ' + key + '/' + name)
                    entry[name] = sorted(values)
            normalized.append(entry)
        return normalized

    tasks = inventory(batch.get('plan_task_progress'), 'task_id', REQUIRED_PROGRESS_TASKS)
    clauses = document.get('requirement_traceability')
    require(type(clauses) is list and all(type(row) is dict and type(row.get('design_clause')) is str
                                        for row in clauses), 'progress design inventory is unavailable')
    breaker = [row for row in clauses if '.managed-v2-breaker' in row['design_clause']]
    designs = inventory(breaker, 'design_clause', REQUIRED_PROGRESS_DESIGNS, bindings=True)
    return dict(plan_anchor=PROGRESS_PLAN_ANCHOR, tasks=tasks, design_bindings=designs)


def progress_structure_sha256(root):
    # CLI scripts start with acceptance/ on sys.path; the Markdown reader lives one level up.
    tools = str(Path(__file__).resolve().parents[1])
    if tools not in sys.path:
        sys.path.insert(0, tools)
    from progress_document import load_progress
    structure = validate_progress_document(load_progress(root))
    return hashlib.sha256(json.dumps(structure, sort_keys=True, separators=(',', ':'),
                                     ensure_ascii=False).encode()).hexdigest()


def validate_progress_binding(record, current):
    require(type(current) is str and re.fullmatch(r'[0-9a-f]{64}', current) is not None
            and record.get('progress_structure_sha256') == current,
            'progress structure digest is missing or changed')


def _timestamp(value):
    return type(value) is int and value > 0


def validate_serial(serial, runtime, inputs, freeze_sha256, read_file, progress_sha256):
    validate_progress_binding(serial, progress_sha256)
    require(serial.get('schema_version') == 2 and serial.get('passed') is True
            and serial.get('state') == 'PASSED' and serial.get('inputs_unchanged') is True,
            'serial commands have not all passed')
    require(serial.get('runtime') == runtime and serial.get('input_hashes') == inputs
            and serial.get('freeze_sha256') == freeze_sha256, 'serial belongs to another freeze')
    commands = serial.get('commands')
    require(type(commands) is list and len(commands) == len(COMMANDS), 'exact required stages')
    start, completed = serial.get('started_at_unix_ms'), serial.get('completed_at_unix_ms')
    require(_timestamp(start) and _timestamp(completed) and completed >= start, 'serial timestamps')
    previous = start
    for row, (stage, command) in zip(commands, COMMANDS):
        require(type(row) is dict and row.get('stage') == stage
                and row.get('command') == list(command), 'wrong serial stage or command')
        require(type(row.get('exit_code')) is int and row['exit_code'] == 0, 'stage failed')
        begin, end = row.get('started_at_unix_ms'), row.get('completed_at_unix_ms')
        require(_timestamp(begin) and _timestamp(end) and previous <= begin <= end <= completed,
                'serial stage is incomplete or outside this run')
        previous = end
        expected_log = f'managed-v2-breaker-final-serial-{stage}.log'
        require(row.get('log') == expected_log, 'unexpected stage log')
        try:
            require(row.get('log_sha256') == hashlib.sha256(read_file(expected_log)).hexdigest(),
                    'retained stage log changed')
            evidence = {name: hashlib.sha256(read_file(name)).hexdigest()
                        for name in STAGE_REPORTS.get(stage, ())}
        except (OSError, KeyError) as error:
            raise ValueError('missing stage output') from error
        require(row.get('evidence_sha256') == evidence, 'stage evidence is missing or changed')


def validate_stage_report(serial, stage, report):
    command = next(row for row in serial['commands'] if row['stage'] == stage)
    if stage == 'verification':
        start = completed = report.get('measured_at_unix_ms')
    else:
        start, completed = report.get('started_at_unix_ms'), report.get('completed_at_unix_ms')
    require(_timestamp(start) and _timestamp(completed)
            and command['started_at_unix_ms'] <= start <= completed <= command['completed_at_unix_ms'],
            'report was not produced inside its current serial command: ' + stage)


def validate_review(review, runtime, inputs, read_file):
    require(review.get('runtime') == runtime and review.get('input_hashes') == inputs,
            'review belongs to another freeze')
    specs, quality, sources = review.get('spec_reviews'), review.get('quality_review'), review.get('source_files')
    require(type(specs) is list and len(specs) == len(REQUIRED_REVIEW_SCOPES)
            and all(type(row) is dict for row in specs), 'missing SPEC review inventory')
    require({row.get('scope') for row in specs} == set(REQUIRED_REVIEW_SCOPES), 'missing or duplicate SPEC scope')
    require(type(quality) is dict and quality.get('status') == 'CLOSED', 'quality review not closed')
    reviewers = [row.get('reviewer') for row in specs]
    require(all(type(name) is str and name.strip() for name in reviewers)
            and len(set(reviewers)) == len(reviewers)
            and type(quality.get('reviewer')) is str and quality['reviewer'].strip()
            and quality['reviewer'] not in reviewers, 'SPEC and quality reviewers must be independent')
    require(all(row.get('status') == 'CLOSED' for row in specs), 'SPEC review not closed')
    require(type(sources) is dict and set(REQUIRED_REVIEW_FILES) <= set(sources), 'missing reviewed source inventory')
    for path, digest in sources.items():
        require(type(path) is str and not Path(path).is_absolute() and '..' not in Path(path).parts,
                'reviewed file escapes workspace')
        try:
            observed = hashlib.sha256(read_file(path)).hexdigest()
        except (OSError, KeyError) as error:
            raise ValueError('reviewed file is unavailable') from error
        require(digest == observed, 'reviewed source changed: ' + path)


def run_audit(audit, target=TARGET):
    running = dict(passed=False, state='RUNNING', production_ready=False,
                   started_at_unix_ms=int(time.time() * 1000))
    write_result(target, running)
    try:
        result = audit()
        require(result.get('passed') is True and result.get('state') == 'LOCAL_SCOPE_VERIFIED',
                'audit did not establish local acceptance')
        write_result(target, result)
        return result
    except BaseException as error:
        write_result(target, dict(running, state='FAILED', error=str(error),
                                 error_type=type(error).__name__, completed_at_unix_ms=int(time.time() * 1000)))
        raise

def audit():
 freeze=load('managed-v2-breaker-freeze.json');runtime=runtime_fingerprint();inputs=snapshot_inputs()
 progress_sha256=progress_structure_sha256(ROOT);validate_progress_binding(freeze,progress_sha256)
 require(freeze['runtime']==runtime and freeze['input_hashes']==inputs,'reviewed freeze changed')
 serial=load('managed-v2-breaker-final-serial-run.json')
 validate_serial(serial,runtime,inputs,sha(OUT/'managed-v2-breaker-freeze.json'),lambda name:(OUT/name).read_bytes(),progress_sha256)
 helper=(OUT/'managed-v2-breaker-final-serial-helpers.log').read_text()
 require(len(re.findall(r'Ran '+str(HELPER_CASES_PER_MODE)+r' tests in ',helper))==2 and len(re.findall(r'Ran 17 tests in ',helper))==2 and len(re.findall(r'^OK$',helper,re.M))==4,f'same {HELPER_CASES_PER_MODE} breaker and 17 progress cases must pass ordinary and optimized runs')
 verify=load('verification.json');validate_verification(verify,runtime);validate_stage_report(serial,'verification',verify)
 go_log=(OUT/'verification-chain-parser-go.log').read_text()
 require('Ran 7 tests in ' in go_log,'Go inventory tool regression suite required')
 require(go_log.count('EVM race inventory: 95 cases in 8 complete shards')==1,'exact current EVM race inventory required')
 shard_rows=re.findall(r'^EVM race shard (\d+)/8: (.+)$',go_log,re.M)
 require([int(n)for n,_ in shard_rows]==list(range(1,9)),'every EVM shard command completed in order')
 selected=[name for _,text in shard_rows for name in text.split(', ')]
 require(len(selected)==95 and len(set(selected))==95,'EVM names selected exactly once')
 process=load('managed-v2-breaker-three-service-smoke.json');validate_stage_report(serial,'bpt-process',process)
 require(process['passed']is True and process['state']=='PASSED' and process['inputs_unchanged']is True,'BPT source process not passed')
 require(process['source_binding_started']==runtime and process['input_hashes_started']==inputs,'BPT process differs from reviewed freeze')
 require([m['namespace']for m in process['namespaces']]==['cached','dynamic','boundaries','shared'],'four original namespaces')
 require([len(m['source_frames'])for m in process['namespaces']]==[28,163,97,12],'all supported source banks')
 require([len(m['quotes'])for m in process['namespaces']]==[50,302,132,32],'full actual API inventory')
 require([len(m['rejections'])for m in process['namespaces']]==[12,0,18,0],'full original reject/zero-output inventory')
 raw=json.loads((ROOT/'integration/test/contracts/managed-v2-breaker.json').read_bytes());cases={c['id']:c for c in raw['cases']}
 for mode in process['namespaces']:
  require(mode['passed']is True and mode['restarted']is True,'mode/restart incomplete')
  require(len(mode['faults'])==6 and len({f['fault']for f in mode['faults']})==6,'six unique source faults')
  for source in mode['source_frames']:require(source['independent_cpp_identical']is True,'full independent C++ protobuf')
  for reject in mode['rejections']:validate_rejected_case(reject['status'],reject['response'],cases[reject['case']])
  for f in mode['faults']:
   require(f['hits']>0 and 0<=f['catchup_to_poison_ms']<=60000,'declared source poison must be read')
   require(set(f['withdrawal_events'])=={'router1','router2'},'two explicit withdrawal observations')
   for event in f['withdrawal_events'].values():require(0<=event['latency_ms']<=25000 and event['observed_monotonic']<=f['poison_started_monotonic']+25,'strict first-poison withdrawal budget')
   require(all(w['evm_status']==503 and w['fast_quote_status']!=200 and w['solana_status']==200 for w in f['withdrawals'].values()),'scope isolation and cached-quote rejection')
 native=process['independent_actual_plan_replay'];require(native['passed']is True and len(native['executions'])==230 and len(native['quote_links'])==516 and native['selected_transactions_executed']==233,'all actual plans replayed')
 require(sum(len(e['quote']['routes'])==2 for e in native['executions'])==1,'actual capacity split must execute')
 diff=process['actual_selected_model_differential'];require(diff['passed']is True and diff['executions']==230 and diff['selected_transactions']==233,'every native step full poststate comparison')
 for key,name in [('native_report_sha256','managed-v2-breaker-actual-selected-native.json'),('vectors_sha256','managed-v2-breaker-actual-selected-vectors.json'),('log_sha256','managed-v2-breaker-actual-selected-model.log')]:require(diff[key]==sha(OUT/name),'actual selected evidence changed')
 worker=load('evm-execution-three-service-smoke.json');validate_stage_report(serial,'worker',worker);require(worker['passed']is True and worker['inputs_unchanged']is True and worker['simulation_checks']==7 and worker['source_faults']==2 and worker['source_binding']==runtime and worker['input_hashes']==inputs,'real worker final runtime/protocol execution')
 review=load('managed-v2-breaker-review.json');validate_review(review,runtime,inputs,lambda name:(ROOT/name).read_bytes())
 require(progress_structure_sha256(ROOT)==progress_sha256,'progress structure changed during closure audit')
 require(runtime_fingerprint()==runtime and snapshot_inputs()==inputs,'evidence mutated during closure audit')
 result={'schema_version':1,'passed':True,'state':'LOCAL_SCOPE_VERIFIED','scope':'finite-local-published-Managed-enabled-breaker-GIVEN_IN','source_tree_sha256':runtime['source_tree_sha256'],'runtime':runtime,'input_hashes':inputs,'progress_structure_sha256':progress_sha256,'verification_sha256':sha(OUT/'verification.json'),'process_sha256':sha(OUT/'managed-v2-breaker-three-service-smoke.json'),'review_sha256':sha(OUT/'managed-v2-breaker-review.json'),'serial_report_sha256':sha(OUT/'managed-v2-breaker-final-serial-run.json'),'worker_report_sha256':sha(OUT/'evm-execution-three-service-smoke.json'),'counts':{'source_banks':300,'canonical_pool_observations':600,'successful_api_quotes':516,'expected_business_rejections':30,'source_faults':24,'router_restarts':4,'native_plan_executions':230,'native_transactions':233,'real_worker_simulations':7,'helper_cases_per_mode':HELPER_CASES_PER_MODE,'progress_cases_per_mode':17},'remaining':['rates/mixed and unmodeled custody/self-funding full accounting','ExactOut','trusted actual-chain source and complete proof/fees','public atomic execution, VERIFIED and build','all original work packages and remaining design requirements'],'production_ready':False,'verified':False,'all_requested_features_complete':False,'full_work_packages_accepted':0,'audited_at_unix_ms':int(time.time()*1000),'audit_script_sha256':sha(__file__)}
 return result

def main():
 result = run_audit(audit)
 print(json.dumps({k: result[k] for k in ['passed', 'state', 'scope', 'counts', 'production_ready', 'full_work_packages_accepted']}))
if __name__ == '__main__': main()
