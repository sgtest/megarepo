package repos

import (
	"context"
	"net/url"
	"strings"

	"github.com/pkg/errors"
	"github.com/sourcegraph/sourcegraph/pkg/api"
	"github.com/sourcegraph/sourcegraph/pkg/jsonc"
	"github.com/sourcegraph/sourcegraph/schema"
)

// A OtherSource yields repositories from a single Other connection configured
// in Sourcegraph via the external services configuration.
type OtherSource struct {
	svc  *ExternalService
	conn *schema.OtherExternalServiceConnection
}

// NewOtherSource returns a new OtherSource from the given external service.
func NewOtherSource(svc *ExternalService) (*OtherSource, error) {
	var c schema.OtherExternalServiceConnection
	if err := jsonc.Unmarshal(svc.Config, &c); err != nil {
		return nil, errors.Wrapf(err, "external service id=%d config error", svc.ID)
	}
	return &OtherSource{svc: svc, conn: &c}, nil
}

// ListRepos returns all Other repositories accessible to all connections configured
// in Sourcegraph via the external services configuration.
func (s OtherSource) ListRepos(ctx context.Context) ([]*Repo, error) {
	urls, err := s.cloneURLs()
	if err != nil {
		return nil, err
	}

	urn := s.svc.URN()
	repos := make([]*Repo, 0, len(urls))
	for _, u := range urls {
		repos = append(repos, otherRepoFromCloneURL(urn, u))
	}

	return repos, nil
}

// ExternalServices returns a singleton slice containing the external service.
func (s OtherSource) ExternalServices() ExternalServices {
	return ExternalServices{s.svc}
}

func (s OtherSource) cloneURLs() ([]*url.URL, error) {
	if len(s.conn.Repos) == 0 {
		return nil, nil
	}

	var base *url.URL
	if s.conn.Url != "" {
		var err error
		if base, err = url.Parse(s.conn.Url); err != nil {
			return nil, err
		}
	}

	cloneURLs := make([]*url.URL, 0, len(s.conn.Repos))
	for _, repo := range s.conn.Repos {
		cloneURL, err := otherRepoCloneURL(base, repo)
		if err != nil {
			return nil, err
		}
		cloneURLs = append(cloneURLs, cloneURL)
	}

	return cloneURLs, nil
}

func otherRepoCloneURL(base *url.URL, repo string) (*url.URL, error) {
	if base == nil {
		return url.Parse(repo)
	}
	return base.Parse(repo)
}

var otherRepoNameReplacer = strings.NewReplacer(":", "-", "@", "-", "//", "")

func otherRepoName(cloneURL *url.URL) string {
	u := *cloneURL
	u.User = nil
	u.Scheme = ""
	u.RawQuery = ""
	u.Fragment = ""
	return otherRepoNameReplacer.Replace(u.String())
}

func otherRepoFromCloneURL(urn string, u *url.URL) *Repo {
	repoURL := u.String()
	repoName := otherRepoName(u)
	u.Path, u.RawQuery = "", ""
	serviceID := u.String()

	return &Repo{
		Name: repoName,
		URI:  repoName,
		ExternalRepo: api.ExternalRepoSpec{
			ID:          string(repoName),
			ServiceType: "other",
			ServiceID:   serviceID,
		},
		Enabled: true,
		Sources: map[string]*SourceInfo{
			urn: {
				ID:       urn,
				CloneURL: repoURL,
			},
		},
	}
}
