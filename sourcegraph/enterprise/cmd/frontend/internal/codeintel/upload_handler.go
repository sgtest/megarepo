package codeintel

import (
	"context"
	"net/http"

	"github.com/sourcegraph/sourcegraph/enterprise/cmd/frontend/internal/codeintel/httpapi"
	codeintelhttpapi "github.com/sourcegraph/sourcegraph/enterprise/cmd/frontend/internal/codeintel/httpapi"
)

// NewCodeIntelUploadHandler creates a new code intel LSIF upload HTTP handler. This is used
// by both the enterprise frontend codeintel init code to install handlers in the frontend API
// as well as the the enterprise frontend executor init code to install handlers in the proxy.
func NewCodeIntelUploadHandler(ctx context.Context, internal bool) (http.Handler, error) {
	if err := initServices(ctx); err != nil {
		return nil, err
	}

	handler := codeintelhttpapi.NewUploadHandler(
		&httpapi.DBStoreShim{services.dbStore},
		services.uploadStore,
		internal,
	)

	return handler, nil
}
