package rest

import (
	"context"

	"k8s.io/apimachinery/pkg/api/meta"
	metainternalversion "k8s.io/apimachinery/pkg/apis/meta/internalversion"
	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"
	"k8s.io/apimachinery/pkg/runtime"
	"k8s.io/apimachinery/pkg/runtime/schema"
	"k8s.io/apiserver/pkg/apis/example"
	"k8s.io/apiserver/pkg/registry/rest"
	"k8s.io/klog/v2"
)

// Unified Storage Spy

type StorageSpyClient interface {
	Storage

	//Counts returns the number of times a certain method was called
	Counts(string) int
}

type StorageSpy struct {
	counts map[string]int
}

type spyStorageClient struct {
	Storage
	spy *StorageSpy
}

func (s *StorageSpy) record(seen string) {
	s.counts[seen]++
}

func NewStorageSpyClient(s Storage) StorageSpyClient {
	return &spyStorageClient{s, &StorageSpy{
		counts: map[string]int{},
	}}
}

func (c *spyStorageClient) Counts(method string) int {
	return c.spy.counts[method]
}

//nolint:golint,unused
type spyStorageShim struct {
	Storage
	spy *StorageSpy
}

//nolint:golint,unused
type spyLegacyStorageShim struct {
	LegacyStorage
	spy *StorageSpy
}

func (c *spyStorageClient) Create(ctx context.Context, obj runtime.Object, valitation rest.ValidateObjectFunc, options *metav1.CreateOptions) (runtime.Object, error) {
	c.spy.record("Storage.Create")
	klog.Info("method: Storage.Create")
	return &dummyObject{}, nil
}

func (c *spyStorageClient) Get(ctx context.Context, name string, options *metav1.GetOptions) (runtime.Object, error) {
	c.spy.record("Storage.Get")
	klog.Info("method: Storage.Get")
	return &example.Pod{}, nil
}

func (c *spyStorageClient) List(ctx context.Context, options *metainternalversion.ListOptions) (runtime.Object, error) {
	c.spy.record("Storage.List")
	klog.Info("method: Storage.List")

	i1 := dummyObject{Foo: "Storage field 1"}
	accessor, err := meta.Accessor(&i1)
	if err != nil {
		return nil, err
	}
	accessor.SetName("Item 1")

	i2 := dummyObject{Foo: "Storage field 2"}
	accessor, err = meta.Accessor(&i2)
	if err != nil {
		return nil, err
	}
	accessor.SetName("Item 2")

	return &dummyList{Items: []dummyObject{i1, i2}}, nil
}

type UpdatedObjInfoObj struct{}

func (u UpdatedObjInfoObj) UpdatedObject(ctx context.Context, oldObj runtime.Object) (newObj runtime.Object, err error) {
	return &example.Pod{}, nil
}

func (u UpdatedObjInfoObj) Preconditions() *metav1.Preconditions { return &metav1.Preconditions{} }

func (c *spyStorageClient) Update(ctx context.Context, name string, objInfo rest.UpdatedObjectInfo, createValidation rest.ValidateObjectFunc, updateValidation rest.ValidateObjectUpdateFunc, forceAllowCreate bool, options *metav1.UpdateOptions) (runtime.Object, bool, error) {
	c.spy.record("Storage.Update")
	klog.Info("method: Storage.Update")
	return &example.Pod{}, false, nil
}

func (c *spyStorageClient) Delete(ctx context.Context, name string, deleteValidation rest.ValidateObjectFunc, options *metav1.DeleteOptions) (runtime.Object, bool, error) {
	c.spy.record("Storage.Delete")
	klog.Info("method: Storage.Delete")
	return nil, false, nil
}

func (c *spyStorageClient) DeleteCollection(ctx context.Context, deleteValidation rest.ValidateObjectFunc, options *metav1.DeleteOptions, listOptions *metainternalversion.ListOptions) (runtime.Object, error) {
	c.spy.record("Storage.DeleteCollection")
	klog.Info("method: Storage.DeleteCollection")
	return nil, nil
}

// LegacyStorage Spy

type LegacyStorageSpyClient interface {
	LegacyStorage

	//Counts returns the number of times a certain method was called
	Counts(string) int
}

type LegacyStorageSpy struct {
	counts map[string]int //nolint:golint,unused
}

type spyLegacyStorageClient struct {
	LegacyStorage
	spy *StorageSpy
}

//nolint:golint,unused
func (s *LegacyStorageSpy) record(seen string) {
	s.counts[seen]++
}

func NewLegacyStorageSpyClient(ls LegacyStorage) LegacyStorageSpyClient {
	return &spyLegacyStorageClient{ls, &StorageSpy{
		counts: map[string]int{},
	}}
}

func (c *spyLegacyStorageClient) Counts(method string) int {
	return c.spy.counts[method]
}

func (c *spyLegacyStorageClient) Create(ctx context.Context, obj runtime.Object, valitation rest.ValidateObjectFunc, options *metav1.CreateOptions) (runtime.Object, error) {
	c.spy.record("LegacyStorage.Create")
	klog.Info("method: LegacyStorage.Create")
	return &dummyObject{}, nil
}

func (c *spyLegacyStorageClient) Get(ctx context.Context, name string, options *metav1.GetOptions) (runtime.Object, error) {
	c.spy.record("LegacyStorage.Get")
	klog.Info("method: LegacyStorage.Get")
	return &example.Pod{}, nil
}

func (c *spyLegacyStorageClient) NewList() runtime.Object {
	// stub for now so that spyLegacyStorageClient implements rest.Lister
	return nil
}

func (c *spyLegacyStorageClient) List(ctx context.Context, options *metainternalversion.ListOptions) (runtime.Object, error) {
	c.spy.record("LegacyStorage.List")
	klog.Info("method: LegacyStorage.List")

	i1 := dummyObject{Foo: "Legacy field 1"}
	accessor, err := meta.Accessor(&i1)
	if err != nil {
		return nil, err
	}
	accessor.SetName("Item 1")

	i3 := dummyObject{Foo: "Legacy field 3"}
	accessor, err = meta.Accessor(&i3)
	if err != nil {
		return nil, err
	}
	accessor.SetName("Item 3")

	return &dummyList{Items: []dummyObject{i1, i3}}, nil
}

func (c *spyLegacyStorageClient) Update(ctx context.Context, name string, objInfo rest.UpdatedObjectInfo, createValidation rest.ValidateObjectFunc, updateValidation rest.ValidateObjectUpdateFunc, forceAllowCreate bool, options *metav1.UpdateOptions) (runtime.Object, bool, error) {
	c.spy.record("LegacyStorage.Update")
	klog.Info("method: LegacyStorage.Update")
	return &example.Pod{}, false, nil
}

func (c *spyLegacyStorageClient) Delete(ctx context.Context, name string, deleteValidation rest.ValidateObjectFunc, options *metav1.DeleteOptions) (runtime.Object, bool, error) {
	c.spy.record("LegacyStorage.Delete")
	klog.Info("method: LegacyStorage.Delete")
	return nil, false, nil
}

func (c *spyLegacyStorageClient) DeleteCollection(ctx context.Context, deleteValidation rest.ValidateObjectFunc, options *metav1.DeleteOptions, listOptions *metainternalversion.ListOptions) (runtime.Object, error) {
	c.spy.record("LegacyStorage.DeleteCollection")
	klog.Info("method: LegacyStorage.DeleteCollection")
	return nil, nil
}

type dummyList struct {
	metav1.TypeMeta `json:",inline"`
	metav1.ListMeta `json:"metadata,omitempty"`
	Items           []dummyObject `json:"items,omitempty"`
}

type dummyObject struct {
	metav1.TypeMeta   `json:",inline"`
	Foo               string
	metav1.ObjectMeta `json:"metadata,omitempty"`
}

func (d *dummyList) GetObjectKind() schema.ObjectKind {
	return nil
}

func (d *dummyList) DeepCopyObject() runtime.Object {
	return nil
}

func (d *dummyObject) GetObjectKind() schema.ObjectKind {
	return nil
}

func (d *dummyObject) DeepCopyObject() runtime.Object {
	return nil
}
