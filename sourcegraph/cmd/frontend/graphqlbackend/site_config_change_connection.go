package graphqlbackend

import (
	"context"
	"strconv"

	"github.com/graph-gophers/graphql-go"
	"github.com/graph-gophers/graphql-go/relay"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

type SiteConfigurationChangeConnectionStore struct {
	db database.DB
}

func (s *SiteConfigurationChangeConnectionStore) ComputeTotal(ctx context.Context) (*int32, error) {
	count, err := s.db.Conf().GetSiteConfigCount(ctx)
	c := int32(count)
	return &c, err
}

func (s *SiteConfigurationChangeConnectionStore) ComputeNodes(ctx context.Context, args *database.PaginationArgs) ([]*SiteConfigurationChangeResolver, error) {
	if args == nil {
		return nil, errors.New("pagination args cannot be nil")
	}

	// NOTE: Do not modify "args" in-place because it is used by the caller of ComputeNodes to
	// determine next/previous page. Instead, dereference the values from args first (if
	// they're non-nil) and then assign them address of the new variables.
	paginationArgs := args.Clone()
	isModifiedPaginationArgs, err := modifyArgs(paginationArgs)
	if err != nil {
		return []*SiteConfigurationChangeResolver{}, err
	}

	history, err := s.db.Conf().ListSiteConfigs(ctx, paginationArgs)
	if err != nil {
		return []*SiteConfigurationChangeResolver{}, err
	}

	totalFetched := len(history)
	if totalFetched == 0 {
		return []*SiteConfigurationChangeResolver{}, nil
	}

	resolvers := []*SiteConfigurationChangeResolver{}
	if paginationArgs.First != nil {
		resolvers = generateResolversForFirst(history, s.db)
	} else if paginationArgs.Last != nil {
		resolvers = generateResolversForLast(history, s.db)
	}

	if isModifiedPaginationArgs {
		if paginationArgs.Last != nil {
			resolvers = resolvers[1:]
		} else if paginationArgs.First != nil && totalFetched == *paginationArgs.First {
			resolvers = resolvers[:len(resolvers)-1]
		}
	}

	return resolvers, nil
}

func (s *SiteConfigurationChangeConnectionStore) MarshalCursor(node *SiteConfigurationChangeResolver, _ database.OrderBy) (*string, error) {
	cursor := string(node.ID())
	return &cursor, nil
}

func (s *SiteConfigurationChangeConnectionStore) UnmarshalCursor(cursor string, _ database.OrderBy) (*string, error) {
	var id int
	err := relay.UnmarshalSpec(graphql.ID(cursor), &id)
	if err != nil {
		return nil, err
	}

	idStr := strconv.Itoa(id)
	return &idStr, err
}

// modifyArgs will fetch one more than the originally requested number of items because we need one
// older item to get the diff of the oldes item in the list.
//
// A separate function so that this can be tested in isolation.
func modifyArgs(args *database.PaginationArgs) (bool, error) {
	var modified bool
	if args.First != nil {
		*args.First += 1
		modified = true
	} else if args.Last != nil && args.Before != nil {
		before, err := strconv.Atoi(*args.Before)
		if err != nil {
			return false, err
		}

		if before > 0 {
			modified = true
			*args.Last += 1
			*args.Before = strconv.Itoa(before - 1)
		}
	}

	return modified, nil
}

func generateResolversForFirst(history []*database.SiteConfig, db database.DB) []*SiteConfigurationChangeResolver {
	// If First is used then "history" is in descending order: 5, 4, 3, 2, 1. So look ahead for
	// the "previousSiteConfig", but also only if we're not at the end of the slice yet.
	//
	// "previousSiteConfig" for the last item in "history" will be nil and that is okay, because
	// we will truncate it from the end result being returned. The user did not request this.
	// _We_ fetched an extra item to determine the "previousSiteConfig" of all the items.
	resolvers := []*SiteConfigurationChangeResolver{}
	totalFetched := len(history)

	for i := 0; i < totalFetched; i++ {
		var previousSiteConfig *database.SiteConfig
		if i < totalFetched-1 {
			previousSiteConfig = history[i+1]
		}

		resolvers = append(resolvers, &SiteConfigurationChangeResolver{
			db:                 db,
			siteConfig:         history[i],
			previousSiteConfig: previousSiteConfig,
		})
	}

	return resolvers
}

func generateResolversForLast(history []*database.SiteConfig, db database.DB) []*SiteConfigurationChangeResolver {
	// If Last is used then history is in ascending order: 1, 2, 3, 4, 5. So look behind for the
	// "previousSiteConfig", but also only if we're not at the start of the slice.
	//
	// "previousSiteConfig" will be nil for the first item in history in this case and that is okay,
	// because we will truncate it from the end result being returned. The user did not request
	// this. _We_ fetched an extra item to determine the "previousSiteConfig" of all the items.
	resolvers := []*SiteConfigurationChangeResolver{}
	totalFetched := len(history)

	for i := 0; i < totalFetched; i++ {
		var previousSiteConfig *database.SiteConfig
		if i > 0 {
			previousSiteConfig = history[i-1]
		}

		resolvers = append(resolvers, &SiteConfigurationChangeResolver{
			db:                 db,
			siteConfig:         history[i],
			previousSiteConfig: previousSiteConfig,
		})
	}

	return resolvers
}
