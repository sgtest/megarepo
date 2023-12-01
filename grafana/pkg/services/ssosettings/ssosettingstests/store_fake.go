package ssosettingstests

import (
	context "context"

	"github.com/grafana/grafana/pkg/services/ssosettings"
	models "github.com/grafana/grafana/pkg/services/ssosettings/models"
)

var _ ssosettings.Store = (*FakeStore)(nil)

type FakeStore struct {
	ExpectedSSOSetting  *models.SSOSettings
	ExpectedSSOSettings []*models.SSOSettings
	ExpectedError       error
}

func NewFakeStore() *FakeStore {
	return &FakeStore{}
}

func (f *FakeStore) Get(ctx context.Context, provider string) (*models.SSOSettings, error) {
	return f.ExpectedSSOSetting, f.ExpectedError
}

func (f *FakeStore) List(ctx context.Context) ([]*models.SSOSettings, error) {
	return f.ExpectedSSOSettings, f.ExpectedError
}

func (f *FakeStore) Upsert(ctx context.Context, settings models.SSOSettings) error {
	return f.ExpectedError
}

func (f *FakeStore) Patch(ctx context.Context, provider string, data map[string]any) error {
	return f.ExpectedError
}

func (f *FakeStore) Delete(ctx context.Context, provider string) error {
	return f.ExpectedError
}
