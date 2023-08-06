import {RenameAcrossDatabasesTest} from "jstests/replsets/libs/rename_across_dbs.js";

const nodes = [{binVersion: 'last-lts'}, {binVersion: 'latest'}, {}];
const options = {
    nodes: nodes,
    setFeatureCompatibilityVersion: lastLTSFCV,
};

new RenameAcrossDatabasesTest(options).run();
