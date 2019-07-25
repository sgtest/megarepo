package db

import (
	"context"
	"sync"

	"github.com/RoaringBitmap/roaring"
	otlog "github.com/opentracing/opentracing-go/log"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/authz"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/types"
	"github.com/sourcegraph/sourcegraph/pkg/actor"
	"github.com/sourcegraph/sourcegraph/pkg/extsvc"
	"github.com/sourcegraph/sourcegraph/pkg/trace"
	"gopkg.in/inconshreveable/log15.v2"
)

var MockAuthzFilter func(ctx context.Context, repos []*types.Repo, p authz.Perms) ([]*types.Repo, error)

// authzFilter is the enforcement mechanism for repository permissions. It is the root
// repository-permission-enforcing function (i.e., all other code that wants to check/enforce
// permissions and is not itself part of the permission-checking code should call this function).
//
// It accepts a list of repositories and a permission type `p` and returns a subset of those
// repositories (preserving their order) for which the currently authenticated user has the specified
// permissions. NOTE: The repos slice is filtered in place and returned. Do not use it after calling
// this function.
//
// The enforcement policy:
//
// - If there are no authz providers and `authzAllowByDefault` is true, then the repository is
//   accessible to everyone.
//
// - Otherwise, each repository must have an external repo spec. If a repo doesn't have one, we
//   cannot definitively associate the repository with an authz provider, and therefore we
//   *never* return the repository.
//
// - Scan through the list of authz providers until we find one that matches the repository. Return
//   whether or not the repository accessible according to that authz provider.
//
// - If no authz providers match the repository, consult `authzAllowByDefault`. If true, then return
//   the repository; otherwise, do not.
func authzFilter(ctx context.Context, repos []*types.Repo, p authz.Perms) (filtered []*types.Repo, err error) {
	var currentUser *types.User

	tr, ctx := trace.New(ctx, "authzFilter", "")
	defer func() {
		if err != nil {
			tr.SetError(err)
		}

		fields := []otlog.Field{
			otlog.String("permission", string(p)),
			otlog.Int("repos.count", len(repos)),
			otlog.Int("filtered.count", len(filtered)),
		}

		if currentUser != nil {
			fields = append(fields, otlog.Object("user", currentUser))
		}

		tr.LogFields(fields...)

		tr.Finish()
	}()

	if MockAuthzFilter != nil {
		return MockAuthzFilter(ctx, repos, p)
	}

	if len(repos) == 0 {
		return repos, nil
	}

	if isInternalActor(ctx) {
		return repos, nil
	}

	if actor.FromContext(ctx).IsAuthenticated() {
		var err error
		currentUser, err = Users.GetByCurrentAuthUser(ctx)
		if err != nil {
			return nil, err
		}
		if currentUser.SiteAdmin {
			return repos, nil
		}
	}

	authzAllowByDefault, authzProviders := authz.GetProviders()
	if authzAllowByDefault && len(authzProviders) == 0 {
		return repos, nil
	}

	var accts []*extsvc.ExternalAccount
	if len(authzProviders) > 0 && currentUser != nil {
		accts, err = ExternalAccounts.List(ctx, ExternalAccountsListOptions{UserID: currentUser.ID})
		if err != nil {
			return nil, err
		}
	}

	toverify := make(map[string]*[]*types.Repo, len(authzProviders))
	for _, r := range repos {
		group := toverify[r.ExternalRepo.ServiceID]
		if group == nil {
			group = getSlice(&reposPool, len(repos))
			toverify[r.ExternalRepo.ServiceID] = group
			defer func() {
				clear(*group)
				reposPool.Put(group)
			}()
		}
		*group = append(*group, r)
	}

	// Walk through all authz providers, checking repo permissions against each. If any own a given
	// repo, we use its permissions for that repo.
	verified := roaring.NewBitmap()
	for _, authzProvider := range authzProviders {
		// determine external account to use
		var providerAcct *extsvc.ExternalAccount
		for _, acct := range accts {
			if acct.ServiceID == authzProvider.ServiceID() && acct.ServiceType == authzProvider.ServiceType() {
				providerAcct = acct
				break
			}
		}

		if providerAcct == nil && currentUser != nil { // no existing external account for authz provider
			if pr, err := authzProvider.FetchAccount(ctx, currentUser, accts); err == nil {
				providerAcct = pr
				if providerAcct != nil {
					err := ExternalAccounts.AssociateUserAndSave(ctx, currentUser.ID, providerAcct.ExternalAccountSpec, providerAcct.ExternalAccountData)
					if err != nil {
						return nil, err
					}
				}
			} else {
				log15.Warn("Could not fetch authz provider account for user", "username", currentUser.Username, "authzProvider", authzProvider.ServiceID(), "error", err)
			}
		}

		serviceID := authzProvider.ServiceID()
		ours, ok := toverify[serviceID]
		if !ok {
			continue
		}

		// check the perms on our repos
		perms, err := authzProvider.RepoPerms(ctx, providerAcct, *ours)
		if err != nil {
			return nil, err
		}

		for _, r := range perms {
			if r.Perms.Include(p) {
				verified.Add(uint32(r.Repo.ID))
			}
		}

		delete(toverify, serviceID)
	}

	if authzAllowByDefault {
		for serviceID, rs := range toverify {
			// 🚨 SECURITY: Defensively bar access to repos with no external repo spec (we don't know
			// where they came from, so can't reliably enforce permissions).
			if serviceID == "" {
				continue
			}

			for _, r := range *rs {
				verified.Add(uint32(r.ID))
			}
		}
	}

	filtered = repos[:0]
	for _, r := range repos {
		if verified.Contains(uint32(r.ID)) {
			filtered = append(filtered, r) // In-place filtering
		}
	}

	clear(repos[len(filtered):])

	return filtered, nil
}

// isInternalActor returns true if the actor represents an internal agent (i.e., non-user-bound
// request that originates from within Sourcegraph itself).
//
// 🚨 SECURITY: internal requests bypass authz provider permissions checks, so correctness is
// important here.
func isInternalActor(ctx context.Context) bool {
	return actor.FromContext(ctx).Internal
}

// reposPool is used to reduce allocations of []*types.Repo slices in authzFilter.
var reposPool = sync.Pool{}

// clear resets the pointers in a []*types.Repo slice to nil so that
// the GC can free the types.Repos they once pointed to. Used together
// with reposPool, before putting slices back.
func clear(rs []*types.Repo) {
	for i := range rs {
		rs[i] = nil
	}
}

// getSlice attempts to get a []*types.Repo slice from the
// given sync.Pool. It allocates a new slice of size n if
// it couldn't be returned by the pool.
func getSlice(p *sync.Pool, n int) *[]*types.Repo {
	if rs, ok := p.Get().(*[]*types.Repo); ok && rs != nil {
		*rs = (*rs)[:0]
		return rs
	}

	rs := make([]*types.Repo, 0, n)
	return &rs
}
