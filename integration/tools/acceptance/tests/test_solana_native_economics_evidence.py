import copy
import sys
import unittest
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from solana_native_economics_evidence import NATIVE, signed_number, verify_costs


def unit_case():
    # Explicit arithmetic fixture only, not an execution artifact.
    request = dict(chainKey='solana:unit', sellToken='token', buyToken=NATIVE, caller='payer', recipient='payer')
    p = dict(preparationHash='unit-preparation', rentLamports='100', rentRefundLamports='0')
    c = dict(modelVersion='xone-solana-native-economics/1', sourceAuthority='RPC_TRANSACTION_RECEIPT', scope='EVALUATED_PREPARED_TRANSACTION_ONLY', amountBasis='OBSERVED_ACTUAL_SETTLEMENT', chainKey='solana:unit', preparationHash='unit-preparation', transactionSha256='unit-wire', nativeUnit='LAMPORTS', networkFeeNative='5000', retainedRentDepositNative='100', actualPayerNativeDelta='5871', depositTreatment='CAPITAL_DEPOSIT_SEPARATE_FROM_NETWORK_FEE', networkFeeIncludedInSwapAmounts=False, historicalBankVerified=False, independentProductionVerification=False, quoteRankingApplied=False, deductedFromQuoteOutput=False, netBuyAmountEstimate='5971', netSellAmountEstimate=None, netBuyUnavailableReason=None, netSellUnavailableReason='INPUT_NOT_NATIVE_SETTLEMENT_ASSET', unavailableReason=None, tokenValuationStatus='NATIVE_UNIT_IDENTITY', valuationToken=NATIVE, nativeCashReconciliation='MATCHED', valuationRatio=dict(numerator='1', denominator='1', method='NATIVE_UNIT_IDENTITY', priceObservedAtMs=None, priceTimePolicy='IDENTICAL_NATIVE_ATOMS_NO_MARKET_PRICE'))
    o = dict(actualInput='1000000', actualOutput='10971', feeLamports='5000', actualPersistentRentLamports='100', payerLamportDelta='5871', costs=c)
    return request, p, o


def verify(case):
    return verify_costs(*case, 'RPC_TRANSACTION_RECEIPT', 'unit-wire')


class NativeEconomicsEvidence(unittest.TestCase):
    def test_actual_excess_output_and_capital_are_independent(self):
        self.assertEqual(verify(unit_case())['net_output'], '5971')
        case = unit_case()
        case[2]['actualPersistentRentLamports'] = '200'
        case[2]['payerLamportDelta'] = '5771'
        case[2]['costs'].update(retainedRentDepositNative='200', actualPayerNativeDelta='5771')
        self.assertEqual(verify(case)['net_output'], '5971')

    def test_guessed_output_double_counted_rent_and_authority_are_rejected(self):
        for key, value in [('netBuyAmountEstimate', '0'), ('netBuyAmountEstimate', '5871'), ('networkFeeNative', '5100'), ('sourceAuthority', 'TRUSTED_CHAIN_BANK'), ('quoteRankingApplied', True), ('historicalBankVerified', True), ('transactionSha256', 'other-wire'), ('tokenValuationStatus', 'UNAVAILABLE')]:
            case = unit_case()
            case[2]['costs'][key] = value
            with self.assertRaises(ValueError, msg=key):
                verify(case)

    def test_negative_and_exact_zero_output_semantics(self):
        for output, delta, net in [('1000', '-4100', None), ('5000', '-100', '0')]:
            case = unit_case()
            case[2].update(actualOutput=output, payerLamportDelta=delta)
            case[2]['costs'].update(actualPayerNativeDelta=delta, netBuyAmountEstimate=net, netBuyUnavailableReason='NETWORK_FEE_EXCEEDS_GROSS_OUTPUT' if net is None else None, unavailableReason='NETWORK_FEE_EXCEEDS_GROSS_OUTPUT' if net is None else None)
            self.assertEqual(verify(case)['net_output'], net)
            changed = copy.deepcopy(case)
            changed[2]['costs']['netBuyAmountEstimate'] = '0' if net is None else None
            with self.assertRaises(ValueError):
                verify(changed)

    def test_native_cash_and_recipient_cannot_be_substituted(self):
        for field, value in [('actualInput', '01'), ('payerLamportDelta', '5872'), ('feeLamports', str(2**64))]:
            case = unit_case()
            case[2][field] = value
            with self.assertRaises(ValueError):
                verify(case)
        case = unit_case()
        case[0]['recipient'] = 'other'
        with self.assertRaises(ValueError):
            verify(case)
        for value in ['+1', '-0', '01', str(2**64), '-18446744073709551616']:
            with self.assertRaises(ValueError):
                signed_number(value)
