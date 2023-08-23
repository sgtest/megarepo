package db

import (
	"testing"

	"github.com/sourcegraph/log/logtest"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/conf/conftypes"
	"github.com/stretchr/testify/require"
)

func TestNewDBFromConfFunc(t *testing.T) {
	t.Run("default nil", func(t *testing.T) {
		conf.Mock(&conf.Unified{
			ServiceConnectionConfig: conftypes.ServiceConnections{
				Qdrant: "",
			},
		})
		getDB := NewDBFromConfFunc(logtest.Scoped(t), nil)
		got, err := getDB()
		require.NoError(t, err)
		require.Nil(t, got)
	})

	t.Run("fake addr", func(t *testing.T) {
		conf.Mock(&conf.Unified{
			ServiceConnectionConfig: conftypes.ServiceConnections{
				Qdrant: "fake_address_but_it_does_not_matter_because_grpc_dialing_is_lazy",
			},
		})
		getDB := NewDBFromConfFunc(logtest.Scoped(t), nil)
		got, err := getDB()
		require.NoError(t, err)
		require.NotNil(t, got)
	})
}
