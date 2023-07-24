package licensecheck

import (
	"bytes"
	"context"
	"encoding/json"
	"io"
	"net/http"
	"net/url"
	"testing"
	"time"

	"github.com/derision-test/glock"
	"github.com/gomodule/redigo/redis"
	"github.com/stretchr/testify/require"

	"github.com/sourcegraph/log/logtest"

	"github.com/sourcegraph/sourcegraph/internal/license"
	"github.com/sourcegraph/sourcegraph/internal/licensing"
	"github.com/sourcegraph/sourcegraph/internal/redispool"
	"github.com/sourcegraph/sourcegraph/lib/pointers"
)

func Test_calcDurationToWaitForNextHandle(t *testing.T) {
	// Connect to local redis for testing, this is the same URL used in rcache.SetupForTest
	store = redispool.NewKeyValue("127.0.0.1:6379", &redis.Pool{
		MaxIdle:     3,
		IdleTimeout: 5 * time.Second,
	})

	cleanupStore := func() {
		_ = store.Del(licensing.LicenseValidityStoreKey)
		_ = store.Del(lastCalledAtStoreKey)
	}

	now := time.Now().Round(time.Second)
	clock := glock.NewMockClock()
	clock.SetCurrent(now)

	tests := map[string]struct {
		lastCalledAt string
		want         time.Duration
		wantErr      bool
	}{
		"returns 0 if last called at is empty": {
			lastCalledAt: "",
			want:         0,
			wantErr:      true,
		},
		"returns 0 if last called at is invalid": {
			lastCalledAt: "invalid",
			want:         0,
			wantErr:      true,
		},
		"returns 0 if last called at is in the future": {
			lastCalledAt: now.Add(time.Minute).Format(time.RFC3339),
			want:         0,
			wantErr:      true,
		},
		"returns 0 if last called at is before licensing.LicenseCheckInterval": {
			lastCalledAt: now.Add(-licensing.LicenseCheckInterval - time.Minute).Format(time.RFC3339),
			want:         0,
			wantErr:      false,
		},
		"returns 0 if last called at is at licensing.LicenseCheckInterval": {
			lastCalledAt: now.Add(-licensing.LicenseCheckInterval).Format(time.RFC3339),
			want:         0,
			wantErr:      false,
		},
		"returns diff between last called at and now": {
			lastCalledAt: now.Add(-time.Hour).Format(time.RFC3339),
			want:         licensing.LicenseCheckInterval - time.Hour,
			wantErr:      false,
		},
	}
	for name, test := range tests {
		t.Run(name, func(t *testing.T) {
			cleanupStore()
			if test.lastCalledAt != "" {
				_ = store.Set(lastCalledAtStoreKey, test.lastCalledAt)
			}

			got, err := calcDurationSinceLastCalled(clock)
			if test.wantErr {
				require.Error(t, err)
			} else {
				require.NoError(t, err)
			}
			require.Equal(t, test.want, got)
		})
	}
}

func mockDotcomURL(t *testing.T, u *string) {
	t.Helper()

	origBaseURL := baseUrl
	t.Cleanup(func() {
		baseUrl = origBaseURL
	})

	if u != nil {
		baseUrl = *u
	}
}

func Test_licenseChecker(t *testing.T) {
	// Connect to local redis for testing, this is the same URL used in rcache.SetupForTest
	store = redispool.NewKeyValue("127.0.0.1:6379", &redis.Pool{
		MaxIdle:     3,
		IdleTimeout: 5 * time.Second,
	})

	cleanupStore := func() {
		_ = store.Del(licensing.LicenseValidityStoreKey)
		_ = store.Del(lastCalledAtStoreKey)
	}

	siteID := "some-site-id"
	token := "test-token"

	t.Run("skips check if license is air-gapped", func(t *testing.T) {
		cleanupStore()
		var featureChecked licensing.Feature
		defaultMock := licensing.MockCheckFeature
		licensing.MockCheckFeature = func(feature licensing.Feature) error {
			featureChecked = feature
			return nil
		}

		t.Cleanup(func() {
			licensing.MockCheckFeature = defaultMock
		})

		doer := &mockDoer{
			status:   '1',
			response: []byte(``),
		}
		handler := licenseChecker{
			siteID: siteID,
			token:  token,
			doer:   doer,
			logger: logtest.NoOp(t),
		}

		err := handler.Handle(context.Background())
		require.NoError(t, err)

		// check feature was checked
		require.Equal(t, licensing.FeatureAllowAirGapped, featureChecked)

		// check doer NOT called
		require.False(t, doer.DoCalled)

		// check result was set to true
		valid, err := store.Get(licensing.LicenseValidityStoreKey).Bool()
		require.NoError(t, err)
		require.True(t, valid)

		// check last called at was set
		lastCalledAt, err := store.Get(lastCalledAtStoreKey).String()
		require.NoError(t, err)
		require.NotEmpty(t, lastCalledAt)
	})

	t.Run("skips check if license has dev tag", func(t *testing.T) {
		defaultMockGetLicense := licensing.MockGetConfiguredProductLicenseInfo
		licensing.MockGetConfiguredProductLicenseInfo = func() (*license.Info, string, error) {
			return &license.Info{
				Tags: []string{"dev"},
			}, "", nil
		}

		t.Cleanup(func() {
			licensing.MockGetConfiguredProductLicenseInfo = defaultMockGetLicense
		})

		_ = store.Del(licensing.LicenseValidityStoreKey)
		_ = store.Del(lastCalledAtStoreKey)

		doer := &mockDoer{
			status:   '1',
			response: []byte(``),
		}
		handler := licenseChecker{
			siteID: siteID,
			token:  token,
			doer:   doer,
			logger: logtest.NoOp(t),
		}

		err := handler.Handle(context.Background())
		require.NoError(t, err)

		// check doer NOT called
		require.False(t, doer.DoCalled)

		// check result was set to true
		valid, err := store.Get(licensing.LicenseValidityStoreKey).Bool()
		require.NoError(t, err)
		require.True(t, valid)

		// check last called at was set
		lastCalledAt, err := store.Get(lastCalledAtStoreKey).String()
		require.NoError(t, err)
		require.NotEmpty(t, lastCalledAt)
	})

	tests := map[string]struct {
		response []byte
		status   int
		want     bool
		err      bool
		baseUrl  *string
		reason   *string
	}{
		"returns error if unable to make a request to license server": {
			response: []byte(`{"error": "some error"}`),
			status:   http.StatusInternalServerError,
			err:      true,
		},
		"returns error if got error": {
			response: []byte(`{"error": "some error"}`),
			status:   http.StatusOK,
			err:      true,
		},
		`returns correct result for "true"`: {
			response: []byte(`{"data": {"is_valid": true}}`),
			status:   http.StatusOK,
			want:     true,
		},
		`returns correct result for "false"`: {
			response: []byte(`{"data": {"is_valid": false, "reason": "some reason"}}`),
			status:   http.StatusOK,
			want:     false,
			reason:   pointers.Ptr("some reason"),
		},
		`uses sourcegraph baseURL from env`: {
			response: []byte(`{"data": {"is_valid": true}}`),
			status:   http.StatusOK,
			want:     true,
			baseUrl:  pointers.Ptr("https://foo.bar"),
		},
	}

	for name, test := range tests {
		t.Run(name, func(t *testing.T) {
			cleanupStore()

			mockDotcomURL(t, test.baseUrl)

			doer := &mockDoer{
				status:   test.status,
				response: test.response,
			}
			checker := licenseChecker{
				siteID: siteID,
				token:  token,
				doer:   doer,
				logger: logtest.NoOp(t),
			}

			err := checker.Handle(context.Background())
			if test.err {
				require.Error(t, err)

				// check result was NOT set
				require.True(t, store.Get(licensing.LicenseValidityStoreKey).IsNil())
			} else {
				require.NoError(t, err)

				// check result was set
				got, err := store.Get(licensing.LicenseValidityStoreKey).Bool()
				require.NoError(t, err)
				require.Equal(t, test.want, got)

				// check result reason was set
				if test.reason != nil {
					got, err := store.Get(licensing.LicenseInvalidReason).String()
					require.NoError(t, err)
					require.Equal(t, *test.reason, got)
				}
			}

			// check last called at was set
			lastCalledAt, err := store.Get(lastCalledAtStoreKey).String()
			require.NoError(t, err)
			require.NotEmpty(t, lastCalledAt)

			// check doer with proper parameters
			rUrl, _ := url.JoinPath(baseUrl, "/.api/license/check")
			require.True(t, doer.DoCalled)
			require.Equal(t, "POST", doer.Request.Method)
			require.Equal(t, rUrl, doer.Request.URL.String())
			require.Equal(t, "application/json", doer.Request.Header.Get("Content-Type"))
			require.Equal(t, "Bearer "+token, doer.Request.Header.Get("Authorization"))
			var body struct {
				SiteID string `json:"siteID"`
			}
			err = json.NewDecoder(doer.Request.Body).Decode(&body)
			require.NoError(t, err)
			require.Equal(t, siteID, body.SiteID)
		})
	}
}

type mockDoer struct {
	DoCalled bool
	Request  *http.Request

	status   int
	response []byte
}

func (d *mockDoer) Do(req *http.Request) (*http.Response, error) {
	d.DoCalled = true
	d.Request = req

	return &http.Response{
		StatusCode: d.status,
		Body:       io.NopCloser(bytes.NewReader(d.response)),
	}, nil
}
