import {
    assertValueOnPath,
    assertValueOnPlanPath,
    checkCascadesOptimizerEnabled,
    navigateToPlanPath,
} from "jstests/libs/optimizer_utils.js";

if (!checkCascadesOptimizerEnabled(db)) {
    jsTestLog("Skipping test because the optimizer is not enabled");
    quit();
}

const coll = db.cqf_chess;
Random.srand(0);

const players = [
    "penguingim1",    "aladdin65",       "aleksey472",        "azuaga",       "benpig",
    "blackboarder",   "bockosrb555",     "bogdan_low_player", "charlytb",     "chchbbuur",
    "chessexplained", "cmcookiemonster", "crptone",           "cselhu3",      "darkzam",
    "dmitri31",       "dorado99",        "ericrosen",         "fast-tsunami", "flaneur"
];
const sources = [1, 2, 3, 4, 5, 6, 7, 8];
const variants = [1, 2, 3, 4, 5, 6, 7, 8];
const results = [1, 2, 3, 4, 5, 6, 7, 8];
const winColor = [true, false, null];

const nbGames = 10000;

function intRandom(max) {
    return Random.randInt(max);
}
function anyOf(as) {
    return as [intRandom(as.length)];
}

coll.drop();

print(`Adding ${nbGames} games`);
const bulk = coll.initializeUnorderedBulkOp();
for (let i = 0; i < nbGames; i++) {
    const users = [anyOf(players), anyOf(players)];
    const winnerIndex = intRandom(2);
    bulk.insert({
        users: users,
        winner: users[winnerIndex],
        loser: users[1 - winnerIndex],
        winColor: anyOf(winColor),
        avgRating: NumberInt(600 + intRandom(2400)),
        source: NumberInt(anyOf(sources)),
        variants: NumberInt(anyOf(variants)),
        mode: !!intRandom(2),
        turns: NumberInt(1 + intRandom(300)),
        minutes: NumberInt(30 + intRandom(3600 * 3)),
        clock: {init: NumberInt(0 + intRandom(10800)), inc: NumberInt(0 + intRandom(180))},
        result: anyOf(results),
        date: new Date(Date.now() - intRandom(118719488)),
        analysed: !!intRandom(2)
    });
    if (i % 1000 == 0) {
        print(`${i} / ${nbGames}`);
    }
}
assert.commandWorked(bulk.execute());

const indexes = [
    {users: 1},
    {winner: 1},
    {loser: 1},
    {winColor: 1},
    {avgRating: 1},
    {source: 1},
    {variants: 1},
    {mode: 1},
    {turns: 1},
    {minutes: 1},
    {'clock.init': 1},
    {'clock.inc': 1},
    {result: 1},
    {date: 1},
    {analysed: 1}
];

print("Adding indexes");
indexes.forEach(index => {
    printjson(index);
    coll.createIndex(index);
});

print("Searching");

const res = coll.explain("executionStats").aggregate([
    {
        $match: {
            avgRating: {$gt: 1000},
            turns: {$lt: 250},
            'clock.init': {$gt: 1},
            minutes: {$gt: 2, $lt: 150}
        }
    },
    {$sort: {date: -1}},
    {$limit: 20}
]);

// TODO: verify expected results.

/**
 * Demonstrate the following:
 * 1. Limit is subsumed into the collation node above.
 * 2. We have one index scan on minutes and the range is between 2 and 150 (we can encode both
 * comparisons as a single index scan).
 */
assertValueOnPlanPath("Collation", res, "child.nodeType");

const indexNode = navigateToPlanPath(res, "child.child.leftChild");
assertValueOnPath("IndexScan", indexNode, "nodeType");
assertValueOnPath("minutes_1", indexNode, "indexDefName");
