package githuboauth

import (
	"fmt"

	"github.com/dghubble/gologin"
	"github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/enterprise/internal/licensing"
	"github.com/sourcegraph/sourcegraph/internal/auth/providers"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/conf/conftypes"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/schema"
)

func Init(logger log.Logger, db database.DB) {
	const pkgName = "githuboauth"
	logger = logger.Scoped(pkgName, "GitHub OAuth config watch")
	conf.ContributeValidator(func(cfg conftypes.SiteConfigQuerier) conf.Problems {
		_, problems := parseConfig(logger, cfg, db)
		return problems
	})

	go func() {
		conf.Watch(func() {
			newProviders, _ := parseConfig(logger, conf.Get(), db)
			if len(newProviders) == 0 {
				providers.Update(pkgName, nil)
				return
			}

			if err := licensing.Check(licensing.FeatureSSO); err != nil {
				logger.Error("Check license for SSO (GitHub OAuth)", log.Error(err))
				providers.Update(pkgName, nil)
				return
			}

			newProvidersList := make([]providers.Provider, 0, len(newProviders))
			for _, p := range newProviders {
				newProvidersList = append(newProvidersList, p.Provider)
			}
			providers.Update(pkgName, newProvidersList)
		})
	}()
}

type Provider struct {
	*schema.GitHubAuthProvider
	providers.Provider
}

func parseConfig(logger log.Logger, cfg conftypes.SiteConfigQuerier, db database.DB) (ps []Provider, problems conf.Problems) {
	for _, pr := range cfg.SiteConfig().AuthProviders {
		if pr.Github == nil {
			continue
		}

		provider, providerProblems := parseProvider(logger, pr.Github, db, pr)
		problems = append(problems, conf.NewSiteProblems(providerProblems...)...)
		if provider != nil {
			alreadyExists := false
			for _, p := range ps {
				if p.CachedInfo().ServiceID == provider.ServiceID {
					problems = append(problems, conf.NewSiteProblems(fmt.Sprintf(`Cannot have more than one auth provider with url %q, only the first one will be used`, provider.ServiceID))...)
					alreadyExists = true
				}
			}
			if alreadyExists {
				continue
			}
			ps = append(ps, Provider{
				GitHubAuthProvider: pr.Github,
				Provider:           provider,
			})
		}
	}
	return ps, problems
}

func getStateConfig() gologin.CookieConfig {
	cfg := gologin.CookieConfig{
		Name:     "github-state-cookie",
		Path:     "/",
		MaxAge:   900, // 15 minutes
		HTTPOnly: true,
		Secure:   conf.IsExternalURLSecure(),
	}
	return cfg
}
