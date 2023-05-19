package query

import (
	"reflect"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestRepoContainsFilePredicate(t *testing.T) {
	t.Run("Unmarshal", func(t *testing.T) {
		type test struct {
			name     string
			params   string
			expected *RepoContainsFilePredicate
		}

		valid := []test{
			{`path`, `path:test`, &RepoContainsFilePredicate{Path: "test"}},
			{`path regex`, `path:test(a|b)*.go`, &RepoContainsFilePredicate{Path: "test(a|b)*.go"}},
			{`content`, `content:test`, &RepoContainsFilePredicate{Content: "test"}},
			{`path and content`, `path:test.go content:abc`, &RepoContainsFilePredicate{Path: "test.go", Content: "abc"}},
			{`content and path`, `content:abc path:test.go`, &RepoContainsFilePredicate{Path: "test.go", Content: "abc"}},
			{`unnamed path`, `test.go`, &RepoContainsFilePredicate{Path: "test.go"}},
			{`unnamed path regex`, `test(a|b)*.go`, &RepoContainsFilePredicate{Path: "test(a|b)*.go"}},
		}

		for _, tc := range valid {
			t.Run(tc.name, func(t *testing.T) {
				p := &RepoContainsFilePredicate{}
				err := p.Unmarshal(tc.params, false)
				if err != nil {
					t.Fatalf("unexpected error: %s", err)
				}

				if !reflect.DeepEqual(tc.expected, p) {
					t.Fatalf("expected %#v, got %#v", tc.expected, p)
				}
			})
		}

		invalid := []test{
			{`empty`, ``, nil},
			{`negated path`, `-path:test`, nil},
			{`negated content`, `-content:test`, nil},
			{`catch invalid content regexp`, `path:foo content:([)`, nil},
			{`unsupported syntax`, `content1 content2`, nil},
			{`invalid unnamed path`, `([)`, nil},
		}

		for _, tc := range invalid {
			t.Run(tc.name, func(t *testing.T) {
				p := &RepoContainsFilePredicate{}
				err := p.Unmarshal(tc.params, false)
				if err == nil {
					t.Fatal("expected error but got none")
				}
			})
		}
	})
}

func TestParseAsPredicate(t *testing.T) {
	tests := []struct {
		input  string
		name   string
		params string
	}{
		{`a()`, "a", ""},
		{`a(b)`, "a", "b"},
	}

	for _, tc := range tests {
		t.Run(tc.input, func(t *testing.T) {
			name, params := ParseAsPredicate(tc.input)
			if name != tc.name {
				t.Fatalf("expected name %s, got %s", tc.name, name)
			}

			if params != tc.params {
				t.Fatalf("expected params %s, got %s", tc.params, params)
			}
		})
	}

}

func TestRepoHasDescriptionPredicate(t *testing.T) {
	t.Run("Unmarshal", func(t *testing.T) {
		type test struct {
			name     string
			params   string
			expected *RepoHasDescriptionPredicate
		}

		valid := []test{
			{`literal`, `test`, &RepoHasDescriptionPredicate{Pattern: "test"}},
			{`regexp`, `test(.*)package`, &RepoHasDescriptionPredicate{Pattern: "test(.*)package"}},
		}

		for _, tc := range valid {
			t.Run(tc.name, func(t *testing.T) {
				p := &RepoHasDescriptionPredicate{}
				err := p.Unmarshal(tc.params, false)
				if err != nil {
					t.Fatalf("unexpected error: %s", err)
				}

				if !reflect.DeepEqual(tc.expected, p) {
					t.Fatalf("expected %#v, got %#v", tc.expected, p)
				}
			})
		}

		invalid := []test{
			{`empty`, ``, nil},
			{`catch invalid regexp`, `([)`, nil},
		}

		for _, tc := range invalid {
			t.Run(tc.name, func(t *testing.T) {
				p := &RepoHasDescriptionPredicate{}
				err := p.Unmarshal(tc.params, false)
				if err == nil {
					t.Fatal("expected error but got none")
				}
			})
		}
	})
}

func TestRepoHasTopicPredicate(t *testing.T) {
	t.Run("errors on empty", func(t *testing.T) {
		var p RepoHasTopicPredicate
		err := p.Unmarshal("", false)
		require.Error(t, err)
	})

	t.Run("sets negated and topic", func(t *testing.T) {
		var p RepoHasTopicPredicate
		err := p.Unmarshal("topic1", true)
		require.NoError(t, err)
		require.Equal(t, "topic1", p.Topic)
		require.True(t, p.Negated)
	})
}

func TestRepoHasKVPMetaPredicate(t *testing.T) {
	strPtr := func(s string) *string { return &s }

	t.Run("Unmarshal", func(t *testing.T) {
		type test struct {
			name     string
			params   string
			expected *RepoHasMetaPredicate
		}

		valid := []test{
			{`key:value`, `key:value`, &RepoHasMetaPredicate{Key: "key", Value: strPtr("value"), Negated: false, KeyOnly: false}},
			{`double quoted special characters`, `"key:colon":"value:colon"`, &RepoHasMetaPredicate{Key: "key:colon", Value: strPtr("value:colon"), Negated: false, KeyOnly: false}},
			{`single quoted special characters`, `'  key:':'value : '`, &RepoHasMetaPredicate{Key: `  key:`, Value: strPtr(`value : `), Negated: false, KeyOnly: false}},
			{`escaped quotes`, `"key\"quote":"value\"quote"`, &RepoHasMetaPredicate{Key: `key"quote`, Value: strPtr(`value"quote`), Negated: false, KeyOnly: false}},
			{`space padding`, `  key:value  `, &RepoHasMetaPredicate{Key: `key`, Value: strPtr(`value`), Negated: false, KeyOnly: false}},
			{`only key`, `key`, &RepoHasMetaPredicate{Key: `key`, Value: nil, Negated: false, KeyOnly: true}},
			{`key tag`, `key:`, &RepoHasMetaPredicate{Key: "key", Value: nil, Negated: false, KeyOnly: false}},
		}

		for _, tc := range valid {
			t.Run(tc.name, func(t *testing.T) {
				p := &RepoHasMetaPredicate{}
				err := p.Unmarshal(tc.params, false)
				if err != nil {
					t.Fatalf("unexpected error: %s", err)
				}

				if !reflect.DeepEqual(tc.expected, p) {
					t.Fatalf("expected %#v, got %#v", tc.expected, p)
				}
			})
		}

		invalid := []test{
			{`empty`, ``, nil},
			{`no key`, `:value`, nil},
			{`no key or value`, `:`, nil},
			{`content outside of qutoes`, `key:"quoted value" abc`, nil},
			{`bonus colons`, `key:value:other`, nil},
		}

		for _, tc := range invalid {
			t.Run(tc.name, func(t *testing.T) {
				p := &RepoHasMetaPredicate{}
				err := p.Unmarshal(tc.params, false)
				if err == nil {
					t.Fatal("expected error but got none")
				}
			})
		}
	})
}

func TestRepoHasKVPPredicate(t *testing.T) {
	t.Run("Unmarshal", func(t *testing.T) {
		type test struct {
			name     string
			params   string
			expected *RepoHasKVPPredicate
		}

		valid := []test{
			{`key:value`, `key:value`, &RepoHasKVPPredicate{Key: "key", Value: "value", Negated: false}},
			{`empty string value`, `key:`, &RepoHasKVPPredicate{Key: "key", Value: "", Negated: false}},
			{`quoted special characters`, `"key:colon":"value:colon"`, &RepoHasKVPPredicate{Key: "key:colon", Value: "value:colon", Negated: false}},
			{`escaped quotes`, `"key\"quote":"value\"quote"`, &RepoHasKVPPredicate{Key: `key"quote`, Value: `value"quote`, Negated: false}},
			{`space padding`, `  key:value  `, &RepoHasKVPPredicate{Key: `key`, Value: `value`, Negated: false}},
			{`single quoted`, `'  key:':'value : '`, &RepoHasKVPPredicate{Key: `  key:`, Value: `value : `, Negated: false}},
		}

		for _, tc := range valid {
			t.Run(tc.name, func(t *testing.T) {
				p := &RepoHasKVPPredicate{}
				err := p.Unmarshal(tc.params, false)
				if err != nil {
					t.Fatalf("unexpected error: %s", err)
				}

				if !reflect.DeepEqual(tc.expected, p) {
					t.Fatalf("expected %#v, got %#v", tc.expected, p)
				}
			})
		}

		invalid := []test{
			{`empty`, ``, nil},
			{`no key`, `:value`, nil},
			{`no key or value`, `:`, nil},
			{`invalid syntax`, `key-value`, nil},
			{`content outside of qutoes`, `key:"quoted value" abc`, nil},
			{`bonus colons`, `key:value:other`, nil},
		}

		for _, tc := range invalid {
			t.Run(tc.name, func(t *testing.T) {
				p := &RepoHasKVPPredicate{}
				err := p.Unmarshal(tc.params, false)
				if err == nil {
					t.Fatal("expected error but got none")
				}
			})
		}
	})
}

func TestRepoContainsPredicate(t *testing.T) {
	t.Run("Unmarshal", func(t *testing.T) {
		type test struct {
			name     string
			params   string
			expected *RepoContainsPredicate
		}

		valid := []test{
			{`path`, `file:test`, &RepoContainsPredicate{File: "test"}},
			{`path regex`, `file:test(a|b)*.go`, &RepoContainsPredicate{File: "test(a|b)*.go"}},
			{`content`, `content:test`, &RepoContainsPredicate{Content: "test"}},
			{`path and content`, `file:test.go content:abc`, &RepoContainsPredicate{File: "test.go", Content: "abc"}},
			{`content and path`, `content:abc file:test.go`, &RepoContainsPredicate{File: "test.go", Content: "abc"}},
		}

		for _, tc := range valid {
			t.Run(tc.name, func(t *testing.T) {
				p := &RepoContainsPredicate{}
				err := p.Unmarshal(tc.params, false)
				if err != nil {
					t.Fatalf("unexpected error: %s", err)
				}

				if !reflect.DeepEqual(tc.expected, p) {
					t.Fatalf("expected %#v, got %#v", tc.expected, p)
				}
			})
		}

		invalid := []test{
			{`empty`, ``, nil},
			{`negated path`, `-file:test`, nil},
			{`negated content`, `-content:test`, nil},
			{`catch invalid content regexp`, `file:foo content:([)`, nil},
		}

		for _, tc := range invalid {
			t.Run(tc.name, func(t *testing.T) {
				p := &RepoContainsPredicate{}
				err := p.Unmarshal(tc.params, false)
				if err == nil {
					t.Fatal("expected error but got none")
				}
			})
		}
	})
}

func TestFileHasOwnerPredicate(t *testing.T) {
	t.Run("Unmarshal", func(t *testing.T) {
		type test struct {
			name     string
			params   string
			expected *FileHasOwnerPredicate
		}

		valid := []test{
			{`just text`, `test`, &FileHasOwnerPredicate{Owner: "test"}},
			{`handle starting with @`, `@octo-org/octocats`, &FileHasOwnerPredicate{Owner: "@octo-org/octocats"}},
			{`email`, `test@example.com`, &FileHasOwnerPredicate{Owner: "test@example.com"}},
			{`empty`, ``, &FileHasOwnerPredicate{Owner: ""}},
		}

		for _, tc := range valid {
			t.Run(tc.name, func(t *testing.T) {
				p := &FileHasOwnerPredicate{}
				err := p.Unmarshal(tc.params, false)
				if err != nil {
					t.Fatalf("unexpected error: %s", err)
				}

				if !reflect.DeepEqual(tc.expected, p) {
					t.Fatalf("expected %#v, got %#v", tc.expected, p)
				}
			})
		}
	})
}
