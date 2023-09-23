/**
 *    Copyright (C) 2021-present MongoDB, Inc.
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

#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>
#include <boost/optional/optional.hpp>
#include <cstdint>
#include <functional>

#include "mongo/db/catalog/collection.h"
#include "mongo/db/catalog/index_catalog_entry.h"
#include "mongo/db/db_raii.h"
#include "mongo/db/exec/sbe/values/slot.h"
#include "mongo/db/namespace_string.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/record_id.h"
#include "mongo/db/storage/record_store.h"
#include "mongo/util/assert_util_core.h"
#include "mongo/util/string_map.h"
#include "mongo/util/uuid.h"

namespace mongo::sbe {
/**
 * A callback which gets called whenever a SCAN stage asks an underlying index scan for a result.
 */
using IndexKeyConsistencyCheckCallback = bool (*)(OperationContext* opCtx,
                                                  StringMap<const IndexCatalogEntry*>&,
                                                  value::SlotAccessor* snapshotIdAccessor,
                                                  value::SlotAccessor* indexIdentAccessor,
                                                  value::SlotAccessor* indexKeyAccessor,
                                                  const CollectionPtr& collection,
                                                  const Record& nextRecord);

using IndexKeyCorruptionCheckCallback = void (*)(OperationContext* opCtx,
                                                 value::SlotAccessor* snapshotIdAccessor,
                                                 value::SlotAccessor* indexKeyAccessor,
                                                 value::SlotAccessor* indexKeyPatternAccessor,
                                                 const RecordId& rid,
                                                 const NamespaceString& nss);

/**
 * Helper class used by SBE PlanStages for acquiring and re-acquiring a CollectionPtr.
 */
class CollectionRef {
public:
    bool isInitialized() const {
        return _collPtr.has_value();
    }

    const CollectionPtr& getPtr() const {
        dassert(isInitialized());
        return *_collPtr;
    }

    operator bool() const {
        return isInitialized() ? static_cast<bool>(getPtr()) : false;
    }

    bool operator!() const {
        return isInitialized() ? !getPtr() : true;
    }

    void reset() {
        _collPtr = boost::none;
    }

    boost::optional<NamespaceString> getCollName() const {
        return _collName;
    }

    /**
     * Given a collection UUID, looks up the UUID in the catalog and stores a pointer to the
     * collection into _collPtr. This method also stores the NamespaceString for the collection
     * into _collName, and it stores the current catalog epoch into _catalogEpoch.
     *
     * This is intended for use during the preparation of an SBE plan. The caller must hold the
     * appropriate db_raii object in order to ensure that SBE plan preparation sees a consistent
     * view of the catalog.
     */
    void acquireCollection(OperationContext* opCtx, const UUID& collUuid);

    /**
     * Re-acquires a pointer to the collection, intended for use during SBE yield recovery or when a
     * closed SBE plan is re-opened. In addition to acquiring the collection pointer, throws a
     * UserException if the collection has been dropped or renamed, or if the catalog has been
     * closed and re-opened. SBE query execution currently cannot survive such events if they occur
     * during a yield or between getMores.
     */
    void restoreCollection(OperationContext* opCtx, const UUID& collUuid);

private:
    boost::optional<CollectionPtr> _collPtr;
    boost::optional<NamespaceString> _collName;
    boost::optional<uint64_t> _catalogEpoch;
};
}  // namespace mongo::sbe
