package graphqlbackend

import (
	"context"
	"net/url"
	"strings"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/backend"
	"github.com/sourcegraph/sourcegraph/pkg/vcs/git"
)

type gitRevSpecExpr struct {
	expr string
	repo *repositoryResolver
}

func (r *gitRevSpecExpr) Expr() string { return r.expr }

func (r *gitRevSpecExpr) Object(ctx context.Context) (*gitObject, error) {
	oid, err := git.ResolveRevision(ctx, backend.CachedGitRepo(r.repo.repo), nil, r.expr, nil)
	if err != nil {
		return nil, err
	}
	return &gitObject{
		oid:  gitObjectID(oid),
		repo: r.repo,
	}, nil
}

type gitRevSpec struct {
	ref    *gitRefResolver
	expr   *gitRevSpecExpr
	object *gitObject
}

func (r *gitRevSpec) ToGitRef() (*gitRefResolver, bool)         { return r.ref, r.ref != nil }
func (r *gitRevSpec) ToGitRevSpecExpr() (*gitRevSpecExpr, bool) { return r.expr, r.expr != nil }
func (r *gitRevSpec) ToGitObject() (*gitObject, bool)           { return r.object, r.object != nil }

type gitRevisionRange struct {
	expr       string
	base, head *gitRevSpec
	mergeBase  *gitObject
}

func (r *gitRevisionRange) Expr() string      { return r.expr }
func (r *gitRevisionRange) Base() *gitRevSpec { return r.base }
func (r *gitRevisionRange) BaseRevSpec() *gitRevSpecExpr {
	expr, _ := r.base.ToGitRevSpecExpr()
	return expr
}
func (r *gitRevisionRange) Head() *gitRevSpec { return r.head }
func (r *gitRevisionRange) HeadRevSpec() *gitRevSpecExpr {
	expr, _ := r.head.ToGitRevSpecExpr()
	return expr
}
func (r *gitRevisionRange) MergeBase() *gitObject { return r.mergeBase }

// escapeRevspecForURL escapes revspec for use in a Sourcegraph URL. For niceness/readability, we do
// NOT escape slashes but we do escape other characters like '#' that are necessary for correctness.
func escapeRevspecForURL(revspec string) string {
	return strings.Replace(url.PathEscape(revspec), "%2F", "/", -1)
}
