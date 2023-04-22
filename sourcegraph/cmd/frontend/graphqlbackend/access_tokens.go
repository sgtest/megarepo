package graphqlbackend

import (
	"context"
	"sort"
	"sync"

	"github.com/graph-gophers/graphql-go"

	"github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/backend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/envvar"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend/graphqlutil"
	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/auth"
	"github.com/sourcegraph/sourcegraph/internal/authz"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

type createAccessTokenInput struct {
	User   graphql.ID
	Scopes []string
	Note   string
}

func (r *schemaResolver) CreateAccessToken(ctx context.Context, args *createAccessTokenInput) (*createAccessTokenResult, error) {
	// 🚨 SECURITY: Creating access tokens for any user by site admins is not
	// allowed on Sourcegraph.com. This check is mostly the defense for a
	// misconfiguration of the site configuration.
	if envvar.SourcegraphDotComMode() && conf.AccessTokensAllow() == conf.AccessTokensAdmin {
		return nil, errors.Errorf("access token configuration value %q is disabled on Sourcegraph.com", conf.AccessTokensAllow())
	}

	userID, err := UnmarshalUserID(args.User)
	if err != nil {
		return nil, err
	}

	switch conf.AccessTokensAllow() {
	case conf.AccessTokensAll:
		// 🚨 SECURITY: Only the current logged in user should be able to create a token
		// for themselves. A site admin should NOT be allowed to do this since they could
		// then use the token to impersonate a user and gain access to their private
		// code.
		if err := auth.CheckSameUser(ctx, userID); err != nil {
			return nil, err
		}
	case conf.AccessTokensAdmin:
		// 🚨 SECURITY: The site has opted in to only allow site admins to create access
		// tokens. In this case, they can create a token for any user.
		if err := auth.CheckCurrentUserIsSiteAdmin(ctx, r.db); err != nil {
			return nil, errors.New("Access token creation has been restricted to admin users. Contact an admin user to create a new access token.")
		}
	case conf.AccessTokensNone:
	default:
		return nil, errors.New("Access token creation is disabled. Contact an admin user to enable.")
	}

	// Validate scopes.
	var hasUserAllScope bool
	seenScope := map[string]struct{}{}
	sort.Strings(args.Scopes)
	for _, scope := range args.Scopes {
		switch scope {
		case authz.ScopeUserAll:
			hasUserAllScope = true
		case authz.ScopeSiteAdminSudo:
			// 🚨 SECURITY: Only site admins may create a token with the "site-admin:sudo" scope.
			if err := auth.CheckCurrentUserIsSiteAdmin(ctx, r.db); err != nil {
				return nil, err
			} else if envvar.SourcegraphDotComMode() {
				return nil, errors.Errorf("creation of access tokens with scope %q is disabled on Sourcegraph.com", authz.ScopeSiteAdminSudo)
			}
		default:
			return nil, errors.Errorf("unknown access token scope %q (valid scopes: %q)", scope, authz.AllScopes)
		}

		if _, seen := seenScope[scope]; seen {
			return nil, errors.Errorf("access token scope %q may not be specified multiple times", scope)
		}
		seenScope[scope] = struct{}{}
	}
	if !hasUserAllScope {
		return nil, errors.Errorf("all access tokens must have scope %q", authz.ScopeUserAll)
	}

	uid := actor.FromContext(ctx).UID
	id, token, err := r.db.AccessTokens().Create(ctx, userID, args.Scopes, args.Note, uid)
	logger := r.logger.Scoped("CreateAccessToken", "access token creation").
		With(log.Int32("userID", uid))

	if conf.CanSendEmail() {
		if err := backend.NewUserEmailsService(r.db, logger).SendUserEmailOnAccessTokenChange(ctx, userID, args.Note, false); err != nil {
			logger.Warn("Failed to send email to inform user of access token creation", log.Error(err))
		}
	}

	return &createAccessTokenResult{id: marshalAccessTokenID(id), token: token}, err
}

type createAccessTokenResult struct {
	id    graphql.ID
	token string
}

func (r *createAccessTokenResult) ID() graphql.ID { return r.id }
func (r *createAccessTokenResult) Token() string  { return r.token }

type deleteAccessTokenInput struct {
	ByID    *graphql.ID
	ByToken *string
}

func (r *schemaResolver) DeleteAccessToken(ctx context.Context, args *deleteAccessTokenInput) (*EmptyResponse, error) {
	if args.ByID == nil && args.ByToken == nil {
		return nil, errors.New("either byID or byToken must be specified")
	}
	if args.ByID != nil && args.ByToken != nil {
		return nil, errors.New("exactly one of byID or byToken must be specified")
	}

	var token *database.AccessToken
	switch {
	case args.ByID != nil:
		accessTokenID, err := unmarshalAccessTokenID(*args.ByID)
		if err != nil {
			return nil, err
		}
		t, err := r.db.AccessTokens().GetByID(ctx, accessTokenID)
		if err != nil {
			return nil, err
		}
		token = t

		// 🚨 SECURITY: Only site admins and the user can delete a user's access token.
		if err := auth.CheckSiteAdminOrSameUser(ctx, r.db, token.SubjectUserID); err != nil {
			return nil, err
		}
		if err := r.db.AccessTokens().DeleteByID(ctx, token.ID); err != nil {
			return nil, err
		}

	case args.ByToken != nil:
		t, err := r.db.AccessTokens().GetByToken(ctx, *args.ByToken)
		if err != nil {
			return nil, err
		}
		token = t

		// 🚨 SECURITY: This is easier than the ByID case because anyone holding the access token's
		// secret value is assumed to be allowed to delete it.
		if err := r.db.AccessTokens().DeleteByToken(ctx, *args.ByToken); err != nil {
			return nil, err
		}

	}

	logger := r.logger.Scoped("DeleteAccessToken", "access token deletion").
		With(log.Int32("userID", token.SubjectUserID))

	if conf.CanSendEmail() {
		if err := backend.NewUserEmailsService(r.db, logger).SendUserEmailOnAccessTokenChange(ctx, token.SubjectUserID, token.Note, true); err != nil {
			logger.Warn("Failed to send email to inform user of access token deletion", log.Error(err))
		}
	}

	return &EmptyResponse{}, nil
}

func (r *siteResolver) AccessTokens(ctx context.Context, args *struct {
	graphqlutil.ConnectionArgs
}) (*accessTokenConnectionResolver, error) {
	// 🚨 SECURITY: Only site admins can list all access tokens. This is safe as the
	// token values themselves are not stored in our database.
	if err := auth.CheckCurrentUserIsSiteAdmin(ctx, r.db); err != nil {
		return nil, err
	}

	var opt database.AccessTokensListOptions
	args.ConnectionArgs.Set(&opt.LimitOffset)
	return &accessTokenConnectionResolver{db: r.db, opt: opt}, nil
}

func (r *UserResolver) AccessTokens(ctx context.Context, args *struct {
	graphqlutil.ConnectionArgs
}) (*accessTokenConnectionResolver, error) {
	// 🚨 SECURITY: Only site admins and the user can list a user's access tokens.
	if err := auth.CheckSiteAdminOrSameUser(ctx, r.db, r.user.ID); err != nil {
		return nil, err
	}

	opt := database.AccessTokensListOptions{SubjectUserID: r.user.ID}
	args.ConnectionArgs.Set(&opt.LimitOffset)
	return &accessTokenConnectionResolver{db: r.db, opt: opt}, nil
}

// accessTokenConnectionResolver resolves a list of access tokens.
//
// 🚨 SECURITY: When instantiating an accessTokenConnectionResolver value, the caller MUST check
// permissions.
type accessTokenConnectionResolver struct {
	opt database.AccessTokensListOptions

	// cache results because they are used by multiple fields
	once         sync.Once
	accessTokens []*database.AccessToken
	err          error
	db           database.DB
}

func (r *accessTokenConnectionResolver) compute(ctx context.Context) ([]*database.AccessToken, error) {
	r.once.Do(func() {
		opt2 := r.opt
		if opt2.LimitOffset != nil {
			tmp := *opt2.LimitOffset
			opt2.LimitOffset = &tmp
			opt2.Limit++ // so we can detect if there is a next page
		}

		r.accessTokens, r.err = r.db.AccessTokens().List(ctx, opt2)
	})
	return r.accessTokens, r.err
}

func (r *accessTokenConnectionResolver) Nodes(ctx context.Context) ([]*accessTokenResolver, error) {
	accessTokens, err := r.compute(ctx)
	if err != nil {
		return nil, err
	}
	if r.opt.LimitOffset != nil && len(accessTokens) > r.opt.LimitOffset.Limit {
		accessTokens = accessTokens[:r.opt.LimitOffset.Limit]
	}

	var l []*accessTokenResolver
	for _, accessToken := range accessTokens {
		l = append(l, &accessTokenResolver{db: r.db, accessToken: *accessToken})
	}
	return l, nil
}

func (r *accessTokenConnectionResolver) TotalCount(ctx context.Context) (int32, error) {
	count, err := r.db.AccessTokens().Count(ctx, r.opt)
	return int32(count), err
}

func (r *accessTokenConnectionResolver) PageInfo(ctx context.Context) (*graphqlutil.PageInfo, error) {
	accessTokens, err := r.compute(ctx)
	if err != nil {
		return nil, err
	}
	return graphqlutil.HasNextPage(r.opt.LimitOffset != nil && len(accessTokens) > r.opt.Limit), nil
}
