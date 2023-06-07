package fakes

import (
	"archive/zip"
	"context"
	"fmt"
	"io/fs"
	"sync"

	"github.com/grafana/grafana-plugin-sdk-go/backend"

	"github.com/grafana/grafana/pkg/plugins"
	"github.com/grafana/grafana/pkg/plugins/backendplugin"
	"github.com/grafana/grafana/pkg/plugins/log"
	"github.com/grafana/grafana/pkg/plugins/repo"
	"github.com/grafana/grafana/pkg/plugins/storage"
)

type FakePluginInstaller struct {
	AddFunc func(ctx context.Context, pluginID, version string, opts plugins.CompatOpts) error
	// Remove removes a plugin from the store.
	RemoveFunc func(ctx context.Context, pluginID string) error
}

func (i *FakePluginInstaller) Add(ctx context.Context, pluginID, version string, opts plugins.CompatOpts) error {
	if i.AddFunc != nil {
		return i.AddFunc(ctx, pluginID, version, opts)
	}
	return nil
}

func (i *FakePluginInstaller) Remove(ctx context.Context, pluginID string) error {
	if i.RemoveFunc != nil {
		return i.RemoveFunc(ctx, pluginID)
	}
	return nil
}

type FakeLoader struct {
	LoadFunc   func(_ context.Context, _ plugins.PluginSource) ([]*plugins.Plugin, error)
	UnloadFunc func(_ context.Context, _ string) error
}

func (l *FakeLoader) Load(ctx context.Context, src plugins.PluginSource) ([]*plugins.Plugin, error) {
	if l.LoadFunc != nil {
		return l.LoadFunc(ctx, src)
	}
	return nil, nil
}

func (l *FakeLoader) Unload(ctx context.Context, pluginID string) error {
	if l.UnloadFunc != nil {
		return l.UnloadFunc(ctx, pluginID)
	}
	return nil
}

type FakePluginClient struct {
	ID      string
	Managed bool
	Log     log.Logger

	startCount     int
	stopCount      int
	exited         bool
	decommissioned bool
	backend.CollectMetricsHandlerFunc
	backend.CheckHealthHandlerFunc
	backend.QueryDataHandlerFunc
	backend.CallResourceHandlerFunc
	mutex sync.RWMutex

	backendplugin.Plugin
}

func (pc *FakePluginClient) PluginID() string {
	return pc.ID
}

func (pc *FakePluginClient) Logger() log.Logger {
	return pc.Log
}

func (pc *FakePluginClient) Start(_ context.Context) error {
	pc.mutex.Lock()
	defer pc.mutex.Unlock()
	pc.exited = false
	pc.startCount++
	return nil
}

func (pc *FakePluginClient) Stop(_ context.Context) error {
	pc.mutex.Lock()
	defer pc.mutex.Unlock()
	pc.stopCount++
	pc.exited = true
	return nil
}

func (pc *FakePluginClient) IsManaged() bool {
	return pc.Managed
}

func (pc *FakePluginClient) Exited() bool {
	pc.mutex.RLock()
	defer pc.mutex.RUnlock()
	return pc.exited
}

func (pc *FakePluginClient) Decommission() error {
	pc.mutex.Lock()
	defer pc.mutex.Unlock()
	pc.decommissioned = true
	return nil
}

func (pc *FakePluginClient) IsDecommissioned() bool {
	pc.mutex.RLock()
	defer pc.mutex.RUnlock()
	return pc.decommissioned
}

func (pc *FakePluginClient) CollectMetrics(ctx context.Context, req *backend.CollectMetricsRequest) (*backend.CollectMetricsResult, error) {
	if pc.CollectMetricsHandlerFunc != nil {
		return pc.CollectMetricsHandlerFunc(ctx, req)
	}

	return nil, backendplugin.ErrMethodNotImplemented
}

func (pc *FakePluginClient) CheckHealth(ctx context.Context, req *backend.CheckHealthRequest) (*backend.CheckHealthResult, error) {
	if pc.CheckHealthHandlerFunc != nil {
		return pc.CheckHealthHandlerFunc(ctx, req)
	}

	return nil, backendplugin.ErrMethodNotImplemented
}

func (pc *FakePluginClient) QueryData(ctx context.Context, req *backend.QueryDataRequest) (*backend.QueryDataResponse, error) {
	if pc.QueryDataHandlerFunc != nil {
		return pc.QueryDataHandlerFunc(ctx, req)
	}

	return nil, backendplugin.ErrMethodNotImplemented
}

func (pc *FakePluginClient) CallResource(ctx context.Context, req *backend.CallResourceRequest, sender backend.CallResourceResponseSender) error {
	if pc.CallResourceHandlerFunc != nil {
		return pc.CallResourceHandlerFunc(ctx, req, sender)
	}

	return backendplugin.ErrMethodNotImplemented
}

func (pc *FakePluginClient) SubscribeStream(_ context.Context, _ *backend.SubscribeStreamRequest) (*backend.SubscribeStreamResponse, error) {
	return nil, backendplugin.ErrMethodNotImplemented
}

func (pc *FakePluginClient) PublishStream(_ context.Context, _ *backend.PublishStreamRequest) (*backend.PublishStreamResponse, error) {
	return nil, backendplugin.ErrMethodNotImplemented
}

func (pc *FakePluginClient) RunStream(_ context.Context, _ *backend.RunStreamRequest, _ *backend.StreamSender) error {
	return backendplugin.ErrMethodNotImplemented
}

type FakePluginRegistry struct {
	Store map[string]*plugins.Plugin
}

func NewFakePluginRegistry() *FakePluginRegistry {
	return &FakePluginRegistry{
		Store: make(map[string]*plugins.Plugin),
	}
}

func (f *FakePluginRegistry) Plugin(_ context.Context, id string) (*plugins.Plugin, bool) {
	p, exists := f.Store[id]
	return p, exists
}

func (f *FakePluginRegistry) Plugins(_ context.Context) []*plugins.Plugin {
	res := make([]*plugins.Plugin, 0, len(f.Store))
	for _, p := range f.Store {
		res = append(res, p)
	}

	return res
}

func (f *FakePluginRegistry) Add(_ context.Context, p *plugins.Plugin) error {
	f.Store[p.ID] = p
	return nil
}

func (f *FakePluginRegistry) Remove(_ context.Context, id string) error {
	delete(f.Store, id)
	return nil
}

type FakePluginRepo struct {
	GetPluginArchiveFunc      func(_ context.Context, pluginID, version string, _ repo.CompatOpts) (*repo.PluginArchive, error)
	GetPluginArchiveByURLFunc func(_ context.Context, archiveURL string, _ repo.CompatOpts) (*repo.PluginArchive, error)
	GetPluginArchiveInfoFunc  func(_ context.Context, pluginID, version string, _ repo.CompatOpts) (*repo.PluginArchiveInfo, error)
}

// GetPluginArchive fetches the requested plugin archive.
func (r *FakePluginRepo) GetPluginArchive(ctx context.Context, pluginID, version string, opts repo.CompatOpts) (*repo.PluginArchive, error) {
	if r.GetPluginArchiveFunc != nil {
		return r.GetPluginArchiveFunc(ctx, pluginID, version, opts)
	}

	return &repo.PluginArchive{}, nil
}

// GetPluginArchiveByURL fetches the requested plugin from the specified URL.
func (r *FakePluginRepo) GetPluginArchiveByURL(ctx context.Context, archiveURL string, opts repo.CompatOpts) (*repo.PluginArchive, error) {
	if r.GetPluginArchiveByURLFunc != nil {
		return r.GetPluginArchiveByURLFunc(ctx, archiveURL, opts)
	}

	return &repo.PluginArchive{}, nil
}

// GetPluginArchiveInfo fetches information for downloading the requested plugin.
func (r *FakePluginRepo) GetPluginArchiveInfo(ctx context.Context, pluginID, version string, opts repo.CompatOpts) (*repo.PluginArchiveInfo, error) {
	if r.GetPluginArchiveInfoFunc != nil {
		return r.GetPluginArchiveInfoFunc(ctx, pluginID, version, opts)
	}
	return &repo.PluginArchiveInfo{}, nil
}

type FakePluginStorage struct {
	ExtractFunc func(_ context.Context, pluginID string, z *zip.ReadCloser) (*storage.ExtractedPluginArchive, error)
}

func NewFakePluginStorage() *FakePluginStorage {
	return &FakePluginStorage{}
}

func (s *FakePluginStorage) Extract(ctx context.Context, pluginID string, z *zip.ReadCloser) (*storage.ExtractedPluginArchive, error) {
	if s.ExtractFunc != nil {
		return s.ExtractFunc(ctx, pluginID, z)
	}
	return &storage.ExtractedPluginArchive{}, nil
}

type FakeProcessManager struct {
	StartFunc func(_ context.Context, pluginID string) error
	StopFunc  func(_ context.Context, pluginID string) error
	Started   map[string]int
	Stopped   map[string]int
}

func NewFakeProcessManager() *FakeProcessManager {
	return &FakeProcessManager{
		Started: make(map[string]int),
		Stopped: make(map[string]int),
	}
}

func (m *FakeProcessManager) Start(ctx context.Context, pluginID string) error {
	m.Started[pluginID]++
	if m.StartFunc != nil {
		return m.StartFunc(ctx, pluginID)
	}
	return nil
}

func (m *FakeProcessManager) Stop(ctx context.Context, pluginID string) error {
	m.Stopped[pluginID]++
	if m.StopFunc != nil {
		return m.StopFunc(ctx, pluginID)
	}
	return nil
}

type FakeBackendProcessProvider struct {
	Requested          map[string]int
	Invoked            map[string]int
	BackendFactoryFunc func(context.Context, *plugins.Plugin) backendplugin.PluginFactoryFunc
}

func NewFakeBackendProcessProvider() *FakeBackendProcessProvider {
	f := &FakeBackendProcessProvider{
		Requested: make(map[string]int),
		Invoked:   make(map[string]int),
	}
	f.BackendFactoryFunc = func(ctx context.Context, p *plugins.Plugin) backendplugin.PluginFactoryFunc {
		f.Requested[p.ID]++
		return func(pluginID string, _ log.Logger, _ []string) (backendplugin.Plugin, error) {
			f.Invoked[pluginID]++
			return &FakePluginClient{}, nil
		}
	}
	return f
}

func (pr *FakeBackendProcessProvider) BackendFactory(ctx context.Context, p *plugins.Plugin) backendplugin.PluginFactoryFunc {
	return pr.BackendFactoryFunc(ctx, p)
}

type FakeLicensingService struct {
	LicenseEdition string
	TokenRaw       string
	LicensePath    string
	LicenseAppURL  string
}

func NewFakeLicensingService() *FakeLicensingService {
	return &FakeLicensingService{}
}

func (s *FakeLicensingService) Edition() string {
	return s.LicenseEdition
}

func (s *FakeLicensingService) Path() string {
	return s.LicensePath
}

func (s *FakeLicensingService) AppURL() string {
	return s.LicenseAppURL
}

func (s *FakeLicensingService) Environment() []string {
	return []string{fmt.Sprintf("GF_ENTERPRISE_LICENSE_TEXT=%s", s.TokenRaw)}
}

type FakeRoleRegistry struct {
	ExpectedErr error
}

func NewFakeRoleRegistry() *FakeRoleRegistry {
	return &FakeRoleRegistry{}
}

func (f *FakeRoleRegistry) DeclarePluginRoles(_ context.Context, _ string, _ string, _ []plugins.RoleRegistration) error {
	return f.ExpectedErr
}

type FakePluginFiles struct {
	OpenFunc   func(name string) (fs.File, error)
	RemoveFunc func() error

	base string
}

func NewFakePluginFiles(base string) *FakePluginFiles {
	return &FakePluginFiles{
		base: base,
	}
}

func (f *FakePluginFiles) Open(name string) (fs.File, error) {
	if f.OpenFunc != nil {
		return f.OpenFunc(name)
	}
	return nil, nil
}

func (f *FakePluginFiles) Base() string {
	return f.base
}

func (f *FakePluginFiles) Files() ([]string, error) {
	return []string{}, nil
}

func (f *FakePluginFiles) Remove() error {
	if f.RemoveFunc != nil {
		return f.RemoveFunc()
	}
	return nil
}

type FakeSourceRegistry struct {
	ListFunc func(_ context.Context) []plugins.PluginSource
}

func (s *FakeSourceRegistry) List(ctx context.Context) []plugins.PluginSource {
	if s.ListFunc != nil {
		return s.ListFunc(ctx)
	}
	return []plugins.PluginSource{}
}

type FakePluginSource struct {
	PluginClassFunc      func(ctx context.Context) plugins.Class
	PluginURIsFunc       func(ctx context.Context) []string
	DefaultSignatureFunc func(ctx context.Context) (plugins.Signature, bool)
}

func (s *FakePluginSource) PluginClass(ctx context.Context) plugins.Class {
	if s.PluginClassFunc != nil {
		return s.PluginClassFunc(ctx)
	}
	return ""
}

func (s *FakePluginSource) PluginURIs(ctx context.Context) []string {
	if s.PluginURIsFunc != nil {
		return s.PluginURIsFunc(ctx)
	}
	return []string{}
}

func (s *FakePluginSource) DefaultSignature(ctx context.Context) (plugins.Signature, bool) {
	if s.DefaultSignatureFunc != nil {
		return s.DefaultSignatureFunc(ctx)
	}
	return plugins.Signature{}, false
}

type FakePluginFileStore struct {
	FileFunc func(ctx context.Context, pluginID, filename string) (*plugins.File, error)
}

func (f *FakePluginFileStore) File(ctx context.Context, pluginID, filename string) (*plugins.File, error) {
	if f.FileFunc != nil {
		return f.FileFunc(ctx, pluginID, filename)
	}
	return nil, nil
}
