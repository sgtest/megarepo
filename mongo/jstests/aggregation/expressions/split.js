// In SERVER-6773, the $split expression was introduced. In this file, we test the functionality and
// error cases of the expression.

import "jstests/libs/sbe_assert_error_override.js";

import {assertErrorCode, testExpression} from "jstests/aggregation/extras/utils.js";

const coll = db.split;

coll.drop();
assert.commandWorked(coll.insert({}));

//
// Tests with constant-folding optimization.
//

// Basic tests.
testExpression(coll, {$split: ["abc", "b"]}, ["a", "c"]);
testExpression(coll, {$split: ["aaa", "b"]}, ["aaa"]);
testExpression(coll, {$split: ["a b a", "b"]}, ["a ", " a"]);
testExpression(coll, {$split: ["a", "a"]}, ["", ""]);
testExpression(coll, {$split: ["aa", "a"]}, ["", "", ""]);
testExpression(coll, {$split: ["aaa", "a"]}, ["", "", "", ""]);
testExpression(coll, {$split: ["", "a"]}, [""]);
testExpression(coll, {$split: ["abc abc cba abc", "abc"]}, ["", " ", " cba ", ""]);

// Ensure that $split operates correctly when the string has embedded null bytes.
testExpression(coll, {$split: ["a\0b\0c", "\0"]}, ["a", "b", "c"]);
testExpression(coll, {$split: ["\0a\0", "a"]}, ["\0", "\0"]);
testExpression(coll, {$split: ["\0\0\0", "a"]}, ["\0\0\0"]);

// Ensure that $split operates correctly when the string has multi-byte tokens or input strings.
testExpression(coll, {$split: ["∫a∫", "a"]}, ["∫", "∫"]);
testExpression(coll, {$split: ["a∫∫a", "∫"]}, ["a", "", "a"]);

// Ensure that $split produces null when given null as input.
testExpression(coll, {$split: ["abc", null]}, null);
testExpression(coll, {$split: [null, "abc"]}, null);

// Ensure that $split produces null when given missing fields as input.
testExpression(coll, {$split: ["$a", "a"]}, null);
testExpression(coll, {$split: ["a", "$a"]}, null);
testExpression(coll, {$split: ["$missing", {$toLower: "$missing"}]}, null);

//
// Error Code tests with constant-folding optimization.
//

// Ensure that $split errors when given more or less than two arguments.
let pipeline = {$project: {split: {$split: []}}};
assertErrorCode(coll, pipeline, 16020);

pipeline = {
    $project: {split: {$split: ["a"]}}
};
assertErrorCode(coll, pipeline, 16020);

pipeline = {
    $project: {split: {$split: ["a", "b", "c"]}}
};
assertErrorCode(coll, pipeline, 16020);

// Ensure that $split errors when given non-string input.
pipeline = {
    $project: {split: {$split: [1, "abc"]}}
};
assertErrorCode(coll, pipeline, 40085);

pipeline = {
    $project: {split: {$split: ["abc", 1]}}
};
assertErrorCode(coll, pipeline, 40086);

// Ensure that $split errors when given an empty separator.
pipeline = {
    $project: {split: {$split: ["abc", ""]}}
};
assertErrorCode(coll, pipeline, 40087);
