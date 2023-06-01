package bitbucketcloudoauth

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
	const pkgName = "bitbucketcloudoauth"
	logger = logger.Scoped(pkgName, "Bitbucket Cloud OAuth config watch")
	conf.ContributeValidator(func(cfg conftypes.SiteConfigQuerier) conf.Problems {
		_, problems := parseConfig(logger, cfg, db)
		return problems
	})

	go conf.Watch(func() {
		newProviders, _ := parseConfig(logger, conf.Get(), db)
		if len(newProviders) == 0 {
			providers.Update(pkgName, nil)
			return
		}

		if err := licensing.Check(licensing.FeatureSSO); err != nil {
			logger.Error("Check license for SSO (Bitbucket Cloud OAuth)", log.Error(err))
			providers.Update(pkgName, nil)
			return
		}

		newProvidersList := make([]providers.Provider, 0, len(newProviders))
		for _, p := range newProviders {
			newProvidersList = append(newProvidersList, p.Provider)
		}
		providers.Update(pkgName, newProvidersList)
	})
}

type Provider struct {
	*schema.BitbucketCloudAuthProvider
	providers.Provider
}

func parseConfig(logger log.Logger, cfg conftypes.SiteConfigQuerier, db database.DB) (ps []Provider, problems conf.Problems) {
	configured := map[string]struct{}{}
	for _, pr := range cfg.SiteConfig().AuthProviders {
		if pr.Bitbucketcloud == nil {
			continue
		}

		provider, providerProblems := parseProvider(logger, pr.Bitbucketcloud, db, pr)
		problems = append(problems, conf.NewSiteProblems(providerProblems...)...)
		if provider == nil {
			continue
		}

		if _, ok := configured[provider.ServiceID]; ok {
			problems = append(problems, conf.NewSiteProblems(fmt.Sprintf(`Cannot have more than one auth provider with url %q, only the first one will be used`, provider.ServiceID))...)
			continue
		}

		ps = append(ps, Provider{
			BitbucketCloudAuthProvider: pr.Bitbucketcloud,
			Provider:                   provider,
		})
		configured[provider.ServiceID] = struct{}{}
	}
	return ps, problems
}

func getStateConfig() gologin.CookieConfig {
	cfg := gologin.CookieConfig{
		Name:     "bitbucketcloud-state-cookie",
		Path:     "/",
		MaxAge:   900, // 15 minutes
		HTTPOnly: true,
		Secure:   conf.IsExternalURLSecure(),
	}
	return cfg
}
