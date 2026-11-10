package evm

import (
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"strings"
)

var pancakeBscTemplates = map[string]pancakeTemplate{
	"pool":       {22962, 22950, "128755d0e9816da897b0634fec16d89fb47a0c7c41bc2660a68b0c7352f84d3e", []pancakeBinding{{8694, "factory"}, {8735, "factory"}, {10453, "factory"}, {10494, "factory"}, {10915, "factory"}, {10960, "factory"}, {11001, "factory"}, {2324, "token0"}, {5186, "token0"}, {7120, "token0"}, {8194, "token0"}, {9074, "token0"}, {16564, "token0"}, {4884, "token1"}, {7169, "token1"}, {8296, "token1"}, {9208, "token1"}, {11277, "token1"}, {16893, "token1"}, {3488, "fee"}, {6983, "fee"}, {7038, "fee"}, {11313, "fee"}, {11776, "fee"}, {11842, "fee"}, {11913, "fee"}, {11984, "fee"}, {3249, "tickSpacing"}, {11241, "tickSpacing"}, {20280, "tickSpacing"}, {20330, "tickSpacing"}, {8563, "maxLiquidityPerTick"}, {20173, "maxLiquidityPerTick"}, {20228, "maxLiquidityPerTick"}}},
	"lm":         {8168, 8115, "df34ef41e16729a12160b2b0e3410729141a521ede0301f52e141b8595014389", []pancakeBinding{{944, "pool"}, {1002, "pool"}, {1341, "pool"}, {3004, "pool"}, {3223, "pool"}, {3654, "pool"}, {4396, "pool"}, {1065, "masterChef"}, {1281, "masterChef"}, {1914, "masterChef"}, {2842, "masterChef"}, {3995, "firstLMPool"}, {7802, "firstLMPool"}, {4031, "secondLMPool"}, {5405, "secondLMPool"}, {7521, "secondLMPool"}, {7686, "secondLMPool"}, {1878, "thirdLMPool"}, {2205, "thirdLMPool"}, {2386, "thirdLMPool"}, {2607, "thirdLMPool"}, {5048, "thirdLMPool"}, {5227, "thirdLMPool"}, {7136, "thirdLMPool"}, {7301, "thirdLMPool"}}},
	"masterChef": {22845, 22792, "64d48f6819e38b7561e0e92a414795f28cbb75e09475d982246659c939365309", []pancakeBinding{{2105, "CAKE"}, {4864, "CAKE"}, {12330, "CAKE"}, {14347, "CAKE"}, {17774, "CAKE"}, {19022, "CAKE"}, {19194, "CAKE"}, {935, "WETH"}, {2559, "WETH"}, {9236, "WETH"}, {9301, "WETH"}, {11172, "WETH"}, {11350, "WETH"}, {17863, "WETH"}, {17941, "WETH"}, {18382, "WETH"}, {18530, "WETH"}, {883, "nonfungiblePositionManager"}, {2666, "nonfungiblePositionManager"}, {4112, "nonfungiblePositionManager"}, {4584, "nonfungiblePositionManager"}, {5572, "nonfungiblePositionManager"}, {5909, "nonfungiblePositionManager"}, {6097, "nonfungiblePositionManager"}, {8080, "nonfungiblePositionManager"}, {8214, "nonfungiblePositionManager"}, {8282, "nonfungiblePositionManager"}, {8416, "nonfungiblePositionManager"}, {9477, "nonfungiblePositionManager"}, {10459, "nonfungiblePositionManager"}, {14934, "nonfungiblePositionManager"}, {16062, "nonfungiblePositionManager"}, {18600, "nonfungiblePositionManager"}}},
	"first":      {5129, 5076, "baac406328c78d73ac2077353a7e657864be21a0a26bad057fe84f18d50cd5b7", []pancakeBinding{{798, "pool"}, {834, "pool"}, {1274, "pool"}, {1876, "pool"}, {2337, "pool"}, {2568, "pool"}, {3024, "pool"}, {919, "masterChef"}, {1214, "masterChef"}, {1831, "masterChef"}, {2142, "masterChef"}}},
	"second":     {8157, 8104, "1f251fdf65b944bcde07aec7c5bcfb8cbd2a5b55c4808d6ab99184a8ae3dbad2", []pancakeBinding{{1348, "pool"}, {1384, "pool"}, {1832, "pool"}, {2865, "pool"}, {3096, "pool"}, {3563, "pool"}, {5034, "pool"}, {1469, "masterChef"}, {1772, "masterChef"}, {2390, "masterChef"}, {2644, "masterChef"}, {2432, "OldLMPool"}, {4210, "OldLMPool"}, {4377, "OldLMPool"}, {4594, "OldLMPool"}, {5808, "OldLMPool"}, {7687, "OldLMPool"}}},
	"third":      {8492, 8439, "baf90c4558c993d9fefd1d102d42a92ee1bc8d0209badca100835cbe38ce2749", []pancakeBinding{{1079, "pool"}, {1137, "pool"}, {1476, "pool"}, {3553, "pool"}, {3772, "pool"}, {4203, "pool"}, {5009, "pool"}, {1200, "masterChef"}, {1416, "masterChef"}, {2013, "masterChef"}, {3391, "masterChef"}, {4576, "firstLMPool"}, {8126, "firstLMPool"}, {2208, "secondLMPool"}, {2389, "secondLMPool"}, {2754, "secondLMPool"}, {2935, "secondLMPool"}, {3156, "secondLMPool"}, {4612, "secondLMPool"}, {7220, "secondLMPool"}, {7342, "secondLMPool"}, {7741, "secondLMPool"}, {7906, "secondLMPool"}}},
	"factory":    {5151, 5139, "829e62aa0f1d3471118cabd7931dc0ba702f63ff04496c7d1a1832453970fd52", []pancakeBinding{{1779, "poolDeployer"}, {4315, "poolDeployer"}}},
}

func validatePancakeBscRuntime(role string, raw []byte, bindings map[string]string) (string, error) {
	t, ok := pancakeBscTemplates[role]
	if !ok || len(raw) != t.RuntimeBytes || len(raw) < 2 {
		return "", errors.New("PANCAKE_LM_RUNTIME_SIZE")
	}
	metadata := (int(raw[len(raw)-2]) << 8) + int(raw[len(raw)-1]) + 2
	if len(raw)-metadata != t.BodyBytes {
		return "", errors.New("PANCAKE_LM_RUNTIME_METADATA")
	}
	normalized := append([]byte(nil), raw[:t.BodyBytes]...)
	for _, b := range t.Bindings {
		expected, ok := bindings[b.Role]
		if !ok || b.Offset < 0 || b.Offset+32 > t.BodyBytes {
			return "", errors.New("PANCAKE_LM_RUNTIME_BINDING")
		}
		expected = strings.TrimPrefix(expected, "0x")
		if len(expected) > 64 {
			return "", errors.New("PANCAKE_LM_RUNTIME_BINDING_WIDTH")
		}
		expected = strings.Repeat("0", 64-len(expected)) + expected
		if hex.EncodeToString(raw[b.Offset:b.Offset+32]) != expected {
			return "", errors.New("PANCAKE_LM_RUNTIME_IMMUTABLE")
		}
		for i := b.Offset; i < b.Offset+32; i++ {
			normalized[i] = 0
		}
	}
	digest := sha256.Sum256(normalized)
	if hex.EncodeToString(digest[:]) != t.Digest {
		return "", errors.New("PANCAKE_LM_RUNTIME_TEMPLATE")
	}
	return t.Digest, nil
}
