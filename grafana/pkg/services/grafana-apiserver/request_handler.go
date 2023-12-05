package grafanaapiserver

import (
	"fmt"
	"net/http"
	"strings"

	"github.com/gorilla/mux"
	restclient "k8s.io/client-go/rest"
	"k8s.io/kube-openapi/pkg/spec3"
)

type requestHandler struct {
	router *mux.Router
}

func getAPIHandler(delegateHandler http.Handler, restConfig *restclient.Config, builders []APIGroupBuilder) (http.Handler, error) {
	useful := false // only true if any routes exist anywhere
	router := mux.NewRouter()
	var err error

	for _, builder := range builders {
		routes := builder.GetAPIRoutes()
		if routes == nil {
			continue
		}

		gv := builder.GetGroupVersion()
		prefix := "/apis/" + gv.String()

		// Root handlers
		var sub *mux.Router
		for _, route := range routes.Root {
			err = validPath(route.Path)
			if err != nil {
				return nil, err
			}

			if sub == nil {
				sub = router.PathPrefix(prefix).Subrouter()
				sub.MethodNotAllowedHandler = &methodNotAllowedHandler{}
			}

			useful = true
			methods, err := methodsFromSpec(route.Path, route.Spec)
			if err != nil {
				return nil, err
			}
			sub.HandleFunc(route.Path, route.Handler).
				Methods(methods...)
		}

		// Namespace handlers
		sub = nil
		prefix += "/namespaces/{namespace}"
		for _, route := range routes.Namespace {
			err = validPath(route.Path)
			if err != nil {
				return nil, err
			}
			if sub == nil {
				sub = router.PathPrefix(prefix).Subrouter()
				sub.MethodNotAllowedHandler = &methodNotAllowedHandler{}
			}

			useful = true
			methods, err := methodsFromSpec(route.Path, route.Spec)
			if err != nil {
				return nil, err
			}
			sub.HandleFunc(route.Path, route.Handler).
				Methods(methods...)
		}
	}

	if !useful {
		return delegateHandler, nil
	}

	// Per Gorilla Mux issue here: https://github.com/gorilla/mux/issues/616#issuecomment-798807509
	// default handler must come last
	router.PathPrefix("/").Handler(delegateHandler)

	return &requestHandler{
		router: router,
	}, nil
}

// The registered path must start with a slash, and (for now) not have any more
func validPath(p string) error {
	if !strings.HasPrefix(p, "/") {
		return fmt.Errorf("path must start with slash")
	}
	if strings.Count(p, "/") > 1 {
		return fmt.Errorf("path can only have one slash (for now)")
	}
	return nil
}

func (h *requestHandler) ServeHTTP(w http.ResponseWriter, req *http.Request) {
	h.router.ServeHTTP(w, req)
}

func methodsFromSpec(slug string, props *spec3.PathProps) ([]string, error) {
	if props == nil {
		return []string{"GET", "POST", "PUT", "PATCH", "DELETE"}, nil
	}

	methods := make([]string, 0)
	if props.Get != nil {
		methods = append(methods, "GET")
	}
	if props.Post != nil {
		methods = append(methods, "POST")
	}
	if props.Put != nil {
		methods = append(methods, "PUT")
	}
	if props.Patch != nil {
		methods = append(methods, "PATCH")
	}
	if props.Delete != nil {
		methods = append(methods, "DELETE")
	}

	if len(methods) == 0 {
		return nil, fmt.Errorf("invalid OpenAPI Spec for slug=%s without any methods in PathProps", slug)
	}

	return methods, nil
}

type methodNotAllowedHandler struct{}

func (h *methodNotAllowedHandler) ServeHTTP(w http.ResponseWriter, req *http.Request) {
	w.WriteHeader(405) // method not allowed
}

// Modify the the OpenAPI spec to include the additional routes.
// Currently this requires: https://github.com/kubernetes/kube-openapi/pull/420
// In future k8s release, the hook will use Config3 rather than the same hook for both v2 and v3
func GetOpenAPIPostProcessor(builders []APIGroupBuilder) func(*spec3.OpenAPI) (*spec3.OpenAPI, error) {
	return func(s *spec3.OpenAPI) (*spec3.OpenAPI, error) {
		if s.Paths == nil {
			return s, nil
		}
		for _, builder := range builders {
			routes := builder.GetAPIRoutes()
			if routes == nil {
				continue
			}

			gv := builder.GetGroupVersion()
			prefix := "/apis/" + gv.String()
			if s.Paths.Paths[prefix] != nil {
				copy := *s // will copy the rest of the properties
				copy.Info.Title = "Grafana API server: " + gv.Group

				for _, route := range routes.Root {
					copy.Paths.Paths[prefix+route.Path] = &spec3.Path{
						PathProps: *route.Spec,
					}
				}

				for _, route := range routes.Namespace {
					copy.Paths.Paths[prefix+"/namespaces/{namespace}"+route.Path] = &spec3.Path{
						PathProps: *route.Spec,
					}
				}

				return &copy, nil
			}
		}
		return s, nil
	}
}
