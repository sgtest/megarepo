package query

import (
	"strings"

	"github.com/grafana/regexp"
)

// RevisionSpecifier represents either a revspec or a ref glob. At most one
// field is set. The default branch is represented by all fields being empty.
type RevisionSpecifier struct {
	// RevSpec is a revision range specifier suitable for passing to git. See
	// the manpage gitrevisions(7).
	RevSpec string

	// RefGlob is a reference glob to pass to git. See the documentation for
	// "--glob" in git-log.
	RefGlob string

	// ExcludeRefGlob is a glob for references to exclude. See the
	// documentation for "--exclude" in git-log.
	ExcludeRefGlob string
}

func (r1 RevisionSpecifier) String() string {
	if r1.ExcludeRefGlob != "" {
		return "*!" + r1.ExcludeRefGlob
	}
	if r1.RefGlob != "" {
		return "*" + r1.RefGlob
	}
	return r1.RevSpec
}

// Less compares two revspecOrRefGlob entities, suitable for use
// with sort.Slice()
//
// possibly-undesired: this results in treating an entity with
// no revspec, but a refGlob, as "earlier" than any revspec.
func (r1 RevisionSpecifier) Less(r2 RevisionSpecifier) bool {
	if r1.RevSpec != r2.RevSpec {
		return r1.RevSpec < r2.RevSpec
	}
	if r1.RefGlob != r2.RefGlob {
		return r1.RefGlob < r2.RefGlob
	}
	return r1.ExcludeRefGlob < r2.ExcludeRefGlob
}

func (r1 RevisionSpecifier) HasRefGlob() bool {
	return r1.RefGlob != "" || r1.ExcludeRefGlob != ""
}

type ParsedRepoFilter struct {
	Repo      string
	RepoRegex *regexp.Regexp // A case-insensitive regex matching the Repo pattern
	Revs      []RevisionSpecifier
}

func (p ParsedRepoFilter) String() string {
	if len(p.Revs) == 0 {
		return p.Repo
	}

	revSpecs := make([]string, len(p.Revs))
	for i, r := range p.Revs {
		revSpecs[i] = r.String()
	}
	return p.Repo + "@" + strings.Join(revSpecs, ":")
}

// ParseRepositoryRevisions parses strings that refer to a repository and 0
// or more revspecs. The format is:
//
//	repo@revs
//
// where repo is a repository regex and revs is a ':'-separated list of revspecs
// and/or ref globs. A ref glob is a revspec prefixed with '*' (which is not a
// valid revspec or ref itself; see `man git-check-ref-format`). The '@' and revs
// may be omitted to refer to the default branch.
//
// Returns an error if the repo pattern is not a valid regular expression.
//
// For example:
//
//   - 'foo' refers to the 'foo' repo at the default branch
//   - 'foo@bar' refers to the 'foo' repo and the 'bar' revspec.
//   - 'foo@bar:baz:qux' refers to the 'foo' repo and 3 revspecs: 'bar', 'baz',
//     and 'qux'.
//   - 'foo@*bar' refers to the 'foo' repo and all refs matching the glob 'bar/*',
//     because git interprets the ref glob 'bar' as being 'bar/*' (see `man git-log`
//     section on the --glob flag)
func ParseRepositoryRevisions(repoAndOptionalRev string) (ParsedRepoFilter, error) {
	var repo string
	var revs []RevisionSpecifier

	i := strings.Index(repoAndOptionalRev, "@")
	if i == -1 {
		// return an empty slice to indicate that there's no revisions; callers
		// have to distinguish between "none specified" and "default" to handle
		// cases where two repo specs both match the same repository, and only one
		// specifies a revspec, which normally implies "master" but in that case
		// really means "didn't specify"
		repo = repoAndOptionalRev
		revs = []RevisionSpecifier{}
	} else {
		repo = repoAndOptionalRev[:i]
		for _, part := range strings.Split(repoAndOptionalRev[i+1:], ":") {
			if part == "" {
				continue
			}
			revs = append(revs, parseRev(part))
		}
		if len(revs) == 0 {
			revs = []RevisionSpecifier{{RevSpec: ""}} // default branch
		}
	}

	// Repo filters don't currently support case sensitivity, so we use a
	// case-insensitive regex here to match as widely as possible during
	// highlighting and other post-processing.
	repoRegex, err := regexp.Compile("(?i)" + repo)
	if err != nil {
		return ParsedRepoFilter{}, err
	}

	return ParsedRepoFilter{Repo: repo, RepoRegex: repoRegex, Revs: revs}, nil
}

func parseRev(spec string) RevisionSpecifier {
	if strings.HasPrefix(spec, "*!") {
		return RevisionSpecifier{ExcludeRefGlob: spec[2:]}
	} else if strings.HasPrefix(spec, "*") {
		return RevisionSpecifier{RefGlob: spec[1:]}
	}
	return RevisionSpecifier{RevSpec: spec}
}
