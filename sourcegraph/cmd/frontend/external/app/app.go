// Package app exports symbols from frontend/internal/app. See the parent
// package godoc for more information.
package app

import (
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/app"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/app/jscontext"
)

type SignOutURL = app.SignOutURL

var RegisterSSOSignOutHandler = app.RegisterSSOSignOutHandler

func SetBillingPublishableKey(value string) {
	jscontext.BillingPublishableKey = value
}

// SetPreMountGrafanaHook allows the enterprise package to inject a tier
// enforcement function during initialization.
func SetPreMountGrafanaHook(hookFn func() error) {
	app.PreMountGrafanaHook = hookFn
}
