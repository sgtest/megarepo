package folders

import (
	"fmt"

	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"

	"github.com/grafana/grafana/pkg/apis/folders/v0alpha1"
	"github.com/grafana/grafana/pkg/kinds"
	"github.com/grafana/grafana/pkg/services/folder"
	"github.com/grafana/grafana/pkg/services/grafana-apiserver/endpoints/request"
	"github.com/grafana/grafana/pkg/services/grafana-apiserver/utils"
)

func convertToK8sResource(v *folder.Folder, namespacer request.NamespaceMapper) *v0alpha1.Folder {
	meta := kinds.GrafanaResourceMetadata{}
	meta.SetUpdatedTimestampMillis(v.Updated.UnixMilli())
	if v.ID > 0 { // nolint:staticcheck
		meta.SetOriginInfo(&kinds.ResourceOriginInfo{
			Name: "SQL",
			Key:  fmt.Sprintf("%d", v.ID), // nolint:staticcheck
		})
	}
	if v.CreatedBy > 0 {
		meta.SetCreatedBy(fmt.Sprintf("user:%d", v.CreatedBy))
	}
	if v.UpdatedBy > 0 {
		meta.SetUpdatedBy(fmt.Sprintf("user:%d", v.UpdatedBy))
	}
	f := &v0alpha1.Folder{
		TypeMeta: v0alpha1.FolderResourceInfo.TypeMeta(),
		ObjectMeta: metav1.ObjectMeta{
			Name:              v.UID,
			ResourceVersion:   fmt.Sprintf("%d", v.Updated.UnixMilli()),
			CreationTimestamp: metav1.NewTime(v.Created),
			Namespace:         namespacer(v.OrgID),
			Annotations:       meta.Annotations,
		},
		Spec: v0alpha1.Spec{
			Title:       v.Title,
			Description: v.Description,
		},
	}
	f.UID = utils.CalculateClusterWideUID(f)
	return f
}
