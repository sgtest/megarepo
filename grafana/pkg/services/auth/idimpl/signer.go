package idimpl

import (
	"context"

	"github.com/go-jose/go-jose/v3"
	"github.com/go-jose/go-jose/v3/jwt"

	"github.com/grafana/grafana/pkg/services/auth"
	"github.com/grafana/grafana/pkg/services/signingkeys"
)

const idSignerKeyPrefix = "id"

var _ auth.IDSigner = (*LocalSigner)(nil)

func ProvideLocalSigner(keyService signingkeys.Service) (*LocalSigner, error) {
	id, key, err := keyService.GetOrCreatePrivateKey(context.Background(), idSignerKeyPrefix, jose.ES256)
	if err != nil {
		return nil, err
	}

	// FIXME: Handle key rotation
	signer, err := jose.NewSigner(jose.SigningKey{Algorithm: jose.ES256, Key: key}, &jose.SignerOptions{
		ExtraHeaders: map[jose.HeaderKey]interface{}{
			"kid": id,
		},
	})
	if err != nil {
		return nil, err
	}

	return &LocalSigner{
		signer: signer,
	}, nil
}

type LocalSigner struct {
	signer jose.Signer
}

func (s *LocalSigner) SignIDToken(ctx context.Context, claims *auth.IDClaims) (string, error) {
	builder := jwt.Signed(s.signer).Claims(claims.Claims)

	token, err := builder.CompactSerialize()
	if err != nil {
		return "", err
	}

	return token, nil
}
