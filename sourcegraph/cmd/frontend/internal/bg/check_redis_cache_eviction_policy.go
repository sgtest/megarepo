package bg

import (
	"errors"
	"fmt"
	"strings"

	"github.com/gomodule/redigo/redis"
	"github.com/sourcegraph/sourcegraph/pkg/redispool"
	"gopkg.in/inconshreveable/log15.v2"
)

const recommendedPolicy = "allkeys-lru"

func CheckRedisCacheEvictionPolicy() {
	cacheConn := redispool.Cache.Get()
	defer cacheConn.Close()

	storeConn := redispool.Store.Get()
	defer storeConn.Close()

	storeRunID, err := getRunID(storeConn)
	if err != nil {
		log15.Error("Reading run_id from redis-store failed", "error", err)
		return
	}

	cacheRunID, err := getRunID(cacheConn)
	if err != nil {
		log15.Error("Reading run_id from redis-cache failed", "error", err)
		return
	}

	if cacheRunID == storeRunID {
		// If users use the same instance for redis-store and redis-cache we
		// don't want to recommend an LRU policy, because that could interfere
		// with the functionality of redis-store, which expects to store items
		// for longer term usage
		return
	}

	vals, err := redis.Strings(cacheConn.Do("CONFIG", "GET", "maxmemory-policy"))
	if err != nil {
		log15.Error("Reading `maxmemory-policy` from Redis failed", "error", err)
		return
	}

	if len(vals) == 2 && vals[1] != recommendedPolicy {
		msg := fmt.Sprintf("ATTENTION: Your Redis cache instance does not have the recommended `maxmemory-policy` set. The current value is '%s'. Recommend for the cache is '%s'.", vals[1], recommendedPolicy)
		log15.Warn("****************************")
		log15.Warn(msg)
		log15.Warn("****************************")
	}
}

func getRunID(c redis.Conn) (string, error) {
	infos, err := redis.String(c.Do("INFO", "server"))
	if err != nil {
		return "", err
	}

	for _, l := range strings.Split(infos, "\n") {
		if strings.HasPrefix(l, "run_id:") {
			s := strings.Split(l, ":")
			return s[1], nil
		}
	}
	return "", errors.New("no run_id found")
}
