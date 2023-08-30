package state

import (
	"context"
	"sync"

	"github.com/grafana/grafana/pkg/services/ngalert/models"
	history_model "github.com/grafana/grafana/pkg/services/ngalert/state/historian/model"
	"github.com/grafana/grafana/pkg/services/screenshot"
)

var _ InstanceStore = &FakeInstanceStore{}

type FakeInstanceStore struct {
	mtx         sync.Mutex
	RecordedOps []any
}

type FakeInstanceStoreOp struct {
	Name string
	Args []any
}

func (f *FakeInstanceStore) ListAlertInstances(_ context.Context, q *models.ListAlertInstancesQuery) ([]*models.AlertInstance, error) {
	f.mtx.Lock()
	defer f.mtx.Unlock()
	f.RecordedOps = append(f.RecordedOps, *q)
	return nil, nil
}

func (f *FakeInstanceStore) SaveAlertInstance(_ context.Context, q models.AlertInstance) error {
	f.mtx.Lock()
	defer f.mtx.Unlock()
	f.RecordedOps = append(f.RecordedOps, q)
	return nil
}

func (f *FakeInstanceStore) FetchOrgIds(_ context.Context) ([]int64, error) { return []int64{}, nil }

func (f *FakeInstanceStore) DeleteAlertInstances(ctx context.Context, q ...models.AlertInstanceKey) error {
	f.mtx.Lock()
	defer f.mtx.Unlock()
	f.RecordedOps = append(f.RecordedOps, FakeInstanceStoreOp{
		Name: "DeleteAlertInstances", Args: []any{
			ctx,
			q,
		},
	})
	return nil
}

func (f *FakeInstanceStore) DeleteAlertInstancesByRule(ctx context.Context, key models.AlertRuleKey) error {
	return nil
}

type FakeRuleReader struct{}

func (f *FakeRuleReader) ListAlertRules(_ context.Context, q *models.ListAlertRulesQuery) (models.RulesGroup, error) {
	return nil, nil
}

type FakeHistorian struct {
	StateTransitions []StateTransition
}

func (f *FakeHistorian) Record(ctx context.Context, rule history_model.RuleMeta, states []StateTransition) <-chan error {
	f.StateTransitions = append(f.StateTransitions, states...)
	errCh := make(chan error)
	close(errCh)
	return errCh
}

// NotAvailableImageService is a service that returns ErrScreenshotsUnavailable.
type NotAvailableImageService struct{}

func (s *NotAvailableImageService) NewImage(_ context.Context, _ *models.AlertRule) (*models.Image, error) {
	return nil, screenshot.ErrScreenshotsUnavailable
}

// NoopImageService is a no-op image service.
type NoopImageService struct{}

func (s *NoopImageService) NewImage(_ context.Context, _ *models.AlertRule) (*models.Image, error) {
	return &models.Image{}, nil
}
