package cloud

import (
	"fmt"
	"strconv"
	"strings"
	"time"

	cloudapiv1 "github.com/sourcegraph/cloud-api/go/cloudapi/v1"

	"github.com/sourcegraph/sourcegraph/lib/errors"
	"github.com/sourcegraph/sourcegraph/lib/pointers"
)

var ErrLeaseTimeNotSet error = errors.New("lease time not set")

// EphemeralInstanceType is the instance type for ephemeral instances. An instance is considered ephemeral if it
// contains "ephemeral_instance": "true" in its Instance Features
const EphemeralInstanceType = "ephemeral"

// InternalInstanceType is the instance type for internal instances. An instance is considered internal if it it is
// in the Dev cloud environment and does not contain "ephemeral_instance": "true" in its Instance Features
const InternalInstanceType = "internal"

type Instance struct {
	ID           string `json:"id"`
	Name         string `json:"name"`
	InstanceType string `json:"instanceType"`
	Environment  string `json:"environment"`
	Version      string `json:"version"`
	URL          string `json:"hostname"`
	AdminEmail   string `json:"adminEmail"`

	CreatedAt time.Time `json:"createdAt"`
	DeletedAt time.Time `json:"deletedAt"`
	ExpiresAt time.Time `json:"ExpiresAt"`

	Project string         `json:"project"`
	Region  string         `json:"region"`
	Status  InstanceStatus `json:"status"`
	// contains various key value pairs that are specific to the instance type
	features *InstanceFeatures
}

func (i *Instance) String() string {
	return fmt.Sprintf(`ID           : %s
Name         : %s
InstanceType : %s
Environment  : %s
Version      : %s
URL          : %s
AdminEmail   : %s
CreatedAt    : %s
DeletetAt    : %s
ExpiresAt    : %s
Project      : %s
Region       : %s
Status       : %s
ActionURL    : %s
Error        : %s
`, i.ID, i.Name, i.InstanceType, i.Environment, i.Version, i.URL, i.AdminEmail,
		i.CreatedAt.Format(time.RFC3339), i.DeletedAt.Format(time.RFC3339), i.ExpiresAt.Format(time.RFC3339), i.Project, i.Region,
		i.Status.Status, i.Status.ActionURL, i.Status.Error)
}

func (i *Instance) IsEphemeral() bool {
	return i.InstanceType == EphemeralInstanceType
}

func (i *Instance) IsInternal() bool {
	return i.InstanceType == InternalInstanceType
}

func (i *Instance) IsExpired() bool {
	if i.ExpiresAt.IsZero() {
		return false
	}

	return time.Now().After(i.ExpiresAt)
}

type InstanceStatus struct {
	Status    string `json:"status"`
	ActionURL string `json:"actionUrl"`
	Error     string `json:"error"`
}

type InstanceFeatures struct {
	features map[string]string
}

func newInstanceStatus(src *cloudapiv1.InstanceState) (*InstanceStatus, error) {
	url, reason, err := parseStatusReason(src.GetReason())
	if err != nil {
		return nil, err
	}

	status := InstanceStatus{
		ActionURL: url,
	}
	switch src.GetInstanceStatus() {
	case cloudapiv1.InstanceStatus_INSTANCE_STATUS_UNSPECIFIED:
		status.Status = "unspecified"
	case cloudapiv1.InstanceStatus_INSTANCE_STATUS_OK:
		status.Status = "completed"
	case cloudapiv1.InstanceStatus_INSTANCE_STATUS_PROGRESSING:
		status.Status = "in progress"
	case cloudapiv1.InstanceStatus_INSTANCE_STATUS_FAILED:
		status.Status = "failed"
		status.Error = reason
	default:
		status.Status = "unknown"
	}

	return &status, nil
}

func newInstance(src *cloudapiv1.Instance) (*Instance, error) {
	details := src.GetInstanceDetails()
	platform := src.GetPlatformDetails()
	status, err := newInstanceStatus(src.GetInstanceState())
	if err != nil {
		return nil, err
	}
	features := newInstanceFeaturesFrom(details.GetInstanceFeatures())
	expiresAt, err := features.GetEphemeralLeaseTime()
	if err != nil && !errors.Is(err, ErrLeaseTimeNotSet) {
		return nil, err
	}

	instanceType := InternalInstanceType
	if features.IsEphemeralInstance() {
		instanceType = EphemeralInstanceType
	}

	return &Instance{
		ID:           src.GetId(),
		Name:         details.Name,
		InstanceType: instanceType,
		Version:      details.Version,
		URL:          pointers.DerefZero(details.Url),
		AdminEmail:   pointers.DerefZero(details.AdminEmail),
		CreatedAt:    platform.GetCreatedAt().AsTime(),
		DeletedAt:    platform.GetDeletedAt().AsTime(),
		ExpiresAt:    expiresAt,
		Project:      platform.GetGcpProjectId(),
		Region:       platform.GetGcpRegion(),
		Status:       *status,
		features:     features,
	}, nil
}

func parseStatusReason(reason string) (string, string, error) {
	if reason == "" {
		return "", "", nil
	}
	parts := strings.Split(reason, ",")
	if len(parts) != 2 {
		return "", "", errors.Newf("invalid status reason format: %q", reason)
	}
	fieldValue := func(s string) (string, error) {
		colonIdx := strings.Index(s, ":")
		if colonIdx == -1 {
			return "", errors.Newf("invalid field format %q", s)
		}
		return s[colonIdx+1:], nil
	}

	url, err := fieldValue(parts[0])
	if err != nil {
		return "", "", errors.Wrapf(err, "field error at pos 0")
	}
	status, err := fieldValue(parts[1])
	if err != nil {
		return "", "", errors.Wrapf(err, "field error at pos 1")
	}

	return url, status, nil
}

func toInstances(items ...*cloudapiv1.Instance) ([]*Instance, error) {
	converted := []*Instance{}
	for _, item := range items {
		inst, err := newInstance(item)
		if err != nil {
			return nil, err
		}
		converted = append(converted, inst)
	}
	return converted, nil
}

func newInstanceFeaturesFrom(src map[string]string) *InstanceFeatures {
	return &InstanceFeatures{
		features: src,
	}
}
func newInstanceFeatures() *InstanceFeatures {
	return &InstanceFeatures{features: make(map[string]string)}
}

func (f *InstanceFeatures) IsEphemeralInstance() bool {
	v, ok := f.features["ephemeral_instance"]
	if !ok {
		return false
	}
	val, err := strconv.ParseBool(v)
	if err != nil {
		return false
	}

	return val
}

func (f *InstanceFeatures) SetEphemeralInstance(v bool) {
	f.features["ephemeral"] = strconv.FormatBool(v)
}

func (f *InstanceFeatures) SetEphemeralLeaseTime(expiresAt time.Time) {
	f.features["ephemeral_instance_lease_time"] = strconv.FormatInt(expiresAt.Unix(), 10)
}

func (f *InstanceFeatures) GetEphemeralLeaseTime() (time.Time, error) {
	seconds, ok := f.features["ephemeral_instance_lease_time"]
	if !ok {
		return time.Time{}, ErrLeaseTimeNotSet
	}
	secondsInt, err := strconv.ParseInt(seconds, 10, 64)
	if err != nil {
		return time.Time{}, errors.Newf("failed to convert 'ephemeral_instance_lease_time' value %q to int64: %v", seconds, err)
	}
	leaseTime := time.Unix(secondsInt, 0)
	return leaseTime, nil
}

func (f *InstanceFeatures) Value() map[string]string {
	return f.features
}
