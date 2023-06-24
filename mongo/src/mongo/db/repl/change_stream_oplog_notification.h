/**
 *    Copyright (C) 2022-present MongoDB, Inc.
 *
 *    This program is free software: you can redistribute it and/or modify
 *    it under the terms of the Server Side Public License, version 1,
 *    as published by MongoDB, Inc.
 *
 *    This program is distributed in the hope that it will be useful,
 *    but WITHOUT ANY WARRANTY; without even the implied warranty of
 *    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *    Server Side Public License for more details.
 *
 *    You should have received a copy of the Server Side Public License
 *    along with this program. If not, see
 *    <http://www.mongodb.com/licensing/server-side-public-license>.
 *
 *    As a special exception, the copyright holders give permission to link the
 *    code of portions of this program with the OpenSSL library under certain
 *    conditions as described in each individual source file and distribute
 *    linked combinations including the program with the OpenSSL library. You
 *    must comply with the Server Side Public License in all respects for
 *    all of the code used other than as permitted herein. If you modify file(s)
 *    with this exception, you may extend this exception to your version of the
 *    file(s), but you are not obligated to do so. If you do not wish to do so,
 *    delete this exception statement from your version. If you delete this
 *    exception statement from all source files in the program, then also delete
 *    it in the license file.
 */

#pragma once

#include <boost/none.hpp>
#include <boost/optional/optional.hpp>
#include <set>

#include "mongo/bson/bsonobj.h"
#include "mongo/client/connection_string.h"
#include "mongo/db/commands/notify_sharding_event_gen.h"
#include "mongo/db/database_name.h"
#include "mongo/db/namespace_string.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/shard_id.h"
#include "mongo/util/uuid.h"

namespace mongo {

/*
CommitPhase is used to implement a double oplog entry protocol to support the change stream.
A first notification is written to the oplog to notify the operation is about to be committed.
A second notification will eventually confirm the operation is committed or aborted.
This is necessary to make sure the change stream  will have a cursor open against any shards owning
data for the nss before the operation is committed (and therefore any insert or update is
performed on those shards).
- kPrepare: Before the commit. Not reported to the user.
- kSuccessful: After the commit. Reported to the user.
- kAborted: After the abort. Not reported to the user.
*/
enum class CommitPhase {
    kSuccessful,
    kAborted,
    kPrepare,
};

/*
 * This function writes a no-op oplog entry on shardCollection event.
 * TODO SERVER-66333: move all other notifyChangeStreams* functions here.
 */
void notifyChangeStreamsOnShardCollection(
    OperationContext* opCtx,
    const NamespaceString& nss,
    const UUID& uuid,
    BSONObj cmd,
    CommitPhase commitPhase,
    const boost::optional<std::set<ShardId>>& shardIds = boost::none);

/**
 * Writes a no-op oplog entry to match the addition of a database to the sharding catalog;
 * such database may have been either created or imported into the cluster (as part of an
 * addShard operation).
 * @param dbName the name of the database being added
 * @param primaryShard the primary shard ID assigned to the database being added (it may differ from
 * the shard ID of the RS where this method gets invoked)
 * @param isImported false when dbName is added to the sharding catalog by a database creation
 * request, true when the addition is the result of an addShard operation.
 */
void notifyChangeStreamsOnDatabaseAdded(OperationContext* opCtx,
                                        const DatabasesAdded& databasesAddedNotification);

/**
 * Writes a no-op oplog entry on movePrimary event.
 */
void notifyChangeStreamsOnMovePrimary(OperationContext* opCtx,
                                      const DatabaseName& dbName,
                                      const ShardId& oldPrimary,
                                      const ShardId& newPrimary);

}  // namespace mongo
