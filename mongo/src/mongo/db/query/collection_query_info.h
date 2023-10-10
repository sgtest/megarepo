/**
 *    Copyright (C) 2019-present MongoDB, Inc.
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

#include <boost/container/small_vector.hpp>
// IWYU pragma: no_include "boost/intrusive/detail/iterator.hpp"
#include <boost/move/utility_core.hpp>
#include <cstddef>
#include <memory>
#include <utility>

#include "mongo/db/catalog/collection.h"
#include "mongo/db/catalog/index_catalog_entry.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/query/classic_plan_cache.h"
#include "mongo/db/query/plan_cache_indexability.h"
#include "mongo/db/query/plan_cache_invalidator.h"
#include "mongo/db/query/plan_summary_stats.h"
#include "mongo/db/update_index_data.h"
#include "mongo/util/decorable.h"

namespace mongo {

class IndexDescriptor;
class OperationContext;

/**
 * Query information for a particular point-in-time view of a collection.
 *
 * Decorates a Collection instance. Lifecycle is the same as the Collection instance.
 */
class CollectionQueryInfo {
public:
    CollectionQueryInfo();

    inline static const auto getCollectionQueryInfo =
        Collection::declareDecoration<CollectionQueryInfo>();
    static const CollectionQueryInfo& get(const CollectionPtr& collection) {
        return CollectionQueryInfo::getCollectionQueryInfo(collection.get());
    }
    static CollectionQueryInfo& get(Collection* collection) {
        return CollectionQueryInfo::getCollectionQueryInfo(collection);
    }
    /**
     * Populate the outData structure using the index keys found in the index definition.
     */
    static void computeUpdateIndexData(const IndexCatalogEntry* entry,
                                       const IndexAccessMethod* accessMethod,
                                       UpdateIndexData* outData);

    /**
     * Gets the PlanCache for this collection.
     */
    PlanCache* getPlanCache() const {
        return &_planCacheState->classicPlanCache;
    }

    /**
     * Gets the number of the current collection version used for Plan Cache invalidation.
     */
    size_t getPlanCacheInvalidatorVersion() const {
        return _planCacheState->planCacheInvalidator.versionNumber();
    }

    /**
     * Gets the "indexability discriminators" used in the PlanCache for generating plan cache keys.
     */
    const PlanCacheIndexabilityState& getPlanCacheIndexabilityState() const {
        return _planCacheState->planCacheIndexabilityState;
    }

    /* get set of index keys for this namespace.  handy to quickly check if a given
       field is indexed (Note it might be a secondary component of a compound index.)
    */
    const UpdateIndexData& getIndexKeys(OperationContext* opCtx) const;

    /**
     * Builds internal cache state based on the current state of the Collection's IndexCatalog.
     */
    void init(OperationContext* opCtx, Collection* coll);

    /**
     * Rebuilds cached index information. Must be called when an index is modified or an index is
     * dropped/created.
     *
     * Must be called under exclusive collection lock.
     */
    void rebuildIndexData(OperationContext* opCtx, const Collection* coll);

    /**
     * Removes all cached query plans after ensuring that the PlanCache is uniquely owned. The
     * PlanCache is made uniquely owned by creating a new instance and thus detaching from the
     * shared instance.
     */
    void clearQueryCache(OperationContext* opCtx, const CollectionPtr& coll);

    /**
     * Removes all cached query plans without ensuring that the PlanCache is uniquely owned, only
     * allowed when setting an index to multikey. Setting an index to multikey can only go one way
     * and has its own concurrency handling.
     */
    void clearQueryCacheForSetMultikey(const CollectionPtr& coll) const;

    void notifyOfQuery(OperationContext* opCtx,
                       const CollectionPtr& coll,
                       const PlanSummaryStats& summaryStats) const;

private:
    /**
     * Stores Clasic and SBE PlanCache-related state. Classic Plan Cache is stored per collection
     * and represented by a mongo::PlanCache object. SBE PlanCache is stored in a process-global
     * object, therefore, it is represented here as a PlanCacheInvalidator which knows what
     * collection version to invalidate.
     */
    struct PlanCacheState {
        PlanCacheState();

        PlanCacheState(OperationContext* opCtx, const Collection* collection);

        /**
         * Clears classic and SBE cache entries with the current collection version.
         */
        void clearPlanCache();

        // Per collection version classic plan cache.
        PlanCache classicPlanCache;

        // SBE PlanCacheInvalidator which can invalidate cache entries associated with a particular
        // version of a collection.
        PlanCacheInvalidator planCacheInvalidator;

        // Holds computed information about the collection's indexes. Used for generating plan
        // cache keys.
        PlanCacheIndexabilityState planCacheIndexabilityState;
    };

    void computeUpdateIndexData(OperationContext* opCtx, const Collection* coll);
    void updatePlanCacheIndexEntries(OperationContext* opCtx, const Collection* coll);

    // ---  index keys cache
    bool _keysComputed;
    UpdateIndexData _indexedPaths;

    std::shared_ptr<PlanCacheState> _planCacheState;
};

}  // namespace mongo
