#!/usr/bin/env python3
"""Verify downloaded isolated WSOL settlement wire against original SBF programs."""
from pathlib import Path
import base64
import hashlib
import json
import sys

from solana_expanded_reference import ROOT, require
from r01_public_catalog_solana_bank import native_imports
from solana_prepare_simulation_worker import simulate

PROGRAM = 'LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo'
TOKEN = 'TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA'
SYSTEM = '11111111111111111111111111111111'
WSOL = 'So11111111111111111111111111111111111111112'


def verify(path):
    doc = json.loads(Path(path).read_bytes())
    p = doc.get('preparation', doc)
    refund = p['refundWrappedSource']
    require(type(refund) is bool and p['unwrapDestinationSol'] is (not refund), 'temp-kind')
    bank = json.loads((ROOT / ('integration/test/contracts/meteora-wsol-bank.json' if refund else
                              'integration/test/contracts/meteora-execution-bank.json')).read_bytes())
    require(p['signed'] is False and p['submitted'] is False and p['ready'] is True, 'temp-unsigned')
    digest = hashlib.sha256(json.dumps({k: v for k, v in p.items() if k != 'preparationHash'},
                                      sort_keys=True, separators=(',', ':'), ensure_ascii=False).encode()).hexdigest()
    require(digest == p['preparationHash'], 'temp-manifest-hash')
    S, P, K, I, M, T, A, C = native_imports()
    from solders.system_program import create_account_with_seed
    wire = base64.b64decode(p['unsignedTransaction'], validate=True)
    tx = T.from_bytes(wire)
    require(wire[0] == 1 and wire[1:65] == bytes(64) and
            wire[65:] == base64.b64decode(p['messageBase64']), 'temp-wire')
    caller = bank['caller']
    require(tx.message.header.num_required_signatures == 1 and
            str(tx.message.account_keys[0]) == caller, 'temp-signer')
    seed = 'xone-' + ('in-' if refund else 'out-') + p['planHash'][:20]
    temp = str(P.create_with_seed(P.from_string(caller), seed, P.from_string(TOKEN)))
    require(p['temporarySeed'] == seed and p['temporaryWsolAccount'] == temp and
            temp not in bank['accounts'], 'temp-seeded-address')
    source = temp if refund else bank['sourceTokenAccount']
    destination = bank['destinationTokenAccount'] if refund else temp
    require(p['sourceTokenAccount'] == source and p['destinationTokenAccount'] == destination,
            'temp-swap-accounts')
    require(p['sourceAccountCreation'] is refund and p['accountCreation'] is False and
            p['sourceWsolRetained'] is False, 'temp-creation')
    count = 6 if refund else 4
    programs = [SYSTEM, TOKEN] + ([SYSTEM, TOKEN] if refund else []) + [PROGRAM, TOKEN]
    require(len(tx.message.instructions) == len(p['instructions']) == count and
            [d['programId'] for d in p['instructions']] == programs, 'temp-instruction-order')
    decoded = []
    for ix, d in zip(tx.message.instructions, p['instructions']):
        require(str(tx.message.account_keys[ix.program_id_index]) == d['programId'] and
                bytes(ix.data) == base64.b64decode(d['dataBase64']) and
                [str(tx.message.account_keys[i]) for i in ix.accounts] ==
                [a['pubkey'] for a in d['accounts']], 'temp-wire-descriptors')
        decoded.append(I(P.from_string(d['programId']), bytes(ix.data),
                         [M(P.from_string(a['pubkey']), a['isSigner'], a['isWritable']) for a in d['accounts']]))
    require(bytes(T.new_unsigned(tx.message)) == wire, 'temp-message-decode')
    # Compilers may order unsigned keys differently. Compare effective rights by key.
    expected_rights = {caller: (True, True)}
    for instruction in decoded:
        expected_rights.setdefault(str(instruction.program_id), (False, False))
        for meta in instruction.accounts:
            key = str(meta.pubkey)
            old = expected_rights.get(key, (False, False))
            expected_rights[key] = (old[0] or meta.is_signer, old[1] or meta.is_writable)
    keys = list(map(str, tx.message.account_keys))
    require(len(set(keys)) == len(keys) and set(keys) == set(expected_rights), 'temp-wire-account-set')
    header = tx.message.header
    for index, key in enumerate(keys):
        signed = index < header.num_required_signatures
        writable = (index < header.num_required_signatures - header.num_readonly_signed_accounts if signed else
                    index < len(keys) - header.num_readonly_unsigned_accounts)
        require(expected_rights[key] == (signed, writable), 'temp-wire-permissions')
    official = create_account_with_seed(dict(from_pubkey=P.from_string(caller), to_pubkey=P.from_string(temp),
                                            base=P.from_string(caller), seed=seed, lamports=2039280,
                                            space=165, owner=P.from_string(TOKEN)))
    require(decoded[0] == official, 'temp-official-create-with-seed')
    require(decoded[1] == I(P.from_string(TOKEN), b'\x12' + bytes(P.from_string(caller)),
                           [M(P.from_string(temp), False, True), M(P.from_string(WSOL), False, False)]),
            'temp-initialize-owner')
    require(decoded[-1] == I(P.from_string(TOKEN), b'\x09',
                            [M(P.from_string(temp), False, True), M(P.from_string(caller), False, True),
                             M(P.from_string(caller), True, False)]), 'temp-close-refund-authority')
    if refund:
        require(bytes(decoded[2].data) == (2).to_bytes(4, 'little') + (1472).to_bytes(8, 'little') and
                [str(a.pubkey) for a in decoded[2].accounts] == [caller, temp] and
                bytes(decoded[3].data) == b'\x11' and
                [str(a.pubkey) for a in decoded[3].accounts] == [temp], 'temp-wrap-transfer-sync')
        # A pre-existing caller WSOL ATA must remain byte-for-byte untouched.
        raw = bytearray(base64.b64decode(bank['accounts'][bank['originalSourceTokenAccount']]['data'][0]))
        raw[64:72] = (3000).to_bytes(8, 'little')
        bank['accounts'][bank['sourceTokenAccount']] = dict(owner=TOKEN, executable=False,
            lamports=2039280 + 3000, data=[base64.b64encode(raw).decode(), 'base64'])
    require(p['rentLamports'] == p['rentRefundLamports'] == '2039280' and p['feeLamports'] == '5000' and
            p['wrapLamports'] == ('1472' if refund else '0') and
            p['expectedNativeRefundLamports'] == ('2039288' if refund else '2040280') and
            p['expectedPayerLamportDelta'] == ('-6464' if refund else '-4000') and
            p['totalFundingLamports'] == ('2045752' if refund else '2044280') and
            p['settlementAsset'] == (bank['pool']['token0'] if refund else 'native:solana:sol'), 'temp-funding')
    simulation = simulate(dict(bank=bank, rpc=dict(method='simulateTransaction', params=[p['unsignedTransaction'],
        dict(sigVerify=False, replaceRecentBlockhash=False, accounts={'addresses': [source, destination, caller]})])))
    require(simulation['value']['err'] is None and
            simulation['value']['accounts'][0 if refund else 1] is None, 'temp-readonly-simulation')
    signer = K.from_seed(hashlib.sha256(b'xone-meteora-reference-payer').digest())
    require(str(signer.pubkey()) == caller, 'temp-test-signer')

    def execute(control=False):
        svm = S().with_mainnet_features().with_blockhash_check(False)
        for pid in [PROGRAM, TOKEN]:
            raw = base64.b64decode(bank['accounts'][pid]['data'][0])
            pd = str(P.from_bytes(raw[4:36]))
            svm.add_program_with_loader(P.from_string(pid), base64.b64decode(bank['accounts'][pd]['data'][0])[45:],
                                        P.from_string('BPFLoaderUpgradeab1e11111111111111111111111'))
        for identity, a in bank['accounts'].items():
            svm.set_account(P.from_string(identity), A(a['lamports'], base64.b64decode(a['data'][0]),
                                                      P.from_string(a['owner']), a['executable'], 0))
        clock = base64.b64decode(bank['accounts']['SysvarC1ock11111111111111111111111111111111']['data'][0])
        svm.set_clock(C(slot=int.from_bytes(clock[:8], 'little'), epoch_start_timestamp=int.from_bytes(clock[8:16], 'little', signed=True),
                        epoch=int.from_bytes(clock[16:24], 'little'), leader_schedule_epoch=int.from_bytes(clock[24:32], 'little'),
                        unix_timestamp=int.from_bytes(clock[32:40], 'little', signed=True)))
        transaction = T.from_bytes(wire)
        if control:
            ix = decoded[-2]
            data = bytearray(ix.data)
            data[8:16] = (1463 if refund else 698).to_bytes(8, 'little')
            transaction = T.new_signed_with_payer(decoded[:-2] + [I(ix.program_id, bytes(data), ix.accounts), decoded[-1]],
                                                   signer.pubkey(), [signer], tx.message.recent_blockhash)
        else:
            transaction.sign([signer], tx.message.recent_blockhash)

        def capture():
            result = {}
            for identity in sorted(set(bank['accounts']) | {temp}):
                a = svm.get_account(P.from_string(identity))
                result[identity] = None if a is None or (a.lamports == 0 and not a.data) else dict(
                    owner=str(a.owner), lamports=a.lamports, executable=a.executable, data_base64=base64.b64encode(a.data).decode())
            return result
        before = capture()
        native = json.loads(svm.send_transaction(transaction).to_json())
        return dict(passed='err' not in native, before=before, after=capture(), native=native)

    n = execute()
    require(n['passed'], 'temp-original-execution:' + str(n['native']))
    require(n['before'][temp] is None and n['after'][temp] is None, 'temp-account-closed')
    require(n['after'][caller]['lamports'] - n['before'][caller]['lamports'] == (-6464 if refund else -4000),
            'temp-actual-native-settlement')
    amount = lambda a: int.from_bytes(base64.b64decode(a['data_base64'])[64:72], 'little')
    require((amount(n['after'][destination]) - amount(n['before'][destination]) if refund else
             amount(n['before'][source]) - amount(n['after'][source])) == (1000 if refund else 699), 'temp-actual-token-settlement')
    for identity in [bank['sourceTokenAccount'], bank['originalSourceTokenAccount']] if refund else [bank['destinationTokenAccount']]:
        require(n['before'][identity] == n['after'][identity], 'temp-existing-wsol-preserved')
    totals = {}
    for side, sign in [('before', -1), ('after', 1)]:
        for a in n[side].values():
            if a is None or a['owner'] != TOKEN:
                continue
            raw = base64.b64decode(a['data_base64'])
            if len(raw) == 165:
                totals[raw[:32]] = totals.get(raw[:32], 0) + sign * int.from_bytes(raw[64:72], 'little')
    require(totals.pop(bytes(P.from_string(WSOL))) == (1464 if refund else -1000) and
            all(v == 0 for v in totals.values()), 'temp-token-conservation')
    expected = bank['after']['meteora']
    for identity, raw in [(bank['pool']['pool_id'], expected['pair_data'])] + [(a['address'], a['data']) for a in expected['bin_arrays']]:
        require(base64.b64decode(n['after'][identity]['data_base64']).hex() == raw, 'temp-pool-order-post-state')
    require(any(a and base64.b64decode(a['data_base64']).hex() == expected['oracle_data'] for a in n['after'].values()),
            'temp-oracle-post-state')
    control = execute(True)
    require(not control['passed'] and control['after'][temp] is None, 'temp-atomic-rollback')
    for identity, a in control['before'].items():
        if identity != caller:
            require(a == control['after'][identity], 'temp-control-full-rollback')
    require(control['before'][caller]['lamports'] - control['after'][caller]['lamports'] == 5000, 'temp-control-fee-only')
    return dict(passed=True, kind='refund' if refund else 'unwrap', input='1464' if refund else '699', output='1000',
                payerLamportDelta=p['expectedPayerLamportDelta'], existingWsolUntouched=True,
                temporaryAccountClosed=True, original=n, control=control, simulation=simulation, publicTransactionsSubmitted=False)


if __name__ == '__main__':
    result = verify(sys.argv[1])
    Path(sys.argv[2]).write_text(json.dumps(result, separators=(',', ':')) + '\n')
    print(json.dumps({k: result[k] for k in ['passed', 'kind', 'input', 'output', 'payerLamportDelta', 'publicTransactionsSubmitted']}))
