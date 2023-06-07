package productsubscription

import (
	"context"
	"encoding/hex"
	"time"

	"github.com/google/uuid"
	"github.com/keegancsmith/sqlf"
	"github.com/lib/pq"

	"github.com/sourcegraph/sourcegraph/enterprise/internal/license"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/licensing"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/dbutil"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

// dbLicense describes an product license row in the product_licenses DB table.
type dbLicense struct {
	ID                       string // UUID
	ProductSubscriptionID    string // UUID
	LicenseKey               string
	CreatedAt                time.Time
	LicenseVersion           *int32
	LicenseTags              []string
	LicenseUserCount         *int
	LicenseExpiresAt         *time.Time
	AccessTokenEnabled       bool
	SiteID                   *string // UUID
	LicenseCheckToken        *[]byte
	RevokedAt                *time.Time
	SalesforceSubscriptionID *string
	SalesforceOpportunityID  *string
}

// errLicenseNotFound occurs when a database operation expects a specific Sourcegraph
// license to exist but it does not exist.
var errLicenseNotFound = errors.New("product license not found")

// errTokenInvalid occurs when license check token cannot be parsed or when querying
// the product_licenses table with the token yields no results
var errTokenInvalid = errors.New("invalid token")

// dbLicenses exposes product licenses in the product_licenses DB table.
type dbLicenses struct {
	db database.DB
}

const createLicenseQuery = `
INSERT INTO product_licenses(id, product_subscription_id, license_key, license_version, license_tags, license_user_count, license_expires_at, license_check_token, salesforce_sub_id, salesforce_opp_id)
VALUES($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) RETURNING id
`

// Create creates a new product license entry for the given subscription.
func (s dbLicenses) Create(ctx context.Context, subscriptionID, licenseKey string, version int, info license.Info) (id string, err error) {
	if mocks.licenses.Create != nil {
		return mocks.licenses.Create(subscriptionID, licenseKey)
	}

	newUUID, err := uuid.NewRandom()
	if err != nil {
		return "", errors.Wrap(err, "new UUID")
	}

	var expiresAt *time.Time
	if !info.ExpiresAt.IsZero() {
		expiresAt = &info.ExpiresAt
	}
	if err = s.db.QueryRowContext(ctx, createLicenseQuery,
		newUUID,
		subscriptionID,
		licenseKey,
		dbutil.NewNullInt64(int64(version)),
		pq.Array(info.Tags),
		dbutil.NewNullInt64(int64(info.UserCount)),
		dbutil.NullTime{Time: expiresAt},
		licensing.GenerateHashedLicenseKeyAccessToken(licenseKey),
		info.SalesforceSubscriptionID,
		info.SalesforceOpportunityID,
	).Scan(&id); err != nil {
		return "", errors.Wrap(err, "insert")
	}

	return id, nil
}

// GetByID retrieves the product license (if any) given its ID.
//
// 🚨 SECURITY: The caller must ensure that the actor is permitted to view this product license.
func (s dbLicenses) GetByID(ctx context.Context, id string) (*dbLicense, error) {
	if mocks.licenses.GetByID != nil {
		return mocks.licenses.GetByID(id)
	}
	results, err := s.list(ctx, []*sqlf.Query{sqlf.Sprintf("id=%s", id)}, nil)
	if err != nil {
		return nil, err
	}
	if len(results) == 0 {
		return nil, errLicenseNotFound
	}
	return results[0], nil
}

// GetByLicenseKey retrieves the product license (if any) given its check license token.
//
// 🚨 SECURITY: The caller must ensure that errTokenInvalid error is handled appropriately
func (s dbLicenses) GetByToken(ctx context.Context, tokenHexEncoded string) (*dbLicense, error) {
	if mocks.licenses.GetByToken != nil {
		return mocks.licenses.GetByToken(tokenHexEncoded)
	}
	token, err := hex.DecodeString(tokenHexEncoded)
	if err != nil {
		return nil, errTokenInvalid
	}
	results, err := s.list(ctx, []*sqlf.Query{sqlf.Sprintf("license_check_token=%s", token)}, nil)
	if err != nil {
		return nil, err
	}
	if len(results) == 0 {
		return nil, errTokenInvalid
	}
	return results[0], nil
}

// GetByID retrieves the product license (if any) given its license key.
func (s dbLicenses) GetByLicenseKey(ctx context.Context, licenseKey string) (*dbLicense, error) {
	if mocks.licenses.GetByLicenseKey != nil {
		return mocks.licenses.GetByLicenseKey(licenseKey)
	}
	results, err := s.list(ctx, []*sqlf.Query{sqlf.Sprintf("license_key=%s", licenseKey)}, nil)
	if err != nil {
		return nil, err
	}
	if len(results) == 0 {
		return nil, errLicenseNotFound
	}
	return results[0], nil
}

// dbLicensesListOptions contains options for listing product licenses.
type dbLicensesListOptions struct {
	LicenseKeySubstring   string
	ProductSubscriptionID string // only list product licenses for this subscription (by UUID)
	*database.LimitOffset
}

func (o dbLicensesListOptions) sqlConditions() []*sqlf.Query {
	conds := []*sqlf.Query{sqlf.Sprintf("TRUE")}
	if o.LicenseKeySubstring != "" {
		conds = append(conds, sqlf.Sprintf("license_key LIKE %s", "%"+o.LicenseKeySubstring+"%"))
	}
	if o.ProductSubscriptionID != "" {
		conds = append(conds, sqlf.Sprintf("product_subscription_id=%s", o.ProductSubscriptionID))
	}
	return conds
}

func (s dbLicenses) Active(ctx context.Context, subscriptionID string) (*dbLicense, error) {
	// Return newest license.
	licenses, err := s.List(ctx, dbLicensesListOptions{
		ProductSubscriptionID: subscriptionID,
		LimitOffset:           &database.LimitOffset{Limit: 1},
	})
	if err != nil {
		return nil, err
	}
	if len(licenses) == 0 {
		return nil, nil
	}
	return licenses[0], nil
}

// AssignSiteID marks the existing license as used by a specific siteID
func (s dbLicenses) AssignSiteID(ctx context.Context, id, siteID string) error {
	q := sqlf.Sprintf(`
UPDATE product_licenses
SET site_id = %s
WHERE id = %s
	`,
		siteID,
		id,
	)

	_, err := s.db.ExecContext(ctx, q.Query(sqlf.PostgresBindVar), q.Args()...)
	return err
}

// List lists all product licenses that satisfy the options.
func (s dbLicenses) List(ctx context.Context, opt dbLicensesListOptions) ([]*dbLicense, error) {
	if mocks.licenses.List != nil {
		return mocks.licenses.List(ctx, opt)
	}

	return s.list(ctx, opt.sqlConditions(), opt.LimitOffset)
}

func (s dbLicenses) list(ctx context.Context, conds []*sqlf.Query, limitOffset *database.LimitOffset) ([]*dbLicense, error) {
	q := sqlf.Sprintf(`
SELECT
	id,
	product_subscription_id,
	license_key,
	created_at,
	license_version,
	license_tags,
	license_user_count,
	license_expires_at,
	access_token_enabled,
	site_id,
	license_check_token,
	revoked_at,
	salesforce_sub_id,
	salesforce_opp_id
FROM product_licenses
WHERE (%s)
ORDER BY created_at DESC
%s`,
		sqlf.Join(conds, ") AND ("),
		limitOffset.SQL(),
	)

	rows, err := s.db.QueryContext(ctx, q.Query(sqlf.PostgresBindVar), q.Args()...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var results []*dbLicense
	for rows.Next() {
		var v dbLicense
		if err := rows.Scan(
			&v.ID,
			&v.ProductSubscriptionID,
			&v.LicenseKey,
			&v.CreatedAt,
			&v.LicenseVersion,
			pq.Array(&v.LicenseTags),
			&v.LicenseUserCount,
			&v.LicenseExpiresAt,
			&v.AccessTokenEnabled,
			&v.SiteID,
			&v.LicenseCheckToken,
			&v.RevokedAt,
			&v.SalesforceSubscriptionID,
			&v.SalesforceOpportunityID,
		); err != nil {
			return nil, err
		}
		results = append(results, &v)
	}
	return results, nil
}

// Count counts all product licenses that satisfy the options (ignoring limit and offset).
func (s dbLicenses) Count(ctx context.Context, opt dbLicensesListOptions) (int, error) {
	q := sqlf.Sprintf("SELECT COUNT(*) FROM product_licenses WHERE (%s)", sqlf.Join(opt.sqlConditions(), ") AND ("))
	var count int
	if err := s.db.QueryRowContext(ctx, q.Query(sqlf.PostgresBindVar), q.Args()...).Scan(&count); err != nil {
		return 0, err
	}
	return count, nil
}

type mockLicenses struct {
	Create          func(subscriptionID, licenseKey string) (id string, err error)
	GetByID         func(id string) (*dbLicense, error)
	GetByLicenseKey func(licenseKey string) (*dbLicense, error)
	GetByToken      func(tokenHexEncoded string) (*dbLicense, error)
	List            func(ctx context.Context, opt dbLicensesListOptions) ([]*dbLicense, error)
}
