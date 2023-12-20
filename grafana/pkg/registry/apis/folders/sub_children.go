package folders

import (
	"context"
	"net/http"

	"k8s.io/apimachinery/pkg/runtime"
	"k8s.io/apiserver/pkg/registry/rest"

	"github.com/grafana/grafana/pkg/apis/folders/v0alpha1"
	"github.com/grafana/grafana/pkg/infra/appcontext"
	"github.com/grafana/grafana/pkg/services/folder"
	"github.com/grafana/grafana/pkg/services/grafana-apiserver/endpoints/request"
)

type subChildrenREST struct {
	service folder.Service
}

var _ = rest.Connecter(&subChildrenREST{})

func (r *subChildrenREST) New() runtime.Object {
	return &v0alpha1.FolderInfo{}
}

func (r *subChildrenREST) Destroy() {
}

func (r *subChildrenREST) ConnectMethods() []string {
	return []string{"GET"}
}

func (r *subChildrenREST) NewConnectOptions() (runtime.Object, bool, string) {
	return nil, false, "" // true means you can use the trailing path as a variable
}

func (r *subChildrenREST) Connect(ctx context.Context, name string, opts runtime.Object, responder rest.Responder) (http.Handler, error) {
	return http.HandlerFunc(func(w http.ResponseWriter, req *http.Request) {
		ns, err := request.NamespaceInfoFrom(ctx, true)
		if err != nil {
			responder.Error(err)
			return
		}

		user, err := appcontext.User(ctx)
		if err != nil {
			responder.Error(err)
			return
		}

		children, err := r.service.GetChildren(ctx, &folder.GetChildrenQuery{
			SignedInUser: user,
			UID:          name,
			OrgID:        ns.OrgID,
		})
		if err != nil {
			responder.Error(err)
			return
		}

		info := &v0alpha1.FolderInfo{
			Items: make([]v0alpha1.FolderItem, 0),
		}
		for _, parent := range children {
			info.Items = append(info.Items, v0alpha1.FolderItem{
				Name:  parent.UID,
				Title: parent.Title,
			})
		}
		responder.Object(http.StatusOK, info)
	}), nil
}
