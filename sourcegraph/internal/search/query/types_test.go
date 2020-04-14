package query

import (
	"regexp"
	"testing"

	"github.com/sourcegraph/sourcegraph/internal/search/query/types"
)

func Test_valueToTypedValue(t *testing.T) {
	value := ".*"
	t.Run("is quoted is string", func(t *testing.T) {
		inputQuoted := true
		got := valueToTypedValue("", value, inputQuoted)
		want := types.Value{String: &value}
		if *got[0].String != *want.String {
			t.Errorf("got %v, want %v", *got[0].String, *want.String)
		}
	})
	t.Run("is not quoted is regex", func(t *testing.T) {
		inputQuoted := false
		got := valueToTypedValue("", value, inputQuoted)
		regexValue, _ := regexp.Compile(value)
		want := types.Value{Regexp: regexValue}
		if got[0].Regexp.String() != want.Regexp.String() {
			t.Errorf("got %v, want %v", got[0].Regexp, want.Regexp)
		}
	})

	value = ".*("
	t.Run("uncompilable regex is string", func(t *testing.T) {
		inputQuoted := false
		got := valueToTypedValue("", value, inputQuoted)
		want := types.Value{String: &value}
		if *got[0].String != *want.String {
			t.Errorf("got %v, want %v", *got[0].String, *want.String)
		}
	})
}
