package corekind

import (
	"sync"

	"github.com/grafana/kindsys"
	"github.com/grafana/thema"

	"github.com/grafana/grafana/pkg/cuectx"
)

var (
	baseOnce    sync.Once
	defaultBase *Base
)

// NewBase provides a registry of all core raw and structured kinds, without any
// composition of slot kinds.
//
// All calling code within grafana/grafana is expected to use Grafana's
// singleton [thema.Runtime], returned from [cuectx.GrafanaThemaRuntime]. If nil
// is passed, the singleton will be used.
func NewBase(rt *thema.Runtime) *Base {
	allrt := cuectx.GrafanaThemaRuntime()
	if rt == nil || rt == allrt {
		baseOnce.Do(func() {
			defaultBase = doNewBase(allrt)
		})
		return defaultBase
	}

	return doNewBase(rt)
}

// All returns a slice of [kindsys.Core] containing all core Grafana kinds.
//
// The returned slice is sorted lexicographically by kind machine name.
func (b *Base) All() []kindsys.Core {
	ret := make([]kindsys.Core, len(b.all))
	copy(ret, b.all)
	return ret
}
