//go:generate go run ./cmd/gen.go BUILD.bazel
package staticcheck

import (
	"golang.org/x/tools/go/analysis"
	"honnef.co/go/tools/analysis/lint"
	"honnef.co/go/tools/staticcheck"

	"github.com/sourcegraph/sourcegraph/dev/linters/nolint"
)

var StaticCheckAnalyzers []*lint.Analyzer = staticcheck.Analyzers
var AnalyzerName = ""
var Analyzer *analysis.Analyzer = GetAnalyzerByName(AnalyzerName)

func GetAnalyzerByName(name string) *analysis.Analyzer {
	for _, a := range StaticCheckAnalyzers {
		if a.Analyzer.Name == name {
			return nolint.Wrap(a.Analyzer)
		}
	}
	return nil
}
