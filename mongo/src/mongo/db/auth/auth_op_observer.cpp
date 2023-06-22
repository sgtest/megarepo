/**
 *    Copyright (C) 2018-present MongoDB, Inc.
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

#include <boost/preprocessor/control/iif.hpp>
#include <set>
#include <utility>

#include <boost/optional/optional.hpp>

#include "mongo/base/string_data.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/bson/bsonmisc.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/db/audit.h"
#include "mongo/db/auth/auth_op_observer.h"
#include "mongo/db/auth/authorization_manager.h"
#include "mongo/db/catalog/collection_options.h"
#include "mongo/db/op_observer/op_observer_util.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/repl/oplog_entry.h"
#include "mongo/util/assert_util_core.h"
#include "mongo/util/decorable.h"
#include "mongo/util/namespace_string_util.h"

namespace mongo {

namespace {

const auto documentIdDecoration = OplogDeleteEntryArgs::declareDecoration<BSONObj>();

}  // namespace

AuthOpObserver::AuthOpObserver() = default;

AuthOpObserver::~AuthOpObserver() = default;

void AuthOpObserver::onInserts(OperationContext* opCtx,
                               const CollectionPtr& coll,
                               std::vector<InsertStatement>::const_iterator first,
                               std::vector<InsertStatement>::const_iterator last,
                               std::vector<bool> fromMigrate,
                               bool defaultFromMigrate,
                               OpStateAccumulator* opAccumulator) {
    for (auto it = first; it != last; it++) {
        audit::logInsertOperation(opCtx->getClient(), coll->ns(), it->doc);
        AuthorizationManager::get(opCtx->getServiceContext())
            ->logOp(opCtx, "i", coll->ns(), it->doc, nullptr);
    }
}

void AuthOpObserver::onUpdate(OperationContext* opCtx,
                              const OplogUpdateEntryArgs& args,
                              OpStateAccumulator* opAccumulator) {
    if (args.updateArgs->update.isEmpty()) {
        return;
    }

    audit::logUpdateOperation(opCtx->getClient(), args.coll->ns(), args.updateArgs->updatedDoc);

    AuthorizationManager::get(opCtx->getServiceContext())
        ->logOp(opCtx, "u", args.coll->ns(), args.updateArgs->update, &args.updateArgs->criteria);
}

void AuthOpObserver::aboutToDelete(OperationContext* opCtx,
                                   const CollectionPtr& coll,
                                   BSONObj const& doc,
                                   OplogDeleteEntryArgs* args,
                                   OpStateAccumulator* opAccumulator) {
    audit::logRemoveOperation(opCtx->getClient(), coll->ns(), doc);

    // Extract the _id field from the document. If it does not have an _id, use the
    // document itself as the _id.
    documentIdDecoration(args) = doc["_id"] ? doc["_id"].wrap() : doc;
}

void AuthOpObserver::onDelete(OperationContext* opCtx,
                              const CollectionPtr& coll,
                              StmtId stmtId,
                              const OplogDeleteEntryArgs& args,
                              OpStateAccumulator* opAccumulator) {
    auto& documentId = documentIdDecoration(args);
    invariant(!documentId.isEmpty());
    AuthorizationManager::get(opCtx->getServiceContext())
        ->logOp(opCtx, "d", coll->ns(), documentId, nullptr);
}

void AuthOpObserver::onCreateCollection(OperationContext* opCtx,
                                        const CollectionPtr& coll,
                                        const NamespaceString& collectionName,
                                        const CollectionOptions& options,
                                        const BSONObj& idIndex,
                                        const OplogSlot& createOpTime,
                                        bool fromMigrate) {
    const auto cmdNss = collectionName.getCommandNS();

    const auto cmdObj =
        repl::MutableOplogEntry::makeCreateCollCmdObj(collectionName, options, idIndex);

    AuthorizationManager::get(opCtx->getServiceContext())
        ->logOp(opCtx, "c", cmdNss, cmdObj, nullptr);
}

void AuthOpObserver::onCollMod(OperationContext* opCtx,
                               const NamespaceString& nss,
                               const UUID& uuid,
                               const BSONObj& collModCmd,
                               const CollectionOptions& oldCollOptions,
                               boost::optional<IndexCollModInfo> indexInfo) {
    const auto cmdNss = nss.getCommandNS();

    // Create the 'o' field object.
    const auto cmdObj = makeCollModCmdObj(collModCmd, oldCollOptions, indexInfo);

    AuthorizationManager::get(opCtx->getServiceContext())
        ->logOp(opCtx, "c", cmdNss, cmdObj, nullptr);
}

void AuthOpObserver::onDropDatabase(OperationContext* opCtx, const DatabaseName& dbName) {
    const NamespaceString cmdNss(NamespaceString::makeCommandNamespace(dbName));
    const auto cmdObj = BSON("dropDatabase" << 1);

    AuthorizationManager::get(opCtx->getServiceContext())
        ->logOp(opCtx, "c", cmdNss, cmdObj, nullptr);
}

repl::OpTime AuthOpObserver::onDropCollection(OperationContext* opCtx,
                                              const NamespaceString& collectionName,
                                              const UUID& uuid,
                                              std::uint64_t numRecords,
                                              const CollectionDropType dropType,
                                              bool markFromMigrate) {
    const auto cmdNss = collectionName.getCommandNS();
    const auto cmdObj = BSON("drop" << collectionName.coll());

    AuthorizationManager::get(opCtx->getServiceContext())
        ->logOp(opCtx, "c", cmdNss, cmdObj, nullptr);

    return {};
}

void AuthOpObserver::onDropIndex(OperationContext* opCtx,
                                 const NamespaceString& nss,
                                 const UUID& uuid,
                                 const std::string& indexName,
                                 const BSONObj& indexInfo) {
    const auto cmdNss = nss.getCommandNS();
    const auto cmdObj = BSON("dropIndexes" << nss.coll() << "index" << indexName);

    AuthorizationManager::get(opCtx->getServiceContext())
        ->logOp(opCtx, "c", cmdNss, cmdObj, &indexInfo);
}

void AuthOpObserver::postRenameCollection(OperationContext* const opCtx,
                                          const NamespaceString& fromCollection,
                                          const NamespaceString& toCollection,
                                          const UUID& uuid,
                                          const boost::optional<UUID>& dropTargetUUID,
                                          bool stayTemp) {
    const auto cmdNss = fromCollection.getCommandNS();

    BSONObjBuilder builder;
    builder.append("renameCollection", NamespaceStringUtil::serialize(fromCollection));
    builder.append("to", NamespaceStringUtil::serialize(toCollection));
    builder.append("stayTemp", stayTemp);
    if (dropTargetUUID) {
        dropTargetUUID->appendToBuilder(&builder, "dropTarget");
    }

    const auto cmdObj = builder.done();

    AuthorizationManager::get(opCtx->getServiceContext())
        ->logOp(opCtx, "c", cmdNss, cmdObj, nullptr);
}

void AuthOpObserver::onRenameCollection(OperationContext* const opCtx,
                                        const NamespaceString& fromCollection,
                                        const NamespaceString& toCollection,
                                        const UUID& uuid,
                                        const boost::optional<UUID>& dropTargetUUID,
                                        std::uint64_t numRecords,
                                        bool stayTemp,
                                        bool markFromMigrate) {
    postRenameCollection(opCtx, fromCollection, toCollection, uuid, dropTargetUUID, stayTemp);
}

void AuthOpObserver::onImportCollection(OperationContext* opCtx,
                                        const UUID& importUUID,
                                        const NamespaceString& nss,
                                        long long numRecords,
                                        long long dataSize,
                                        const BSONObj& catalogEntry,
                                        const BSONObj& storageMetadata,
                                        bool isDryRun) {
    AuthorizationManager::get(opCtx->getServiceContext())
        ->logOp(opCtx, "m", nss, catalogEntry, &storageMetadata);
}

void AuthOpObserver::onApplyOps(OperationContext* opCtx,
                                const DatabaseName& dbName,
                                const BSONObj& applyOpCmd) {
    const NamespaceString cmdNss(NamespaceString::makeCommandNamespace(dbName));

    AuthorizationManager::get(opCtx->getServiceContext())
        ->logOp(opCtx, "c", cmdNss, applyOpCmd, nullptr);
}

void AuthOpObserver::onEmptyCapped(OperationContext* opCtx,
                                   const NamespaceString& collectionName,
                                   const UUID& uuid) {
    const auto cmdNss = collectionName.getCommandNS();
    const auto cmdObj = BSON("emptycapped" << collectionName.coll());

    AuthorizationManager::get(opCtx->getServiceContext())
        ->logOp(opCtx, "c", cmdNss, cmdObj, nullptr);
}

void AuthOpObserver::onReplicationRollback(OperationContext* opCtx,
                                           const RollbackObserverInfo& rbInfo) {
    // Invalidate any in-memory auth data if necessary.
    const auto& rollbackNamespaces = rbInfo.rollbackNamespaces;
    if (rollbackNamespaces.count(NamespaceString::kServerConfigurationNamespace) == 1 ||
        rollbackNamespaces.count(NamespaceString::kAdminUsersNamespace) == 1 ||
        rollbackNamespaces.count(NamespaceString::kAdminRolesNamespace) == 1) {
        AuthorizationManager::get(opCtx->getServiceContext())->invalidateUserCache(opCtx);
    }
}


}  // namespace mongo
