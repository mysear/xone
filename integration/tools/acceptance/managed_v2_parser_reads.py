"""Declarative original-ABI read closure required for a complete Managed source.

Native reference banks also retain trader/LP balances, allowances and other
execution observations. Those are compared by the independent VM, and are not
mandatory queries for the ordinary Pool-state reader.
"""


def required_reads(bank):
    getters, codes = set(), set()
    for row in bank['pools']:
        pool = row['pool']
        multi = pool['multi_asset']
        managed = multi['managed_v2']
        pid, vault = pool['pool_id'], multi['vault']
        tokens = {a['token'] for a in multi['assets']}
        dependencies = {d['role']: d['address'] for d in managed['dependencies']}
        factory, provider, collector = managed['factory'], managed['fee_provider'], managed['protocol_collector']
        recovery = dependencies['recovery-mode-helper']
        codes.update({pid, vault, factory, provider, collector, *dependencies.values(), *tokens})
        pool_methods = {'getVault', 'getPoolId', 'getProtocolFeesCollector', 'getOwner', 'getPausedState',
            'inRecoveryMode', 'getSwapEnabled', 'getMustAllowlistLPs', 'getManagementAumFeeParams',
            'getProviderFeeId', 'getProtocolFeePercentageCache', 'getNormalizedWeights', 'getScalingFactors',
            'getActualSupply', 'getSwapFeePercentage', 'getGradualSwapFeeUpdateParams',
            'getGradualWeightUpdateParams', 'name', 'totalSupply'}
        factory_methods = {'getVault', 'isPoolFromFactory', 'getProtocolFeePercentagesProvider',
            'getWeightedMath', 'getRecoveryModeHelper', 'getCreationCodeContracts', 'getPauseConfiguration'}
        vault_methods = {'getPool', 'getPoolTokens', 'getPoolTokenInfo', 'getPausedState',
                         'getProtocolFeesCollector', 'WETH'}
        found_creation = False
        for g in row['getters']:
            address, method, args = g['to'], g['method'], g['args']
            needed = False
            if address == pid:
                needed = method in pool_methods
                if method == 'balanceOf':
                    needed = args in ([vault], [managed['manager']], [collector])
                elif method == 'getCircuitBreakerState':
                    needed = len(args) == 1 and args[0] in tokens
            elif address == factory:
                needed = method in factory_methods
                if method == 'getCreationCodeContracts':
                    result = bytes.fromhex(g['result'][2:])
                    if len(result) != 64 or any(result[:12]) or any(result[32:44]):
                        raise ValueError('original factory creation-code ABI is not two canonical addresses')
                    codes.update(('0x' + result[12:32].hex(), '0x' + result[44:64].hex()))
                    found_creation = True
            elif address == vault:
                needed = method in vault_methods
            elif address == provider:
                needed = method == 'getVault' or (method in {'getFeeTypePercentage', 'getFeeTypeMaximumPercentage'}
                    and len(args) == 1 and int(args[0]) == int(managed['provider_fee_id']))
            elif address == collector:
                needed = method == 'vault'
            elif address == recovery:
                needed = method == 'getVault'
            elif address in tokens:
                needed = method == 'decimals' or method == 'balanceOf' and args == [vault]
            if needed:
                getters.add((address.lower(), g['data'].lower()))
        if not found_creation:
            raise ValueError('original source lacks factory creation-code observations')
    return getters, {address.lower() for address in codes}
