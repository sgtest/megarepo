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

#include "mongo/db/query/optimizer/metadata_factory.h"

#include <absl/container/node_hash_map.h>
#include <boost/move/utility_core.hpp>
#include <boost/preprocessor/control/iif.hpp>
#include <type_traits>
#include <utility>
#include <vector>

#include <boost/none.hpp>
#include <boost/optional/optional.hpp>

#include "mongo/db/query/optimizer/partial_schema_requirements.h"
#include "mongo/db/query/optimizer/rewrites/const_eval.h"
#include "mongo/db/query/optimizer/syntax/syntax.h"
#include "mongo/db/query/optimizer/utils/utils.h"
#include "mongo/util/assert_util.h"


namespace mongo::optimizer {

MultikeynessTrie createTrie(const IndexDefinitions& indexDefs) {
    MultikeynessTrie multikeynessTrie;
    // Collect non-multiKeyPaths from each index.
    for (const auto& [indexDefName, indexDef] : indexDefs) {
        // Skip partial indexes. A path could be non-multikey on a partial index (subset of the
        // collection), but still be multikey on the overall collection.
        if (!psr::isNoop(indexDef.getPartialReqMap())) {
            continue;
        }

        for (const auto& component : indexDef.getCollationSpec()) {
            multikeynessTrie.add(component._path);
        }
    }
    // The empty path refers to the whole document, which can't be an array.
    multikeynessTrie.isMultiKey = false;
    return multikeynessTrie;
}

ScanDefinition createScanDef(ScanDefOptions options, IndexDefinitions indexDefs) {

    MultikeynessTrie multikeynessTrie = createTrie(indexDefs);
    return createScanDef(DatabaseNameUtil::deserialize(boost::none, "test"),
                         UUID::gen(),
                         std::move(options),
                         std::move(indexDefs),
                         std::move(multikeynessTrie),
                         ConstEval::constFold,
                         {DistributionType::Centralized},
                         true);
}

ScanDefinition createScanDef(ScanDefOptions options,
                             IndexDefinitions indexDefs,
                             const ConstFoldFn& constFold,
                             DistributionAndPaths distributionAndPaths,
                             const bool exists,
                             boost::optional<CEType> ce,
                             const PathToIntervalFn& pathToInterval) {

    MultikeynessTrie multikeynessTrie = createTrie(indexDefs);

    return createScanDef(DatabaseNameUtil::deserialize(boost::none, "test"),
                         UUID::gen(),
                         std::move(options),
                         std::move(indexDefs),
                         std::move(multikeynessTrie),
                         constFold,
                         std::move(distributionAndPaths),
                         exists,
                         std::move(ce),
                         {} /*shardingMetadata*/,
                         pathToInterval);
}

ScanDefinition createScanDef(DatabaseName dbName,
                             boost::optional<UUID> uuid,
                             ScanDefOptions options,
                             IndexDefinitions indexDefs,
                             MultikeynessTrie multikeynessTrie,
                             const ConstFoldFn& constFold,
                             DistributionAndPaths distributionAndPaths,
                             const bool exists,
                             boost::optional<CEType> ce,
                             ShardingMetadata shardingMetadata,
                             const PathToIntervalFn& pathToInterval) {

    // Simplify partial filter requirements using the non-multikey paths.
    for (auto& [indexDefName, indexDef] : indexDefs) {
        ProjectionRenames projRenames_unused;
        [[maybe_unused]] const bool hasEmptyInterval =
            simplifyPartialSchemaReqPaths(boost::none /*scanProjName*/,
                                          multikeynessTrie,
                                          indexDef.getPartialReqMap(),
                                          projRenames_unused,
                                          constFold,
                                          pathToInterval);
        tassert(6624157,
                "We should not be seeing renames from partial index filters",
                projRenames_unused.empty());

        // If "hasEmptyInterval" is set, we have a partial filter index with an unsatisfiable
        // condition, which is thus guaranteed to never contain any documents.
    }
    return {std::move(dbName),
            std::move(uuid),
            std::move(options),
            std::move(indexDefs),
            std::move(multikeynessTrie),
            std::move(distributionAndPaths),
            exists,
            std::move(ce),
            std::move(shardingMetadata)};
}

}  // namespace mongo::optimizer
