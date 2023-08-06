import {TagsTest} from "jstests/replsets/libs/tags.js";

var oldVersion = "last-lts";
var newVersion = "latest";
let nodes = [
    {binVersion: oldVersion},
    {binVersion: newVersion},
    {binVersion: oldVersion},
    {binVersion: newVersion},
    {binVersion: oldVersion}
];
new TagsTest({nodes: nodes}).run();
