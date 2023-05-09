package productsubscription

import (
	"context"
	"encoding/hex"
	"strings"

	"github.com/keegancsmith/sqlf"

	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/basestore"
	"github.com/sourcegraph/sourcegraph/internal/hashutil"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

// defaultRawAccessToken is currently just a hash of the license key.
func defaultRawAccessToken(licenseKey []byte) []byte {
	return hashutil.ToSHA256Bytes(licenseKey)
}

type dbTokens struct {
	store *basestore.Store
}

func newDBTokens(db database.DB) dbTokens {
	return dbTokens{store: basestore.NewWithHandle(db.Handle())}
}

// LookupAccessToken returns the subscription ID corresponding to a token,
// trimming token prefixes if there are any.
func (t dbTokens) LookupAccessToken(ctx context.Context, token string) (string, error) {
	if !strings.HasPrefix(token, productSubscriptionAccessTokenPrefix) {
		return "", errors.New("invalid token: unknown prefix")
	}

	decoded, err := hex.DecodeString(strings.TrimPrefix(token, productSubscriptionAccessTokenPrefix))
	if err != nil {
		return "", errors.New("invalid token: unknown encoding")
	}

	query := sqlf.Sprintf(`
SELECT product_subscription_id
FROM product_licenses
WHERE
	access_token_enabled=true
	AND digest(license_key, 'sha256')=%s`,
		decoded,
	)
	subID, found, err := basestore.ScanFirstString(t.store.Query(ctx, query))
	if err != nil {
		return "", errors.New("invalid token")
	}
	if !found {
		return "", &productSubscriptionNotFoundError{}
	}
	return subID, nil
}

type productSubscriptionNotFoundError struct{}

func (e productSubscriptionNotFoundError) Error() string {
	return "product subscription not found"
}

func (e productSubscriptionNotFoundError) NotFound() bool {
	return true
}
