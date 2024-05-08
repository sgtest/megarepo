/**
 * Tests that the commitShardSplit commands throw an error if the provided
 * tenantId is unsupported (i.e. '', 'admin', 'local' or 'config') or if there is no recipient node.
 *
 * @tags: [
 *   incompatible_with_macos,
 *   incompatible_with_windows_tls,
 *   requires_persistence,
 *   requires_fcv_63,
 *   serverless,
 * ]
 */

import {ShardSplitTest} from "jstests/serverless/libs/shard_split_test.js";

const test =
    new ShardSplitTest({recipientSetName: "recipientSet", recipientTagName: "recipientTag"});

const donorPrimary = test.donor.getPrimary();

const tenantId = ObjectId();

jsTestLog("Testing 'commitShardSplit' command without recipient nodes.");

assert.commandFailedWithCode(donorPrimary.adminCommand({
    commitShardSplit: 1,
    migrationId: UUID(),
    tenantIds: [tenantId],
    recipientSetName: test.recipientSetName,
    recipientTagName: test.recipientTagName
}),
                             ErrorCodes.TenantMigrationAborted);

test.addRecipientNodes();

jsTestLog("Testing 'commitShardSplit' with unsupported tenantIds.");
const unsupportedtenantIds = ['admin', 'admin', 'local', 'config'];
unsupportedtenantIds.forEach((invalidTenantId) => {
    const operation = test.createSplitOperation([invalidTenantId]);
    assert.commandFailedWithCode(operation.commit(), ErrorCodes.BadValue);
});

test.stop();
