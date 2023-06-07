package repos

import (
	"context"
	"os/exec"
	"path/filepath"
	"strings"

	homedir "github.com/mitchellh/go-homedir"
	"github.com/sourcegraph/log"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/jsonc"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/lib/errors"
	"github.com/sourcegraph/sourcegraph/schema"
)

type LocalRepoMetadata struct {
	AbsPath string
}

// LocalGitSource connects to a local code host.
type LocalGitSource struct {
	svc    *types.ExternalService
	config *schema.LocalGitExternalService
	logger log.Logger
}

func NewLocalGitSource(ctx context.Context, logger log.Logger, svc *types.ExternalService) (*LocalGitSource, error) {
	rawConfig, err := svc.Config.Decrypt(ctx)
	if err != nil {
		return nil, errors.Errorf("external service id=%d config error: %s", svc.ID, err)
	}
	var config schema.LocalGitExternalService
	if err := jsonc.Unmarshal(rawConfig, &config); err != nil {
		return nil, errors.Errorf("external service id=%d config error: %s", svc.ID, err)
	}

	return &LocalGitSource{
		svc:    svc,
		config: &config,
		logger: logger,
	}, nil
}

func (s *LocalGitSource) CheckConnection(ctx context.Context) error {
	return nil
}

func (s *LocalGitSource) ExternalServices() types.ExternalServices {
	return types.ExternalServices{s.svc}
}

func (s *LocalGitSource) ListRepos(ctx context.Context, results chan SourceResult) {
	urn := s.svc.URN()
	repoPaths := getRepoPaths(s.config, s.logger)
	for _, r := range repoPaths {
		uri := "file://" + r.Path
		s.logger.Info("found repo ", log.String("uri", uri))
		results <- SourceResult{
			Source: s,
			Repo: &types.Repo{
				Name: r.fullName(),
				URI:  uri,
				ExternalRepo: api.ExternalRepoSpec{
					ID:          uri,
					ServiceType: extsvc.VariantLocalGit.AsType(),
					ServiceID:   uri,
				},
				Sources: map[string]*types.SourceInfo{
					urn: {
						ID:       urn,
						CloneURL: uri,
					},
				},
				// Looks like this needs to be convertible to a JSON object
				Metadata: struct{}{},
			},
		}
	}
}

// Checks if git thinks the given path is a valid .git folder for a repository
func isBareRepo(path string) bool {
	c := exec.Command("git", "-C", path, "rev-parse", "--is-bare-repository")
	out, err := c.CombinedOutput()

	if err != nil {
		return false
	}

	return strings.TrimSpace(string(out)) != "false"
}

// Check if git thinks the given path is a proper git checkout
func isGitRepo(path string) bool {
	// Executing git rev-parse --git-dir in the root of a worktree returns .git
	c := exec.Command("git", "-C", path, "rev-parse")
	err := c.Run()
	return err == nil
}

type repoConfig struct {
	Path  string
	Group string
}

func (c repoConfig) fullName() api.RepoName {
	name := ""
	if c.Group != "" {
		name = c.Group + "/"
	}
	name += strings.TrimSuffix(filepath.Base(c.Path), ".git")
	return api.RepoName(name)
}

func getRepoPaths(config *schema.LocalGitExternalService, logger log.Logger) []repoConfig {
	paths := []repoConfig{}
	for _, pathConfig := range config.Repos {
		pattern, err := homedir.Expand(pathConfig.Pattern)
		if err != nil {
			logger.Error("unable to resolve home directory", log.String("pattern", pattern), log.Error(err))
			continue
		}
		matches, err := filepath.Glob(pattern)
		if err != nil {
			logger.Error("unable to resolve glob pattern", log.String("pattern", pattern), log.Error(err))
			continue
		}

		for _, match := range matches {
			if isGitRepo(match) {
				paths = append(paths, repoConfig{Path: match, Group: pathConfig.Group})
			} else {
				logger.Info("path matches glob pattern but is not a git repository", log.String("pattern", pattern), log.String("path", match))
			}
		}
	}

	return paths
}
