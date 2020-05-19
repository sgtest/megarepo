package extsvc

import (
	"github.com/google/go-cmp/cmp"
	"testing"
)

func TestExtractRateLimitConfig(t *testing.T) {
	for _, tc := range []struct {
		name        string
		config      string
		kind        string
		displayName string
		want        RateLimitConfig
	}{
		{
			name:        "GitLab default",
			config:      `{"url": "https://example.com/"}`,
			kind:        "GITLAB",
			displayName: "GitLab 1",
			want: RateLimitConfig{
				BaseURL:     "https://example.com/",
				DisplayName: "GitLab 1",
				Limit:       10.0,
				IsDefault:   true,
			},
		},
		{
			name:        "GitHub default",
			config:      `{"url": "https://example.com/"}`,
			kind:        "GITHUB",
			displayName: "GitHub 1",
			want: RateLimitConfig{
				BaseURL:     "https://example.com/",
				DisplayName: "GitHub 1",
				Limit:       1.3888888888888888,
				IsDefault:   true,
			},
		},
		{
			name:        "Bitbucket Server default",
			config:      `{"url": "https://example.com/"}`,
			kind:        "BITBUCKETSERVER",
			displayName: "BitbucketServer 1",
			want: RateLimitConfig{
				BaseURL:     "https://example.com/",
				DisplayName: "BitbucketServer 1",
				Limit:       8.0,
				IsDefault:   true,
			},
		},
		{
			name:        "Bitbucket Cloud default",
			config:      `{"url": "https://example.com/"}`,
			kind:        "BITBUCKETCLOUD",
			displayName: "BitbucketCloud 1",
			want: RateLimitConfig{
				BaseURL:     "https://example.com/",
				DisplayName: "BitbucketCloud 1",
				Limit:       2.0,
				IsDefault:   true,
			},
		},
		{
			name:        "GitLab non-default",
			config:      `{"url": "https://example.com/", "rateLimit": {"enabled": true, "requestsPerHour": 3600}}`,
			kind:        "GITLAB",
			displayName: "GitLab 1",
			want: RateLimitConfig{
				BaseURL:     "https://example.com/",
				DisplayName: "GitLab 1",
				Limit:       1.0,
				IsDefault:   false,
			},
		},
		{
			name:        "GitHub default",
			config:      `{"url": "https://example.com/", "rateLimit": {"enabled": true, "requestsPerHour": 3600}}`,
			kind:        "GITHUB",
			displayName: "GitHub 1",
			want: RateLimitConfig{
				BaseURL:     "https://example.com/",
				DisplayName: "GitHub 1",
				Limit:       1.0,
				IsDefault:   false,
			},
		},
		{
			name:        "Bitbucket Server default",
			config:      `{"url": "https://example.com/", "rateLimit": {"enabled": true, "requestsPerHour": 3600}}`,
			kind:        "BITBUCKETSERVER",
			displayName: "BitbucketServer 1",
			want: RateLimitConfig{
				BaseURL:     "https://example.com/",
				DisplayName: "BitbucketServer 1",
				Limit:       1.0,
				IsDefault:   false,
			},
		},
		{
			name:        "Bitbucket Cloud default",
			config:      `{"url": "https://example.com/", "rateLimit": {"enabled": true, "requestsPerHour": 3600}}`,
			kind:        "BITBUCKETCLOUD",
			displayName: "BitbucketCloud 1",
			want: RateLimitConfig{
				BaseURL:     "https://example.com/",
				DisplayName: "BitbucketCloud 1",
				Limit:       1.0,
				IsDefault:   false,
			},
		},
	} {
		t.Run(tc.name, func(t *testing.T) {
			rlc, err := ExtractRateLimitConfig(tc.config, tc.kind, tc.displayName)
			if err != nil {
				t.Fatal(err)
			}
			if diff := cmp.Diff(tc.want, rlc); diff != "" {
				t.Fatal(diff)
			}
		})
	}
}
