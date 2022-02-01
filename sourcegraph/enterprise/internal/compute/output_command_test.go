package compute

import (
	"context"
	"os"
	"regexp"
	"testing"

	"github.com/hexops/autogold"

	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/comby"
	"github.com/sourcegraph/sourcegraph/internal/gitserver/gitdomain"
	"github.com/sourcegraph/sourcegraph/internal/search/result"
	"github.com/sourcegraph/sourcegraph/internal/vcs/git"
)

func Test_output(t *testing.T) {
	test := func(input string, cmd *Output) string {
		result, err := output(context.Background(), input, cmd.SearchPattern, cmd.OutputPattern, cmd.Separator)
		if err != nil {
			return err.Error()
		}
		return result.Value
	}

	autogold.Want(
		"regexp search outputs only digits",
		"(1)~(2)~(3)~").
		Equal(t, test("a 1 b 2 c 3", &Output{
			SearchPattern: &Regexp{Value: regexp.MustCompile(`(\d)`)},
			OutputPattern: "($1)",
			Separator:     "~",
		}))

	// If we are not on CI skip the test if comby is not installed.
	if os.Getenv("CI") == "" && !comby.Exists() {
		t.Skip("comby is not installed on the PATH. Try running 'bash <(curl -sL get.comby.dev)'.")
	}

	autogold.Want(
		"structural search output",
		`train(regional, intercity)
train(commuter, lightrail)`).
		Equal(t, test("Im a train. train(intercity, regional). choo choo. train(lightrail, commuter)", &Output{
			SearchPattern: &Comby{Value: `train(:[x], :[y])`},
			OutputPattern: "train(:[y], :[x])",
		}))
}

func fileMatch(content string) result.Match {
	git.Mocks.ReadFile = func(_ api.CommitID, _ string) ([]byte, error) {
		return []byte(content), nil
	}
	return &result.FileMatch{
		File: result.File{Path: "my/awesome/path"},
	}
}

func commitMatch(content string) result.Match {
	return &result.CommitMatch{
		Commit: gitdomain.Commit{
			Author:    gitdomain.Signature{Name: "bob"},
			Committer: &gitdomain.Signature{},
			Message:   gitdomain.Message(content),
		},
	}
}

func TestRun(t *testing.T) {
	test := func(q string, m result.Match) string {
		defer git.ResetMocks()
		computeQuery, _ := Parse(q)
		res, err := computeQuery.Command.Run(context.Background(), m)
		if err != nil {
			return err.Error()
		}
		return res.(*Text).Value
	}

	autogold.Want(
		"template substitution regexp",
		"(1)\n(2)\n(3)\n").
		Equal(t, test(`content:output((\d) -> ($1))`, fileMatch("a 1 b 2 c 3")))

	autogold.Want(
		"template substitution regexp with commit author",
		"bob: (1)\nbob: (2)\nbob: (3)\n").
		Equal(t, test(`content:output((\d) -> $author: ($1))`, commitMatch("a 1 b 2 c 3")))

	// If we are not on CI skip the test if comby is not installed.
	if os.Getenv("CI") == "" && !comby.Exists() {
		t.Skip("comby is not installed on the PATH. Try running 'bash <(curl -sL get.comby.dev)'.")
	}

	autogold.Want(
		"template substitution structural",
		">bar<").
		Equal(t, test(`content:output.structural(foo(:[arg]) -> >:[arg]<)`, fileMatch("foo(bar)")))

}
