package symbols

import (
	"context"
	"fmt"
	"path"
	"testing"

	"github.com/sourcegraph/sourcegraph/cmd/symbols/internal/pkg/ctags"
	"github.com/sourcegraph/sourcegraph/pkg/symbols/protocol"
	"github.com/sourcegraph/sourcegraph/pkg/testutil"
)

func BenchmarkSearch(b *testing.B) {
	service := Service{
		FetchTar: testutil.FetchTarFromGithub,
		NewParser: func() (ctags.Parser, error) {
			return ctags.NewParser("universal-ctags")
		},
		Path: "/tmp/symbols-cache",
	}
	if err := service.Start(); err != nil {
		b.Fatal(err)
	}

	ctx := context.Background()
	b.ResetTimer()

	tests := []protocol.SearchArgs{
		{Repo: "github.com/sourcegraph/go-langserver", CommitID: "391a062a7d9977510e7e883e412769b07fed8b5e"},
		{Repo: "github.com/moby/moby", CommitID: "6e5c2d639f67ae70f54d9f2285f3261440b074aa"},
	}

	for _, test := range tests {
		b.Run(fmt.Sprintf("%s@%s", path.Base(string(test.Repo)), test.CommitID[:3]), func(b *testing.B) {
			for n := 0; n < b.N; n++ {
				_, err := service.search(ctx, test)
				if err != nil {
					b.Fatal(err)
				}
			}
		})
	}
}
