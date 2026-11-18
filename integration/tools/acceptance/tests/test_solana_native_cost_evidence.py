import base64
import hashlib
import sys
import unittest
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from solana_native_cost_evidence import amount, canonical, verify


def seal(p):
    p['preparationHash'] = hashlib.sha256(canonical({k: v for k, v in p.items() if k != 'preparationHash'})).hexdigest()


def unit_document():
    message = b'\x01\x00\x01unit-only-no-execution-claim'
    p = dict(ready=True, signed=False, submitted=False, messageBase64=base64.b64encode(message).decode(), unsignedTransaction=base64.b64encode(b'\x01' + b'\0' * 64 + message).decode(), planHash='unit-plan', quoteViewId='unit-view', observedSlot=10, feeLamports='5000', rentLamports='10', rentRefundLamports='5', wrapLamports='3', totalFundingLamports='5013', payerLamports='9000', recentBlockhash='unit-block', lastValidBlockHeight=11, quotedInput='1', quotedOutput='2', accountSnapshot={'snapshotHash': 'unit-source'})
    p['costs'] = dict(modelVersion='xone-solana-native-cost/1', status='MESSAGE_BOUND_RPC_ESTIMATE', scope='DECLARED_SINGLE_SIGNER_PREPARED_SOLANA_TRANSACTION', chainKey='solana:unit', nativeUnit='LAMPORTS', sourceAuthority='PREPARATION_RPC_OBSERVATION', historicalBankVerified=False, planHash='unit-plan', quoteViewId='unit-view', messageSha256=hashlib.sha256(message).hexdigest(), accountSnapshotHash='unit-source', accountObservedSlot=10, feeMinimumContextSlot=10, feeObservedSlot=11, feeContextPolicy='MIN_CONTEXT_SLOT_NOT_SAME_BANK', validity=dict(recentBlockhash='unit-block', lastValidBlockHeight=11), networkFeeNative='5000', nativeKnownSubtotal='5000', nativeKnownSubtotalKind='NETWORK_FEE_ONLY', nativeFundingRequired='5013', rentDepositNative='10', expectedRefundableRentNative='5', expectedRetainedRentDepositNative='5', swapOrWrapPrincipalNative='3', wrapPrincipalNative='3', totalNativeEstimate=None, netBuyAmountEstimate=None, netSellAmountEstimate=None, tokenValuationStatus='UNAVAILABLE')
    seal(p)
    return dict(quote={'chainKey': 'solana:unit', 'planHash': 'unit-plan', 'quoteViewId': 'unit-view'}, preparation=p, evidence={'success': True, 'sourceBankUnchanged': True, 'preparationHash': p['preparationHash'], 'feeLamports': '5000', 'actualInput': '1', 'actualOutput': '2'})


class SolanaNativeCostEvidence(unittest.TestCase):
    def test_explicit_unit_fixture_and_canonical_amount_width(self):
        self.assertEqual(verify(unit_document())['network_fee'], 5000)
        for value in [1, '-1', '+1', '01', ' 1', str(2**64), '\u0661']:
            with self.assertRaises(ValueError):
                amount(value)

    def test_resealed_scope_slots_funding_and_double_counting_are_rejected(self):
        for field, value in [('networkFeeNative', '5013'), ('nativeKnownSubtotal', '5013'), ('nativeFundingRequired', '5000'), ('feeMinimumContextSlot', 9), ('feeObservedSlot', 9), ('expectedRetainedRentDepositNative', '10'), ('tokenValuationStatus', 'AVAILABLE'), ('netBuyAmountEstimate', '1'), ('chainKey', 'solana:other'), ('historicalBankVerified', True)]:
            d = unit_document()
            d['preparation']['costs'][field] = value
            seal(d['preparation'])
            d['evidence']['preparationHash'] = d['preparation']['preparationHash']
            with self.assertRaises(ValueError, msg=field):
                verify(d)

    def test_changed_message_and_native_settlement_are_rejected(self):
        for field, value in [('actualOutput', '3'), ('feeLamports', '5001'), ('sourceBankUnchanged', False)]:
            d = unit_document()
            d['evidence'][field] = value
            with self.assertRaises(ValueError):
                verify(d)
        d = unit_document()
        d['preparation']['messageBase64'] = base64.b64encode(b'other-message').decode()
        seal(d['preparation'])
        with self.assertRaises(ValueError):
            verify(d)
