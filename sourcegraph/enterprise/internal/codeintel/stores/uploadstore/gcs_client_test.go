package uploadstore

import (
	"bytes"
	"context"
	"io"
	"io/ioutil"
	"testing"
	"time"

	"cloud.google.com/go/storage"
	"github.com/sourcegraph/sourcegraph/internal/observation"
)

func TestGCSInit(t *testing.T) {
	gcsClient := NewMockGcsAPI()
	bucketHandle := NewMockGcsBucketHandle()
	gcsClient.BucketFunc.SetDefaultReturn(bucketHandle)
	bucketHandle.AttrsFunc.SetDefaultReturn(nil, storage.ErrBucketNotExist)

	client := testGCSClient(gcsClient, true)
	if err := client.Init(context.Background()); err != nil {
		t.Fatalf("unexpected error initializing client: %s", err.Error())
	}

	if calls := gcsClient.BucketFunc.History(); len(calls) != 1 {
		t.Fatalf("unexpected number of Bucket calls. want=%d have=%d", 1, len(calls))
	} else if value := calls[0].Arg0; value != "test-bucket" {
		t.Errorf("unexpected bucket argument. want=%s have=%s", "test-bucket", value)
	}

	if calls := bucketHandle.CreateFunc.History(); len(calls) != 1 {
		t.Fatalf("unexpected number of Create calls. want=%d have=%d", 1, len(calls))
	} else if value := calls[0].Arg1; value != "pid" {
		t.Errorf("unexpected projectId argument. want=%s have=%s", "pid", value)
	}
	if calls := bucketHandle.UpdateFunc.History(); len(calls) != 0 {
		t.Fatalf("unexpected number of Update calls. want=%d have=%d", 0, len(calls))
	}
}

func TestGCSInitBucketExists(t *testing.T) {
	gcsClient := NewMockGcsAPI()
	bucketHandle := NewMockGcsBucketHandle()
	gcsClient.BucketFunc.SetDefaultReturn(bucketHandle)

	client := testGCSClient(gcsClient, true)
	if err := client.Init(context.Background()); err != nil {
		t.Fatalf("unexpected error initializing client: %s", err.Error())
	}

	if calls := gcsClient.BucketFunc.History(); len(calls) != 1 {
		t.Fatalf("unexpected number of Bucket calls. want=%d have=%d", 1, len(calls))
	} else if value := calls[0].Arg0; value != "test-bucket" {
		t.Errorf("unexpected bucket argument. want=%s have=%s", "test-bucket", value)
	}

	if calls := bucketHandle.CreateFunc.History(); len(calls) != 0 {
		t.Fatalf("unexpected number of Create calls. want=%d have=%d", 0, len(calls))
	}
	if calls := bucketHandle.UpdateFunc.History(); len(calls) != 1 {
		t.Fatalf("unexpected number of Update calls. want=%d have=%d", 1, len(calls))
	}
}

func TestGCSUnmanagedInit(t *testing.T) {
	gcsClient := NewMockGcsAPI()
	bucketHandle := NewMockGcsBucketHandle()
	gcsClient.BucketFunc.SetDefaultReturn(bucketHandle)
	bucketHandle.AttrsFunc.SetDefaultReturn(nil, storage.ErrBucketNotExist)

	client := testGCSClient(gcsClient, false)
	if err := client.Init(context.Background()); err != nil {
		t.Fatalf("unexpected error initializing client: %s", err.Error())
	}

	if calls := gcsClient.BucketFunc.History(); len(calls) != 0 {
		t.Fatalf("unexpected number of Bucket calls. want=%d have=%d", 0, len(calls))
	}
	if calls := bucketHandle.UpdateFunc.History(); len(calls) != 0 {
		t.Fatalf("unexpected number of Update calls. want=%d have=%d", 0, len(calls))
	}
	if calls := bucketHandle.CreateFunc.History(); len(calls) != 0 {
		t.Fatalf("unexpected number of Create calls. want=%d have=%d", 0, len(calls))
	}
}

func TestGCSGet(t *testing.T) {
	gcsClient := NewMockGcsAPI()
	bucketHandle := NewMockGcsBucketHandle()
	objectHandle := NewMockGcsObjectHandle()
	gcsClient.BucketFunc.SetDefaultReturn(bucketHandle)
	bucketHandle.ObjectFunc.SetDefaultReturn(objectHandle)
	objectHandle.NewRangeReaderFunc.SetDefaultReturn(ioutil.NopCloser(bytes.NewReader([]byte("TEST PAYLOAD"))), nil)

	client := testGCSClient(gcsClient, false)
	rc, err := client.Get(context.Background(), "test-key", 0)
	if err != nil {
		t.Fatalf("unexpected error getting key: %s", err.Error())
	}

	defer rc.Close()
	contents, err := ioutil.ReadAll(rc)
	if err != nil {
		t.Fatalf("unexpected error reading object: %s", err.Error())
	}

	if string(contents) != "TEST PAYLOAD" {
		t.Fatalf("unexpected contents. want=%s have=%s", "TEST PAYLOAD", contents)
	}

	if calls := gcsClient.BucketFunc.History(); len(calls) != 1 {
		t.Fatalf("unexpected number of Bucket calls. want=%d have=%d", 1, len(calls))
	} else if value := calls[0].Arg0; value != "test-bucket" {
		t.Errorf("unexpected bucket argument. want=%s have=%s", "test-bucket", value)
	}

	if calls := objectHandle.NewRangeReaderFunc.History(); len(calls) != 1 {
		t.Fatalf("unexpected number of NewRangeReader calls. want=%d have=%d", 1, len(calls))
	} else if value := calls[0].Arg1; value != 0 {
		t.Errorf("unexpected offset argument. want=%d have=%d", 0, value)
	} else if value := calls[0].Arg2; value != -1 {
		t.Errorf("unexpected length argument. want=%d have=%d", -1, value)
	}
}

func TestGCSGetSkipBytes(t *testing.T) {
	gcsClient := NewMockGcsAPI()
	bucketHandle := NewMockGcsBucketHandle()
	objectHandle := NewMockGcsObjectHandle()
	gcsClient.BucketFunc.SetDefaultReturn(bucketHandle)
	bucketHandle.ObjectFunc.SetDefaultReturn(objectHandle)
	objectHandle.NewRangeReaderFunc.SetDefaultReturn(ioutil.NopCloser(bytes.NewReader([]byte("TEST PAYLOAD"))), nil)

	client := testGCSClient(gcsClient, false)
	rc, err := client.Get(context.Background(), "test-key", 20)
	if err != nil {
		t.Fatalf("unexpected error getting key: %s", err.Error())
	}

	defer rc.Close()
	contents, err := ioutil.ReadAll(rc)
	if err != nil {
		t.Fatalf("unexpected error reading object: %s", err.Error())
	}

	if string(contents) != "TEST PAYLOAD" {
		t.Fatalf("unexpected contents. want=%s have=%s", "TEST PAYLOAD", contents)
	}

	if calls := gcsClient.BucketFunc.History(); len(calls) != 1 {
		t.Fatalf("unexpected number of Bucket calls. want=%d have=%d", 1, len(calls))
	} else if value := calls[0].Arg0; value != "test-bucket" {
		t.Errorf("unexpected bucket argument. want=%s have=%s", "test-bucket", value)
	}

	if calls := objectHandle.NewRangeReaderFunc.History(); len(calls) != 1 {
		t.Fatalf("unexpected number of NewRangeReader calls. want=%d have=%d", 1, len(calls))
	} else if value := calls[0].Arg1; value != 20 {
		t.Errorf("unexpected offset argument. want=%d have=%d", 20, value)
	} else if value := calls[0].Arg2; value != -1 {
		t.Errorf("unexpected length argument. want=%d have=%d", -1, value)
	}
}

func TestGCSUpload(t *testing.T) {
	buf := &bytes.Buffer{}

	gcsClient := NewMockGcsAPI()
	bucketHandle := NewMockGcsBucketHandle()
	objectHandle := NewMockGcsObjectHandle()

	gcsClient.BucketFunc.SetDefaultReturn(bucketHandle)
	bucketHandle.ObjectFunc.SetDefaultReturn(objectHandle)
	objectHandle.NewWriterFunc.SetDefaultReturn(nopCloser{buf})

	client := testGCSClient(gcsClient, false)

	size, err := client.Upload(context.Background(), "test-key", bytes.NewReader([]byte("TEST PAYLOAD")))
	if err != nil {
		t.Fatalf("unexpected error getting key: %s", err.Error())
	} else if size != 12 {
		t.Errorf("unexpected size`. want=%d have=%d", 12, size)
	}

	if calls := gcsClient.BucketFunc.History(); len(calls) != 1 {
		t.Fatalf("unexpected number of Bucket calls. want=%d have=%d", 1, len(calls))
	} else if value := calls[0].Arg0; value != "test-bucket" {
		t.Errorf("unexpected bucket argument. want=%s have=%s", "test-bucket", value)
	}

	if calls := objectHandle.NewWriterFunc.History(); len(calls) != 1 {
		t.Fatalf("unexpected number of NewWriter calls. want=%d have=%d", 1, len(calls))
	} else {
		if value := buf.String(); value != "TEST PAYLOAD" {
			t.Errorf("unexpected payload. want=%s have=%s", "TEST PAYLOAD", value)
		}
	}
}

func TestGCSCombine(t *testing.T) {
	gcsClient := NewMockGcsAPI()
	bucketHandle := NewMockGcsBucketHandle()
	objectHandle1 := NewMockGcsObjectHandle()
	objectHandle2 := NewMockGcsObjectHandle()
	objectHandle3 := NewMockGcsObjectHandle()
	objectHandle4 := NewMockGcsObjectHandle()
	composer := NewMockGcsComposer()
	composer.RunFunc.SetDefaultReturn(&storage.ObjectAttrs{Size: 42}, nil)

	gcsClient.BucketFunc.SetDefaultReturn(bucketHandle)
	objectHandle1.ComposerFromFunc.SetDefaultReturn(composer)
	bucketHandle.ObjectFunc.SetDefaultHook(func(name string) gcsObjectHandle {
		return map[string]gcsObjectHandle{
			"test-key":  objectHandle1,
			"test-src1": objectHandle2,
			"test-src2": objectHandle3,
			"test-src3": objectHandle4,
		}[name]
	})

	client := testGCSClient(gcsClient, false)

	size, err := client.Compose(context.Background(), "test-key", "test-src1", "test-src2", "test-src3")
	if err != nil {
		t.Fatalf("unexpected error getting key: %s", err.Error())
	} else if size != 42 {
		t.Errorf("unexpected size`. want=%d have=%d", 42, size)
	}

	if calls := objectHandle1.ComposerFromFunc.History(); len(calls) != 1 {
		t.Fatalf("unexpected number of ComposerFrom calls. want=%d have=%d", 1, len(calls))
	} else {
		expectedHandles := []gcsObjectHandle{
			objectHandle2,
			objectHandle3,
			objectHandle4,
		}

		matches := 0
		for _, candidate := range expectedHandles {
			for _, handle := range calls[0].Arg0 {
				if handle == candidate {
					matches++
				}
			}
		}

		if matches != len(calls[0].Arg0) {
			t.Errorf("unexpected instances. want=%d to match but have=%d", len(calls[0].Arg0), matches)
		}
	}

	if calls := composer.RunFunc.History(); len(calls) != 1 {
		t.Fatalf("unexpected number of Run calls. want=%d have=%d", 1, len(calls))
	}

	if calls := objectHandle2.DeleteFunc.History(); len(calls) != 1 {
		t.Fatalf("unexpected number of Delete calls. want=%d have=%d", 1, len(calls))
	}
	if calls := objectHandle3.DeleteFunc.History(); len(calls) != 1 {
		t.Fatalf("unexpected number of Delete calls. want=%d have=%d", 1, len(calls))
	}
	if calls := objectHandle4.DeleteFunc.History(); len(calls) != 1 {
		t.Fatalf("unexpected number of Delete calls. want=%d have=%d", 1, len(calls))
	}
}

func TestGCSDelete(t *testing.T) {
	gcsClient := NewMockGcsAPI()
	bucketHandle := NewMockGcsBucketHandle()
	objectHandle := NewMockGcsObjectHandle()
	gcsClient.BucketFunc.SetDefaultReturn(bucketHandle)
	bucketHandle.ObjectFunc.SetDefaultReturn(objectHandle)
	objectHandle.NewRangeReaderFunc.SetDefaultReturn(ioutil.NopCloser(bytes.NewReader([]byte("TEST PAYLOAD"))), nil)

	client := testGCSClient(gcsClient, false)
	if err := client.Delete(context.Background(), "test-key"); err != nil {
		t.Fatalf("unexpected error getting key: %s", err.Error())
	}

	if calls := gcsClient.BucketFunc.History(); len(calls) != 1 {
		t.Fatalf("unexpected number of Bucket calls. want=%d have=%d", 1, len(calls))
	} else if value := calls[0].Arg0; value != "test-bucket" {
		t.Errorf("unexpected bucket argument. want=%s have=%s", "test-bucket", value)
	}

	if calls := objectHandle.DeleteFunc.History(); len(calls) != 1 {
		t.Fatalf("unexpected number of Delete calls. want=%d have=%d", 1, len(calls))
	}
}

func TestGCSLifecycle(t *testing.T) {
	client := rawGCSClient(nil, true)

	if lifecycle := client.lifecycle(); len(lifecycle.Rules) != 1 {
		t.Fatalf("unexpected lifecycle rules")
	} else if value := lifecycle.Rules[0].Condition.AgeInDays; value != 3 {
		t.Errorf("unexpected expiration days. want=%d have=%d", 3, value)
	}
}

func testGCSClient(client gcsAPI, manageBucket bool) Store {
	return newLazyStore(rawGCSClient(client, manageBucket))
}

func rawGCSClient(client gcsAPI, manageBucket bool) *gcsStore {
	return newGCSWithClient(client, "test-bucket", time.Hour*24*3, manageBucket, GCSConfig{ProjectID: "pid"}, makeOperations(&observation.TestContext))
}

type nopCloser struct {
	io.Writer
}

func (nopCloser) Close() error {
	return nil
}
