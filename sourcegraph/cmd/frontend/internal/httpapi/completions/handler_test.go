package completions

import (
	"fmt"
	"net/http"
	"net/http/httptest"
	"net/url"
	"testing"
	"time"

	"github.com/stretchr/testify/require"

	"github.com/sourcegraph/sourcegraph/internal/completions/types"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/database/dbmocks"
	"github.com/sourcegraph/sourcegraph/internal/featureflag"
	"github.com/sourcegraph/sourcegraph/schema"
)

func TestCheckClientCodyIgnoreCompatibility(t *testing.T) {
	t.Cleanup(func() { conf.Mock(nil) })

	featureFlags := dbmocks.NewMockFeatureFlagStore()
	db := dbmocks.NewMockDB()
	db.FeatureFlagsFunc.SetDefaultReturn(featureFlags)

	mockRequest := func(q url.Values) *http.Request {
		target := "/.api/completions/code" + "?" + q.Encode()
		req := httptest.NewRequest("GET", target, nil)
		return req
	}

	ccf := &schema.CodyContextFilters{
		Exclude: []*schema.CodyContextFilterItem{{RepoNamePattern: ".*sensitive.*"}},
	}

	tests := []struct {
		name                          string
		ccf                           *schema.CodyContextFilters
		q                             url.Values
		want                          *codyIgnoreCompatibilityError
		allowPreReleaseClientVersions bool
	}{
		{
			name: "Cody context filters not defined in the site config",
			q:    url.Values{},
			want: nil,
		},
		{
			name: "missing client name and version",
			ccf:  ccf,
			q:    url.Values{},
			want: &codyIgnoreCompatibilityError{
				reason:     "\"client-name\" query param is required.",
				statusCode: http.StatusNotAcceptable,
			},
		},
		{
			name: "missing client name",
			ccf:  ccf,
			q: url.Values{
				"client-version": []string{"1.1.1"},
			},
			want: &codyIgnoreCompatibilityError{
				reason:     "\"client-name\" query param is required.",
				statusCode: http.StatusNotAcceptable,
			},
		},
		{
			name: "missing client version",
			ccf:  ccf,
			q: url.Values{
				"client-name": []string{string(types.CodyClientVscode)},
			},
			want: &codyIgnoreCompatibilityError{
				reason:     "\"client-version\" query param is required.",
				statusCode: http.StatusNotAcceptable,
			},
		},
		{
			name: "not supported client",
			ccf:  ccf,
			q: url.Values{
				"client-name": []string{"sublime"},
			},
			want: &codyIgnoreCompatibilityError{
				reason:     fmt.Sprintf("please use one of the supported clients: %s, %s, %s.", types.CodyClientVscode, types.CodyClientJetbrains, types.CodyClientWeb),
				statusCode: http.StatusNotAcceptable,
			},
		},
		{
			name: "version doesn't follow semver spec (missing major, minor and patch versions)",
			ccf:  ccf,
			q: url.Values{
				"client-name":    []string{string(types.CodyClientVscode)},
				"client-version": []string{"dev"},
			},
			want: &codyIgnoreCompatibilityError{
				reason:     "Cody for vscode version \"dev\" doesn't follow semver spec.",
				statusCode: http.StatusBadRequest,
			},
		},
		{
			name: "version doesn't follow semver spec (random string)",
			ccf:  ccf,
			q: url.Values{
				"client-name":    []string{string(types.CodyClientVscode)},
				"client-version": []string{"."},
			},
			want: &codyIgnoreCompatibilityError{
				reason:     "Cody for vscode version \".\" doesn't follow semver spec.",
				statusCode: http.StatusBadRequest,
			},
		},
		{
			name: "version doesn't follow semver spec (empty pre-release identifier)",
			ccf:  ccf,
			q: url.Values{
				"client-name":    []string{string(types.CodyClientVscode)},
				"client-version": []string{"1.2.3-"},
			},
			want: &codyIgnoreCompatibilityError{
				reason:     "Cody for vscode version \"1.2.3-\" doesn't follow semver spec.",
				statusCode: http.StatusBadRequest,
			},
		},
		{
			name: "version doesn't follow semver spec (not allowed symbols in pre-release identifier)",
			ccf:  ccf,
			q: url.Values{
				"client-name":    []string{string(types.CodyClientVscode)},
				"client-version": []string{"1.2.3-a^1"},
			},
			want: &codyIgnoreCompatibilityError{
				reason:     "Cody for vscode version \"1.2.3-a^1\" doesn't follow semver spec.",
				statusCode: http.StatusBadRequest,
			},
		},
		{
			name: "version doesn't follow semver spec (empty build identifier)",
			ccf:  ccf,
			q: url.Values{
				"client-name":    []string{string(types.CodyClientVscode)},
				"client-version": []string{"1.2.3-alpha+"},
			},
			want: &codyIgnoreCompatibilityError{
				reason:     "Cody for vscode version \"1.2.3-alpha+\" doesn't follow semver spec.",
				statusCode: http.StatusBadRequest,
			},
		},
		{
			name: "vscode: major version doesn't match constraint (shorthand semver version)",
			ccf:  ccf,
			q: url.Values{
				"client-name":    []string{string(types.CodyClientVscode)},
				"client-version": []string{"0.1"},
			},
			want: &codyIgnoreCompatibilityError{
				reason:     fmt.Sprintf("Cody for %s version \"0.1\" doesn't match version constraint \">= 1.20.0\"", types.CodyClientVscode),
				statusCode: http.StatusNotAcceptable,
			},
		},
		{
			name: "vscode: minor version doesn't match constraint (shorthand semver version)",
			ccf:  ccf,
			q: url.Values{
				"client-name":    []string{string(types.CodyClientVscode)},
				"client-version": []string{"1.2"},
			},
			want: &codyIgnoreCompatibilityError{
				reason:     fmt.Sprintf("Cody for %s version \"1.2\" doesn't match version constraint \">= 1.20.0\"", types.CodyClientVscode),
				statusCode: http.StatusNotAcceptable,
			},
		},
		{
			name: "vscode: minor version doesn't match constraint",
			ccf:  ccf,
			q: url.Values{
				"client-name":    []string{string(types.CodyClientVscode)},
				"client-version": []string{"1.19.0"},
			},
			want: &codyIgnoreCompatibilityError{
				reason:     fmt.Sprintf("Cody for %s version \"1.19.0\" doesn't match version constraint \">= 1.20.0\"", types.CodyClientVscode),
				statusCode: http.StatusNotAcceptable,
			},
		},
		{
			name: "vscode: version matches constraint (standard semver version)",
			ccf:  ccf,
			q: url.Values{
				"client-name":    []string{string(types.CodyClientVscode)},
				"client-version": []string{"1.20.0"},
			},
			want: nil,
		},
		{
			name: "vscode: version matches constraint (major-only shorthand version)",
			ccf:  ccf,
			q: url.Values{
				"client-name":    []string{string(types.CodyClientVscode)},
				"client-version": []string{"2"},
			},
			want: nil,
		},
		{
			name: "vscode: version matches constraint (major-only shorthand version with leading \"v\")",
			ccf:  ccf,
			q: url.Values{
				"client-name":    []string{string(types.CodyClientVscode)},
				"client-version": []string{"v2"},
			},
			want: nil,
		},
		{
			name: "vscode: version matches constraint (version with build metadata)",
			ccf:  ccf,
			q: url.Values{
				"client-name":    []string{string(types.CodyClientVscode)},
				"client-version": []string{"2.0.0+20130313144700"},
			},
			want: nil,
		},
		{
			// See https://pkg.go.dev/github.com/Masterminds/semver#readme-working-with-pre-release-versions
			name: "vscode: pre-release version doesn't match constraint if the constraint is defined without a pre-release comparator",
			ccf:  ccf,
			q: url.Values{
				"client-name":    []string{string(types.CodyClientVscode)},
				"client-version": []string{"2.3.11-alpha"},
			},
			want: &codyIgnoreCompatibilityError{
				reason:     fmt.Sprintf("Cody for %s version \"2.3.11-alpha\" doesn't match version constraint \">= 1.20.0\"", types.CodyClientVscode),
				statusCode: http.StatusNotAcceptable,
			},
		},
		{
			// See https://pkg.go.dev/github.com/Masterminds/semver#readme-working-with-pre-release-versions
			name: "vscode: version matches constraint (pre-release version with build metadata)",
			ccf:  ccf,
			q: url.Values{
				"client-name":    []string{string(types.CodyClientVscode)},
				"client-version": []string{"2.3.11-beta+exp.sha.5114f85a"},
			},
			want: &codyIgnoreCompatibilityError{
				reason:     fmt.Sprintf("Cody for %s version \"2.3.11-beta+exp.sha.5114f85a\" doesn't match version constraint \">= 1.20.0\"", types.CodyClientVscode),
				statusCode: http.StatusNotAcceptable,
			},
		},
		{
			name: "jetbrains: version doesn't match constraint",
			ccf:  ccf,
			q: url.Values{
				"client-name":    []string{string(types.CodyClientJetbrains)},
				"client-version": []string{"1.14.0"},
			},
			want: &codyIgnoreCompatibilityError{
				reason:     fmt.Sprintf("Cody for %s version \"1.14.0\" doesn't match version constraint \">= 6.0.0\"", types.CodyClientJetbrains),
				statusCode: http.StatusNotAcceptable,
			},
		},
		{
			name: "jetbrains: version matches constraint",
			ccf:  ccf,
			q: url.Values{
				"client-name":    []string{string(types.CodyClientJetbrains)},
				"client-version": []string{"6.0.0"},
			},
			want: nil,
		},
		{
			// See https://pkg.go.dev/github.com/Masterminds/semver#readme-working-with-pre-release-versions
			name: "jetbrains: pre-release version doesn't match constraint if \"cody-context-filters-allow-pre-release-client-versions\" feature flag is enabled",
			ccf:  ccf,
			q: url.Values{
				"client-name":    []string{string(types.CodyClientJetbrains)},
				"client-version": []string{"5.9-localbuild"},
			},
			want: &codyIgnoreCompatibilityError{
				reason:     fmt.Sprintf("Cody for %s version \"5.9-localbuild\" doesn't match version constraint \">= 6.0.0-0\"", types.CodyClientJetbrains),
				statusCode: http.StatusNotAcceptable,
			},
			allowPreReleaseClientVersions: true,
		},
		{
			// See https://pkg.go.dev/github.com/Masterminds/semver#readme-working-with-pre-release-versions
			name: "jetbrains: pre-release version matches constraint if \"cody-context-filters-allow-pre-release-client-versions\" feature flag is enabled",
			ccf:  ccf,
			q: url.Values{
				"client-name":    []string{string(types.CodyClientJetbrains)},
				"client-version": []string{"6.0.0"},
			},
			want:                          nil,
			allowPreReleaseClientVersions: true,
		},
		{
			// See https://pkg.go.dev/github.com/Masterminds/semver#readme-working-with-pre-release-versions
			name: "jetbrains: pre-release version doesn't match constraint if \"cody-context-filters-allow-pre-release-client-versions\" feature flag is not enabled",
			ccf:  ccf,
			q: url.Values{
				"client-name":    []string{string(types.CodyClientJetbrains)},
				"client-version": []string{"6.0-localbuild"},
			},
			want: &codyIgnoreCompatibilityError{
				reason:     fmt.Sprintf("Cody for %s version \"6.0-localbuild\" doesn't match version constraint \">= 6.0.0\"", types.CodyClientJetbrains),
				statusCode: http.StatusNotAcceptable,
			},
		},
		{
			// See https://pkg.go.dev/github.com/Masterminds/semver#readme-working-with-pre-release-versions
			name: "vscode: pre-release version doesn't match constraint if \"cody-context-filters-allow-pre-release-client-versions\" feature flag is enabled (feature flag should work only for jetbrains client)",
			ccf:  ccf,
			q: url.Values{
				"client-name":    []string{string(types.CodyClientVscode)},
				"client-version": []string{"1.22.0-alpha"},
			},
			want: &codyIgnoreCompatibilityError{
				reason:     fmt.Sprintf("Cody for %s version \"1.22.0-alpha\" doesn't match version constraint \">= 1.20.0\"", types.CodyClientVscode),
				statusCode: http.StatusNotAcceptable,
			},
			allowPreReleaseClientVersions: true,
		},
		{
			name: "web: version param not required",
			ccf:  ccf,
			q: url.Values{
				"client-name": []string{string(types.CodyClientWeb)},
			},
			want: nil,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Cleanup(func() { featureFlags.GetFeatureFlagFunc.SetDefaultReturn(nil, nil) })

			if tt.ccf != nil {
				conf.Mock(&conf.Unified{
					SiteConfiguration: schema.SiteConfiguration{
						CodyContextFilters: tt.ccf,
					},
				})
			}

			if tt.allowPreReleaseClientVersions {
				featureFlags.GetFeatureFlagFunc.SetDefaultReturn(&featureflag.FeatureFlag{
					Name:      "cody-context-filters-allow-pre-release-client-versions",
					Bool:      &featureflag.FeatureFlagBool{Value: true},
					Rollout:   nil,
					CreatedAt: time.Now(),
					UpdatedAt: time.Now(),
					DeletedAt: nil,
				}, nil)
			}

			req := mockRequest(tt.q)
			got := checkClientCodyIgnoreCompatibility(req.Context(), db, req)
			require.Equal(t, tt.want, got)
		})
	}
}
