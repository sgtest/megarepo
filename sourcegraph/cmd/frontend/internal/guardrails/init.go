package guardrails

import (
	"context"
	"sync"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/enterprise"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/envvar"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/guardrails/attribution"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/guardrails/resolvers"
	"github.com/sourcegraph/sourcegraph/internal/codeintel"
	"github.com/sourcegraph/sourcegraph/internal/codygateway"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/conf/conftypes"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/gitserver"
	"github.com/sourcegraph/sourcegraph/internal/httpcli"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/search/client"
)

// MockHttpClient is used to inject a test double, since the external client used
// by default is prevented from hitting localhost, which is useful in testing.
var MockHttpClient httpcli.Doer

func Init(
	_ context.Context,
	observationCtx *observation.Context,
	db database.DB,
	_ codeintel.Services,
	_ conftypes.UnifiedWatchable,
	enterpriseServices *enterprise.Services,
) error {
	var resolver *resolvers.GuardrailsResolver
	if envvar.SourcegraphDotComMode() {
		// On DotCom guardrails endpoint runs search, and is initialized at startup.
		searchClient := client.New(observationCtx.Logger, db, gitserver.NewClient("http.guardrails.search"))
		service := attribution.NewLocalSearch(observationCtx, searchClient)
		resolver = resolvers.NewGuardrailsResolver(service)
	} else {
		// On an Enterprise instance endpoint proxies to gateway, and is re-initialized
		// in case site-config changes.
		initLogic := &enterpriseInitialization{observationCtx: observationCtx}
		resolver = resolvers.NewGuardrailsResolver(initLogic.Service())
		go conf.Watch(func() {
			resolver.UpdateService(initLogic.Service())
		})
	}
	enterpriseServices.GuardrailsResolver = resolver
	return nil
}

// enterpriseInitialization is a factory for attribution.Service for an enterprise instance
// as opposed to dotcom.
type enterpriseInitialization struct {
	observationCtx *observation.Context
	mu             sync.Mutex
	client         codygateway.Client
	endpoint       string
	token          string
}

// Service creates an attribution.Service. It tries to get gateway endpoint from site config
// and if possible, returns a configured gateway proxy implementation.
// Otherwise it returns an uninitialized service that always returns an error.
func (e *enterpriseInitialization) Service() attribution.Service {
	e.mu.Lock()
	defer e.mu.Unlock()
	config := conf.Get().SiteConfig()
	endpoint, token := conf.GetAttributionGateway(config)
	if e.endpoint != endpoint || e.token != token {
		e.endpoint = endpoint
		e.token = token

		// We communicate out of the cluster so we need to use ExternalDoer.
		httpClient := httpcli.ExternalDoer
		if MockHttpClient != nil {
			httpClient = MockHttpClient
		}

		e.client = codygateway.NewClient(httpClient, endpoint, token)
	}
	if e.endpoint == "" || e.token == "" {
		return attribution.Uninitialized{}
	}
	if e.client == nil {
		return attribution.Uninitialized{}
	}
	return attribution.NewGatewayProxy(e.observationCtx, e.client)
}

func alwaysAllowed(context.Context, string) (bool, error) {
	return true, nil
}

func NewAttributionTest(observationCtx *observation.Context) func(context.Context, string) (bool, error) {
	// Attribution is only-enterprise, dotcom lets everything through.
	if envvar.SourcegraphDotComMode() {
		return alwaysAllowed
	}
	initLogic := &enterpriseInitialization{observationCtx: observationCtx}
	return func(ctx context.Context, snippet string) (bool, error) {
		// Check if attribution is on, permit everything if it's off.
		c := conf.GetConfigFeatures(conf.Get().SiteConfig())
		if !c.Attribution {
			return true, nil
		}
		// Attribution not available. Mode is permissive.
		attribution, err := initLogic.Service().SnippetAttribution(ctx, snippet, 1)
		// Attribution not available. Mode is permissive.
		if err != nil {
			return true, err
		}
		// Permit completion if no attribution found.
		return len(attribution.RepositoryNames) == 0, nil
	}
}
