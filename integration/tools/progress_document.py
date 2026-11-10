"""Read progress directly from the human-readable DEVELOPMENT_PROGRESS.md tables.

Design requirements stay in the design document. Static source/test bindings are
validation code; no embedded JSON or separately edited progress snapshot is used.
"""
from pathlib import Path
import json
import re

DOCUMENT = 'DEVELOPMENT_PROGRESS.md'
SCOPE_PLANS = {
    'rayclmm': 'plan-2026-10-27-raydium-clmm-static',
    'rayclmm-token-fee': 'plan-2026-10-28-raydium-clmm-token-fee',
    'rayclmm-bitmap': 'plan-2026-10-28-raydium-clmm-bitmap',
    'rayclmm-seed': 'plan-2026-10-29-raydium-clmm-custom-seed',
    'rayclmm-dynamic': 'plan-2026-10-29-raydium-clmm-dynamic',
}
STATUSES = {'PARTIAL_LOCAL', 'LOCAL_SCOPE_VERIFIED', 'NOT_STARTED', 'NOT_IMPLEMENTED', 'IN_PROGRESS', 'ACCEPTED'}


def read_plan(root, anchor):
    if not re.fullmatch(r'[a-z0-9-]+', anchor):
        raise ValueError('Invalid progress section')
    text = (Path(root) / DOCUMENT).read_text()
    marker = '<a id="' + anchor + '"></a>'
    if text.count(marker) != 1:
        raise ValueError('Missing or duplicate progress section: ' + anchor)
    section = text.split(marker, 1)[1]
    return re.split(r'<a id="[a-z0-9-]+"></a>', section, maxsplit=1)[0]


def _table(root, anchor, headers):
    lines = [line for line in read_plan(root, anchor).splitlines() if line.startswith('|')]
    if len(lines) < 3:
        raise ValueError('Missing progress table: ' + anchor)
    split = lambda line: [part.replace(r'\|', '|').replace('<br>', '\n').strip() for part in re.split(r'(?<!\\)\|', line)[1:-1]]
    if split(lines[0]) != headers:
        raise ValueError('Unexpected progress columns: ' + anchor)
    rows = [split(line) for line in lines[2:]]
    if any(len(row) != len(headers) for row in rows) or len({row[0] for row in rows}) != len(rows):
        raise ValueError('Invalid or duplicate progress row: ' + anchor)
    return [dict(zip(headers, row)) for row in rows]


def _status(value):
    if value not in STATUSES:
        raise ValueError('Unknown progress status: ' + value)
    return value


def _refs(value):
    return re.findall(r'\]\(([^)]+)\)', value)


def _list(value):
    return [part.strip() for part in value.split(',') if part.strip()]


def _report(root, filename):
    path = Path(root) / 'integration/artifacts/local' / filename
    return json.loads(path.read_text()) if path.is_file() else {}


def _design_clause(root, section):
    path = Path(root) / 'XONE_SYSTEM_DESIGN.md'
    if not path.is_file():
        return ''
    text = path.read_text()
    match = re.search(r'^### ' + re.escape(section) + r'\b[^\n]*\n(.*?)(?=^### |\Z)', text, re.M | re.S)
    if match is None:
        raise ValueError('Unknown design section: ' + section)
    return match[1].strip()


def _bindings():
    from ci.progress_bindings import BINDINGS
    return BINDINGS


def _tasks(root, anchor):
    rows = _table(root, anchor, ['任务', '内容', '工作包', '状态', '验证'])
    return [{'task_id': row['任务'], 'title': row['内容'], 'work_packages': _list(row['工作包']),
             'status': _status(row['状态']), 'plan': DOCUMENT, 'plan_anchor': anchor,
             'evidence_refs': [ref.split('#', 1)[0] for ref in _refs(row['验证'])]} for row in rows]


def _batch_report(root, batch, scope=None):
    selected = scope or next((name for name, anchor in SCOPE_PLANS.items()
                              if anchor == batch.get('plan_anchor')), None)
    if selected:
        return _report(root, selected + '-final-source.json')
    # A new non-protocol batch has no inherited CLMM math counts or hash.
    reports = [json.loads((Path(root) / ref).read_text()) for ref in batch['evidence']
               if (Path(root) / ref).is_file() and ref.endswith('.json')]
    fingerprints = {report['source_tree_sha256'] for report in reports if report.get('source_tree_sha256')}
    if len(fingerprints) > 1:
        raise ValueError('Latest batch reports have conflicting verification fingerprints')
    return next((report for report in reports if report.get('source_tree_sha256')), {})


def load_progress(root, scope=None):
    root = Path(root)
    work = _table(root, 'work-packages', ['工作包', '功能', '方案', '状态', '已完成范围', '待完成范围'])
    mappings = _table(root, 'design-traceability', ['设计功能', '工作包', '状态', '已完成', '剩余', '验证'])
    history = _table(root, 'batch-history', ['序号', '批次', '结果', '验证'])
    bindings = _bindings()
    clauses = []
    work_ids = {row['工作包'] for row in work}
    for row in mappings:
        packages = _list(row['工作包'])
        if not packages or not set(packages) <= work_ids:
            raise ValueError('Functional scope requires known work packages')
        rule = bindings.get(row['设计功能'], {})
        refs = _refs(row['验证'])
        if row['状态'] in {'PARTIAL_LOCAL', 'LOCAL_SCOPE_VERIFIED', 'ACCEPTED'}:
            if not rule.get('paths') or not rule.get('tests') or not rule.get('evidence_refs'):
                raise ValueError('Completed functional scope requires source/test/evidence rules')
            if not refs or not set(refs) <= set(rule['evidence_refs']):
                raise ValueError('Readable evidence differs from the validation rule')
        clauses.append({'design_clause': row['设计功能'], 'work_packages': packages, 'status': _status(row['状态']),
                        'implemented': row['已完成'], 'remaining': row['剩余'], 'paths': list(rule.get('paths', [])),
                        'tests': list(rule.get('tests', [])), 'evidence_refs': list(rule.get('evidence_refs', _refs(row['验证'])))})
    packages = []
    for row in work:
        if not re.fullmatch(r'WP\d{2}', row['工作包']):
            raise ValueError('Invalid work package')
        section = row['方案'].removeprefix('§')
        relevant = [item for item in clauses if row['工作包'] in item['work_packages']]
        packages.append({'id': row['工作包'], 'title': row['功能'], 'design_section': section, 'status': _status(row['状态']),
                         'implemented': row['已完成范围'], 'remaining': row['待完成范围'],
                         'paths': sorted({path for item in relevant for path in item['paths']}),
                         'design_requirements_verbatim': _design_clause(root, section),
                         'full_acceptance_passed': row['状态'] == 'ACCEPTED',
                         'evidence_refs': sorted({path for item in relevant for path in item['evidence_refs']})})
    text = (root / DOCUMENT).read_text()
    anchors = re.findall(r'<a id="(plan-[a-z0-9-]+)"></a>', text)
    batches = []
    assigned = set()
    for row in history:
        name = re.sub(r'\[([^]]+)\]\([^)]+\)', r'\1', row['批次'])
        batch = {'name': name, 'status': _status(row['结果']), 'evidence': _refs(row['验证'])}
        plans = [ref[1:] for ref in _refs(row['批次']) if ref.startswith('#plan-')]
        if len(plans) > 1:
            raise ValueError('Historical batch has ambiguous task plans')
        if plans:
            anchor = plans[0]
            if anchor not in anchors or anchor in assigned:
                raise ValueError('Unknown or duplicate historical task plan')
            assigned.add(anchor)
            batch['plan_anchor'] = anchor
            batch['plan_task_progress'] = _tasks(root, anchor)
        batches.append(batch)
    if assigned != set(anchors):
        raise ValueError('Every task plan must be explicitly linked from its historical batch')
    selected = SCOPE_PLANS.get(scope) if scope is not None else None
    if scope is not None and selected is None:
        raise ValueError('Unknown historical scope')
    if selected:
        index = next((i for i, batch in enumerate(batches)
                      if batch.get('plan_anchor') == selected), None)
        if index is None:
            raise ValueError('Historical scope has no matching batch')
        batches = batches[:index + 1]
    current = dict(batches[-1])
    current.setdefault('plan_task_progress', [])
    current['evidence'] = sorted(set(current['evidence'] + [p for task in current['plan_task_progress'] for p in task['evidence_refs']]))
    final = _batch_report(root, current, scope)
    current['source_tree_sha256'] = final.get('source_tree_sha256', '')
    batches[-1] = dict(current)
    return {'schema_version': 2, 'design_version': '2.3', 'production_ready': False, 'work_packages': packages,
            'requirement_traceability': clauses, 'batch_history': batches, 'current_batch': current}


def load_coverage(root, scope=None):
    rows = _table(root, 'protocol-coverage', ['协议', '适配版本', '链族', '设计条款', '工作包', '解析', '数学', '执行', '范围与剩余功能'])
    entries = [{'protocol_id': row['协议'], 'adapter_version': row['适配版本'], 'chain_family': row['链族'],
                'design_sections': _list(row['设计条款']), 'work_packages': _list(row['工作包']),
                'parser': _status(row['解析']), 'math': _status(row['数学']), 'execution': _status(row['执行']),
                'remaining': row['范围与剩余功能'].split('；')} for row in rows]
    batch = load_progress(root, scope)['current_batch']
    final = _batch_report(root, batch, scope)
    # Keep provenance visible even when the report cannot establish verified counts.
    verified = final if final.get('passed') is True and final.get('state') in (None, 'PASSED', 'LOCAL_SCOPE_VERIFIED') else {}
    return {'schema_version': 2, 'design_version': '2.3', 'production_ready': False, 'entries': entries,
            'current_batch_status': batch['status'],
            'source_tree_sha256': final.get('source_tree_sha256', ''),
            'verified_math_cases': verified.get('cases', verified.get('rayclmm_cases', 0)), 'verified_shared_sequence_count': verified.get('shared_sequences', 0),
            'verified_shared_sequence_steps': verified.get('sequence_steps', 0), 'verified_process_checks': verified.get('process_checks', 0), 'verified_tickmath_vectors': verified.get('tickmath_vectors', 0)}
