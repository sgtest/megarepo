package mssql

import (
	"context"
	"fmt"
	"testing"

	"github.com/grafana/grafana/pkg/setting"
	"github.com/grafana/grafana/pkg/tsdb/sqleng"
	"github.com/grafana/grafana/pkg/tsdb/sqleng/proxyutil"
	mssql "github.com/microsoft/go-mssqldb"
	"github.com/stretchr/testify/require"
)

func TestMSSQLProxyDriver(t *testing.T) {
	settings := proxyutil.SetupTestSecureSocksProxySettings(t)
	proxySettings := setting.SecureSocksDSProxySettings{
		Enabled:      true,
		ClientCert:   settings.ClientCert,
		ClientKey:    settings.ClientKey,
		RootCA:       settings.RootCA,
		ProxyAddress: settings.ProxyAddress,
		ServerName:   settings.ServerName,
	}
	opts := proxyutil.GetSQLProxyOptions(proxySettings, sqleng.DataSourceInfo{UID: "1", JsonData: sqleng.JsonData{SecureDSProxy: true}})
	cnnstr := "server=127.0.0.1;port=1433;user id=sa;password=yourStrong(!)Password;database=db"
	driverName, err := createMSSQLProxyDriver(cnnstr, "127.0.0.1", opts)
	require.NoError(t, err)

	t.Run("Driver should not be registered more than once", func(t *testing.T) {
		testDriver, err := createMSSQLProxyDriver(cnnstr, "127.0.0.1", opts)
		require.NoError(t, err)
		require.Equal(t, driverName, testDriver)
	})

	t.Run("A new driver should be created for a new connection string", func(t *testing.T) {
		testDriver, err := createMSSQLProxyDriver("server=localhost;user id=sa;password=yourStrong(!)Password;database=db2", "localhost", opts)
		require.NoError(t, err)
		require.NotEqual(t, driverName, testDriver)
	})

	t.Run("Connector should use dialer context that routes through the socks proxy to db", func(t *testing.T) {
		connector, err := mssql.NewConnector(cnnstr)
		require.NoError(t, err)
		driver, err := newMSSQLProxyDriver(connector, "127.0.0.1", opts)
		require.NoError(t, err)

		conn, err := driver.OpenConnector(cnnstr)
		require.NoError(t, err)

		_, err = conn.Connect(context.Background())
		require.Contains(t, err.Error(), fmt.Sprintf("socks connect tcp %s->127.0.0.1:1433", settings.ProxyAddress))
	})

	t.Run("Open should use the connector that routes through the socks proxy to db", func(t *testing.T) {
		connector, err := mssql.NewConnector(cnnstr)
		require.NoError(t, err)
		driver, err := newMSSQLProxyDriver(connector, "127.0.0.1", opts)
		require.NoError(t, err)

		_, err = driver.Open(cnnstr)
		require.Contains(t, err.Error(), fmt.Sprintf("socks connect tcp %s->127.0.0.1:1433", settings.ProxyAddress))
	})
}
