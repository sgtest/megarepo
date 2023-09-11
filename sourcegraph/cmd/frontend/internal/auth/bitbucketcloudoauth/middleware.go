package bitbucketcloudoauth

import (
	"net/http"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/auth"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/auth/oauth"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/schema"
)

const authPrefix = auth.AuthURLPrefix + "/bitbucketcloud"

func init() {
	oauth.AddIsOAuth(func(p schema.AuthProviders) bool {
		return p.Bitbucketcloud != nil
	})
}

func Middleware(db database.DB) *auth.Middleware {
	return &auth.Middleware{
		API: func(next http.Handler) http.Handler {
			return oauth.NewMiddleware(db, extsvc.TypeBitbucketCloud, authPrefix, true, next)
		},
		App: func(next http.Handler) http.Handler {
			return oauth.NewMiddleware(db, extsvc.TypeBitbucketCloud, authPrefix, false, next)
		},
	}
}
