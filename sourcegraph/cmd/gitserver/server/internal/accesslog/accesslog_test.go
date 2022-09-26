package accesslog

import (
	"context"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/sourcegraph/log/logtest"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/sourcegraph/sourcegraph/internal/conf/conftypes"
	"github.com/sourcegraph/sourcegraph/internal/requestclient"
	"github.com/sourcegraph/sourcegraph/schema"
)

func TestRecord(t *testing.T) {
	t.Run("OK", func(t *testing.T) {
		ctx := context.Background()
		ctx = withContext(ctx, &paramsContext{})

		meta := map[string]string{"cmd": "git", "args": "grep foo"}

		Record(ctx, "github.com/foo/bar", meta)

		pc := fromContext(ctx)
		require.NotNil(t, pc)
		assert.Equal(t, "github.com/foo/bar", pc.repo)
		assert.Equal(t, meta, pc.metadata)
	})

	t.Run("OK not initialized context", func(t *testing.T) {
		ctx := context.Background()

		meta := map[string]string{"cmd": "git", "args": "grep foo"}

		Record(ctx, "github.com/foo/bar", meta)
		pc := fromContext(ctx)
		assert.Nil(t, pc)
	})
}

type accessLogConf struct {
	disabled bool
	callback func()
}

var _ conftypes.WatchableSiteConfig = &accessLogConf{}

func (a *accessLogConf) Watch(cb func()) { a.callback = cb }
func (a *accessLogConf) SiteConfig() schema.SiteConfiguration {
	return schema.SiteConfiguration{
		Log: &schema.Log{
			AuditLog: &schema.AuditLog{
				GitserverAccess: !a.disabled,
				GraphQL:         false,
				SecurityEvents:  false,
			},
		},
	}
}

func TestHTTPMiddleware(t *testing.T) {
	t.Run("OK for access log setting", func(t *testing.T) {
		logger, exportLogs := logtest.Captured(t)
		h := HTTPMiddleware(logger, &accessLogConf{}, func(w http.ResponseWriter, r *http.Request) {
			meta := map[string]string{"cmd": "git", "args": "grep foo"}
			Record(r.Context(), "github.com/foo/bar", meta)
		})

		rec := httptest.NewRecorder()
		req := httptest.NewRequest("GET", "/", nil)
		ctx := req.Context()
		ctx = requestclient.WithClient(ctx, &requestclient.Client{IP: "192.168.1.1"})
		req = req.WithContext(ctx)

		h.ServeHTTP(rec, req)
		logs := exportLogs()
		require.Len(t, logs, 2)
		assert.Equal(t, accessLoggingEnabledMessage, logs[0].Message)
		assert.Equal(t, accessEventMessage, logs[1].Message)
		assert.Equal(t, "github.com/foo/bar", logs[1].Fields["params"].(map[string]any)["repo"])

		auditLogFields := map[string]interface{}{
			"entity": "gitserver",
			"actor": map[string]interface{}{
				"actorUID":        "unknown",
				"ip":              "192.168.1.1",
				"X-Forwarded-For": "",
			},
		}
		assert.Equal(t, auditLogFields, logs[1].Fields["audit"])
	})

	t.Run("handle, no recording", func(t *testing.T) {
		logger, exportLogs := logtest.Captured(t)
		var handled bool
		h := HTTPMiddleware(logger, &accessLogConf{}, func(w http.ResponseWriter, r *http.Request) {
			handled = true
		})
		rec := httptest.NewRecorder()
		req := httptest.NewRequest("GET", "/", nil)

		h.ServeHTTP(rec, req)

		// Should have handled but not logged
		assert.True(t, handled)
		logs := exportLogs()
		require.Len(t, logs, 1)
		assert.NotEqual(t, accessEventMessage, logs[0].Message)
	})

	t.Run("disabled, then enabled", func(t *testing.T) {
		logger, exportLogs := logtest.Captured(t)
		cfg := &accessLogConf{disabled: true}
		var handled bool
		h := HTTPMiddleware(logger, cfg, func(w http.ResponseWriter, r *http.Request) {
			meta := map[string]string{"cmd": "git", "args": "grep foo"}
			Record(r.Context(), "github.com/foo/bar", meta)
			handled = true
		})
		rec := httptest.NewRecorder()
		req := httptest.NewRequest("GET", "/", nil)

		// Request with access logging disabled
		h.ServeHTTP(rec, req)

		// Disabled, should have been handled but without a log message
		assert.True(t, handled)
		logs := exportLogs()
		require.Len(t, logs, 0)

		// Now we re-enable
		handled = false
		cfg.disabled = false
		cfg.callback()
		h.ServeHTTP(rec, req)

		// Enabled, should have handled AND generated a log message
		assert.True(t, handled)
		logs = exportLogs()
		require.Len(t, logs, 2)
		assert.Equal(t, accessLoggingEnabledMessage, logs[0].Message)
		assert.Equal(t, accessEventMessage, logs[1].Message)
	})
}

func Test_shouldLog(t *testing.T) {
	tests := []struct {
		name   string
		logCfg *schema.Log
		want   bool
	}{
		{
			name:   "shouldn't log if 'log' property is missing",
			logCfg: nil,
			want:   false,
		},
		{
			name: "should log if GitServerAccessLogs is true",
			logCfg: &schema.Log{
				GitserverAccessLogs: true,
			},
			want: true,
		},
		{
			name: "should log if AuditLog.GitserverAccess is true",
			logCfg: &schema.Log{
				AuditLog: &schema.AuditLog{
					GitserverAccess: true,
				},
				GitserverAccessLogs: false,
			},
			want: true,
		},
		{
			name: "should log if both settings are true",
			logCfg: &schema.Log{
				AuditLog: &schema.AuditLog{
					GitserverAccess: true,
				},
				GitserverAccessLogs: true,
			},
			want: true,
		},
		{
			name: "shouldn't log if both settings are false",
			logCfg: &schema.Log{
				AuditLog: &schema.AuditLog{
					GitserverAccess: false,
				},
				GitserverAccessLogs: false,
			},
			want: false,
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			cfg := schema.SiteConfiguration{Log: tt.logCfg}
			assert.Equalf(t, tt.want, shouldLog(cfg), "shouldLog(%v)", cfg)
		})
	}
}
