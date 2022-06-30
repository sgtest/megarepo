package jobutil

import (
	"testing"

	"github.com/sourcegraph/log/logtest"
)

func TestMembership(t *testing.T) {
	defer func() {
		if r := recover(); r != nil {
			t.Fatalf("Please add a case statement for your job (%v)", r)
		}
	}()

	mapper := Mapper{
		Log: logtest.Scoped(t),
	}
	for _, j := range allJobs {
		Sexp(j)
		PrettyMermaid(j)
		PrettyJSON(j)
		mapper.Map(j)
	}
}
