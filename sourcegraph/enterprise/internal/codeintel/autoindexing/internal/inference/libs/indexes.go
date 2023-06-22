package libs

import (
	"fmt"

	lua "github.com/yuin/gopher-lua"
	luar "layeh.com/gopher-luar"

	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/luasandbox/util"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

var Indexes = indexesAPI{}

type indexesAPI struct{}

var defaultIndexers = map[string]string{
	"clang":      "sourcegraph/lsif-clang",
	"go":         "sourcegraph/scip-go",
	"java":       "sourcegraph/scip-java",
	"python":     "sourcegraph/scip-python",
	"rust":       "sourcegraph/scip-rust",
	"typescript": "sourcegraph/scip-typescript",
	"ruby":       "sourcegraph/scip-ruby",
}

// To update, run `DOCKER_USER=... DOCKER_PASS=... ./update-shas.sh`
var defaultIndexerSHAs = map[string]string{
	"sourcegraph/scip-go":         "sha256:2a04ea639f4f72a23decfd9badf3663275a753fdf6d77c49ed47d8246e49402f",
	"sourcegraph/lsif-clang":      "sha256:ea814e5ab5c6e1e6ab4d001e4f4afddcc7b44128edbeeedf1d97da553813a4c8",
	"sourcegraph/lsif-rust":       "sha256:83cb769788987eb52f21a18b62d51ebb67c9436e1b0d2e99904c70fef424f9d1",
	"sourcegraph/scip-rust":       "sha256:e9c400fd1d3146cd9a3d98f89c6d9a70e0a116618057e0dac452219b1c60b658",
	"sourcegraph/scip-java":       "sha256:b0d0a6387d4be4e11bc6305021edf5ba2e58157fecff0fa6a74a7308a9b5a668",
	"sourcegraph/scip-python":     "sha256:ab7f4b6c42870761248fa0d2c4774e2e1a6f5b1b65dd06f75f064a9418625b83",
	"sourcegraph/scip-typescript": "sha256:4c9b65a449916bf2d8716c8b4b0a45666cd303a05b78e02980d25b23c1e55e92",
	"sourcegraph/scip-ruby":       "sha256:ffbe40f23bd49a0163417589374f0bda97619e15f18bb09a048135094d21176d",
}

func DefaultIndexerForLang(language string) (string, bool) {
	indexer, ok := defaultIndexers[language]
	if !ok {
		return "", false
	}

	sha, ok := defaultIndexerSHAs[indexer]
	if !ok {
		panic(fmt.Sprintf("no SHA set for indexer %q", indexer))
	}

	return fmt.Sprintf("%s@%s", indexer, sha), true
}

func (api indexesAPI) LuaAPI() map[string]lua.LGFunction {
	return map[string]lua.LGFunction{
		"get": util.WrapLuaFunction(func(state *lua.LState) error {
			language := state.CheckString(1)

			if indexer, ok := conf.SiteConfig().CodeIntelAutoIndexingIndexerMap[language]; ok {
				state.Push(luar.New(state, indexer))
				return nil
			}

			if indexer, ok := DefaultIndexerForLang(language); ok {
				state.Push(luar.New(state, indexer))
				return nil
			}

			return errors.Newf("no indexer is registered for %q", language)
		}),
	}
}
