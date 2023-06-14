package resolvers

import (
	"context"

	"github.com/sourcegraph/sourcegraph/enterprise/internal/own"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/own/types"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/lib/errors"

	edb "github.com/sourcegraph/sourcegraph/enterprise/internal/database"
)

func computeRecentContributorSignals(ctx context.Context, db edb.EnterpriseDB, path string, repoID api.RepoID) ([]reasonAndReference, error) {
	enabled, err := db.OwnSignalConfigurations().IsEnabled(ctx, types.SignalRecentContributors)
	if err != nil {
		return nil, errors.Wrap(err, "IsEnabled")
	}
	if !enabled {
		return nil, nil
	}

	recentAuthors, err := db.RecentContributionSignals().FindRecentAuthors(ctx, repoID, path)
	if err != nil {
		return nil, errors.Wrap(err, "FindRecentAuthors")
	}

	var rrs []reasonAndReference
	for _, a := range recentAuthors {
		rrs = append(rrs, reasonAndReference{
			reason: ownershipReason{recentContributionsCount: a.ContributionCount},
			reference: own.Reference{
				// Just use the email.
				Email: a.AuthorEmail,
			},
		})
	}
	return rrs, nil
}

type recentContributorOwnershipSignal struct {
	total int32
}

func (g *recentContributorOwnershipSignal) Title() (string, error) {
	return "recent contributor", nil
}

func (g *recentContributorOwnershipSignal) Description() (string, error) {
	return "Owner is associated because they have contributed to this file in the last 90 days.", nil
}
