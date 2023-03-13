package highlight

import (
	"strings"
	"testing"

	"github.com/grafana/regexp"
)

type languageTestCase struct {
	Config   syntaxHighlightConfig
	Path     string
	Expected string
	Found    bool
}

func TestGetLanguageFromConfig(t *testing.T) {
	cases := []languageTestCase{
		{
			Config: syntaxHighlightConfig{
				Extensions: map[string]string{
					"go": "not go",
				},
			},
			Path:     "example.go",
			Found:    true,
			Expected: "not go",
		},
		{
			Config: syntaxHighlightConfig{
				Extensions: map[string]string{},
			},
			Path:     "example.go",
			Found:    false,
			Expected: "",
		},

		{
			Config: syntaxHighlightConfig{
				Extensions: map[string]string{
					"strato": "scala",
				},
			},
			Path:     "test.strato",
			Found:    true,
			Expected: "scala",
		},

		{
			Config: syntaxHighlightConfig{
				Patterns: []languagePattern{
					{
						pattern:  regexp.MustCompile("asdf"),
						language: "not matching",
					},
					{
						pattern:  regexp.MustCompile("\\.bashrc"),
						language: "bash",
					},
				},
			},
			Path:     "/home/example/.bashrc",
			Found:    true,
			Expected: "bash",
		},
	}

	for _, testCase := range cases {
		language, found := getLanguageFromConfig(testCase.Config, testCase.Path)
		if found != testCase.Found {
			t.Fatalf("Got: %v, Expected: %v", testCase.Found, found)
		}

		if language != testCase.Expected {
			t.Fatalf("Got: %s, Expected: %s", testCase.Expected, language)
		}
	}
}

func TestShebagn(t *testing.T) {
	type testCase struct {
		Contents string
		Expected string
	}

	cases := []testCase{
		{
			Contents: "#!/usr/bin/env python",
			Expected: "python",
		},
		{
			Contents: "#!/usr/bin/env node",
			Expected: "javascript",
		},
		{
			Contents: "#!/usr/bin/env ruby",
			Expected: "ruby",
		},
		{
			Contents: "#!/usr/bin/env perl",
			Expected: "perl",
		},
		{
			Contents: "#!/usr/bin/env php",
			Expected: "php",
		},
		{
			Contents: "#!/usr/bin/env lua",
			Expected: "lua",
		},
		{
			Contents: "#!/usr/bin/env tclsh",
			Expected: "tcl",
		},
		{
			Contents: "#!/usr/bin/env fish",
			Expected: "fish",
		},
	}

	for _, testCase := range cases {
		language, _ := getLanguage("", testCase.Contents)
		language = strings.ToLower(language)
		if language != testCase.Expected {
			t.Fatalf("%s\nGot: %s, Expected: %s", testCase.Contents, testCase.Expected, language)
		}
	}
}
