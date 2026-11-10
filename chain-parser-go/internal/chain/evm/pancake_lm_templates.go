package evm

import (
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"strings"
)

type pancakeBinding struct {
	Offset int
	Role   string
}
type pancakeTemplate struct {
	RuntimeBytes int
	BodyBytes    int
	Digest       string
	Bindings     []pancakeBinding
}

var pancakeTemplates = map[string]pancakeTemplate{
	"pool":       {22964, 22911, "7b39db168808f3bb63aac29a396464c672dd6509dade03f7078499fc3803684c", []pancakeBinding{{2324, "token0"}, {3249, "tickSpacing"}, {3488, "fee"}, {4884, "token1"}, {5186, "token0"}, {6983, "fee"}, {7038, "fee"}, {7120, "token0"}, {7169, "token1"}, {8194, "token0"}, {8296, "token1"}, {8563, "maxLiquidityPerTick"}, {8694, "factory"}, {8735, "factory"}, {9074, "token0"}, {9208, "token1"}, {10453, "factory"}, {10494, "factory"}, {10915, "factory"}, {10960, "factory"}, {11001, "factory"}, {11241, "tickSpacing"}, {11277, "token1"}, {11313, "fee"}, {11776, "fee"}, {11842, "fee"}, {11913, "fee"}, {11984, "fee"}, {16547, "token0"}, {16876, "token1"}, {20147, "maxLiquidityPerTick"}, {20202, "maxLiquidityPerTick"}, {20254, "tickSpacing"}, {20304, "tickSpacing"}}},
	"lm":         {3331, 3278, "3d6848ee2e6cf3d9c6857bab9a877f5bdd59a830d16a6638485745ebbcd9b197", []pancakeBinding{{569, "pool"}, {614, "pool"}, {664, "masterChef"}, {817, "masterChef"}, {864, "pool"}, {1268, "masterChef"}, {1313, "pool"}, {1492, "masterChef"}, {1587, "pool"}, {1755, "pool"}, {2076, "pool"}}},
	"masterChef": {21898, 21845, "c79545617ca1a25eaf407e57b1231031b574a63332e23c409af64157f896ba24", []pancakeBinding{{867, "nonfungiblePositionManager"}, {919, "WETH"}, {2065, "CAKE"}, {2519, "WETH"}, {2626, "nonfungiblePositionManager"}, {4022, "nonfungiblePositionManager"}, {4442, "nonfungiblePositionManager"}, {4705, "CAKE"}, {5361, "nonfungiblePositionManager"}, {5707, "nonfungiblePositionManager"}, {5845, "nonfungiblePositionManager"}, {7623, "nonfungiblePositionManager"}, {7757, "nonfungiblePositionManager"}, {7825, "nonfungiblePositionManager"}, {7959, "nonfungiblePositionManager"}, {8779, "WETH"}, {8844, "WETH"}, {8995, "nonfungiblePositionManager"}, {9903, "nonfungiblePositionManager"}, {10591, "WETH"}, {10769, "WETH"}, {11708, "CAKE"}, {13631, "CAKE"}, {14143, "nonfungiblePositionManager"}, {15208, "nonfungiblePositionManager"}, {16947, "CAKE"}, {17036, "WETH"}, {17114, "WETH"}, {17523, "WETH"}, {17646, "WETH"}, {17716, "nonfungiblePositionManager"}, {18113, "CAKE"}, {18285, "CAKE"}}},
	"factory":    {3900, 3847, "ca9caada1721c9dffa86fa20f7cf8ccc3c199e9019646f42c2e85617f896bed5", []pancakeBinding{{1435, "poolDeployer"}, {3214, "poolDeployer"}}},
}

func validatePancakeRuntime(role string, raw []byte, bindings map[string]string) (string, error) {
	t, ok := pancakeTemplates[role]
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
