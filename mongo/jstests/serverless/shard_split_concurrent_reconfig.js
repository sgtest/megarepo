/**
 * Prove that it's possible to run reconfigs during a shard split.
 *
 * @tags: [requires_fcv_63, serverless]
 */

import {configureFailPoint} from "jstests/libs/fail_point_util.js";
import {ShardSplitTest} from "jstests/serverless/libs/shard_split_test.js";

const tenantIds = [ObjectId(), ObjectId()];
const test = new ShardSplitTest({quickGarbageCollection: true});

test.addRecipientNodes();
test.donor.awaitSecondaryNodes();

const donorPrimary = test.donor.getPrimary();
const pauseAfterBlockingFp = configureFailPoint(donorPrimary, "pauseShardSplitAfterBlocking");
const split = test.createSplitOperation(tenantIds);
const commitThread = split.commitAsync();

pauseAfterBlockingFp.wait();

// Prepare a new config which removes all of the recipient nodes
const config = assert.commandWorked(donorPrimary.adminCommand({replSetGetConfig: 1})).config;
const recipientHosts = test.recipientNodes.map(node => node.host);
config.members = config.members.filter(member => !recipientHosts.includes(member.host));
config.version++;
assert.commandWorked(donorPrimary.adminCommand({replSetReconfig: config}));
pauseAfterBlockingFp.off();

assert.commandFailedWithCode(commitThread.returnData(), ErrorCodes.TenantMigrationAborted);
split.forget();

test.cleanupSuccesfulAborted(split.migrationId, tenantIds);
test.stop();
