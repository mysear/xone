package config

import (
	"encoding/json"
	"errors"
	"io"
	"strings"
)

const OfficialUniswapV3Pool076 = "uniswap-v3-core-1.0.1-solc-0.7.6-800-istanbul"
const OfficialV3RuntimeTemplateSHA256 = "3ef71385c2a22b945bead7904f24b25db877de9db51d1401f2f9daea07ea5c0c"
const OfficialV3CompilerSHA256 = "b94e69dfb056b3e26080f805ab43b668afbc0ac70bf124bfb7391ecfc0172ad2"

// V3PoolTemplateConfig selects a built-in reviewed source/compiler profile. It
// accepts no user supplied bytecode, mask, immutable offset, path or source pin.
// FactoryDeployment field: PoolTemplate *V3PoolTemplateConfig `json:"pool_template,omitempty"`.
type V3PoolTemplateConfig struct {
	Profile string `json:"profile"`
}

func (c *V3PoolTemplateConfig) Validate() error {
	if c == nil {
		return nil
	}
	if c.Profile != OfficialUniswapV3Pool076 {
		return errors.New("V3_TEMPLATE_PROFILE_UNREVIEWED")
	}
	return nil
}
func (c *V3PoolTemplateConfig) UnmarshalJSON(raw []byte) error {
	decoder := json.NewDecoder(strings.NewReader(string(raw)))
	token, err := decoder.Token()
	if err != nil || token != json.Delim('{') {
		return errors.New("V3_TEMPLATE_CONFIG_OBJECT")
	}
	seen := false
	for decoder.More() {
		token, err = decoder.Token()
		if err != nil || token != "profile" || seen {
			return errors.New("V3_TEMPLATE_CONFIG_PROPERTY")
		}
		seen = true
		if err = decoder.Decode(&c.Profile); err != nil {
			return err
		}
	}
	if token, err = decoder.Token(); err != nil || token != json.Delim('}') {
		return errors.New("V3_TEMPLATE_CONFIG_OBJECT")
	}
	if decoder.Decode(new(any)) != io.EOF {
		return errors.New("V3_TEMPLATE_CONFIG_TRAILING")
	}
	return c.Validate()
}

type V3TemplateDeployment struct {
	Chain, Genesis, Factory, FactoryCodeSHA256, SourceSHA256, TemplateSHA256 string
	DeploymentHeight                                                         uint64
}

// Deployment binds the pool profile to exact public chain identity and a
// source-rebuilt official factory. ProvenanceID is never approval authority.
func (c V3PoolTemplateConfig) Deployment(chain, genesis, factory, code string) (V3TemplateDeployment, error) {
	if err := c.Validate(); err != nil {
		return V3TemplateDeployment{}, err
	}
	d, ok := officialV3Deployments[chain]
	if !ok || genesis != d.Genesis || factory != d.Factory || code != d.FactoryCodeSHA256 {
		return V3TemplateDeployment{}, errors.New("V3_TEMPLATE_DEPLOYMENT_UNREVIEWED")
	}
	d.Chain = chain
	d.TemplateSHA256 = OfficialV3RuntimeTemplateSHA256
	return d, nil
}

var officialV3Deployments = map[string]V3TemplateDeployment{
	"evm:1":     {Genesis: "0xd4e56740f876aef8c010b86a40d5f56745a118d0906a34e69aec8c0db1cb8fa3", Factory: "0x1f98431c8ad98523631ae4a59f267346ea31f984", FactoryCodeSHA256: "fd5e0ec0633afeb92b2f6b6cac36e13a9732bd1914f8158a78f127d51d54a572", SourceSHA256: "7bddf43e2a649b5109030a16000d44e087a69437afbcc4c547a869582bddeac9", DeploymentHeight: 12369621},
	"evm:8453":  {Genesis: "0xf712aa9241cc24369b143cf6dce85f0902a9731e70d66818a3a5845b296c73dd", Factory: "0x33128a8fc17869897dce68ed026d694621f6fdfd", FactoryCodeSHA256: "8545609892cc8d7d608dd4420ee110ab98448730570824fb029228e33846d28c", SourceSHA256: "a4ecd26c233e0f725945b48045133529725331acdcd8c75104b78d918f4c6dd2", DeploymentHeight: 1371680},
	"evm:42161": {Genesis: "0x7ee576b35482195fc49205cec9af72ce14f003b9ae69f6ba0faef4514be8b442", Factory: "0x1f98431c8ad98523631ae4a59f267346ea31f984", FactoryCodeSHA256: "fd5e0ec0633afeb92b2f6b6cac36e13a9732bd1914f8158a78f127d51d54a572", SourceSHA256: "dc30848475b88b3d21a43c61413a151e411eaf9315855b188b9137c59debc0d3", DeploymentHeight: 165},
	"evm:10":    {Genesis: "0x7ca38a1916c42007829c55e69d3e9a73265554b586a499015373241b8a3fa48b", Factory: "0x1f98431c8ad98523631ae4a59f267346ea31f984", FactoryCodeSHA256: "fd5e0ec0633afeb92b2f6b6cac36e13a9732bd1914f8158a78f127d51d54a572", SourceSHA256: "60a35bfac38d4a4049b70d1345fb1d620be2a0de98579216dcd56f36ad72c38b", DeploymentHeight: 0},
	"evm:137":   {Genesis: "0xa9c28ce2141b56c474f1dc504bee9b01eb1bd7d1a507580d5519d4437a97de1b", Factory: "0x1f98431c8ad98523631ae4a59f267346ea31f984", FactoryCodeSHA256: "fd5e0ec0633afeb92b2f6b6cac36e13a9732bd1914f8158a78f127d51d54a572", SourceSHA256: "9d1856735810590b15d7d8dab28970f353fd5689ddbf95934447ec2d115037cd", DeploymentHeight: 22757547},
	"evm:43114": {Genesis: "0x31ced5b9beb7f8782b014660da0cb18cc409f121f408186886e1ca3e8eeca96b", Factory: "0x740b1c1de25031c31ff4fc9a62f554a55cdc1bad", FactoryCodeSHA256: "8e2e7189ed878d23159e2f650684b91220a4a69e433c59cf298653302b813b1b", SourceSHA256: "f7e537e524db044b80aa521ce419d265666f700f898140bcb9816297ae47e22c", DeploymentHeight: 27832972},
}

// ValidateFactoryPoolCode is the per-source hook for DynamicCatalogConfig.
// Legacy mode still requires its exact pool hash. Template mode may omit it;
// a supplied hash remains an additional strict constraint on that one instance.
func ValidateFactoryPoolCode(chain, version, factory, factoryCode, poolCode string, template *V3PoolTemplateConfig) error {
	if template == nil {
		if !catalogHex(poolCode, 32, false) {
			return errors.New("DYNAMIC_CATALOG_POOL_CODE_REQUIRED")
		}
		return nil
	}
	if version != "uniswap-v3" {
		return errors.New("V3_TEMPLATE_PROTOCOL_REQUIRED")
	}
	d, ok := officialV3Deployments[chain]
	if !ok {
		return errors.New("V3_TEMPLATE_DEPLOYMENT_UNREVIEWED")
	}
	if _, err := template.Deployment(chain, d.Genesis, factory, factoryCode); err != nil {
		return err
	}
	if poolCode != "" && !catalogHex(poolCode, 32, false) {
		return errors.New("DYNAMIC_CATALOG_POOL_CODE_REQUIRED")
	}
	return nil
}
