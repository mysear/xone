package evm

const (
	selectorIsPaused      = "0xb187bd26" // isPaused()
	selectorMetadata      = "0x392f37e9" // metadata()
	selectorGetFee        = "0xcc56b2c5" // getFee(address,bool)
	selectorCoins         = "0xc6610657" // coins(uint256)
	selectorBalances      = "0x4903b0d1" // balances(uint256)
	selectorFee           = "0xddca3f43" // fee()
	selectorAPrecise      = "0x76a2f0f0" // A_precise()
	selectorAdminFee      = "0xfee3f7f9" // admin_fee()
	selectorGetVault      = "0x8d928af8" // getVault()
	selectorGetPoolID     = "0x38fff2d0" // getPoolId()
	selectorGetPool       = "0xf6c00927" // getPool(bytes32)
	selectorPausedState   = "0x1c0de051" // getPausedState()
	selectorAmplification = "0x6daccffa" // getAmplificationParameter()
	selectorScaling       = "0x1dd746ea" // getScalingFactors()
	selectorPoolTokens    = "0xf94d4668" // getPoolTokens(bytes32)
	selectorSwapFee       = "0x55c67628" // getSwapFeePercentage()
)

const selectorSwapFeeModule = "0x23c43a51"

const selectorUnstakedFeeModule = "0x7693bc11"
