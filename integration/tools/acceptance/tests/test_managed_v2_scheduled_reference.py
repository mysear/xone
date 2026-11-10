"""Original VM schedule reference: core qualification plus opt-in full clean replay."""
import json
import os
import pathlib
import shutil
import subprocess
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[4]
CLI = ROOT / 'integration/tools/ci/managed-v2-scheduled-golden.cjs'
REF = ROOT / 'integration/tools/reference/balancer-managed-v2-scheduled'
NODE = os.environ.get('XONE_REFERENCE_NODE', shutil.which('node') or 'node')
COPIES = [ROOT / p / 'test/contracts/managed-v2-scheduled.json'
          for p in ('integration', 'chain-parser-go', 'chain-parser-rust', 'dex-router')]


def run(*args, cwd=ROOT):
    return subprocess.run([NODE, *map(str, args)], cwd=cwd, text=True,
                          capture_output=True, timeout=1800)


class ScheduledReference(unittest.TestCase):
    def test_registered_cli_validates_all_four_copies(self):
        self.assertTrue(CLI.is_file(), 'scheduled CLI must exist')
        before = [(p.read_bytes(), p.stat().st_mtime_ns) for p in COPIES]
        p = run(CLI, '--validate')
        self.assertEqual(p.returncode, 0, p.stderr + p.stdout)
        self.assertIn('"swaps":53', p.stdout)
        self.assertEqual(before, [(p.read_bytes(), p.stat().st_mtime_ns) for p in COPIES])
        self.assertEqual(len({p.stat().st_ino for p in COPIES}), 4)
        self.assertTrue(all(p.stat().st_nlink == 1 for p in COPIES))
        self.assertTrue(all(x[0] == before[0][0] for x in before))

    def test_atomic_counts_namespaces_and_actual_drift(self):
        b = json.loads(COPIES[0].read_bytes())
        self.assertEqual([len(b['branches'][k]['states']) for k in ('two', 'three')], [64, 69])
        self.assertEqual([sum(c['namespace'] == k for c in b['cases'])
                          for k in ('two', 'three', 'reschedule')], [17, 28, 8])
        self.assertEqual(len(b['checkpoints']), 41)
        self.assertEqual(len(b['management']), 12)
        self.assertEqual(sum(m['status'] == '0x0' for m in b['management']), 2)
        rs = b['branches']['reschedule']
        self.assertEqual({int(r['computed_start_sum']) - 10**18 for r in rs['reschedules']}, {-1, 1})
        sums = {int(s['sum_weights']) - 10**18 for s in rs['states']}
        self.assertTrue({-2, -1, 1, 2}.issubset(sums))
        cases = {c['id']: c for c in rs['cases']}
        self.assertTrue(all(cases[c]['receipt_status'] == '0x1'
                            for r in rs['reschedules'] for c in r['cases']))
        # Shared prefix hashes belong to independent namespaces and must survive indexing.
        hashes = [b['branches'][k]['prefix_replay']['headers'][0]['hash']
                  for k in ('two', 'three', 'reschedule')]
        self.assertEqual(len(set(hashes)), 1)
        self.assertEqual(len({c['id'] for c in b['cases']}), 53)

    def test_one_copy_readonly(self):
        with tempfile.TemporaryDirectory(prefix='scheduled-one-') as tmp:
            p = pathlib.Path(tmp) / 'managed-v2-scheduled.json'
            shutil.copy2(COPIES[0], p)
            before = (p.read_bytes(), p.stat().st_mtime_ns)
            result = run(CLI, '--validate', '--input', p)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(before, (p.read_bytes(), p.stat().st_mtime_ns))

    def test_core_tampering_rejected(self):
        # One process, real structural checks, no local math oracle or forged pass flag.
        script = r'''
const fs=require('node:fs'),assert=require('node:assert/strict');
const base='./integration/tools/reference/balancer-managed-v2-scheduled/';
const {check}=require(base+'checker.cjs'),{validateEndpoint}=require(base+'reschedule/checker.cjs'),I=require(base+'inputs.cjs'),{ethers}=I.E.rr('ethers');
const b=JSON.parse(fs.readFileSync('integration/test/contracts/managed-v2-scheduled.json'));
const clone=()=>JSON.parse(JSON.stringify(b));
const mutations=[
 ['reserved token bit',x=>{let z=x.branches.three,s=z.states[0],w='0x'+(BigInt(s.packed.tokens[0].word)|(1n<<200n)).toString(16);s.packed.tokens[0].word=w;s.token_states[0].raw_word=w;s.storage.find(r=>r.address===z.configuration.pool&&r.key===s.packed.tokens[0].key).value=w;} ],
 ['pool word time',x=>x.branches.three.states[0].packed.pool_word='0x0'],
 ['sourceH',x=>x.branches.reschedule.states[0].source_h='0x999'],
 ['clock',x=>x.branches.three.states[0].source_clock++],
 ['signed nonce',x=>x.branches.three.transactions[0].transaction.nonce='0xff'],
 ['two coherent Vault cash forgery',x=>{const z=x.branches.two,c=z.cases[0],s=z.states.find(s=>s.id===c.after),g=s.getters.find(g=>g.to===z.vault&&g.method==='getPoolTokenInfo'&&g.args[1]===c.token_in),abi=new ethers.utils.Interface(I.A.locate(I.A.load().builds,'Vault','Vault').c.abi);g.decoded[0]=String(BigInt(g.decoded[0])+1n);g.result=abi.encodeFunctionResult(g.method,g.decoded);} ],
 ['getter source block',x=>x.branches.reschedule.states[0].getters[0].block_number='0xff'],
 ['two prefix header inventory',x=>x.branches.two.prefix_replay.headers.pop()],
 ['current raw getter',x=>{let g=x.branches.reschedule.states[0].getters.find(g=>g.method==='getNormalizedWeights');g.result=g.result.slice(0,-1)+(g.result.endsWith('0')?'1':'0');}],
 ['forged rawOut',x=>x.branches.reschedule.cases.at(-1).expected_raw_out='1'],
 ['manager receipt',x=>{let z=x.branches.reschedule;z.transactions.find(t=>t.id===z.reschedules[0].transaction).receipt.status='0x0';}],
 ['bad caller success',x=>{let z=x.branches.reschedule;z.transactions.find(t=>t.id===z.reschedules[0].invalid_user_end.transaction).receipt.status='0x1';}],
 ['normalized computed start',x=>x.branches.reschedule.reschedules[0].computed_start[0]='1'],
 ['two chain options',x=>x.branches.two.chain_options.chain.allowUnlimitedContractSize=true],
 ['detached two case nonce',x=>x.branches.two.cases[0].execution.transaction.nonce='0xff'],
 ['Node alias metadata',x=>x.inputs.node.node.sha256='0'.repeat(64)],
 ['manifest SHA',x=>x.inputs.manifest_sha256='0'.repeat(64)],
 ['source inventory',x=>x.branches.three.inputs.source_manifest.pop()],
 ['branch identity alias',x=>x.cases[0].namespace='three'],
 ['trader wallet',x=>{let z=x.branches.reschedule,s=z.states.at(-1);s.global_wallets[0].balances[z.configuration.roles.trader]='1';}]
];
for(const [label,mutate]of mutations){const x=clone();mutate(x);assert.throws(()=>check(x),undefined,label);}
for(const r of b.branches.reschedule.reschedules){const z=b.branches.reschedule,source=z.states.find(s=>s.id===r.source),a={weights:r.computed_start,source,config:z.configuration};validateEndpoint({...a,origin:'computed-original-getter'});assert.throws(()=>validateEndpoint({...a,origin:'user-end-array'}));assert.throws(()=>validateEndpoint({...a,source:undefined,origin:'computed-original-getter'}));}
console.log(JSON.stringify({rejected:mutations.length,endpointOrigins:2}));
'''
        result = run('-e', script)
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        self.assertIn('"rejected":20', result.stdout)

    def test_output_boundaries_and_links(self):
        script = r'''
const fs=require('node:fs'),os=require('node:os'),path=require('node:path'),assert=require('node:assert/strict');
const {output,args,defaults}=require('./integration/tools/ci/managed-v2-scheduled-golden.cjs');
const dir=fs.mkdtempSync(path.join(os.tmpdir(),'scheduled-links-'));
try{const file=path.join(dir,'managed-v2-scheduled.json'),src=defaults[0];fs.symlinkSync(src,file);assert.throws(()=>output(file));fs.unlinkSync(file);fs.linkSync(src,file);assert.throws(()=>output(file));fs.unlinkSync(file);assert.throws(()=>output(path.resolve('integration/test/contracts/managed-v2-aum.json')));assert.throws(()=>output(path.resolve('integration/tools/reference/balancer-managed-v2-scheduled/README.md')));assert.throws(()=>args(['--output','relative.json']));assert.throws(()=>args(['--validate','--output',file]));assert.equal(output(file),file);}finally{fs.rmSync(dir,{recursive:true,force:true});}
'''
        result = run('-e', script)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_unknown_source_file_rejected(self):
        extra = REF / 'unregistered-test-source.cjs'
        try:
            extra.write_text('// deliberately outside authenticated inventory\n')
            result = run(CLI, '--validate', '--input', COPIES[0])
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('exact scheduled producer/checker input closure', result.stderr)
        finally:
            extra.unlink(missing_ok=True)

    @unittest.skipUnless(os.environ.get('XONE_SCHEDULED_FULL_CHECK') == '1',
                         'set XONE_SCHEDULED_FULL_CHECK=1 for clean-copy original VM byteexact replay')
    def test_full_byteexact_from_clean_registered_inputs(self):
        b = json.loads(COPIES[0].read_bytes())
        # Copy only registered files, no prototype directories, absolute source checkout,
        # unregistered caches, old gates or generated verified outputs.
        paths = {f['path'] for f in b['inputs']['files']}
        paths.update(f['path'] for f in b['inputs']['original']['files'])
        paths.update(f['path'] for f in b['inputs']['node']['files'])
        paths.update(['integration/tools/reference/balancer-managed-v2-aum/file-manifest.json',
                      'integration/tools/reference/balancer-managed-v2-scheduled/file-manifest.json',
                      b['inputs']['original_bank']['path']])
        paths.update(str(p.relative_to(ROOT)) for p in COPIES)
        with tempfile.TemporaryDirectory(prefix='scheduled-clean-') as tmp:
            clone = pathlib.Path(tmp)
            for relative in sorted(paths):
                dest = clone / relative
                dest.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(ROOT / relative, dest)
            copied = [clone / p.relative_to(ROOT) for p in COPIES]
            before = [(p.read_bytes(), p.stat().st_mtime_ns) for p in copied]
            result = run(clone / CLI.relative_to(ROOT), '--check', cwd=clone)
            self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
            self.assertIn('"swaps":53', result.stdout)
            self.assertEqual(before, [(p.read_bytes(), p.stat().st_mtime_ns) for p in copied])


if __name__ == '__main__':
    unittest.main()
