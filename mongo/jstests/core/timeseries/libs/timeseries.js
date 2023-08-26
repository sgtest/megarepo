// Helper functions for testing time-series collections.

import {documentEq} from "jstests/aggregation/extras/utils.js";
import {FeatureFlagUtil} from "jstests/libs/feature_flag_util.js";
import {FixtureHelpers} from "jstests/libs/fixture_helpers.js";

export var TimeseriesTest = class {
    static getBucketMaxSpanSecondsFromGranularity(granularity) {
        switch (granularity) {
            case 'seconds':
                return 60 * 60;
            case 'minutes':
                return 60 * 60 * 24;
            case 'hours':
                return 60 * 60 * 24 * 30;
            default:
                assert(false, 'Invalid granularity: ' + granularity);
        }
    }

    static getBucketRoundingSecondsFromGranularity(granularity) {
        switch (granularity) {
            case 'seconds':
                return 60;
            case 'minutes':
                return 60 * 60;
            case 'hours':
                return 60 * 60 * 24;
            default:
                assert(false, 'Invalid granularity: ' + granularity);
        }
    }

    /**
     * Returns whether time-series scalability improvements (like bucket reopening) are enabled.
     * TODO SERVER-66438 remove this helper.
     */
    static timeseriesScalabilityImprovementsEnabled(conn) {
        return FeatureFlagUtil.isPresentAndEnabled(conn, "TimeseriesScalabilityImprovements");
    }

    /**
     * Returns whether time-series always use compressed buckets are enabled.
     * TODO SERVER-70605 remove this helper.
     */
    static timeseriesAlwaysUseCompressedBucketsEnabled(conn) {
        // TODO SERVER-79460: Clean this up so the ignoreFCV option does not need to be explicitly
        // specified.
        return FeatureFlagUtil.isPresentAndEnabled(conn,
                                                   "TimeseriesAlwaysUseCompressedBuckets",
                                                   /*user=*/ undefined,
                                                   /*ignoreFCV=*/ true);
    }

    // TODO SERVER-68058 remove this helper.
    static arbitraryUpdatesEnabled(conn) {
        return FeatureFlagUtil.isPresentAndEnabled(conn, "TimeseriesUpdatesSupport");
    }

    /**
     * Adjusts the values in 'fields' by a random amount.
     * Ensures that the new values stay in the range [0, 100].
     */
    static updateUsages(fields) {
        for (const field in fields) {
            fields[field] += Math.round(Random.genNormal(0, 1));
            fields[field] = Math.max(fields[field], 0);
            fields[field] = Math.min(fields[field], 100);
        }
    }

    /**
     * Decompresses a compressed bucket document. Replaces the compressed data in-place.
     */
    static decompressBucket(compressedBucket) {
        assert.hasFields(
            compressedBucket,
            ["control"],
            "TimeseriesTest.decompressBucket() should only be called on a bucket document");
        assert.eq(
            2,
            compressedBucket.control.version,
            "TimeseriesTest.decompressBucket() should only be called on a compressed bucket document");

        for (const column in compressedBucket.data) {
            compressedBucket.data[column] = decompressBSONColumn(compressedBucket.data[column]);
        }

        // The control object should reflect that the data is uncompressed.
        compressedBucket.control.version = 1;
        delete compressedBucket.control.count;
    }

    /**
     * Returns a random element from an array.
     */
    static getRandomElem(arr) {
        return arr[Random.randInt(arr.length)];
    }

    static getRandomUsage() {
        return Random.randInt(101);
    }

    /**
     * Generates time-series data based on the TSBS document-per-event format.
     *
     * https://github.com/timescale/tsbs/blob/7508b34755e05f55a14ec4bac2913ae758b4fd78/cmd/tsbs_generate_data/devops/cpu.go
     * https://github.com/timescale/tsbs/blob/7508b34755e05f55a14ec4bac2913ae758b4fd78/cmd/tsbs_generate_data/devops/host.go
     */
    static generateHosts(numHosts) {
        const hosts = new Array(numHosts);

        const regions = [
            "ap-northeast-1",
            "ap-southeast-1",
            "ap-southeast-2",
            "eu-central-1",
            "eu-west-1",
            "sa-east-1",
            "us-east-1",
            "us-west-1",
            "us-west-2",
        ];

        const dataCenters = [
            [
                "ap-northeast-1a",
                "ap-northeast-1c",
            ],
            [
                "ap-southeast-1a",
                "ap-southeast-1b",
            ],
            [
                "ap-southeast-2a",
                "ap-southeast-2b",
            ],
            [
                "eu-central-1a",
                "eu-central-1b",
            ],
            [
                "eu-west-1a",
                "eu-west-1b",
                "eu-west-1c",
            ],
            [
                "sa-east-1a",
                "sa-east-1b",
                "sa-east-1c",
            ],
            [
                "us-east-1a",
                "us-east-1b",
                "us-east-1c",
                "us-east-1e",
            ],
            [
                "us-west-1a",
                "us-west-1b",
            ],
            [
                "us-west-2a",
                "us-west-2b",
                "us-west-2c",
            ],
        ];

        for (let i = 0; i < hosts.length; i++) {
            const regionIndex = Random.randInt(regions.length);
            hosts[i] = {
                fields: {
                    usage_guest: TimeseriesTest.getRandomUsage(),
                    usage_guest_nice: TimeseriesTest.getRandomUsage(),
                    usage_idle: TimeseriesTest.getRandomUsage(),
                    usage_iowait: TimeseriesTest.getRandomUsage(),
                    usage_irq: TimeseriesTest.getRandomUsage(),
                    usage_nice: TimeseriesTest.getRandomUsage(),
                    usage_softirq: TimeseriesTest.getRandomUsage(),
                    usage_steal: TimeseriesTest.getRandomUsage(),
                    usage_system: TimeseriesTest.getRandomUsage(),
                    usage_user: TimeseriesTest.getRandomUsage(),
                },
                tags: {
                    arch: TimeseriesTest.getRandomElem(["x64", "x86"]),
                    datacenter: TimeseriesTest.getRandomElem(dataCenters[regionIndex]),
                    hostname: "host_" + i,
                    hostid: i,
                    os: TimeseriesTest.getRandomElem(
                        ["Ubuntu15.10", "Ubuntu16.10", "Ubuntu16.04LTS"]),
                    rack: Random.randInt(100).toString(),
                    region: regions[regionIndex],
                    service: Random.randInt(20).toString(),
                    service_environment:
                        TimeseriesTest.getRandomElem(["production", "staging", "test"]),
                    service_version: Random.randInt(2).toString(),
                    team: TimeseriesTest.getRandomElem(["CHI", "LON", "NYC", "SF"]),
                }

            };
        }

        return hosts;
    }

    /**
     * Runs the provided test with both ordered and unordered inserts.
     */
    static run(testFn, theDb) {
        const insert = function(ordered) {
            jsTestLog('Running test with {ordered: ' + ordered + '} inserts');
            return function(coll, docs, options) {
                return coll.insert(docs, Object.extend({ordered: ordered}, options));
            };
        };

        testFn(insert(true));
        testFn(insert(false));
    }

    static ensureDataIsDistributedIfSharded(coll, splitPointDate) {
        const db = coll.getDB();
        const buckets = db[this.getBucketsCollName(coll.getName())];
        if (FixtureHelpers.isSharded(buckets)) {
            const timeFieldName =
                db.getCollectionInfos({name: coll.getName()})[0].options.timeseries.timeField;

            const splitPoint = {[`control.min.${timeFieldName}`]: splitPointDate};
            assert.commandWorked(db.adminCommand(
                {split: `${db.getName()}.${buckets.getName()}`, middle: splitPoint}));

            const allShards = db.getSiblingDB("config").shards.find().sort({_id: 1}).toArray().map(
                doc => doc._id);
            const currentShards = buckets
                                      .aggregate([
                                          {"$collStats": {storageStats: {}}},
                                          {$project: {shard: 1}},
                                          {$sort: {shard: 1}}
                                      ])
                                      .toArray()
                                      .map(doc => doc.shard);

            if (!documentEq(allShards, currentShards)) {
                let otherShard;
                for (let i in allShards) {
                    if (!currentShards.includes(allShards[i])) {
                        otherShard = allShards[i];
                        break;
                    }
                }
                assert(otherShard);

                assert.commandWorked(db.adminCommand({
                    movechunk: `${db.getName()}.${buckets.getName()}`,
                    find: splitPoint,
                    to: otherShard,
                    _waitForDelete: true
                }));

                const updatedShards = buckets
                                          .aggregate([
                                              {"$collStats": {storageStats: {}}},
                                              {$project: {shard: 1}},
                                              {$sort: {shard: 1}}
                                          ])
                                          .toArray()
                                          .map(doc => doc.shard);
                assert.eq(updatedShards.length, currentShards.length + 1);
            }
        }
    }

    static getBucketsCollName(collName) {
        return `system.buckets.${collName}`;
    }
};
