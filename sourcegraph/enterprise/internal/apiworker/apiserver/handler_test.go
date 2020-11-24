package apiserver

import (
	"context"
	"testing"

	"github.com/efritz/glock"
	"github.com/google/go-cmp/cmp"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/apiworker/apiclient"
	"github.com/sourcegraph/sourcegraph/internal/workerutil"
	workerstoremocks "github.com/sourcegraph/sourcegraph/internal/workerutil/dbworker/store/mocks"
)

func TestDequeue(t *testing.T) {
	transformedJob := apiclient.Job{
		ID: 42,
		DockerSteps: []apiclient.DockerStep{
			{
				Image:    "alpine:latest",
				Commands: []string{"ls", "-a"},
			},
		},
	}

	store := workerstoremocks.NewMockStore()
	store.DequeueWithIndependentTransactionContextFunc.SetDefaultReturn(testRecord{ID: 42, Payload: "secret"}, store, true, nil)
	recordTransformer := func(record workerutil.Record) (apiclient.Job, error) {
		if tr, ok := record.(testRecord); !ok {
			t.Errorf("mismatched record type.")
		} else if tr.Payload != "secret" {
			t.Errorf("unexpected payload. want=%q have=%q", "secret", tr.Payload)
		}

		return transformedJob, nil
	}

	options := Options{
		QueueOptions: map[string]QueueOptions{
			"test_queue": {Store: store, RecordTransformer: recordTransformer},
		},
		MaximumNumTransactions: 10,
	}
	handler := newHandler(options, glock.NewMockClock())

	job, dequeued, err := handler.dequeue(context.Background(), "test_queue", "deadbeef")
	if err != nil {
		t.Fatalf("unexpected error dequeueing job: %s", err)
	}
	if !dequeued {
		t.Fatalf("expected job to be dequeued")
	}
	if job.ID != 42 {
		t.Errorf("unexpected id. want=%d have=%d", 42, job.ID)
	}
	if diff := cmp.Diff(transformedJob, job); diff != "" {
		t.Errorf("unexpected job (-want +got):\n%s", diff)
	}
}

func TestDequeueNoRecord(t *testing.T) {
	options := Options{
		QueueOptions: map[string]QueueOptions{
			"test_queue": {Store: workerstoremocks.NewMockStore()},
		},
		MaximumNumTransactions: 10,
	}
	handler := newHandler(options, glock.NewMockClock())

	_, dequeued, err := handler.dequeue(context.Background(), "test_queue", "deadbeef")
	if err != nil {
		t.Fatalf("unexpected error dequeueing job: %s", err)
	}
	if dequeued {
		t.Fatalf("did not expect a job to be dequeued")
	}
}

func TestDequeueUnknownQueue(t *testing.T) {
	options := Options{}
	handler := newHandler(options, glock.NewMockClock())

	if _, _, err := handler.dequeue(context.Background(), "test_queue", "deadbeef"); err != ErrUnknownQueue {
		t.Fatalf("unexpected error. want=%q have=%q", ErrUnknownQueue, err)
	}
}

func TestDequeueMaxTransactions(t *testing.T) {
	store := workerstoremocks.NewMockStore()
	store.DequeueWithIndependentTransactionContextFunc.PushReturn(testRecord{ID: 41}, store, true, nil)
	store.DequeueWithIndependentTransactionContextFunc.PushReturn(testRecord{ID: 42}, store, true, nil)
	store.DequeueWithIndependentTransactionContextFunc.PushReturn(testRecord{ID: 43}, store, true, nil)
	recordTransformer := func(record workerutil.Record) (apiclient.Job, error) { return apiclient.Job{}, nil }

	options := Options{
		QueueOptions: map[string]QueueOptions{
			"test_queue": {Store: store, RecordTransformer: recordTransformer},
		},
		MaximumNumTransactions: 2,
	}
	handler := newHandler(options, glock.NewMockClock())

	_, dequeued1, err := handler.dequeue(context.Background(), "test_queue", "deadbeef")
	if err != nil {
		t.Fatalf("unexpected error dequeueing job: %s", err)
	}
	if !dequeued1 {
		t.Fatalf("expected job to be dequeued")
	}

	_, dequeued2, err := handler.dequeue(context.Background(), "test_queue", "deadbeef")
	if err != nil {
		t.Fatalf("unexpected error dequeueing job: %s", err)
	}
	if !dequeued2 {
		t.Fatalf("expected a second job to be dequeued")
	}

	_, dequeued3, err := handler.dequeue(context.Background(), "test_queue", "deadbeef")
	if err != nil {
		t.Fatalf("unexpected error dequeueing job: %s", err)
	}
	if dequeued3 {
		t.Fatalf("did not expect a third job to be dequeued")
	}

	if err := handler.markComplete(context.Background(), "test_queue", "deadbeef", 42); err != nil {
		t.Fatalf("unexpected error completing job: %s", err)
	}

	_, dequeued4, err := handler.dequeue(context.Background(), "test_queue", "deadbeef")
	if err != nil {
		t.Fatalf("unexpected error dequeueing job: %s", err)
	}
	if !dequeued4 {
		t.Fatalf("expected a third job to be dequeued after a release")
	}
}

func TestSetLogContents(t *testing.T) {
	store := workerstoremocks.NewMockStore()
	store.DequeueWithIndependentTransactionContextFunc.SetDefaultReturn(testRecord{ID: 42}, store, true, nil)
	recordTransformer := func(record workerutil.Record) (apiclient.Job, error) { return apiclient.Job{ID: 42}, nil }

	options := Options{
		QueueOptions: map[string]QueueOptions{
			"test_queue": {Store: store, RecordTransformer: recordTransformer},
		},
		MaximumNumTransactions: 10,
	}
	handler := newHandler(options, glock.NewMockClock())

	job, dequeued, err := handler.dequeue(context.Background(), "test_queue", "deadbeef")
	if err != nil {
		t.Fatalf("unexpected error dequeueing job: %s", err)
	}
	if !dequeued {
		t.Fatalf("expected a job to be dequeued")
	}

	if err := handler.setLogContents(context.Background(), "test_queue", "deadbeef", job.ID, "<log payload>"); err != nil {
		t.Fatalf("unexpected error setting log contents: %s", err)
	}

	if value := len(store.SetLogContentsFunc.History()); value != 1 {
		t.Fatalf("unexpected number of calls to SetLogContents. want=%d have=%d", 1, value)
	}
	call := store.SetLogContentsFunc.History()[0]
	if call.Arg1 != 42 {
		t.Errorf("unexpected job identifier. want=%d have=%d", 42, call.Arg1)
	}
	if call.Arg2 != "<log payload>" {
		t.Errorf("unexpected log contents. want=%s have=%s", "<log payload>", call.Arg2)
	}
}

func TestSetLogContentsUnknownQueue(t *testing.T) {
	options := Options{}
	handler := newHandler(options, glock.NewMockClock())

	if err := handler.setLogContents(context.Background(), "test_queue", "deadbjeef", 42, "<log payload>"); err != ErrUnknownQueue {
		t.Fatalf("unexpected error. want=%q have=%q", ErrUnknownQueue, err)
	}
}

func TestSetLogContentsUnknownJob(t *testing.T) {
	options := Options{
		QueueOptions: map[string]QueueOptions{
			"test_queue": {Store: workerstoremocks.NewMockStore()},
		},
	}
	handler := newHandler(options, glock.NewMockClock())

	if err := handler.setLogContents(context.Background(), "test_queue", "deadbeef", 42, "<log payload>"); err != ErrUnknownJob {
		t.Fatalf("unexpected error. want=%q have=%q", ErrUnknownJob, err)
	}
}

func TestMarkComplete(t *testing.T) {
	store := workerstoremocks.NewMockStore()
	store.DequeueWithIndependentTransactionContextFunc.SetDefaultReturn(testRecord{ID: 42}, store, true, nil)
	recordTransformer := func(record workerutil.Record) (apiclient.Job, error) { return apiclient.Job{ID: 42}, nil }

	options := Options{
		QueueOptions: map[string]QueueOptions{
			"test_queue": {Store: store, RecordTransformer: recordTransformer},
		},
		MaximumNumTransactions: 10,
	}
	handler := newHandler(options, glock.NewMockClock())

	job, dequeued, err := handler.dequeue(context.Background(), "test_queue", "deadbeef")
	if err != nil {
		t.Fatalf("unexpected error dequeueing job: %s", err)
	}
	if !dequeued {
		t.Fatalf("expected a job to be dequeued")
	}

	if err := handler.markComplete(context.Background(), "test_queue", "deadbeef", job.ID); err != nil {
		t.Fatalf("unexpected error completing job: %s", err)
	}

	if value := len(store.MarkCompleteFunc.History()); value != 1 {
		t.Fatalf("unexpected number of calls to MarkComplete. want=%d have=%d", 1, value)
	}
	call := store.MarkCompleteFunc.History()[0]
	if call.Arg1 != 42 {
		t.Errorf("unexpected job identifier. want=%d have=%d", 42, call.Arg1)
	}
}

func TestMarkCompleteUnknownJob(t *testing.T) {
	options := Options{
		QueueOptions: map[string]QueueOptions{
			"test_queue": {Store: workerstoremocks.NewMockStore()},
		},
	}
	handler := newHandler(options, glock.NewMockClock())

	if err := handler.markComplete(context.Background(), "test_queue", "deadbeef", 42); err != ErrUnknownJob {
		t.Fatalf("unexpected error. want=%q have=%q", ErrUnknownJob, err)
	}
}

func TestMarkCompleteUnknownQueue(t *testing.T) {
	options := Options{}
	handler := newHandler(options, glock.NewMockClock())

	if err := handler.markComplete(context.Background(), "test_queue", "deadbeef", 42); err != ErrUnknownQueue {
		t.Fatalf("unexpected error. want=%q have=%q", ErrUnknownQueue, err)
	}
}

func TestMarkErrored(t *testing.T) {
	store := workerstoremocks.NewMockStore()
	store.DequeueWithIndependentTransactionContextFunc.SetDefaultReturn(testRecord{ID: 42}, store, true, nil)
	recordTransformer := func(record workerutil.Record) (apiclient.Job, error) { return apiclient.Job{ID: 42}, nil }

	options := Options{
		QueueOptions: map[string]QueueOptions{
			"test_queue": {Store: store, RecordTransformer: recordTransformer},
		},
		MaximumNumTransactions: 10,
	}
	handler := newHandler(options, glock.NewMockClock())

	job, dequeued, err := handler.dequeue(context.Background(), "test_queue", "deadbeef")
	if err != nil {
		t.Fatalf("unexpected error dequeueing job: %s", err)
	}
	if !dequeued {
		t.Fatalf("expected a job to be dequeued")
	}

	if err := handler.markErrored(context.Background(), "test_queue", "deadbeef", job.ID, "OH NO"); err != nil {
		t.Fatalf("unexpected error completing job: %s", err)
	}

	if value := len(store.MarkErroredFunc.History()); value != 1 {
		t.Fatalf("unexpected number of calls to MarkErrored. want=%d have=%d", 1, value)
	}
	call := store.MarkErroredFunc.History()[0]
	if call.Arg1 != 42 {
		t.Errorf("unexpected job identifier. want=%d have=%d", 42, call.Arg1)
	}
	if call.Arg2 != "OH NO" {
		t.Errorf("unexpected job error. want=%s have=%s", "OH NO", call.Arg2)
	}
}

func TestMarkErroredUnknownJob(t *testing.T) {
	options := Options{
		QueueOptions: map[string]QueueOptions{
			"test_queue": {Store: workerstoremocks.NewMockStore()},
		},
	}
	handler := newHandler(options, glock.NewMockClock())

	if err := handler.markErrored(context.Background(), "test_queue", "deadbeef", 42, "OH NO"); err != ErrUnknownJob {
		t.Fatalf("unexpected error. want=%q have=%q", ErrUnknownJob, err)
	}
}

func TestMarkErroredUnknownQueue(t *testing.T) {
	options := Options{}
	handler := newHandler(options, glock.NewMockClock())

	if err := handler.markErrored(context.Background(), "test_queue", "deadbeef", 42, "OH NO"); err != ErrUnknownQueue {
		t.Fatalf("unexpected error. want=%q have=%q", ErrUnknownQueue, err)
	}
}

func TestMarkFailed(t *testing.T) {
	store := workerstoremocks.NewMockStore()
	store.DequeueWithIndependentTransactionContextFunc.SetDefaultReturn(testRecord{ID: 42}, store, true, nil)
	recordTransformer := func(record workerutil.Record) (apiclient.Job, error) { return apiclient.Job{ID: 42}, nil }

	options := Options{
		QueueOptions: map[string]QueueOptions{
			"test_queue": {Store: store, RecordTransformer: recordTransformer},
		},
		MaximumNumTransactions: 10,
	}
	handler := newHandler(options, glock.NewMockClock())

	job, dequeued, err := handler.dequeue(context.Background(), "test_queue", "deadbeef")
	if err != nil {
		t.Fatalf("unexpected error dequeueing job: %s", err)
	}
	if !dequeued {
		t.Fatalf("expected a job to be dequeued")
	}

	if err := handler.markFailed(context.Background(), "test_queue", "deadbeef", job.ID, "OH NO"); err != nil {
		t.Fatalf("unexpected error completing job: %s", err)
	}

	if value := len(store.MarkFailedFunc.History()); value != 1 {
		t.Fatalf("unexpected number of calls to MarkFailed. want=%d have=%d", 1, value)
	}
	call := store.MarkFailedFunc.History()[0]
	if call.Arg1 != 42 {
		t.Errorf("unexpected job identifier. want=%d have=%d", 42, call.Arg1)
	}
	if call.Arg2 != "OH NO" {
		t.Errorf("unexpected job error. want=%s have=%s", "OH NO", call.Arg2)
	}
}

type testRecord struct {
	ID      int
	Payload string
}

func (r testRecord) RecordID() int { return r.ID }
