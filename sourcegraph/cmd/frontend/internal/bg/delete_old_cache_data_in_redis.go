package bg

import (
	"github.com/inconshreveable/log15"

	"github.com/sourcegraph/sourcegraph/internal/rcache"
	"github.com/sourcegraph/sourcegraph/internal/redispool"
)

func DeleteOldCacheDataInRedis() {
	for _, kv := range []redispool.KeyValue{redispool.Store, redispool.Cache} {
		pool, ok := kv.Pool()
		if !ok { // redis disabled, nothing to delete
			continue
		}

		c := pool.Get()
		defer c.Close()

		err := rcache.DeleteOldCacheData(c)
		if err != nil {
			log15.Error("Unable to delete old cache data in redis search. Please report this issue.", "error", err)
			return
		}
	}
}
