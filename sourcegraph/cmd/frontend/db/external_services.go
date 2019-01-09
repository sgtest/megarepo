package db

import (
	"context"
	"database/sql"
	"encoding/json"
	"errors"
	"fmt"
	"strings"
	"sync"
	"time"

	"github.com/keegancsmith/sqlf"
	"github.com/lib/pq"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/types"
	"github.com/sourcegraph/sourcegraph/pkg/db/dbconn"
	"github.com/sourcegraph/sourcegraph/pkg/db/dbutil"
	"github.com/sourcegraph/sourcegraph/pkg/jsonc"
	"github.com/sourcegraph/sourcegraph/pkg/legacyconf"
	"github.com/sourcegraph/sourcegraph/schema"
	log15 "gopkg.in/inconshreveable/log15.v2"
)

type externalServices struct {
	GitHubValidators []func(cfg *schema.GitHubConnection) error
	GitLabValidators []func(cfg *schema.GitLabConnection) error
}

// ExternalServicesListOptions contains options for listing external services.
type ExternalServicesListOptions struct {
	Kinds []string
	*LimitOffset
}

func (o ExternalServicesListOptions) sqlConditions() []*sqlf.Query {
	conds := []*sqlf.Query{sqlf.Sprintf("deleted_at IS NULL")}
	if len(o.Kinds) > 0 {
		kinds := []*sqlf.Query{}
		for _, kind := range o.Kinds {
			kinds = append(kinds, sqlf.Sprintf("%s", kind))
		}
		conds = append(conds, sqlf.Sprintf("kind IN (%s)", sqlf.Join(kinds, ", ")))
	}
	return conds
}

func (c *externalServices) validateConfig(kind, config string) error {
	// All configs must be valid JSON.
	// If this requirement is ever changed, you will need to update
	// serveExternalServiceConfigs to handle this case.

	switch kind {
	case "PHABRICATOR":
		var cfg schema.PhabricatorConnection
		if err := jsonc.Unmarshal(config, &cfg); err != nil {
			return err
		}
		if len(cfg.Repos) == 0 && cfg.Token == "" {
			return errors.New(`Either "token" or "repos" must be set`)
		}
	case "BITBUCKETSERVER":
		var cfg schema.BitbucketServerConnection
		if err := jsonc.Unmarshal(config, &cfg); err != nil {
			return err
		}
		if cfg.Token != "" && cfg.Password != "" {
			return errors.New("Specify either a token or a username/password, but not both")
		} else if cfg.Token == "" && cfg.Username == "" && cfg.Password == "" {
			return errors.New("Specify either a token or a username/password to authenticate")
		}
	case "GITHUB":
		var cfg schema.GitHubConnection
		if err := jsonc.Unmarshal(config, &cfg); err != nil {
			return err
		}

		for _, validate := range c.GitHubValidators {
			if err := validate(&cfg); err != nil {
				return err
			}
		}
	case "GITLAB":
		var cfg schema.GitLabConnection
		if err := jsonc.Unmarshal(config, &cfg); err != nil {
			return err
		}
		if strings.Contains(cfg.Url, "example.com") {
			return fmt.Errorf(`invalid GitLab URL detected: %s (did you forget to remove "example.com"?)`, cfg.Url)
		}

		for _, validate := range c.GitLabValidators {
			if err := validate(&cfg); err != nil {
				return err
			}
		}
	default:
		if _, err := jsonc.Parse(config); err != nil {
			return err
		}
	}
	return nil
}

// Create creates a external service.
//
// 🚨 SECURITY: The caller must ensure that the actor is a site admin.
func (c *externalServices) Create(ctx context.Context, externalService *types.ExternalService) error {
	if err := c.validateConfig(externalService.Kind, externalService.Config); err != nil {
		return err
	}

	externalService.CreatedAt = time.Now()
	externalService.UpdatedAt = externalService.CreatedAt

	return dbconn.Global.QueryRowContext(
		ctx,
		"INSERT INTO external_services(kind, display_name, config, created_at, updated_at) VALUES($1, $2, $3, $4, $5) RETURNING id",
		externalService.Kind, externalService.DisplayName, externalService.Config, externalService.CreatedAt, externalService.UpdatedAt,
	).Scan(&externalService.ID)
}

// ExternalServiceUpdate contains optional fields to update.
type ExternalServiceUpdate struct {
	DisplayName *string
	Config      *string
}

// Update updates a external service.
//
// 🚨 SECURITY: The caller must ensure that the actor is a site admin.
func (c *externalServices) Update(ctx context.Context, id int64, update *ExternalServiceUpdate) error {
	if update.Config != nil {
		// Query to get the kind (which is immutable) so we can validate the new config.
		externalService, err := c.GetByID(ctx, id)
		if err != nil {
			return err
		}

		if err := c.validateConfig(externalService.Kind, *update.Config); err != nil {
			return err
		}
	}

	execUpdate := func(ctx context.Context, tx *sql.Tx, update *sqlf.Query) error {
		q := sqlf.Sprintf("UPDATE external_services SET %s, updated_at=now() WHERE id=%d AND deleted_at IS NULL", update, id)
		res, err := tx.ExecContext(ctx, q.Query(sqlf.PostgresBindVar), q.Args()...)
		if err != nil {
			return err
		}
		affected, err := res.RowsAffected()
		if err != nil {
			return err
		}
		if affected == 0 {
			return externalServiceNotFoundError{id: id}
		}
		return nil
	}
	return dbutil.Transaction(ctx, dbconn.Global, func(tx *sql.Tx) error {
		if update.DisplayName != nil {
			if err := execUpdate(ctx, tx, sqlf.Sprintf("display_name=%s", update.DisplayName)); err != nil {
				return err
			}
		}
		if update.Config != nil {
			if err := execUpdate(ctx, tx, sqlf.Sprintf("config=%s", update.Config)); err != nil {
				return err
			}
		}
		return nil
	})
}

type externalServiceNotFoundError struct {
	id int64
}

func (e externalServiceNotFoundError) Error() string {
	return fmt.Sprintf("external service not found: %v", e.id)
}

func (e externalServiceNotFoundError) NotFound() bool {
	return true
}

// Delete deletes an external service.
//
// 🚨 SECURITY: The caller must ensure that the actor is a site admin.
func (*externalServices) Delete(ctx context.Context, id int64) error {
	res, err := dbconn.Global.ExecContext(ctx, "UPDATE external_services SET deleted_at=now() WHERE id=$1 AND deleted_at IS NULL", id)
	if err != nil {
		return err
	}
	nrows, err := res.RowsAffected()
	if err != nil {
		return err
	}
	if nrows == 0 {
		return externalServiceNotFoundError{id: id}
	}
	return nil
}

// GetByID returns the external service for id.
//
// 🚨 SECURITY: The caller must ensure that the actor is a site admin.
func (c *externalServices) GetByID(ctx context.Context, id int64) (*types.ExternalService, error) {
	if Mocks.ExternalServices.GetByID != nil {
		return Mocks.ExternalServices.GetByID(id)
	}

	conds := []*sqlf.Query{sqlf.Sprintf("id=%d", id)}
	externalServices, err := c.list(ctx, conds, nil)
	if err != nil {
		return nil, err
	}
	if len(externalServices) == 0 {
		return nil, fmt.Errorf("external service not found: id=%d", id)
	}
	return externalServices[0], nil
}

// List returns all external services.
//
// 🚨 SECURITY: The caller must ensure that the actor is a site admin.
func (c *externalServices) List(ctx context.Context, opt ExternalServicesListOptions) ([]*types.ExternalService, error) {
	if Mocks.ExternalServices.List != nil {
		return Mocks.ExternalServices.List(opt)
	}
	return c.list(ctx, opt.sqlConditions(), opt.LimitOffset)
}

// listConfigs decodes the list configs into result.
//
// 🚨 SECURITY: The caller must ensure that the actor is a site admin.
func (c *externalServices) listConfigs(ctx context.Context, kind string, result interface{}) error {
	services, err := c.List(ctx, ExternalServicesListOptions{Kinds: []string{kind}})
	if err != nil {
		return err
	}

	// Decode the jsonc configs into Go objects.
	var cfgs []interface{}
	for _, service := range services {
		var cfg interface{}
		if err := jsonc.Unmarshal(service.Config, &cfg); err != nil {
			return err
		}
		cfgs = append(cfgs, cfg)
	}

	// Now move our untyped config list into the typed list (result). We could
	// do this using reflection, but JSON marshaling and unmarshaling is easier
	// and fast enough for our purposes. Note that service.Config is jsonc, not
	// plain JSON so we could not simply treat it as json.RawMessage.
	buf, err := json.Marshal(cfgs)
	if err != nil {
		return err
	}
	return json.Unmarshal(buf, result)
}

// ListAWSCodeCommitConnections returns a list of AWSCodeCommit configs.
//
// 🚨 SECURITY: The caller must ensure that the actor is a site admin.
func (c *externalServices) ListAWSCodeCommitConnections(ctx context.Context) ([]*schema.AWSCodeCommitConnection, error) {
	var connections []*schema.AWSCodeCommitConnection
	if err := c.listConfigs(ctx, "AWSCODECOMMIT", &connections); err != nil {
		return nil, err
	}
	return connections, nil
}

// ListBitbucketServerConnections returns a list of BitbucketServer configs.
//
// 🚨 SECURITY: The caller must ensure that the actor is a site admin.
func (c *externalServices) ListBitbucketServerConnections(ctx context.Context) ([]*schema.BitbucketServerConnection, error) {
	var connections []*schema.BitbucketServerConnection
	if err := c.listConfigs(ctx, "BITBUCKET", &connections); err != nil {
		return nil, err
	}
	return connections, nil
}

// ListGitHubConnections returns a list of GitHubConnection configs.
//
// 🚨 SECURITY: The caller must ensure that the actor is a site admin.
func (c *externalServices) ListGitHubConnections(ctx context.Context) ([]*schema.GitHubConnection, error) {
	var connections []*schema.GitHubConnection
	if err := c.listConfigs(ctx, "GITHUB", &connections); err != nil {
		return nil, err
	}
	return connections, nil
}

// ListGitLabConnections returns a list of GitLabConnection configs.
//
// 🚨 SECURITY: The caller must ensure that the actor is a site admin.
func (c *externalServices) ListGitLabConnections(ctx context.Context) ([]*schema.GitLabConnection, error) {
	var connections []*schema.GitLabConnection
	if err := c.listConfigs(ctx, "GITLAB", &connections); err != nil {
		return nil, err
	}
	return connections, nil
}

// ListGitoliteConnections returns a list of GitoliteConnection configs.
//
// 🚨 SECURITY: The caller must ensure that the actor is a site admin.
func (c *externalServices) ListGitoliteConnections(ctx context.Context) ([]*schema.GitoliteConnection, error) {
	var connections []*schema.GitoliteConnection
	if err := c.listConfigs(ctx, "GITOLITE", &connections); err != nil {
		return nil, err
	}
	return connections, nil
}

// ListPhabricatorConnections returns a list of PhabricatorConnection configs.
//
// 🚨 SECURITY: The caller must ensure that the actor is a site admin.
func (c *externalServices) ListPhabricatorConnections(ctx context.Context) ([]*schema.PhabricatorConnection, error) {
	var connections []*schema.PhabricatorConnection
	if err := c.listConfigs(ctx, "PHABRICATOR", &connections); err != nil {
		return nil, err
	}
	return connections, nil
}

// migrateOnce ensures that the migration is only attempted
// once per frontend instance (to avoid unnecessary queries).
var migrateOnce sync.Once

// migrateJsonConfigToExternalServices performs a one time migration to populate
// the new external_services database table with relavant entries in the site config.
// It is idempotent.
//
// This migration can be deleted as soon as (whichever happens first):
//   - All customers have updated to 3.0 or newer.
//   - 3 months after 3.0 is released.
func (c *externalServices) migrateJsonConfigToExternalServices(ctx context.Context) {
	migrateOnce.Do(func() {
		// Run in a transaction because we are racing with other frontend replicas.
		err := dbutil.Transaction(ctx, dbconn.Global, func(tx *sql.Tx) error {
			now := time.Now()

			// Attempt to insert a fake config into the DB with id 0.
			// This will fail if the migration has already run.
			if _, err := tx.ExecContext(
				ctx,
				"INSERT INTO external_services(id, kind, display_name, config, created_at, updated_at, deleted_at) VALUES($1, $2, $3, $4, $5, $6, $7)",
				0, "migration", "", "{}", now, now, now,
			); err != nil {
				return err
			}

			migrate := func(config interface{}, name string) error {
				// Marshaling and unmarshaling is a lazy way to get around
				// Go's lack of covariance for slice types.
				buf, err := json.Marshal(config)
				if err != nil {
					return err
				}
				var configs []interface{}
				if err := json.Unmarshal(buf, &configs); err != nil {
					return nil
				}

				for i, config := range configs {
					jsonConfig, err := json.MarshalIndent(config, "", "  ")
					if err != nil {
						return err
					}

					kind := strings.ToUpper(name)
					displayName := fmt.Sprintf("Migrated %s %d", name, i+1)
					if _, err := tx.ExecContext(
						ctx,
						"INSERT INTO external_services(kind, display_name, config, created_at, updated_at) VALUES($1, $2, $3, $4, $5)",
						kind, displayName, string(jsonConfig), now, now,
					); err != nil {
						return err
					}
				}
				return nil
			}

			var legacyConfig struct {
				AwsCodeCommit   []*schema.AWSCodeCommitConnection   `json:"awsCodeCommit"`
				BitbucketServer []*schema.BitbucketServerConnection `json:"bitbucketServer"`
				Github          []*schema.GitHubConnection          `json:"github"`
				Gitlab          []*schema.GitLabConnection          `json:"gitlab"`
				Gitolite        []*schema.GitoliteConnection        `json:"gitolite"`
				Phabricator     []*schema.PhabricatorConnection     `json:"phabricator"`
			}
			if err := jsonc.Unmarshal(legacyconf.Raw(), &legacyConfig); err != nil {
				return err
			}
			if err := migrate(legacyConfig.AwsCodeCommit, "AWSCodeCommit"); err != nil {
				return err
			}

			if err := migrate(legacyConfig.BitbucketServer, "BitbucketServer"); err != nil {
				return err
			}

			if err := migrate(legacyConfig.Github, "GitHub"); err != nil {
				return err
			}

			if err := migrate(legacyConfig.Gitlab, "GitLab"); err != nil {
				return err
			}

			if err := migrate(legacyConfig.Gitolite, "Gitolite"); err != nil {
				return err
			}

			if err := migrate(legacyConfig.Phabricator, "Phabricator"); err != nil {
				return err
			}

			return nil
		})

		if err != nil {
			if pqErr, ok := err.(*pq.Error); ok {
				if pqErr.Constraint == "external_services_pkey" {
					// This is expected when multiple frontend attempt to migrate concurrently.
					// Only one will win.
					return
				}
			}
			log15.Error("migrate transaction failed", "err", err)
		}
	})
}

func (c *externalServices) list(ctx context.Context, conds []*sqlf.Query, limitOffset *LimitOffset) ([]*types.ExternalService, error) {
	c.migrateJsonConfigToExternalServices(ctx)
	q := sqlf.Sprintf(`
		SELECT id, kind, display_name, config, created_at, updated_at
		FROM external_services
		WHERE (%s)
		ORDER BY id DESC
		%s`,
		sqlf.Join(conds, ") AND ("),
		limitOffset.SQL(),
	)

	rows, err := dbconn.Global.QueryContext(ctx, q.Query(sqlf.PostgresBindVar), q.Args()...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var results []*types.ExternalService
	for rows.Next() {
		var h types.ExternalService
		if err := rows.Scan(&h.ID, &h.Kind, &h.DisplayName, &h.Config, &h.CreatedAt, &h.UpdatedAt); err != nil {
			return nil, err
		}
		results = append(results, &h)
	}
	return results, nil
}

// Count counts all external services that satisfy the options (ignoring limit and offset).
//
// 🚨 SECURITY: The caller must ensure that the actor is a site admin.
func (c *externalServices) Count(ctx context.Context, opt ExternalServicesListOptions) (int, error) {
	q := sqlf.Sprintf("SELECT COUNT(*) FROM external_services WHERE (%s)", sqlf.Join(opt.sqlConditions(), ") AND ("))
	var count int
	if err := dbconn.Global.QueryRowContext(ctx, q.Query(sqlf.PostgresBindVar), q.Args()...).Scan(&count); err != nil {
		return 0, err
	}
	return count, nil
}

// MockExternalServices mocks the external services store.
type MockExternalServices struct {
	GetByID func(id int64) (*types.ExternalService, error)
	List    func(opt ExternalServicesListOptions) ([]*types.ExternalService, error)
}
