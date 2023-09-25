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

#include "mongo/db/index/expression_params.h"

#include <memory>
#include <s2.h>
#include <utility>


#include "mongo/base/status.h"
#include "mongo/base/status_with.h"
#include "mongo/base/string_data.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/bson/bsontypes.h"
#include "mongo/bson/util/bson_extract.h"
#include "mongo/db/geo/geoconstants.h"
#include "mongo/db/geo/hash.h"
#include "mongo/db/hasher.h"
#include "mongo/db/index/2d_common.h"
#include "mongo/db/index/s2_common.h"
#include "mongo/db/index_names.h"
#include "mongo/db/query/collation/collator_interface.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/str.h"

namespace mongo {

using str::stream;

void ExpressionParams::parseTwoDParams(const BSONObj& infoObj, TwoDIndexingParams* out) {
    BSONObjIterator i(infoObj.getObjectField("key"));

    while (i.more()) {
        BSONElement e = i.next();
        if (e.type() == String && IndexNames::GEO_2D == e.str()) {
            uassert(16800, "can't have 2 geo fields", out->geo.empty());
            uassert(16801, "2d has to be first in index", out->other.empty());
            out->geo = e.fieldName();
        } else {
            int order = 1;
            if (e.isNumber()) {
                order = e.safeNumberInt();
            }
            out->other.emplace_back(e.fieldName(), order);
        }
    }

    uassert(16802, "no geo field specified", out->geo.size());

    auto result = GeoHashConverter::createFromDoc(infoObj);
    uassertStatusOK(result.getStatus());
    out->geoHashConverter.reset(result.getValue().release());
}

void ExpressionParams::parseHashParams(const BSONObj& infoObj,
                                       int* versionOut,
                                       BSONObj* keyPattern) {
    // In case we have hashed indexes based on other hash functions in the future, we store
    // a hashVersion number. If hashVersion changes, "makeSingleHashKey" will need to change
    // accordingly.  Defaults to 0 if "hashVersion" is not included in the index spec or if
    // the value of "hashversion" is not a number
    *versionOut = infoObj["hashVersion"].numberInt();

    // Extract and validate the index key pattern
    int numHashFields = 0;
    *keyPattern = infoObj.getObjectField("key");
    for (auto&& indexField : *keyPattern) {
        // The 'indexField' can either be ascending (1), descending (-1), or HASHED. Any other field
        // types should have failed validation while parsing.
        invariant(indexField.isNumber() || (indexField.valueStringData() == IndexNames::HASHED));
        numHashFields += (indexField.isNumber()) ? 0 : 1;
    }
    // We shouldn't be here if there are no hashed fields in the index.
    invariant(numHashFields > 0);
    uassert(31303,
            str::stream() << "A maximum of one index field is allowed to be hashed but found "
                          << numHashFields << " for 'key' " << *keyPattern,
            numHashFields == 1);
}

void ExpressionParams::initialize2dsphereParams(const BSONObj& infoObj,
                                                const CollatorInterface* collator,
                                                S2IndexingParams* out) {
    // Set up basic params.
    out->collator = collator;
    out->maxKeysPerInsert = 200;

    // Near distances are specified in meters...sometimes.
    out->radius = kRadiusOfEarthInMeters;

    static const std::string kIndexVersionFieldName("2dsphereIndexVersion");
    static const std::string kFinestIndexedLevel("finestIndexedLevel");
    static const std::string kCoarsestIndexedLevel("coarsestIndexedLevel");

    long long indexVersion;

    // Determine which version of this index we're using.  If none was set in the descriptor,
    // assume S2_INDEX_VERSION_1 (alas, the first version predates the existence of the version
    // field).
    Status status = bsonExtractIntegerFieldWithDefault(
        infoObj, kIndexVersionFieldName, S2_INDEX_VERSION_1, &indexVersion);
    uassertStatusOK(status);

    out->indexVersion = static_cast<S2IndexVersion>(indexVersion);

    // Note: In version > 2, these levels are for non-points.
    // Points are always indexed to the finest level.
    // Default levels were optimized for buildings and state regions
    long long defaultFinestIndexedLevel = S2::kAvgEdge.GetClosestLevel(110.0 / out->radius);
    long long defaultCoarsestIndexedLevel =
        S2::kAvgEdge.GetClosestLevel(2000 * 1000.0 / out->radius);
    long long defaultMaxCellsInCovering = 20;

    if (out->indexVersion <= S2_INDEX_VERSION_2) {
        defaultFinestIndexedLevel = S2::kAvgEdge.GetClosestLevel(500.0 / out->radius);
        defaultCoarsestIndexedLevel = S2::kAvgEdge.GetClosestLevel(100.0 * 1000 / out->radius);
        defaultMaxCellsInCovering = 50;
    }

    long long finestIndexedLevel, coarsestIndexedLevel, maxCellsInCovering;

    status = bsonExtractIntegerFieldWithDefault(
        infoObj, "finestIndexedLevel", defaultFinestIndexedLevel, &finestIndexedLevel);
    uassertStatusOK(status);

    status = bsonExtractIntegerFieldWithDefault(
        infoObj, "coarsestIndexedLevel", defaultCoarsestIndexedLevel, &coarsestIndexedLevel);
    uassertStatusOK(status);

    status = bsonExtractIntegerFieldWithDefault(
        infoObj, "maxCellsInCovering", defaultMaxCellsInCovering, &maxCellsInCovering);
    uassertStatusOK(status);

    // This is advisory.
    out->maxCellsInCovering = maxCellsInCovering;

    // These are not advisory.
    out->finestIndexedLevel = finestIndexedLevel;
    out->coarsestIndexedLevel = coarsestIndexedLevel;

    uassert(16747, "coarsestIndexedLevel must be >= 0", out->coarsestIndexedLevel >= 0);
    uassert(16748, "finestIndexedLevel must be <= 30", out->finestIndexedLevel <= 30);
    uassert(16749,
            "finestIndexedLevel must be >= coarsestIndexedLevel",
            out->finestIndexedLevel >= out->coarsestIndexedLevel);


    massert(17395,
            stream() << "unsupported geo index version { " << kIndexVersionFieldName << " : "
                     << out->indexVersion << " }, only support versions: [" << S2_INDEX_VERSION_1
                     << "," << S2_INDEX_VERSION_2 << "," << S2_INDEX_VERSION_3 << "]",
            out->indexVersion == S2_INDEX_VERSION_3 || out->indexVersion == S2_INDEX_VERSION_2 ||
                out->indexVersion == S2_INDEX_VERSION_1);
}
}  // namespace mongo
