import {TagsTest} from "jstests/replsets/libs/tags.js";

var oldVersion = "last-lts";
var newVersion = "latest";
let nodes = [
    {binVersion: newVersion},
    {binVersion: oldVersion},
    {binVersion: newVersion},
    {binVersion: oldVersion},
    {binVersion: newVersion}
];
new TagsTest({nodes: nodes}).run();
