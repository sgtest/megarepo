/**
 * Sharding test of `$search` aggregation stage within $unionWith and $lookup stages.
 */
import {getUUIDFromListCollections} from "jstests/libs/uuid_util.js";
import {
    mongotCommandForQuery,
    mongotMultiCursorResponseForBatch
} from "jstests/with_mongot/mongotmock/lib/mongotmock.js";
import {
    ShardingTestWithMongotMock
} from "jstests/with_mongot/mongotmock/lib/shardingtest_with_mongotmock.js";

const stWithMock = new ShardingTestWithMongotMock({
    name: "sharded_search",
    shards: {
        rs0: {nodes: 1},
        rs1: {nodes: 1},
    },
    mongos: 1
});
stWithMock.start();
const st = stWithMock.st;

const dbName = jsTestName();

const mongos = st.s;
const testDB = mongos.getDB(dbName);
// Ensure db's primary shard is shard1 so we only set the correct mongot to have history.
assert.commandWorked(
    mongos.getDB("admin").runCommand({enableSharding: dbName, primaryShard: st.shard1.shardName}));

const shardedSearchColl = testDB.getCollection("search_sharded");
const unshardedSearchColl = testDB.getCollection("search_unsharded");
const shardedBaseColl = testDB.getCollection("base_sharded");
const unshardedBaseColl = testDB.getCollection("base_unsharded");

const baseCollDocs = [
    {"_id": 100, "localField": "cakes", "weird": false},
    // Split between first and second shard will be here.
    {"_id": 101, "localField": "cakes and kale", "weird": true},
];

const searchCollDocs = [
    {"_id": 0, "title": "cakes"},
    {"_id": 1, "title": "cookies and cakes"},
    {"_id": 2, "title": "vegetables"},
    {"_id": 3, "title": "oranges"},
    // Split between first and second shard will be here.
    {"_id": 4, "title": "cakes and oranges"},
    {"_id": 5, "title": "cakes and apples"},
    {"_id": 6, "title": "apples"},
    {"_id": 7, "title": "cakes and kale"},
];

function loadData(coll, docs) {
    coll.drop();
    var bulk = coll.initializeUnorderedBulkOp();
    for (const doc of docs) {
        bulk.insert(doc);
    }
    assert.commandWorked(bulk.execute());
}

loadData(unshardedBaseColl, baseCollDocs);
loadData(shardedBaseColl, baseCollDocs);
loadData(unshardedSearchColl, searchCollDocs);
loadData(shardedSearchColl, searchCollDocs);

// Shard search collection.
st.shardColl(shardedSearchColl, {_id: 1}, {_id: 4}, {_id: 4});
// Shard base collection.
st.shardColl(shardedBaseColl, {_id: 1}, {_id: 101}, {_id: 101});

const shard0Conn = st.rs0.getPrimary();
const shard1Conn = st.rs1.getPrimary();
const d0Mongot = stWithMock.getMockConnectedToHost(shard0Conn);
const d1Mongot = stWithMock.getMockConnectedToHost(shard1Conn);
const sMongot = stWithMock.getMockConnectedToHost(stWithMock.st.s);

// We're going to be running sharded queries which can be non-deterministic in their order of
// search commands.
// Consider the following scenario with two shards, where one of the shards (or the network) is
// arbitrarily slow. The pipeline for this query is something like:
// [{$search: {...}}, ... ,{$lookup: {pipeline: [$search: {...}]}}, ... ] Shard 0 gets the above
// pipeline, and begins to plan a sharded $lookup execution. To do so it runs planShardedSearch on
// the partner mongot, and then distributes the query to Shard 1.
// Shard 1's initial query for some reason is behind. It hasn't run anything to do with the first
// search query yet (planShardedSearch or search) -- and the sharded lookup comes in and Shard 1 has
// to run a search query against the partner mock. Shard 1 runs the inner lookup query before
// planShardedSearch/search for the outer query. If the mock is checking the order of commands, this
// would fail the test. However, this sequence of events is allowed if rare, so we disable order
// checking.
d0Mongot.disableOrderCheck();
d1Mongot.disableOrderCheck();

const mongotQuery = {
    query: "cakes",
    path: "title"
};

//------------------------
// Define mocks' responses
//------------------------

function searchQueryExpectedByMock(searchColl) {
    return mongotCommandForQuery(mongotQuery,
                                 searchColl.getName(),
                                 testDB.getName(),
                                 getUUIDFromListCollections(testDB, searchColl.getName()));
}

const shard1HistorySharded = [
    {
        expectedCommand: searchQueryExpectedByMock(shardedSearchColl),
        response: mongotMultiCursorResponseForBatch(
            [{_id: 0, $searchScore: 0.99}, {_id: 1, $searchScore: 0.20}],
            NumberLong(0),
            // Unmerged search metadata.  There are a total of 2 docs from this mongot.
            [{count: 1}, {count: 1}],  // mongot can return any number of metadata docs to merge.
            NumberLong(0),
            shardedSearchColl.getFullName(),
            true)
    },
];

const shard0HistorySharded = [
    {
        expectedCommand: searchQueryExpectedByMock(shardedSearchColl),
        response: mongotMultiCursorResponseForBatch(
            [
                {_id: 4, $searchScore: 0.33},
                {_id: 5, $searchScore: 0.38},
                {_id: 7, $searchScore: 0.45}
            ],
            NumberLong(0),
            // Unmerged search metadata.  There are a total of 3 docs from this mongot.
            [{count: 2}, {count: 1}, {count: 0}, {count: 0}],
            NumberLong(0),
            shardedSearchColl.getFullName(),
            true)
    },
];

const shard1HistoryUnsharded = [
    {
        expectedCommand: searchQueryExpectedByMock(unshardedSearchColl),
        response: {
            cursor: {
                id: NumberLong(0),
                ns: unshardedSearchColl.getFullName(),
                nextBatch: [
                    {_id: 0, $searchScore: 0.99},
                    {_id: 1, $searchScore: 0.20},
                    {_id: 4, $searchScore: 0.33},
                    {_id: 5, $searchScore: 0.38},
                    {_id: 7, $searchScore: 0.45}
                ]
            },
            vars: {SEARCH_META: {count: 5}}
        }
    },
];

//--------------
// $lookup tests
//--------------
const makeLookupPipeline = (searchColl) => [
    {$project: {"_id": 0}},
    {
        $lookup: {
            from: searchColl.getName(),
            let: { local_title: "$localField" },
            pipeline: [
                {
                    $search: mongotQuery
                },
                {
                    $match: {
                        $expr: {
                            $eq: ["$title", "$$local_title"]
                        }
                    }
                },
                {
                    $project: {
                        "_id": 0,
                        "ref_id": "$_id",
                        "searchMeta": "$$SEARCH_META",
                    }
                }],
            as: "cake_data"
        }
    }
];

const expectedLookupResults = [
    {"localField": "cakes", "weird": false, "cake_data": [{"ref_id": 0, "searchMeta": {count: 5}}]},
    {
        "localField": "cakes and kale",
        "weird": true,
        "cake_data": [{"ref_id": 7, "searchMeta": {count: 5}}]
    }
];

const kPlan = "planSearch";
// For some tests, planSearch can be issue from different shard randomly, to make tests
// deterministic we have to set 'maybeUnused' to true for all shards.
const kPlanMaybe = "planSearchMaybe";
const kSearch = "search";
let cursorId = 1000;

function setupMockRequest(searchColl, mongot, requestType) {
    if (requestType == kPlan || requestType == kPlanMaybe) {
        const mergingPipelineHistory = [{
            expectedCommand: {
                planShardedSearch: searchColl.getName(),
                query: mongotQuery,
                $db: dbName,
                searchFeatures: {shardedSort: 1}
            },
            response: {
                ok: 1,
                protocolVersion: NumberInt(42),
                metaPipeline:  // Sum counts in the shard metadata.
                    [{$group: {_id: null, count: {$sum: "$count"}}}, {$project: {_id: 0, count: 1}}]
            },
            maybeUnused: requestType == kPlanMaybe,
        }];
        mongot.setMockResponses(mergingPipelineHistory, cursorId);
    } else {
        assert(requestType == kSearch, "invalid request type");
        assert(mongot != sMongot, "only plan requests should go to mongoS");
        assert(!(searchColl == unshardedSearchColl && mongot == d0Mongot),
               "unsharded requests should not go to secondary");
        const history =
            (mongot == d1Mongot
                 ? (searchColl == shardedSearchColl ? shard1HistorySharded : shard1HistoryUnsharded)
                 : shard0HistorySharded);
        if (searchColl === shardedSearchColl) {
            mongot.setMockResponses(history, cursorId, NumberLong(cursorId + 1001));
        } else {
            mongot.setMockResponses(history, cursorId);
        }
    }
    cursorId += 1;
}

function setupAllMockRequests(searchColl, mockResponses) {
    for (const i of mockResponses["mongos"]) {
        setupMockRequest(searchColl, sMongot, i);
    }
    for (const i of mockResponses["primary"]) {
        setupMockRequest(searchColl, d1Mongot, i);
    }
    for (const i of mockResponses["secondary"]) {
        setupMockRequest(searchColl, d0Mongot, i);
    }
}

function lookupTest(baseColl, searchColl, mockResponses) {
    setupAllMockRequests(searchColl, mockResponses);
    assert.sameMembers(expectedLookupResults,
                       baseColl.aggregate(makeLookupPipeline(searchColl)).toArray());
    stWithMock.assertEmptyMocks();
}

// Test all combinations of sharded/unsharded base/search collection.
lookupTest(unshardedBaseColl,
           unshardedSearchColl,
           {mongos: [], primary: [kSearch, kSearch], secondary: []});

lookupTest(unshardedBaseColl,
           shardedSearchColl,
           {mongos: [], primary: [kPlan, kSearch, kPlan, kSearch], secondary: [kSearch, kSearch]});

lookupTest(
    shardedBaseColl, unshardedSearchColl, {mongos: [], primary: [kSearch, kSearch], secondary: []});

lookupTest(shardedBaseColl, shardedSearchColl, {
    mongos: [],
    // There's one doc per shard, but each shard will dispatch the $search to all shards.
    primary: [kPlan, kSearch, kSearch],
    secondary: [kPlan, kSearch, kSearch]
});

// ----------------
// $unionWith tests
// ----------------
const makeUnionWithPipeline = (searchColl) => [{
    $unionWith: {
        coll: searchColl.getName(),
        pipeline: [
            {$search: mongotQuery},
            {
                $project: {
                    "_id": 0,
                    "ref_id": "$_id",
                    "title": "$title",
                    "searchMeta": "$$SEARCH_META",
                }
            }
        ]
    }
}];

const expectedUnionWithResult = [
    {_id: 100, "localField": "cakes", "weird": false},
    {_id: 101, "localField": "cakes and kale", "weird": true},
    {"ref_id": 0, "title": "cakes", "searchMeta": {"count": 5}},
    {"ref_id": 1, "title": "cookies and cakes", "searchMeta": {"count": 5}},
    {"ref_id": 4, "title": "cakes and oranges", "searchMeta": {"count": 5}},
    {"ref_id": 5, "title": "cakes and apples", "searchMeta": {"count": 5}},
    {"ref_id": 7, "title": "cakes and kale", "searchMeta": {"count": 5}}
];

function unionTest(baseColl, searchColl, mockResponses) {
    setupAllMockRequests(searchColl, mockResponses);
    assert.sameMembers(baseColl.aggregate(makeUnionWithPipeline(searchColl)).toArray(),
                       expectedUnionWithResult);
    stWithMock.assertEmptyMocks();
}

// Test all combinations of sharded/unsharded base/search collection.
unionTest(unshardedBaseColl, unshardedSearchColl, {mongos: [], primary: [kSearch], secondary: []});

unionTest(unshardedBaseColl,
          shardedSearchColl,
          {mongos: [], primary: [kPlan, kSearch], secondary: [kSearch]});

unionTest(shardedBaseColl, unshardedSearchColl, {mongos: [], primary: [kSearch], secondary: []});

// The $unionWith is dispatched to shards randomly instead of always primary, so planShardedSearch
// may be issued in either shard.
unionTest(shardedBaseColl,
          shardedSearchColl,
          {mongos: [], primary: [kPlanMaybe, kSearch], secondary: [kPlanMaybe, kSearch]});

stWithMock.stop();
