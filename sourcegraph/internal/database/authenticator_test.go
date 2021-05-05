package database

import (
	"context"
	"encoding/json"
	"net/http"
	"testing"

	"github.com/google/go-cmp/cmp"
	"github.com/pkg/errors"

	"github.com/sourcegraph/sourcegraph/internal/encryption"
	et "github.com/sourcegraph/sourcegraph/internal/encryption/testing"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/auth"
)

func TestEncryptAuthenticator(t *testing.T) {
	ctx := context.Background()

	t.Run("errors", func(t *testing.T) {
		for name, tc := range map[string]struct {
			enc encryption.Encrypter
			a   auth.Authenticator
		}{
			"bad authenticator": {
				enc: et.TestKey{},
				a:   &badAuthenticator{},
			},
			"bad encrypter": {
				enc: &et.BadKey{Err: errors.New("encryption is bad")},
				a:   &auth.BasicAuth{},
			},
		} {
			t.Run(name, func(t *testing.T) {
				if _, err := EncryptAuthenticator(ctx, tc.enc, tc.a); err == nil {
					t.Error("unexpected nil error")
				}
			})
		}
	})

	t.Run("success", func(t *testing.T) {
		enc := &mockEncrypter{}
		a := &auth.BasicAuth{
			Username: "foo",
			Password: "bar",
		}

		want, err := json.Marshal(struct {
			Type AuthenticatorType
			Auth auth.Authenticator
		}{
			Type: AuthenticatorTypeBasicAuth,
			Auth: a,
		})
		if err != nil {
			t.Fatal(err)
		}

		if have, err := EncryptAuthenticator(ctx, enc, a); err != nil {
			t.Errorf("unexpected error: %v", err)
		} else if diff := cmp.Diff(string(have), string(want)); diff != "" {
			t.Errorf("unexpected byte slice (-have +want):\n%s", diff)
		}

		if enc.called != 1 {
			t.Errorf("mock encrypter called an unexpected number of times: have=%d want=1", enc.called)
		}
	})
}

type mockEncrypter struct {
	called int
}

var _ encryption.Encrypter = &mockEncrypter{}

func (me *mockEncrypter) Encrypt(ctx context.Context, value []byte) ([]byte, error) {
	me.called++
	return value, nil
}

type badAuthenticator struct{}

var _ auth.Authenticator = &badAuthenticator{}

func (*badAuthenticator) Authenticate(*http.Request) error {
	return errors.New("never called")
}

func (*badAuthenticator) Hash() string {
	return "never called"
}
