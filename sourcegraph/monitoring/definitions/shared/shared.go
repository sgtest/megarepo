// Package shared contains shared declarations between dashboards. In general, you should NOT be making
// changes to this package: we use a declarative style for monitoring intentionally, so you should err
// on the side of repeating yourself and NOT writing shared or programatically generated monitoring.
//
// When editing this package or introducing any shared declarations, you should abide strictly by the
// following rules:
//
// 1. Do NOT declare a shared definition unless 5+ dashboards will use it. Sharing dashboard
//    declarations means the codebase becomes more complex and non-declarative which we want to avoid
//    so repeat yourself instead if it applies to less than 5 dashboards.
//
// 2. ONLY declare shared Observables. Introducing shared Rows or Groups prevents individual dashboard
//    maintainers from holistically considering both the layout of dashboards as well as the
//    metrics and alerts defined within them -- which we do not want.
//
// 3. Use the sharedObservable type and do NOT parameterize more than just the container name. It may
//    be tempting to pass an alerting threshold as an argument, or parameterize whether a critical
//    alert is defined -- but this makes reasoning about alerts at a high level much more difficult.
//    If you have a need for this, it is a strong signal you should NOT be using the shared definition
//    anymore and should instead copy it and apply your modifications.
//
// Learn more about monitoring in https://about.sourcegraph.com/handbook/engineering/observability/monitoring_pillars
package shared

import (
	"fmt"

	"github.com/sourcegraph/sourcegraph/monitoring/monitoring"
)

// Observable is a variant of normal Observables that offer convenience functions for
// customizing shared observables.
type Observable monitoring.Observable

// Observable is a convenience adapter that casts this SharedObservable as an normal Observable.
func (o Observable) Observable() monitoring.Observable { return monitoring.Observable(o) }

// WithWarning overrides this Observable's warning-level alert with the given alert.
func (o Observable) WithWarning(a *monitoring.ObservableAlertDefinition) Observable {
	o.Warning = a
	return o
}

// WithCritical overrides this Observable's critical-level alert with the given alert.
func (o Observable) WithCritical(a *monitoring.ObservableAlertDefinition) Observable {
	o.Critical = a
	return o
}

// WithNoAlerts disables alerting on this Observable and sets the given interpretation instead.
func (o Observable) WithNoAlerts(interpretation string) Observable {
	o.Warning = nil
	o.Critical = nil
	o.NoAlert = true
	o.PossibleSolutions = ""
	o.Interpretation = interpretation
	return o
}

type sharedObservable func(containerName string, owner monitoring.ObservableOwner) Observable

// CadvisorNameMatcher generates Prometheus matchers that capture metrics that match the given container name
// while excluding some irrelevant series
func CadvisorNameMatcher(containerName string) string {
	// This matcher excludes:
	// * jaeger sidecar (jaeger-agent)
	// * pod sidecars (_POD_)
	// as well as matching on the name of the container exactly with "_{container}_"
	return fmt.Sprintf(`name=~".*_%s_.*",name!~".*(_POD_|_jaeger-agent_).*"`, containerName)
}
