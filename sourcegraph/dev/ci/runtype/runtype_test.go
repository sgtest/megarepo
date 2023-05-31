package runtype

import (
	"testing"

	"github.com/stretchr/testify/assert"
)

// TestComputeRunType should be used for high-level testing of critical run types.
func TestComputeRunType(t *testing.T) {
	type args struct {
		tag    string
		branch string
		env    map[string]string
	}
	tests := []struct {
		name string
		args args
		want RunType
	}{{
		name: "pull request by default",
		args: args{
			branch: "some-random-feature-branch",
		},
		want: PullRequest,
	}, {
		name: "main",
		args: args{
			branch: "main",
		},
		want: MainBranch,
	}, {
		name: "tagged release",
		args: args{
			branch: "1.3",
			tag:    "v1.2.3",
		},
		want: TaggedRelease,
	}, {
		name: "bext release",
		args: args{
			branch: "bext/release",
		},
		want: BextReleaseBranch,
	}, {
		name: "bext nightly",
		args: args{
			branch: "main",
			env: map[string]string{
				"BEXT_NIGHTLY": "true",
			},
		},
		want: BextNightly,
	}, {
		name: "vsce nightly",
		args: args{
			branch: "main",
			env: map[string]string{
				"VSCE_NIGHTLY": "true",
			},
		},
		want: VsceNightly,
	}, {
		name: "vsce release",
		args: args{
			branch: "vsce/release",
		},
		want: VsceReleaseBranch,
	}, {
		name: "app release",
		args: args{
			branch: "app/release",
		},
		want: AppRelease,
	}, {
		name: "app release insiders",
		args: args{
			branch: "app/insiders",
		},
		want: AppInsiders,
	}, {
		name: "cody nightly",
		args: args{
			branch: "main",
			env: map[string]string{
				"CODY_NIGHTLY": "true",
			},
		},
		want: CodyNightly,
	}, {
		name: "cody release",
		args: args{
			branch: "cody/release",
		},
		want: CodyReleaseBranch,
	}, {
		name: "release nightly",
		args: args{
			branch: "main",
			env: map[string]string{
				"RELEASE_NIGHTLY": "true",
			},
		},
		want: ReleaseNightly,
	}}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := Compute(tt.args.tag, tt.args.branch, tt.args.env)
			assert.Equal(t, tt.want.String(), got.String())
		})
	}
}

func TestRunTypeString(t *testing.T) {
	// Check all individual types have a name defined at least
	var tested int
	for rt := PullRequest; rt < None; rt += 1 {
		tested += 1
		assert.NotEmpty(t, rt.String(), "RunType: %d with matcher %+v", rt, rt.Matcher())
	}
	assert.Equal(t, int(None), tested)
}

func TestRunTypeMatcher(t *testing.T) {
	// Check all individual types have a matcher defined at least
	// Start a PullRequest+1 because PullRequest is the default RunType, and does not have
	// a matcher.
	var tested int
	for rt := PullRequest + 1; rt < None; rt += 1 {
		tested += 1
		assert.NotNil(t, rt.Matcher(), "RunType: %d with name %q", rt, rt.String())
	}
	assert.Equal(t, int(None)-1, tested)
}

func TestRunTypeMatcherMatches(t *testing.T) {
	type args struct {
		tag    string
		branch string
	}
	tests := []struct {
		name    string
		matcher RunTypeMatcher
		args    args
		want    bool
	}{{
		name: "branch prefix",
		matcher: RunTypeMatcher{
			Branch: "main-dry-run/",
		},
		args: args{branch: "main-dry-run/asdf"},
		want: true,
	}, {
		name: "branch regexp",
		matcher: RunTypeMatcher{
			Branch:       `^[0-9]+\.[0-9]+$`,
			BranchRegexp: true,
		},
		args: args{branch: "1.2"},
		want: true,
	}, {
		name: "branch exact",
		matcher: RunTypeMatcher{
			Branch:      "main",
			BranchExact: true,
		},
		args: args{branch: "main"},
		want: true,
	}, {
		name: "tag prefix",
		matcher: RunTypeMatcher{
			TagPrefix: "v",
		},
		args: args{branch: "main", tag: "v1.2.3"},
		want: true,
	}, {
		name: "env includes",
		matcher: RunTypeMatcher{
			EnvIncludes: map[string]string{
				"KEY": "VALUE",
			},
		},
		args: args{branch: "main"},
		want: true,
	}, {
		name: "env not includes",
		matcher: RunTypeMatcher{
			EnvIncludes: map[string]string{
				"KEY": "NOT_VALUE",
			},
		},
		args: args{branch: "main"},
		want: false,
	}}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := tt.matcher.Matches(tt.args.tag, tt.args.branch, map[string]string{
				"KEY": "VALUE",
			})
			assert.Equal(t, tt.want, got)
		})
	}
}

func TestRunTypeMatcherExtractBranchArgument(t *testing.T) {
	tests := []struct {
		name            string
		matcher         *RunTypeMatcher
		branch          string
		want            string
		wantErrContains string
	}{{
		name:    "gets 1 segment argument",
		matcher: &RunTypeMatcher{Branch: "prefix/"},
		branch:  "prefix/argument",
		want:    "argument",
	}, {
		name:    "gets 2 segment argument",
		matcher: &RunTypeMatcher{Branch: "prefix/"},
		branch:  "prefix/argument/name",
		want:    "argument",
	}, {
		name:    "missing unrequired argument",
		matcher: &RunTypeMatcher{Branch: "prefix/"},
		branch:  "prefix/",
	}, {
		name: "missing required argument",
		matcher: &RunTypeMatcher{
			Branch:                 "prefix/",
			BranchArgumentRequired: true,
		},
		branch:          "prefix/",
		wantErrContains: "branch argument expected",
	}}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := tt.matcher.ExtractBranchArgument(tt.branch)
			if tt.wantErrContains != "" {
				assert.Error(t, err)
				assert.Contains(t, err.Error(), tt.wantErrContains)
			}
			assert.Equal(t, tt.want, got)
		})
	}
}
