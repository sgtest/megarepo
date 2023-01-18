package store

import (
	"bytes"
	"io"
	"strings"

	"github.com/gomodule/redigo/redis"
	"github.com/prometheus/client_golang/prometheus"
	dto "github.com/prometheus/client_model/go"
	"github.com/prometheus/common/expfmt"

	"github.com/sourcegraph/sourcegraph/internal/redispool"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

const DefaultMetricsExpiry = 30

type Store interface {
	prometheus.Gatherer
}

func NewDefaultStore() Store {
	return &defaultStore{}
}

type defaultStore struct{}

func (*defaultStore) Gather() ([]*dto.MetricFamily, error) {
	return prometheus.DefaultGatherer.Gather()
}

type DistributedStore interface {
	Store
	Ingest(instance string, mfs []*dto.MetricFamily) error
}

func NewDistributedStore(prefix string) DistributedStore {
	return &distributedStore{
		prefix: prefix,
		expiry: DefaultMetricsExpiry,
	}
}

type distributedStore struct {
	prefix string
	expiry int
}

func (d *distributedStore) Gather() ([]*dto.MetricFamily, error) {
	pool, ok := redispool.Cache.Pool()
	if !ok {
		// Redis is disabled. This means we are using Sourcegraph App which
		// does not expose prometheus metrics. For now that means we can skip
		// this store doing anything.
		return nil, nil
	}

	reConn := pool.Get()
	defer reConn.Close()

	// First, list all the keys for which we hold metrics.
	keys, err := redis.Values(reConn.Do("KEYS", d.prefix+"*"))
	if err != nil {
		return nil, errors.Wrap(err, "listing entries from redis")
	}

	if len(keys) == 0 {
		return nil, nil
	}

	// Then bulk retrieve all the metrics blobs for all the instances.
	encodedMetrics, err := redis.Strings(reConn.Do("MGET", keys...))
	if err != nil {
		return nil, errors.Wrap(err, "retrieving blobs from redis")
	}

	// Then decode the serialized metrics into proper metric families required
	// by the Gatherer interface.
	mfs := []*dto.MetricFamily{}
	for _, metrics := range encodedMetrics {
		// Decode each metrics blob separately.
		dec := expfmt.NewDecoder(strings.NewReader(metrics), expfmt.FmtText)
		for {
			var mf dto.MetricFamily
			if err := dec.Decode(&mf); err != nil {
				if err == io.EOF {
					break
				}

				return nil, errors.Wrap(err, "decoding metrics data")
			}
			mfs = append(mfs, &mf)
		}
	}

	return mfs, nil
}

func (d *distributedStore) Ingest(instance string, mfs []*dto.MetricFamily) error {
	pool, ok := redispool.Cache.Pool()
	if !ok {
		// Redis is disabled. This means we are using Sourcegraph App which
		// does not expose prometheus metrics. For now that means we can skip
		// this store doing anything.
		return nil
	}

	// First, encode the metrics to text format so we can store them.
	var enc bytes.Buffer
	encoder := expfmt.NewEncoder(&enc, expfmt.FmtText)

	for _, a := range mfs {
		if err := encoder.Encode(a); err != nil {
			return errors.Wrap(err, "encoding metric family")
		}
	}

	encodedMetrics := enc.String()

	reConn := pool.Get()
	defer reConn.Close()

	// Store the metrics and set an expiry on the key, if we haven't retrieved
	// an updated set of metric data, we consider the host down and prune it
	// from the gatherer.
	err := reConn.Send("SETEX", d.prefix+instance, d.expiry, encodedMetrics)
	if err != nil {
		return errors.Wrap(err, "writing metrics blob to redis")
	}

	return nil
}
