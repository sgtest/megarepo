package folders

import (
	"context"
	"net/http"

	"k8s.io/apimachinery/pkg/runtime"
	"k8s.io/apiserver/pkg/registry/rest"

	"github.com/grafana/grafana/pkg/apis/folder/v0alpha1"
	"github.com/grafana/grafana/pkg/infra/appcontext"
	"github.com/grafana/grafana/pkg/services/apiserver/endpoints/request"
	"github.com/grafana/grafana/pkg/services/folder"
	"github.com/grafana/grafana/pkg/services/guardian"
)

type subAccessREST struct {
	service folder.Service
}

var _ = rest.Connecter(&subAccessREST{})

func (r *subAccessREST) New() runtime.Object {
	return &v0alpha1.FolderAccessInfo{}
}

func (r *subAccessREST) Destroy() {
}

func (r *subAccessREST) ConnectMethods() []string {
	return []string{"GET"}
}

func (r *subAccessREST) NewConnectOptions() (runtime.Object, bool, string) {
	return nil, false, "" // true means you can use the trailing path as a variable
}

func (r *subAccessREST) Connect(ctx context.Context, name string, opts runtime.Object, responder rest.Responder) (http.Handler, error) {
	ns, err := request.NamespaceInfoFrom(ctx, true)
	if err != nil {
		return nil, err
	}
	user, err := appcontext.User(ctx)
	if err != nil {
		return nil, err
	}
	// Can view is managed here (and in the Authorizer)
	f, err := r.service.Get(ctx, &folder.GetFolderQuery{
		UID:          &name,
		OrgID:        ns.OrgID,
		SignedInUser: user,
	})
	if err != nil {
		return nil, err
	}
	guardian, err := guardian.NewByFolder(ctx, f, ns.OrgID, user)
	if err != nil {
		return nil, err
	}

	return http.HandlerFunc(func(w http.ResponseWriter, req *http.Request) {
		access := &v0alpha1.FolderAccessInfo{}
		access.CanEdit, _ = guardian.CanEdit()
		access.CanSave, _ = guardian.CanSave()
		access.CanAdmin, _ = guardian.CanAdmin()
		access.CanDelete, _ = guardian.CanDelete()
		responder.Object(http.StatusOK, access)
	}), nil
}
