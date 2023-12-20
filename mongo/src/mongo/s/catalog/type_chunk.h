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

#include <boost/move/utility_core.hpp>
#include <boost/optional.hpp>
#include <boost/optional/optional.hpp>
// IWYU pragma: no_include "ext/alloc_traits.h"
#include <cstdint>
#include <memory>
#include <string>
#include <utility>
#include <vector>

#include "mongo/base/status.h"
#include "mongo/base/status_with.h"
#include "mongo/bson/bson_field.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/bson/oid.h"
#include "mongo/bson/timestamp.h"
#include "mongo/db/keypattern.h"
#include "mongo/db/namespace_string.h"
#include "mongo/db/repl/optime.h"
#include "mongo/db/shard_id.h"
#include "mongo/s/catalog/type_chunk_base_gen.h"
#include "mongo/s/chunk_version.h"
#include "mongo/s/shard_key_pattern.h"
#include "mongo/stdx/type_traits.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/time_support.h"
#include "mongo/util/uuid.h"

namespace mongo {

class BSONObjBuilder;
class Status;
template <typename T>
class StatusWith;

/**
 * Contains the minimum representation of a chunk - its bounds in the format [min, max) along with
 * utilities for parsing and persistence.
 */
class ChunkRange {
public:
    static constexpr char kMinKey[] = "min";
    static constexpr char kMaxKey[] = "max";

    ChunkRange(BSONObj minKey, BSONObj maxKey);

    /**
     * Parses a chunk range using the format { min: <min bound>, max: <max bound> }.
     */
    static StatusWith<ChunkRange> fromBSON(const BSONObj& obj);

    /**
     * A throwing version of 'fromBSON'.
     */
    static ChunkRange fromBSONThrowing(const BSONObj& obj) {
        return uassertStatusOK(fromBSON(obj));
    }

    const BSONObj& getMin() const {
        return _minKey;
    }

    const BSONObj& getMax() const {
        return _maxKey;
    }

    Status extractKeyPattern(KeyPattern* shardKeyPatternOut) const;

    /**
     * Checks whether the specified key is within the bounds of this chunk range.
     */
    bool containsKey(const BSONObj& key) const;

    /**
     * Writes the contents of this chunk range as { min: <min bound>, max: <max bound> }.
     */
    void append(BSONObjBuilder* builder) const;

    BSONObj toBSON() const;

    std::string toString() const;

    /**
     * Returns true if two chunk ranges match exactly in terms of the min and max keys (including
     * element order within the keys).
     */
    bool operator==(const ChunkRange& other) const;
    bool operator!=(const ChunkRange& other) const;

    /**
     * Returns true if either min is less than rhs min, or in the case that min == rhs min, true if
     * max is less than rhs max. Otherwise returns false.
     */
    bool operator<(const ChunkRange& rhs) const;

    /**
     * Returns true iff the union of *this and the argument range is the same as *this.
     */
    bool covers(ChunkRange const& other) const;

    /**
     * Returns the range of overlap between *this and other, if any.
     */
    boost::optional<ChunkRange> overlapWith(ChunkRange const& other) const;

    /**
     * Returns true if there is any overlap between the two ranges.
     */
    bool overlaps(const ChunkRange& other) const;

    /**
     * Returns a range that includes *this and other. If the ranges do not overlap, it includes
     * all the space between, as well.
     */
    ChunkRange unionWith(ChunkRange const& other) const;

private:
    /** Does not enforce the non-empty range invariant. */
    ChunkRange() = default;

    friend ChunkRange idlPreparsedValue(stdx::type_identity<ChunkRange>) {
        return {};
    }

    BSONObj _minKey;
    BSONObj _maxKey;
};

class ChunkHistory : public ChunkHistoryBase {
public:
    using ChunkHistoryBase::serialize;
    using ChunkHistoryBase::toBSON;

    ChunkHistory() : ChunkHistoryBase() {}
    ChunkHistory(mongo::Timestamp ts, mongo::ShardId shard) : ChunkHistoryBase() {
        setValidAfter(std::move(ts));
        setShard(std::move(shard));
    }
    ChunkHistory(const ChunkHistoryBase& b) : ChunkHistoryBase(b) {}

    static StatusWith<std::vector<ChunkHistory>> fromBSON(const BSONArray& source);

    bool operator==(const ChunkHistory& other) const {
        return getValidAfter() == other.getValidAfter() && getShard() == other.getShard();
    }
};

/**
 * This class represents the layouts and contents of documents contained in the config server's
 * config.chunks and shard server's config.chunks.uuid collections. All manipulation of documents
 * coming from these collections should be done with this class. The shard's config.chunks.uuid
 * collections use the epoch field as the uuid value, and epochs match 1:1 to collection instances
 * (mmapped in config.collections). Therefore, the shard collections do not need to include epoch or
 * namespace fields, as these will be known in order to access the collections.
 *
 * Expected config server config.chunks collection format:
 *   {
 *      _id : "test.foo-a_MinKey",
 *      uuid : Bindata(UUID),
 *      min : {
 *              "a" : { "$minKey" : 1 }
 *      },
 *      max : {
 *              "a" : { "$maxKey" : 1 }
 *      },
 *      shard : "test-rs1",
 *      lastmod : Timestamp(1, 0),
 *      jumbo : false              // optional field
 *   }
 *
 * Expected shard server config.chunks.<epoch> collection format:
 *   {
 *      _id: {
 *             "a" : { "$minKey" : 1 }
 *      }
 *      max : {
 *              "a" : { "$maxKey" : 1 }
 *      }
 *      shard : "test-rs1",
 *      lastmod : Timestamp(1, 0),
 *   }
 *
 * Note: it is intended to change the config server's collection schema to mirror the new shard
 * server's collection schema, but that will be future work when the new schema is stable and there
 * is time to do the extra work, as well as handle the backwards compatibility issues it poses.
 */
class ChunkType {
public:
    // Name of the chunks collection in the config server.
    static const NamespaceString ConfigNS;

    // The shard chunks collections' common namespace prefix.
    static const std::string ShardNSPrefix;

    // Field names and types in the chunks collections.
    static const BSONField<OID> name;
    static const BSONField<BSONObj> minShardID;
    static const BSONField<UUID> collectionUUID;
    static const BSONField<BSONObj> min;
    static const BSONField<BSONObj> max;
    static const BSONField<std::string> shard;
    static const BSONField<bool> jumbo;
    static const BSONField<Date_t> lastmod;
    static const BSONField<BSONObj> history;
    static const BSONField<int64_t> estimatedSizeBytes;
    static const BSONField<Timestamp> onCurrentShardSince;
    static const BSONField<bool> historyIsAt40;

    ChunkType();
    ChunkType(UUID collectionUUID, ChunkRange range, ChunkVersion version, ShardId shardId);

    /**
     * Constructs a new ChunkType object from BSON with the following format:
     * {min: <>, max: <>, shard: <>, uuid: <>, history: <>, jumbo: <>, lastmod: <>,
     * lastmodEpoch: <>, lastmodTimestamp: <>, onCurrentShardSince: <>}
     */
    static StatusWith<ChunkType> parseFromNetworkRequest(const BSONObj& source);

    /**
     * Constructs a new ChunkType object from BSON with the following format:
     * {_id: <>, min: <>, max: <>, shard: <>, uuid: <>, history: <>, jumbo: <>, lastmod: <>,
     * estimatedSizeByte: <>, onCurrentShardSince: <>}
     *
     * Returns ErrorCodes::NoSuchKey if the '_id' field is missing
     */
    static StatusWith<ChunkType> parseFromConfigBSON(const BSONObj& source,
                                                     const OID& epoch,
                                                     const Timestamp& timestamp);

    /**
     * Constructs a new ChunkType object from BSON with the following format:
     * {_id: <>, max: <>, shard: <>, history: <>, lastmod: <>, onCurrentShardSince: <>}
     * Also does validation of the contents.
     */
    static StatusWith<ChunkType> parseFromShardBSON(const BSONObj& source,
                                                    const OID& epoch,
                                                    const Timestamp& timestamp);

    /**
     * Returns the BSON representation of the entry for the config server's config.chunks
     * collection.
     */
    BSONObj toConfigBSON() const;

    /**
     * Returns the BSON representation of the entry for a shard server's config.chunks.<epoch>
     * collection.
     */
    BSONObj toShardBSON() const;

    const OID& getName() const;
    void setName(const OID& id);

    /**
     * Getters and setters.
     */
    const UUID& getCollectionUUID() const {
        invariant(_collectionUUID);
        return *_collectionUUID;
    }
    void setCollectionUUID(const UUID& uuid);

    const BSONObj& getMin() const {
        return _min.get();
    }
    void setMin(const BSONObj& min);

    const BSONObj& getMax() const {
        return _max.get();
    }
    void setMax(const BSONObj& max);

    ChunkRange getRange() const {
        return ChunkRange(getMin(), getMax());
    }

    bool isVersionSet() const {
        return _version.is_initialized();
    }
    const ChunkVersion& getVersion() const {
        return _version.get();
    }
    void setVersion(const ChunkVersion& version);

    const ShardId& getShard() const {
        return _shard.get();
    }
    void setShard(const ShardId& shard);

    boost::optional<int64_t> getEstimatedSizeBytes() const {
        return _estimatedSizeBytes;
    }

    void setEstimatedSizeBytes(const boost::optional<int64_t>& estimatedSize);

    bool getJumbo() const {
        return _jumbo.get_value_or(false);
    }
    void setJumbo(bool jumbo);

    const boost::optional<Timestamp>& getOnCurrentShardSince() const {
        return _onCurrentShardSince;
    }
    void setOnCurrentShardSince(const Timestamp& onCurrentShardSince);

    void setHistory(std::vector<ChunkHistory> history) {
        _history = std::move(history);
        if (!_history.empty()) {
            invariant(_shard == _history.front().getShard());
        }
    }
    const std::vector<ChunkHistory>& getHistory() const {
        return _history;
    }

    void addHistoryToBSON(BSONObjBuilder& builder) const;

    /**
     * Returns OK if all the mandatory fields have been set. Otherwise returns NoSuchKey and
     * information about the first field that is missing.
     */
    Status validate() const;

    /**
     * Returns a std::string representation of the current internal state.
     */
    std::string toString() const;

private:
    /**
     * Parses the base chunk data on all usages:
     * {history: <>, shard: <>, onCurrentShardSince: <>}
     */
    static StatusWith<ChunkType> _parseChunkBase(const BSONObj& source);


    // Convention: (M)andatory, (O)ptional, (S)pecial; (C)onfig, (S)hard.

    // (M)(C)     auto-generated object id
    boost::optional<OID> _id;
    // (O)(C)     uuid of the collection in the CollectionCatalog
    boost::optional<UUID> _collectionUUID;
    // (M)(C)(S)  first key of the range, inclusive
    boost::optional<BSONObj> _min;
    // (M)(C)(S)  last key of the range, non-inclusive
    boost::optional<BSONObj> _max;
    // (M)(C)(S)  version of this chunk
    boost::optional<ChunkVersion> _version;
    // (M)(C)(S)  shard this chunk lives in
    boost::optional<ShardId> _shard;
    // (O)(C)     chunk size used for chunk merging operation
    boost::optional<int64_t> _estimatedSizeBytes;
    // (O)(C)     too big to move?
    boost::optional<bool> _jumbo;
    // (M)(C)(S)  timestamp since this chunk belongs to the current shard
    boost::optional<Timestamp> _onCurrentShardSince;
    // history of the chunk
    std::vector<ChunkHistory> _history;
};

}  // namespace mongo
