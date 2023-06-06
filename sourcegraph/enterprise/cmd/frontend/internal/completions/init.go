package completions

import (
	"context"
	"net/http"

	"github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/enterprise"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/frontend/internal/completions/resolvers"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/cody"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/completions/httpapi"
	"github.com/sourcegraph/sourcegraph/internal/conf/conftypes"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/observation"
)

func Init(
	_ context.Context,
	observationCtx *observation.Context,
	db database.DB,
	_ codeintel.Services,
	_ conftypes.UnifiedWatchable,
	enterpriseServices *enterprise.Services,
) error {
	logger := log.Scoped("completions", "Cody completions")

	enterpriseServices.NewChatCompletionsStreamHandler = func() http.Handler {
		completionsHandler := httpapi.NewChatCompletionsStreamHandler(logger.Scoped("chat", "chat completions handler"), db)
		return requireVerifiedEmailMiddleware(db, observationCtx.Logger, completionsHandler)
	}
	enterpriseServices.NewCodeCompletionsHandler = func() http.Handler {
		codeCompletionsHandler := httpapi.NewCodeCompletionsHandler(logger.Scoped("code", "code completions handler"), db)
		return requireVerifiedEmailMiddleware(db, observationCtx.Logger, codeCompletionsHandler)
	}
	enterpriseServices.CompletionsResolver = resolvers.NewCompletionsResolver(db, observationCtx.Logger)

	return nil
}

func requireVerifiedEmailMiddleware(db database.DB, logger log.Logger, next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if err := cody.CheckVerifiedEmailRequirement(r.Context(), db, logger); err != nil {
			// Report HTTP 403 Forbidden if user has no verified email address.
			http.Error(w, err.Error(), http.StatusForbidden)
			return
		}

		next.ServeHTTP(w, r)
	})
}
