package entity

import (
	"encoding/json"
	"fmt"
	"reflect"
	"strconv"
	"strings"
	"time"

	"k8s.io/apimachinery/pkg/api/meta"
	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"
	"k8s.io/apimachinery/pkg/runtime"
	"k8s.io/apimachinery/pkg/types"
	"k8s.io/apiserver/pkg/endpoints/request"

	"github.com/grafana/grafana/pkg/infra/grn"
	"github.com/grafana/grafana/pkg/kinds"
	entityStore "github.com/grafana/grafana/pkg/services/store/entity"
)

type Key struct {
	Group       string
	Resource    string
	Namespace   string
	Name        string
	Subresource string
}

func ParseKey(key string) (*Key, error) {
	// /<group>/<resource>/<namespace>/<name>(/<subresource>)
	parts := strings.SplitN(key, "/", 6)
	if len(parts) != 5 && len(parts) != 6 {
		return nil, fmt.Errorf("invalid key (expecting 4 or 5 parts) " + key)
	}

	if parts[0] != "" {
		return nil, fmt.Errorf("invalid key (expecting leading slash) " + key)
	}

	k := &Key{
		Group:     parts[1],
		Resource:  parts[2],
		Namespace: parts[3],
		Name:      parts[4],
	}

	if len(parts) == 6 {
		k.Subresource = parts[5]
	}

	return k, nil
}

func (k *Key) String() string {
	if len(k.Subresource) > 0 {
		return fmt.Sprintf("/%s/%s/%s/%s/%s", k.Group, k.Resource, k.Namespace, k.Name, k.Subresource)
	}
	return fmt.Sprintf("/%s/%s/%s/%s", k.Group, k.Resource, k.Namespace, k.Name)
}

func (k *Key) IsEqual(other *Key) bool {
	return k.Group == other.Group &&
		k.Resource == other.Resource &&
		k.Namespace == other.Namespace &&
		k.Name == other.Name &&
		k.Subresource == other.Subresource
}

func (k *Key) TenantID() (int64, error) {
	if k.Namespace == "default" {
		return 1, nil
	}
	tid := strings.Split(k.Namespace, "-")
	if len(tid) != 2 || !(tid[0] == "org" || tid[0] == "tenant") {
		return 0, fmt.Errorf("invalid namespace, expected org|tenant-${#}")
	}
	intVar, err := strconv.ParseInt(tid[1], 10, 64)
	if err != nil {
		return 0, fmt.Errorf("invalid namespace, expected number")
	}
	return intVar, nil
}

func (k *Key) ToGRN() (*grn.GRN, error) {
	tid, err := k.TenantID()
	if err != nil {
		return nil, err
	}

	fullResource := k.Resource
	if k.Subresource != "" {
		fullResource = fmt.Sprintf("%s/%s", k.Resource, k.Subresource)
	}

	return &grn.GRN{
		ResourceGroup:      k.Group,
		ResourceKind:       fullResource,
		ResourceIdentifier: k.Name,
		TenantID:           tid,
	}, nil
}

// Convert an etcd key to GRN style
func keyToGRN(key string) (*grn.GRN, error) {
	k, err := ParseKey(key)
	if err != nil {
		return nil, err
	}
	return k.ToGRN()
}

// this is terrible... but just making it work!!!!
func entityToResource(rsp *entityStore.Entity, res runtime.Object) error {
	var err error

	metaAccessor, err := meta.Accessor(res)
	if err != nil {
		return err
	}

	if rsp.GRN == nil {
		return fmt.Errorf("invalid entity, missing GRN")
	}

	if len(rsp.Meta) > 0 {
		err = json.Unmarshal(rsp.Meta, res)
		if err != nil {
			return err
		}
	}

	metaAccessor.SetName(rsp.GRN.ResourceIdentifier)
	if rsp.GRN.TenantID != 1 {
		metaAccessor.SetNamespace(fmt.Sprintf("tenant-%d", rsp.GRN.TenantID))
	} else {
		metaAccessor.SetNamespace("default") // org 1
	}
	metaAccessor.SetUID(types.UID(rsp.Guid))
	metaAccessor.SetResourceVersion(rsp.Version)
	metaAccessor.SetCreationTimestamp(metav1.Unix(rsp.CreatedAt/1000, rsp.CreatedAt%1000*1000000))

	grafanaAccessor := kinds.MetaAccessor(metaAccessor)

	if rsp.Folder != "" {
		grafanaAccessor.SetFolder(rsp.Folder)
	}
	if rsp.CreatedBy != "" {
		grafanaAccessor.SetCreatedBy(rsp.CreatedBy)
	}
	if rsp.UpdatedBy != "" {
		grafanaAccessor.SetUpdatedBy(rsp.UpdatedBy)
	}
	if rsp.UpdatedAt != 0 {
		updatedAt := time.UnixMilli(rsp.UpdatedAt).UTC()
		grafanaAccessor.SetUpdatedTimestamp(&updatedAt)
	}
	grafanaAccessor.SetSlug(rsp.Slug)

	if rsp.Origin != nil {
		originTime := time.UnixMilli(rsp.Origin.Time).UTC()
		grafanaAccessor.SetOriginInfo(&kinds.ResourceOriginInfo{
			Name: rsp.Origin.Source,
			Key:  rsp.Origin.Key,
			// Path: rsp.Origin.Path,
			Timestamp: &originTime,
		})
	}

	if len(rsp.Labels) > 0 {
		metaAccessor.SetLabels(rsp.Labels)
	}

	// TODO fields?

	if len(rsp.Body) > 0 {
		spec := reflect.ValueOf(res).Elem().FieldByName("Spec")
		if spec != (reflect.Value{}) && spec.CanSet() {
			err = json.Unmarshal(rsp.Body, spec.Addr().Interface())
			if err != nil {
				return err
			}
		}
	}

	if len(rsp.Status) > 0 {
		status := reflect.ValueOf(res).Elem().FieldByName("Status")
		if status != (reflect.Value{}) && status.CanSet() {
			err = json.Unmarshal(rsp.Status, status.Addr().Interface())
			if err != nil {
				return err
			}
		}
	}

	return nil
}

func resourceToEntity(key string, res runtime.Object, requestInfo *request.RequestInfo) (*entityStore.Entity, error) {
	metaAccessor, err := meta.Accessor(res)
	if err != nil {
		return nil, err
	}

	g, err := keyToGRN(key)
	if err != nil {
		return nil, err
	}

	grafanaAccessor := kinds.MetaAccessor(metaAccessor)

	rsp := &entityStore.Entity{
		GRN:          g,
		GroupVersion: requestInfo.APIVersion,
		Key:          key,
		Name:         metaAccessor.GetName(),
		Guid:         string(metaAccessor.GetUID()),
		Version:      metaAccessor.GetResourceVersion(),
		Folder:       grafanaAccessor.GetFolder(),
		CreatedAt:    metaAccessor.GetCreationTimestamp().Time.UnixMilli(),
		CreatedBy:    grafanaAccessor.GetCreatedBy(),
		UpdatedBy:    grafanaAccessor.GetUpdatedBy(),
		Slug:         grafanaAccessor.GetSlug(),
		Origin: &entityStore.EntityOriginInfo{
			Source: grafanaAccessor.GetOriginName(),
			Key:    grafanaAccessor.GetOriginKey(),
			// Path: 	grafanaAccessor.GetOriginPath(),
		},
		Labels: metaAccessor.GetLabels(),
	}

	if t := grafanaAccessor.GetUpdatedTimestamp(); t != nil {
		rsp.UpdatedAt = t.UnixMilli()
	}

	if t := grafanaAccessor.GetOriginTimestamp(); t != nil {
		rsp.Origin.Time = t.UnixMilli()
	}

	rsp.Meta, err = json.Marshal(meta.AsPartialObjectMetadata(metaAccessor))
	if err != nil {
		return nil, err
	}

	// TODO: store entire object in body?
	spec := reflect.ValueOf(res).Elem().FieldByName("Spec")
	if spec != (reflect.Value{}) {
		rsp.Body, err = json.Marshal(spec.Interface())
		if err != nil {
			return nil, err
		}
	}

	status := reflect.ValueOf(res).Elem().FieldByName("Status")
	if status != (reflect.Value{}) {
		rsp.Status, err = json.Marshal(status.Interface())
		if err != nil {
			return nil, err
		}
	}

	return rsp, nil
}
