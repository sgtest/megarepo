/**
 * Tests that the analyzeShardKey command returns monotonicity metrics for compound shard keys that
 * use range sharding.
 *
 * @tags: [requires_fcv_70, resource_intensive]
 */
import {
    AnalyzeShardKeyUtil
} from "jstests/sharding/analyze_shard_key/libs/analyze_shard_key_util.js";
import {
    kOrderTypes,
    testAnalyzeShardKeysShardedCollection,
    testAnalyzeShardKeysUnshardedCollection,
} from "jstests/sharding/analyze_shard_key/libs/monotonicity_common.js";
import {numNodesPerRS} from "jstests/sharding/analyze_shard_key/libs/monotonicity_common.js";

// Construct test cases for testing the monotonicity of compound shard keys that use range sharding.
// For each test case:
// - 'shardKey' is the shard key being analyzed.
// - 'indexKey' is the supporting index for the shard key.
// - 'fieldOpts' specifies the name, type and order for each field inside the documents in the
//   collection. The order refers to whether the value is constant, fluctuating, increasing or
//   decreasing.
// - 'expected' is the expected monotonicity. Since the shard key is compound, its monotonicity is
//   determined by the monotonicity of the first non-constant shard key field.
const testCases = [];

for (let orderType0 of kOrderTypes) {
    const fieldName0 = AnalyzeShardKeyUtil.getRandomFieldName("a");
    const fieldType0 = AnalyzeShardKeyUtil.getRandomElement(orderType0.supportedFieldTypes);

    for (let orderType1 of kOrderTypes) {
        const fieldName1 = AnalyzeShardKeyUtil.getRandomFieldName("b");
        const fieldType1 = AnalyzeShardKeyUtil.getRandomElement(orderType1.supportedFieldTypes);

        // Test compound shard key with a shard key index.
        testCases.push({
            shardKey: {[fieldName0]: 1, [fieldName1]: 1},
            indexKey: {[fieldName0]: 1, [fieldName1]: 1},
            fieldOpts: [
                {name: fieldName0, type: fieldType0, order: orderType0.name},
                {name: fieldName1, type: fieldType1, order: orderType1.name}
            ],
            expected: orderType0.name == "constant" ? orderType1.monotonicity
                                                    : orderType0.monotonicity
        });

        for (let orderType2 of kOrderTypes) {
            const fieldName2 = AnalyzeShardKeyUtil.getRandomFieldName("c");
            const fieldType2 = AnalyzeShardKeyUtil.getRandomElement(orderType2.supportedFieldTypes);

            // Test compound shard key with a compound shard key prefixed index.
            testCases.push({
                shardKey: {[fieldName0]: 1, [fieldName1]: 1},
                indexKey: {[fieldName0]: 1, [fieldName1]: 1, [fieldName2]: 1},
                fieldOpts: [
                    {name: fieldName0, type: fieldType0, order: orderType0.name},
                    {name: fieldName1, type: fieldType1, order: orderType1.name},
                    {name: fieldName2, type: fieldType2, order: orderType2.name}
                ],
                expected: orderType0.name == "constant" ? orderType1.monotonicity
                                                        : orderType0.monotonicity
            });
        }
    }
}

// This test requires the collection to contain at least a few thousands of documents to smooth out
// the insertion order noise caused by parallel oplog application on secondaries.
const numDocsRange = {
    min: 7500,
    max: 10000
};

{
    const st = new ShardingTest({shards: 2, rs: {nodes: numNodesPerRS, oplogSize: 500}});

    testAnalyzeShardKeysUnshardedCollection(st.s, testCases, numDocsRange);
    testAnalyzeShardKeysShardedCollection(st, testCases, numDocsRange);

    st.stop();
}

if (!jsTestOptions().useAutoBootstrapProcedure) {  // TODO: SERVER-80318 Remove block
    const rst = new ReplSetTest({nodes: numNodesPerRS, oplogSize: 250});
    rst.startSet();
    rst.initiate();
    const primary = rst.getPrimary();

    testAnalyzeShardKeysUnshardedCollection(primary, testCases, numDocsRange);

    rst.stopSet();
}
