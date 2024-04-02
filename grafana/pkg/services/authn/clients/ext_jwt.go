package clients

import (
	"context"
	"fmt"
	"net/http"
	"slices"
	"strconv"
	"strings"

	"github.com/go-jose/go-jose/v3/jwt"

	authlib "github.com/grafana/authlib/authn"

	"github.com/grafana/grafana/pkg/infra/log"
	"github.com/grafana/grafana/pkg/services/authn"
	"github.com/grafana/grafana/pkg/services/login"
	"github.com/grafana/grafana/pkg/services/signingkeys"
	"github.com/grafana/grafana/pkg/services/user"
	"github.com/grafana/grafana/pkg/setting"
)

var _ authn.Client = new(ExtendedJWT)

var (
	acceptedSigningMethods = []string{"RS256", "ES256"}
)

const (
	rfc9068ShortMediaType          = "at+jwt"
	extJWTAuthenticationHeaderName = "X-Access-Token"
	extJWTAuthorizationHeaderName  = "X-Grafana-Id"
)

func ProvideExtendedJWT(userService user.Service, cfg *setting.Cfg,
	signingKeys signingkeys.Service) *ExtendedJWT {
	verifier := authlib.NewVerifier[ExtendedJWTClaims](authlib.IDVerifierConfig{
		SigningKeysURL: cfg.ExtJWTAuth.JWKSUrl,
		AllowedAudiences: []string{
			cfg.ExtJWTAuth.ExpectAudience,
		},
	})

	return &ExtendedJWT{
		cfg:         cfg,
		log:         log.New(authn.ClientExtendedJWT),
		userService: userService,
		signingKeys: signingKeys,
		verifier:    verifier,
	}
}

type ExtendedJWT struct {
	cfg         *setting.Cfg
	log         log.Logger
	userService user.Service
	signingKeys signingkeys.Service
	verifier    authlib.Verifier[ExtendedJWTClaims]
}

type ExtendedJWTClaims struct {
	jwt.Claims
	// Access policy scopes
	Scopes []string `json:"scopes"`
	// Grafana roles
	Permissions []string `json:"permissions"`
	// On-behalf-of user
	DelegatedPermissions []string `json:"delegatedPermissions"`
}

func (s *ExtendedJWT) Authenticate(ctx context.Context, r *authn.Request) (*authn.Identity, error) {
	jwtToken := s.retrieveAuthenticationToken(r.HTTPRequest)

	claims, err := s.verifyRFC9068Token(ctx, jwtToken, rfc9068ShortMediaType)
	if err != nil {
		s.log.Error("Failed to verify JWT", "error", err)
		return nil, errJWTInvalid.Errorf("Failed to verify JWT: %w", err)
	}

	idToken := s.retrieveAuthorizationToken(r.HTTPRequest)
	if idToken != "" {
		idTokenClaims, err := s.verifyRFC9068Token(ctx, idToken, "jwt")
		if err != nil {
			s.log.Error("Failed to verify id token", "error", err)
			return nil, errJWTInvalid.Errorf("Failed to verify id token: %w", err)
		}

		return s.authenticateAsUser(idTokenClaims, claims)
	}

	return s.authenticateAsService(claims)
}

func (s *ExtendedJWT) authenticateAsUser(idTokenClaims,
	accessTokenClaims *ExtendedJWTClaims) (*authn.Identity, error) {
	// Only allow access policies to impersonate
	if !strings.HasPrefix(accessTokenClaims.Subject, fmt.Sprintf("%s:", authn.NamespaceAccessPolicy)) {
		s.log.Error("Invalid subject", "subject", accessTokenClaims.Subject)
		return nil, errJWTInvalid.Errorf("Failed to parse sub: %s", "invalid subject format")
	}
	// Allow only user impersonation
	_, err := strconv.ParseInt(strings.TrimPrefix(idTokenClaims.Subject, fmt.Sprintf("%s:", authn.NamespaceUser)), 10, 64)
	if err != nil {
		s.log.Error("Failed to parse sub", "error", err)
		return nil, errJWTInvalid.Errorf("Failed to parse sub: %w", err)
	}

	return &authn.Identity{
		ID:              idTokenClaims.Subject,
		OrgID:           s.getDefaultOrgID(),
		AuthenticatedBy: login.ExtendedJWTModule,
		AuthID:          accessTokenClaims.Subject,
		ClientParams: authn.ClientParams{
			SyncPermissions: true,
			FetchPermissionsParams: authn.FetchPermissionsParams{
				ActionsLookup: accessTokenClaims.DelegatedPermissions,
			},
			FetchSyncedUser: true,
		}}, nil
}

func (s *ExtendedJWT) authenticateAsService(claims *ExtendedJWTClaims) (*authn.Identity, error) {
	if !strings.HasPrefix(claims.Subject, fmt.Sprintf("%s:", authn.NamespaceAccessPolicy)) {
		s.log.Error("Invalid subject", "subject", claims.Subject)
		return nil, errJWTInvalid.Errorf("Failed to parse sub: %s", "invalid subject format")
	}

	return &authn.Identity{
		ID:              claims.Subject,
		OrgID:           s.getDefaultOrgID(),
		AuthenticatedBy: login.ExtendedJWTModule,
		AuthID:          claims.Subject,
		ClientParams: authn.ClientParams{
			SyncPermissions: true,
			FetchPermissionsParams: authn.FetchPermissionsParams{
				Roles: claims.Permissions,
			},
			FetchSyncedUser: false,
		},
	}, nil
}

func (s *ExtendedJWT) Test(ctx context.Context, r *authn.Request) bool {
	if !s.cfg.ExtJWTAuth.Enabled {
		return false
	}

	rawToken := s.retrieveAuthenticationToken(r.HTTPRequest)
	if rawToken == "" {
		return false
	}

	parsedToken, err := jwt.ParseSigned(rawToken)
	if err != nil {
		return false
	}

	var claims jwt.Claims
	if err := parsedToken.UnsafeClaimsWithoutVerification(&claims); err != nil {
		return false
	}

	return true
}

func (s *ExtendedJWT) Name() string {
	return authn.ClientExtendedJWT
}

func (s *ExtendedJWT) Priority() uint {
	// This client should come before the normal JWT client, because it is more specific, because of the Issuer check
	return 15
}

// retrieveAuthenticationToken retrieves the JWT token from the request.
func (s *ExtendedJWT) retrieveAuthenticationToken(httpRequest *http.Request) string {
	jwtToken := httpRequest.Header.Get(extJWTAuthenticationHeaderName)

	// Strip the 'Bearer' prefix if it exists.
	return strings.TrimPrefix(jwtToken, "Bearer ")
}

// retrieveAuthorizationToken retrieves the JWT token from the request.
func (s *ExtendedJWT) retrieveAuthorizationToken(httpRequest *http.Request) string {
	jwtToken := httpRequest.Header.Get(extJWTAuthorizationHeaderName)

	// Strip the 'Bearer' prefix if it exists.
	return strings.TrimPrefix(jwtToken, "Bearer ")
}

// verifyRFC9068Token verifies the token against the RFC 9068 specification.
func (s *ExtendedJWT) verifyRFC9068Token(ctx context.Context, rawToken string, typ string) (*ExtendedJWTClaims, error) {
	parsedToken, err := jwt.ParseSigned(rawToken)
	if err != nil {
		return nil, fmt.Errorf("failed to parse JWT: %w", err)
	}

	if len(parsedToken.Headers) != 1 {
		return nil, fmt.Errorf("only one header supported, got %d", len(parsedToken.Headers))
	}

	parsedHeader := parsedToken.Headers[0]

	typeHeader := parsedHeader.ExtraHeaders["typ"]
	if typeHeader == nil {
		return nil, fmt.Errorf("missing 'typ' field from the header")
	}

	jwtType := strings.ToLower(typeHeader.(string))
	if !strings.EqualFold(jwtType, typ) {
		return nil, fmt.Errorf("invalid JWT type: %s", jwtType)
	}

	if !slices.Contains(acceptedSigningMethods, parsedHeader.Algorithm) {
		return nil, fmt.Errorf("invalid algorithm: %s. Accepted algorithms: %s",
			parsedHeader.Algorithm, strings.Join(acceptedSigningMethods, ", "))
	}

	keyID := parsedHeader.KeyID
	if keyID == "" {
		return nil, fmt.Errorf("missing 'kid' field from the header")
	}

	claims, err := s.verifier.Verify(ctx, rawToken)
	if err != nil {
		return nil, fmt.Errorf("failed to verify JWT: %w", err)
	}

	if claims.Expiry == nil {
		return nil, fmt.Errorf("missing 'exp' claim")
	}

	if claims.Subject == "" {
		return nil, fmt.Errorf("missing 'sub' claim")
	}

	if claims.IssuedAt == nil {
		return nil, fmt.Errorf("missing 'iat' claim")
	}

	return &claims.Rest, nil
}

func (s *ExtendedJWT) getDefaultOrgID() int64 {
	orgID := int64(1)
	if s.cfg.AutoAssignOrg && s.cfg.AutoAssignOrgId > 0 {
		orgID = int64(s.cfg.AutoAssignOrgId)
	}
	return orgID
}
