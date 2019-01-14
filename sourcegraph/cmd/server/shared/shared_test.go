package shared

import (
	"go/parser"
	"go/token"
	"strings"
	"testing"
)

func TestEnsurePostgresVersion(t *testing.T) {
	fset := token.NewFileSet()
	f, err := parser.ParseFile(fset, "../dockerfile.go", nil, parser.ParseComments)
	if err != nil {
		t.Fatal(err)
	}
	install := []string{}
	for _, cg := range f.Comments {
		for _, c := range cg.List {
			if strings.HasPrefix(c.Text, "//docker:") {
				parts := strings.SplitN(c.Text[9:], " ", 2)
				switch parts[0] {
				case "install":
					install = append(install, strings.Fields(parts[1])...)
				}
			}
		}
	}

	for _, pkg := range install {
		if pkg == "'postgresql=11.1-r0'" {
			return
		}
	}
	t.Fatal("Could not find postgres 11.1 specified in docker:install. We have to stay on postgres 11.1 since changing versions would cause existing deployments to break. Got:", install)
}
