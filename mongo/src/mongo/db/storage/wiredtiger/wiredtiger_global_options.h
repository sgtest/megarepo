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

#pragma once

#include <cstddef>
#include <string>

#include "mongo/base/status.h"
#include "mongo/base/string_data.h"
#include "mongo/util/options_parser/environment.h"

namespace mongo {

class WiredTigerGlobalOptions {
public:
    static constexpr auto kDefaultTimeseriesCollectionCompressor = "zstd"_sd;
    static constexpr auto kDefaultColumnStoreIndexCompressor = "zstd"_sd;

    WiredTigerGlobalOptions()
        : cacheSizeGB(0),
          statisticsLogDelaySecs(0),
          zstdCompressorLevel(0),
          directoryForIndexes(false),
          maxCacheOverflowFileSizeGBDeprecated(0),
          useCollectionPrefixCompression(false),
          useIndexPrefixCompression(false){};

    Status store(const optionenvironment::Environment& params);

    double cacheSizeGB;
    size_t statisticsLogDelaySecs;
    std::string journalCompressor;
    int zstdCompressorLevel;
    bool directoryForIndexes;
    double maxCacheOverflowFileSizeGBDeprecated;
    std::string engineConfig;

    std::string collectionBlockCompressor;
    bool useCollectionPrefixCompression;
    bool useIndexPrefixCompression;
    std::string collectionConfig;
    std::string indexConfig;

    static Status validateWiredTigerCompressor(const std::string&);

    /**
     * Returns current history file size limit in MB.
     * Always returns 0 for unbounded.
     */
    std::size_t getMaxHistoryFileSizeMB() const {
        return 0;
    }
};

extern WiredTigerGlobalOptions wiredTigerGlobalOptions;

}  // namespace mongo
