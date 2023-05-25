package gitserver

import (
	"crypto/md5"
	"encoding/binary"
	"sync"
	"sync/atomic"
	"testing"

	"github.com/prometheus/client_golang/prometheus"
	"github.com/prometheus/client_golang/prometheus/promauto"
	"golang.org/x/exp/slices"
	"google.golang.org/grpc"

	"github.com/sourcegraph/log"
	"github.com/sourcegraph/log/logtest"

	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/conf"

	"github.com/sourcegraph/sourcegraph/internal/gitserver/protocol"
	proto "github.com/sourcegraph/sourcegraph/internal/gitserver/v1"
	"github.com/sourcegraph/sourcegraph/internal/grpc/defaults"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

const maxMessageSizeBytes = 64 * 1024 * 1024 // 64MiB

var addrForRepoInvoked = promauto.NewCounterVec(prometheus.CounterOpts{
	Name: "src_gitserver_addr_for_repo_invoked",
	Help: "Number of times gitserver.AddrForRepo was invoked",
}, []string{"user_agent"})

// NewGitserverAddressesFromConf fetches the current set of gitserver addresses
// and pinned repos for gitserver.
func NewGitserverAddressesFromConf(cfg *conf.Unified) GitserverAddresses {
	addrs := GitserverAddresses{
		Addresses: cfg.ServiceConnectionConfig.GitServers,
	}
	if cfg.ExperimentalFeatures != nil {
		addrs.PinnedServers = cfg.ExperimentalFeatures.GitServerPinnedRepos
	}
	return addrs
}

type TestClientSourceOptions struct {
	// ClientFunc is the function that is used to return a gRPC client
	// given the provided connection.
	ClientFunc func(conn *grpc.ClientConn) proto.GitserverServiceClient

	// Logger is the log.Logger instance that the test ClientSource will use to
	// log various metadata to.
	Logger log.Logger
}

func NewTestClientSource(t *testing.T, addrs []string, options ...func(o *TestClientSourceOptions)) ClientSource {
	logger := logtest.Scoped(t)
	opts := TestClientSourceOptions{
		ClientFunc: func(conn *grpc.ClientConn) proto.GitserverServiceClient {
			return proto.NewGitserverServiceClient(conn)
		},

		Logger: logger,
	}

	for _, o := range options {
		o(&opts)
	}

	conns := make(map[string]connAndErr)
	var testAddresses []AddressWithClient
	for _, addr := range addrs {
		conn, err := defaults.Dial(addr, logger)
		conns[addr] = connAndErr{address: addr, conn: conn, err: err}
		testAddresses = append(testAddresses, &testConnAndErr{
			address:    addr,
			conn:       conn,
			err:        err,
			clientFunc: opts.ClientFunc,
		})
	}

	source := testGitserverConns{
		conns: &GitserverConns{
			GitserverAddresses: GitserverAddresses{
				Addresses: addrs,
			},
			grpcConns: conns,
		},
		testAddresses: testAddresses,

		clientFunc: opts.ClientFunc,
	}

	return &source
}

type testGitserverConns struct {
	conns         *GitserverConns
	testAddresses []AddressWithClient

	clientFunc func(conn *grpc.ClientConn) proto.GitserverServiceClient
}

// AddrForRepo returns the gitserver address to use for the given repo name.
func (c *testGitserverConns) AddrForRepo(userAgent string, repo api.RepoName) string {
	return c.conns.AddrForRepo(userAgent, repo)
}

// Addresses returns the current list of gitserver addresses.
func (c *testGitserverConns) Addresses() []AddressWithClient {
	return c.testAddresses
}

// ClientForRepo returns a client or host for the given repo name.
func (c *testGitserverConns) ClientForRepo(userAgent string, repo api.RepoName) (proto.GitserverServiceClient, error) {
	conn, err := c.conns.ConnForRepo(userAgent, repo)
	if err != nil {
		return nil, err
	}

	return c.clientFunc(conn), nil
}

type testConnAndErr struct {
	address    string
	conn       *grpc.ClientConn
	err        error
	clientFunc func(conn *grpc.ClientConn) proto.GitserverServiceClient
}

// Address implements AddressWithClient
func (t *testConnAndErr) Address() string {
	return t.address
}

// GRPCClient implements AddressWithClient
func (t *testConnAndErr) GRPCClient() (proto.GitserverServiceClient, error) {
	return t.clientFunc(t.conn), t.err
}

var _ ClientSource = &testGitserverConns{}
var _ AddressWithClient = &testConnAndErr{}

type GitserverAddresses struct {
	// The current list of gitserver addresses
	Addresses []string

	// A list of overrides to pin a repo to a specific gitserver instance. This
	// ensures that, even if the number of gitservers changes, these repos will
	// not be moved.
	PinnedServers map[string]string
}

// AddrForRepo returns the gitserver address to use for the given repo name.
func (g GitserverAddresses) AddrForRepo(userAgent string, repo api.RepoName) string {
	addrForRepoInvoked.WithLabelValues(userAgent).Inc()

	repo = protocol.NormalizeRepo(repo) // in case the caller didn't already normalize it
	rs := string(repo)

	if pinnedAddr, ok := g.PinnedServers[rs]; ok {
		return pinnedAddr
	}

	return addrForKey(rs, g.Addresses)
}

// addrForKey returns the gitserver address to use for the given string key,
// which is hashed for sharding purposes.
func addrForKey(key string, addrs []string) string {
	sum := md5.Sum([]byte(key))
	serverIndex := binary.BigEndian.Uint64(sum[:]) % uint64(len(addrs))
	return addrs[serverIndex]
}

type GitserverConns struct {
	GitserverAddresses
	// invariant: there is one conn for every gitserver address
	grpcConns map[string]connAndErr
}

func (g *GitserverConns) ConnForRepo(userAgent string, repo api.RepoName) (*grpc.ClientConn, error) {
	addr := g.AddrForRepo(userAgent, repo)
	ce, ok := g.grpcConns[addr]
	if !ok {
		return nil, errors.Newf("no gRPC connection found for address %q", addr)
	}
	return ce.conn, ce.err
}

// AddressWithClient is a gitserver address with a client.
type AddressWithClient interface {
	Address() string                                   // returns the address of the endpoint that this GRPC client is targeting
	GRPCClient() (proto.GitserverServiceClient, error) // returns the gRPC client to use to contact the given address
}

type connAndErr struct {
	address string
	conn    *grpc.ClientConn
	err     error
}

func (c *connAndErr) Address() string {
	return c.address
}

func (c *connAndErr) GRPCClient() (proto.GitserverServiceClient, error) {
	return proto.NewGitserverServiceClient(c.conn), c.err
}

type atomicGitServerConns struct {
	conns     atomic.Pointer[GitserverConns]
	watchOnce sync.Once
}

func (a *atomicGitServerConns) AddrForRepo(userAgent string, repo api.RepoName) string {
	return a.get().AddrForRepo(userAgent, repo)
}

func (a *atomicGitServerConns) ClientForRepo(userAgent string, repo api.RepoName) (proto.GitserverServiceClient, error) {
	conn, err := a.get().ConnForRepo(userAgent, repo)
	if err != nil {
		return nil, err
	}
	return proto.NewGitserverServiceClient(conn), nil
}

func (a *atomicGitServerConns) Addresses() []AddressWithClient {
	conns := a.get()
	addrs := make([]AddressWithClient, 0, len(conns.Addresses))
	for _, addr := range conns.Addresses {
		addrs = append(addrs, &connAndErr{
			address: addr,
			conn:    conns.grpcConns[addr].conn,
			err:     conns.grpcConns[addr].err,
		})
	}
	return addrs
}

func (a *atomicGitServerConns) get() *GitserverConns {
	a.initOnce()
	return a.conns.Load()
}

func (a *atomicGitServerConns) initOnce() {
	// Initialize lazily because conf.Watch cannot be used during init time.
	a.watchOnce.Do(func() {
		conf.Watch(func() {
			a.update(conf.Get())
		})
	})
}

func (a *atomicGitServerConns) update(cfg *conf.Unified) {
	after := GitserverConns{
		GitserverAddresses: NewGitserverAddressesFromConf(cfg),
		grpcConns:          nil, // to be filled in
	}

	before := a.conns.Load()
	if before == nil {
		before = &GitserverConns{}
	}

	if slices.Equal(before.Addresses, after.Addresses) {
		// No change in addresses. Reuse the old connections.
		// We still update newAddrs in case the pinned repos have changed.
		after.grpcConns = before.grpcConns
		a.conns.Store(&after)
		return
	}
	log.Scoped("", "gitserver gRPC connections").Info(
		"new gitserver addresses",
		log.Strings("before", before.Addresses),
		log.Strings("after", after.Addresses),
	)

	// Open connections for each address
	clientLogger := log.Scoped("gitserver.client", "gitserver gRPC client")

	after.grpcConns = make(map[string]connAndErr, len(after.Addresses))
	for _, addr := range after.Addresses {
		conn, err := defaults.Dial(
			addr,
			clientLogger,

			// Allow large messages to accomodate large diffs
			grpc.WithDefaultCallOptions(grpc.MaxCallRecvMsgSize(maxMessageSizeBytes)),
		)
		after.grpcConns[addr] = connAndErr{conn: conn, err: err}
	}

	a.conns.Store(&after)

	// After making the new conns visible, close the old conns
	for _, ce := range before.grpcConns {
		if ce.err == nil {
			ce.conn.Close()
		}
	}
}

var _ ClientSource = &atomicGitServerConns{}
var _ AddressWithClient = &connAndErr{}
