package licensing

import (
	"context"
	"fmt"
	"testing"

	"github.com/sourcegraph/enterprise/pkg/license"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/db"
)

func TestEnforcementPreCreateUser(t *testing.T) {
	tests := []struct {
		license         *license.Info
		activeUserCount uint
		wantErr         bool
	}{
		// See the impl for why we treat UserCount == 0 as unlimited.
		{
			license:         &license.Info{UserCount: 0},
			activeUserCount: 5,
			wantErr:         false,
		},

		// Non-true-up licenses.
		{
			license:         &license.Info{UserCount: 10},
			activeUserCount: 0,
			wantErr:         false,
		},
		{
			license:         &license.Info{UserCount: 10},
			activeUserCount: 5,
			wantErr:         false,
		},
		{
			license:         &license.Info{UserCount: 10},
			activeUserCount: 9,
			wantErr:         false,
		},
		{
			license:         &license.Info{UserCount: 10},
			activeUserCount: 10,
			wantErr:         true,
		},
		{
			license:         &license.Info{UserCount: 10},
			activeUserCount: 11,
			wantErr:         true,
		},
		{
			license:         &license.Info{UserCount: 10},
			activeUserCount: 12,
			wantErr:         true,
		},

		// True-up licenses.
		{
			license:         &license.Info{Tags: []string{TrueUpUserCountTag}, UserCount: 10},
			activeUserCount: 5,
			wantErr:         false,
		},
		{
			license:         &license.Info{Tags: []string{TrueUpUserCountTag}, UserCount: 10},
			activeUserCount: 15,
			wantErr:         false,
		},
	}
	for _, test := range tests {
		t.Run(fmt.Sprintf("license %s with %d active users", test.license, test.activeUserCount), func(t *testing.T) {
			MockGetConfiguredProductLicenseInfo = func() (*license.Info, error) {
				return test.license, nil
			}
			defer func() { MockGetConfiguredProductLicenseInfo = nil }()
			db.Mocks.Users.Count = func(context.Context, *db.UsersListOptions) (int, error) {
				return int(test.activeUserCount), nil
			}
			defer func() { db.Mocks = db.MockStores{} }()

			err := db.Users.PreCreateUser(context.Background())
			if gotErr := (err != nil); gotErr != test.wantErr {
				t.Errorf("got error %v, want %v", gotErr, test.wantErr)
			}
		})
	}
}
