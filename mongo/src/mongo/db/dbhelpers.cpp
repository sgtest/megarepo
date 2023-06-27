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


#include <set>
#include <string>
#include <utility>

#include <boost/move/utility_core.hpp>
#include <boost/optional/optional.hpp>
#include <boost/preprocessor/control/iif.hpp>
#include <boost/smart_ptr/intrusive_ptr.hpp>

#include "mongo/base/status_with.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/db/catalog/clustered_collection_util.h"
#include "mongo/db/catalog/collection.h"
#include "mongo/db/catalog/collection_catalog.h"
#include "mongo/db/catalog/collection_operation_source.h"
#include "mongo/db/catalog/collection_write_path.h"
#include "mongo/db/catalog/index_catalog.h"
#include "mongo/db/catalog/index_catalog_entry.h"
#include "mongo/db/catalog_raii.h"
#include "mongo/db/curop.h"
#include "mongo/db/db_raii.h"
#include "mongo/db/dbhelpers.h"
#include "mongo/db/index/index_access_method.h"
#include "mongo/db/index/index_descriptor.h"
#include "mongo/db/matcher/expression_parser.h"
#include "mongo/db/matcher/extensions_callback_real.h"
#include "mongo/db/namespace_string.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/ops/delete.h"
#include "mongo/db/ops/update.h"
#include "mongo/db/ops/update_request.h"
#include "mongo/db/ops/write_ops_parsers.h"
#include "mongo/db/pipeline/expression_context.h"
#include "mongo/db/query/canonical_query.h"
#include "mongo/db/query/find_command.h"
#include "mongo/db/query/get_executor.h"
#include "mongo/db/query/index_bounds_builder.h"
#include "mongo/db/query/internal_plans.h"
#include "mongo/db/query/plan_executor.h"
#include "mongo/db/query/plan_yield_policy.h"
#include "mongo/db/record_id_helpers.h"
#include "mongo/db/repl/oplog.h"
#include "mongo/db/shard_role.h"
#include "mongo/db/storage/snapshot.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/intrusive_counter.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kDefault


namespace mongo {

using std::set;
using std::string;
using std::unique_ptr;

bool Helpers::findOne(OperationContext* opCtx,
                      const CollectionPtr& collection,
                      const BSONObj& query,
                      BSONObj& result) {
    RecordId loc = findOne(opCtx, collection, query);
    if (loc.isNull())
        return false;
    result = collection->docFor(opCtx, loc).value();
    return true;
}

BSONObj Helpers::findOneForTesting(OperationContext* opCtx,
                                   const CollectionPtr& collection,
                                   const BSONObj& query,
                                   const bool invariantOnError) {
    BSONObj ret;
    bool found = findOne(opCtx, collection, query, ret);
    if (invariantOnError) {
        invariant(found);
    }

    return ret.getOwned();
}


/* fetch a single object from collection ns that matches query
   set your db SavedContext first
*/
RecordId Helpers::findOne(OperationContext* opCtx,
                          const CollectionPtr& collection,
                          const BSONObj& query) {
    if (!collection)
        return RecordId();

    auto findCommand = std::make_unique<FindCommandRequest>(collection->ns());
    findCommand->setFilter(query);
    return findOne(opCtx, collection, std::move(findCommand));
}

RecordId Helpers::findOne(OperationContext* opCtx,
                          const CollectionPtr& collection,
                          std::unique_ptr<FindCommandRequest> findCommand) {
    if (!collection)
        return RecordId();

    const ExtensionsCallbackReal extensionsCallback(opCtx, &collection->ns());

    const boost::intrusive_ptr<ExpressionContext> expCtx;
    auto statusWithCQ =
        CanonicalQuery::canonicalize(opCtx,
                                     std::move(findCommand),
                                     false,
                                     expCtx,
                                     extensionsCallback,
                                     MatchExpressionParser::kAllowAllSpecialFeatures);

    massertStatusOK(statusWithCQ.getStatus());
    unique_ptr<CanonicalQuery> cq = std::move(statusWithCQ.getValue());
    cq->setForceGenerateRecordId(true);

    auto exec = uassertStatusOK(getExecutor(opCtx,
                                            &collection,
                                            std::move(cq),
                                            nullptr /* extractAndAttachPipelineStages */,
                                            PlanYieldPolicy::YieldPolicy::NO_YIELD));

    PlanExecutor::ExecState state;
    BSONObj obj;
    RecordId loc;
    if (PlanExecutor::ADVANCED == (state = exec->getNext(&obj, &loc))) {
        return loc;
    }
    return RecordId();
}

bool Helpers::findById(OperationContext* opCtx,
                       const NamespaceString& nss,
                       BSONObj query,
                       BSONObj& result,
                       bool* nsFound,
                       bool* indexFound) {
    auto collCatalog = CollectionCatalog::get(opCtx);
    const Collection* collection = collCatalog->lookupCollectionByNamespace(opCtx, nss);
    if (!collection) {
        return false;
    }

    if (nsFound)
        *nsFound = true;

    const IndexCatalog* catalog = collection->getIndexCatalog();
    const IndexDescriptor* desc = catalog->findIdIndex(opCtx);

    if (!desc) {
        if (clustered_util::isClusteredOnId(collection->getClusteredInfo())) {
            if (indexFound) {
                // A collection clustered on _id implicitly has an _id index but no explicit
                // IndexDescriptor tied to it.
                *indexFound = 1;
            }

            Snapshotted<BSONObj> doc;
            if (collection->findDoc(opCtx,
                                    record_id_helpers::keyForObj(IndexBoundsBuilder::objFromElement(
                                        query["_id"], collection->getDefaultCollator())),
                                    &doc)) {
                result = std::move(doc.value());
                return true;
            }
        }

        return false;
    }

    if (indexFound)
        *indexFound = 1;

    const IndexCatalogEntry* entry = catalog->getEntry(desc);
    auto recordId = entry->accessMethod()->asSortedData()->findSingle(
        opCtx, CollectionPtr(collection), entry, query["_id"].wrap());
    if (recordId.isNull())
        return false;
    result = collection->docFor(opCtx, recordId).value();
    return true;
}

RecordId Helpers::findById(OperationContext* opCtx,
                           const CollectionPtr& collection,
                           const BSONObj& idquery) {
    MONGO_verify(collection);
    const IndexCatalog* catalog = collection->getIndexCatalog();
    const IndexDescriptor* desc = catalog->findIdIndex(opCtx);
    if (!desc && clustered_util::isClusteredOnId(collection->getClusteredInfo())) {
        // There is no explicit IndexDescriptor for _id on a collection clustered by _id. However,
        // the RecordId can be constructed directly from the input.
        return record_id_helpers::keyForObj(
            IndexBoundsBuilder::objFromElement(idquery["_id"], collection->getDefaultCollator()));
    }

    uassert(13430, "no _id index", desc);
    const IndexCatalogEntry* entry = catalog->getEntry(desc);
    return entry->accessMethod()->asSortedData()->findSingle(
        opCtx, collection, entry, idquery["_id"].wrap());
}

// Acquires necessary locks to read the collection with the given namespace. If this is an oplog
// read, use AutoGetOplog for simplified locking.
const CollectionPtr& getCollectionForRead(
    OperationContext* opCtx,
    const NamespaceString& ns,
    boost::optional<AutoGetCollectionForReadCommand>& autoColl,
    boost::optional<AutoGetOplog>& autoOplog) {
    if (ns.isOplog()) {
        // Simplify locking rules for oplog collection.
        autoOplog.emplace(opCtx, OplogAccessMode::kRead);
        return autoOplog->getCollection();
    } else {
        autoColl.emplace(opCtx, NamespaceString(ns));
        return autoColl->getCollection();
    }
}

bool Helpers::getSingleton(OperationContext* opCtx, const NamespaceString& nss, BSONObj& result) {
    boost::optional<AutoGetCollectionForReadCommand> autoColl;
    boost::optional<AutoGetOplog> autoOplog;
    const auto& collection = getCollectionForRead(opCtx, nss, autoColl, autoOplog);
    if (!collection) {
        return false;
    }

    auto exec =
        InternalPlanner::collectionScan(opCtx, &collection, PlanYieldPolicy::YieldPolicy::NO_YIELD);
    PlanExecutor::ExecState state = exec->getNext(&result, nullptr);

    CurOp::get(opCtx)->done();

    // Non-yielding collection scans from InternalPlanner will never error.
    invariant(PlanExecutor::ADVANCED == state || PlanExecutor::IS_EOF == state);

    if (PlanExecutor::ADVANCED == state) {
        result = result.getOwned();
        return true;
    }

    return false;
}

bool Helpers::getLast(OperationContext* opCtx, const NamespaceString& nss, BSONObj& result) {
    boost::optional<AutoGetCollectionForReadCommand> autoColl;
    boost::optional<AutoGetOplog> autoOplog;
    const auto& collection = getCollectionForRead(opCtx, nss, autoColl, autoOplog);
    if (!collection) {
        return false;
    }

    auto exec = InternalPlanner::collectionScan(
        opCtx, &collection, PlanYieldPolicy::YieldPolicy::NO_YIELD, InternalPlanner::BACKWARD);
    PlanExecutor::ExecState state = exec->getNext(&result, nullptr);

    // Non-yielding collection scans from InternalPlanner will never error.
    invariant(PlanExecutor::ADVANCED == state || PlanExecutor::IS_EOF == state);

    if (PlanExecutor::ADVANCED == state) {
        result = result.getOwned();
        return true;
    }

    return false;
}

UpdateResult Helpers::upsert(OperationContext* opCtx,
                             ScopedCollectionAcquisition& coll,
                             const BSONObj& o,
                             bool fromMigrate) {
    BSONElement e = o["_id"];
    MONGO_verify(e.type());
    BSONObj id = e.wrap();
    return upsert(opCtx, coll, id, o, fromMigrate);
}

UpdateResult Helpers::upsert(OperationContext* opCtx,
                             ScopedCollectionAcquisition& coll,
                             const BSONObj& filter,
                             const BSONObj& updateMod,
                             bool fromMigrate) {
    OldClientContext context(opCtx, coll.nss());

    auto request = UpdateRequest();
    request.setNamespaceString(coll.nss());

    request.setQuery(filter);
    request.setUpdateModification(write_ops::UpdateModification::parseFromClassicUpdate(updateMod));
    request.setUpsert();
    if (fromMigrate) {
        request.setSource(OperationSource::kFromMigrate);
    }
    request.setYieldPolicy(PlanYieldPolicy::YieldPolicy::NO_YIELD);

    return ::mongo::update(opCtx, coll, request);
}

void Helpers::update(OperationContext* opCtx,
                     ScopedCollectionAcquisition& coll,
                     const BSONObj& filter,
                     const BSONObj& updateMod,
                     bool fromMigrate) {
    OldClientContext context(opCtx, coll.nss());

    auto request = UpdateRequest();
    request.setNamespaceString(coll.nss());

    request.setQuery(filter);
    request.setUpdateModification(write_ops::UpdateModification::parseFromClassicUpdate(updateMod));
    if (fromMigrate) {
        request.setSource(OperationSource::kFromMigrate);
    }
    request.setYieldPolicy(PlanYieldPolicy::YieldPolicy::NO_YIELD);

    ::mongo::update(opCtx, coll, request);
}

Status Helpers::insert(OperationContext* opCtx,
                       const ScopedCollectionAcquisition& coll,
                       const BSONObj& doc) {
    OldClientContext context(opCtx, coll.nss());
    return collection_internal::insertDocument(
        opCtx, coll.getCollectionPtr(), InsertStatement{doc}, &CurOp::get(opCtx)->debug());
}

void Helpers::putSingleton(OperationContext* opCtx,
                           ScopedCollectionAcquisition& coll,
                           BSONObj obj) {
    OldClientContext context(opCtx, coll.nss());

    auto request = UpdateRequest();
    request.setNamespaceString(coll.nss());

    request.setUpdateModification(write_ops::UpdateModification::parseFromClassicUpdate(obj));
    request.setUpsert();

    ::mongo::update(opCtx, coll, request);

    CurOp::get(opCtx)->done();
}

BSONObj Helpers::toKeyFormat(const BSONObj& o) {
    BSONObjBuilder keyObj(o.objsize());
    BSONForEach(e, o) {
        keyObj.appendAs(e, "");
    }
    return keyObj.obj();
}

BSONObj Helpers::inferKeyPattern(const BSONObj& o) {
    BSONObjBuilder kpBuilder;
    BSONForEach(e, o) {
        kpBuilder.append(e.fieldName(), 1);
    }
    return kpBuilder.obj();
}

void Helpers::emptyCollection(OperationContext* opCtx, const ScopedCollectionAcquisition& coll) {
    OldClientContext context(opCtx, coll.nss());
    repl::UnreplicatedWritesBlock uwb(opCtx);
    deleteObjects(opCtx, coll, BSONObj(), false);
}

bool Helpers::findByIdAndNoopUpdate(OperationContext* opCtx,
                                    const CollectionPtr& collection,
                                    const BSONObj& idQuery,
                                    BSONObj& result) {
    auto recordId = Helpers::findById(opCtx, collection, idQuery);
    if (recordId.isNull()) {
        return false;
    }

    Snapshotted<BSONObj> snapshottedDoc;
    auto foundDoc = collection->findDoc(opCtx, recordId, &snapshottedDoc);
    if (!foundDoc) {
        return false;
    }

    result = snapshottedDoc.value();

    // Use an UnreplicatedWritesBlock to avoid generating an oplog entry for this no-op update.
    // The update is being used to generated write conflicts and isn't modifying the data itself, so
    // secondaries don't need to know about it. Also set CollectionUpdateArgs::update to an empty
    // BSONObj because that's a second way OpObserverImpl::onUpdate() detects and ignores no-op
    // updates.
    repl::UnreplicatedWritesBlock uwb(opCtx);
    CollectionUpdateArgs args(snapshottedDoc.value());
    args.criteria = idQuery;
    args.update = BSONObj();
    collection_internal::updateDocument(opCtx,
                                        collection,
                                        recordId,
                                        snapshottedDoc,
                                        result,
                                        collection_internal::kUpdateNoIndexes,
                                        nullptr /* indexesAffected */,
                                        nullptr /* opDebug */,
                                        &args);

    return true;
}

}  // namespace mongo
