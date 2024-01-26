/**
 *    Copyright (C) 2024-present MongoDB, Inc.
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

#include <boost/optional/optional.hpp>

#include "mongo/db/change_stream_pre_images_truncate_markers_per_nsUUID.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/shard_role.h"
#include "mongo/db/storage/collection_truncate_markers.h"
#include "mongo/db/tenant_id.h"
#include "mongo/util/concurrent_shared_values_map.h"
#include "mongo/util/uuid.h"

namespace mongo {
/**
 * Statistics for a truncate pass over a given tenant's pre-images collection.
 */
struct PreImagesTruncateStats {
    int64_t bytesDeleted{0};
    int64_t docsDeleted{0};

    // The number of 'nsUUID's scanned in the truncate pass.
    int64_t scannedInternalCollections{0};

    // The maximum wall time from the pre-images truncated across the collection.
    Date_t maxStartWallTime{};
};

/**
 * Manages truncate markers specific to the tenant's pre-images collection.
 */
class PreImagesTenantMarkers {
public:
    /**
     * Returns a 'PreImagesTenantMarkers' instance populated with truncate markers that span the
     * tenant's pre-images collection.
     *
     * Note: Pre-images inserted concurrently with creation might not be covered by the resulting
     * truncate markers.
     */
    static PreImagesTenantMarkers createMarkers(OperationContext* opCtx,
                                                boost::optional<TenantId> tenantId,
                                                const CollectionAcquisition& preImagesCollection);

    /**
     * Opens a fresh snapshot and ensures the all pre-images visible in the snapshot are
     * covered by truncate markers.
     */
    void refreshMarkers(OperationContext* opCtx, const CollectionAcquisition& preImagesCollection);

    PreImagesTruncateStats truncateExpiredPreImages(
        OperationContext* opCtx, const CollectionAcquisition& preImagesCollection);

    /**
     * Updates or creates the 'PreImagesTruncateMarkersPerNsUUID' to account for a
     * newly inserted pre-image generated from the users collection with UUID 'nsUUID'.
     *
     * 'numRecords' should always be 1 except for during initialization.
     *
     * Callers are responsible for calling this only once the data inserted is committed.
     */
    void updateOnInsert(const RecordId& recordId,
                        const UUID& nsUUID,
                        Date_t wallTime,
                        int64_t bytesInserted,
                        int64_t numRecords = 1);

private:
    friend class PreImagesTruncateManagerTest;

    PreImagesTenantMarkers(boost::optional<TenantId> tenantId) : _tenantId{tenantId} {}

    boost::optional<TenantId> _tenantId;
    ConcurrentSharedValuesMap<UUID, PreImagesTruncateMarkersPerNsUUID, UUID::Hash> _markersMap;
};

}  // namespace mongo
