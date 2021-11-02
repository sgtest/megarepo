package batches

import (
	"context"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/enterprise"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/frontend/internal/batches/migrations"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/frontend/internal/batches/resolvers"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/frontend/internal/batches/webhooks"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/batches/store"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/batches/types/scheduler/window"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/encryption/keyring"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/oobmigration"
)

// Init initializes the given enterpriseServices to include the required
// resolvers for Batch Changes and sets up webhook handlers for changeset
// events.
func Init(ctx context.Context, db database.DB, outOfBandMigrationRunner *oobmigration.Runner, enterpriseServices *enterprise.Services, observationContext *observation.Context) error {
	// Validate site configuration.
	conf.ContributeValidator(func(c conf.Unified) (problems conf.Problems) {
		if _, err := window.NewConfiguration(c.BatchChangesRolloutWindows); err != nil {
			problems = append(problems, conf.NewSiteProblem(err.Error()))
		}

		return
	})

	// Initialize store.
	cstore := store.New(db, observationContext, keyring.Default().BatchChangesCredentialKey)

	// Register enterprise services.
	enterpriseServices.BatchChangesResolver = resolvers.New(cstore)
	enterpriseServices.GitHubWebhook = webhooks.NewGitHubWebhook(cstore)
	enterpriseServices.BitbucketServerWebhook = webhooks.NewBitbucketServerWebhook(cstore)
	enterpriseServices.GitLabWebhook = webhooks.NewGitLabWebhook(cstore)

	// Register Batch Changes OOB migrations.
	return migrations.Register(cstore, outOfBandMigrationRunner)
}
