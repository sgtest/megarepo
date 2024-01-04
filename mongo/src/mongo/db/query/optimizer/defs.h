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

#include <absl/container/node_hash_map.h>
#include <boost/optional.hpp>
#include <boost/optional/optional.hpp>
#include <cstddef>
#include <cstdint>
#include <limits>
#include <set>
#include <sstream>
#include <string>
#include <utility>
#include <vector>

#include "mongo/db/query/optimizer/containers.h"
#include "mongo/db/query/optimizer/syntax/syntax_fwd_declare.h"
#include "mongo/db/query/optimizer/utils/strong_alias.h"
#include "mongo/db/query/util/named_enum.h"


namespace mongo::optimizer {

/**
 * Representation of a field name. Can be empty.
 */
struct FieldNameAliasTag {
    static constexpr bool kAllowEmpty = true;
};
using FieldNameType = StrongStringAlias<FieldNameAliasTag>;

using FieldPathType = std::vector<FieldNameType>;
using FieldNameOrderedSet = std::set<FieldNameType>;
using FieldNameSet = opt::unordered_set<FieldNameType, FieldNameType::Hasher>;

/**
 * Representation of a variable name. Cannot be empty.
 */
struct ProjectionNameAliasTag {
    static constexpr bool kAllowEmpty = false;
};
using ProjectionName = StrongStringAlias<ProjectionNameAliasTag>;

using ProjectionNameSet = opt::unordered_set<ProjectionName, ProjectionName::Hasher>;
using ProjectionNameOrderedSet = std::set<ProjectionName>;
using ProjectionNameVector = std::vector<ProjectionName>;

template <typename T>
using ProjectionNameMap = opt::unordered_map<ProjectionName, T, ProjectionName::Hasher>;

// Key: new/target projection, value: existing/source projection.
using ProjectionRenames = ProjectionNameMap<ProjectionName>;

// Map from scanDefName to rid projection name.
using RIDProjectionsMap = opt::unordered_map<std::string, ProjectionName>;

/**
 * A set of projection names which remembers the order in which elements were inserted.
 */
class ProjectionNameOrderPreservingSet {
public:
    ProjectionNameOrderPreservingSet() = default;
    ProjectionNameOrderPreservingSet(ProjectionNameVector v);

    ProjectionNameOrderPreservingSet(const ProjectionNameOrderPreservingSet& other) = default;
    ProjectionNameOrderPreservingSet(ProjectionNameOrderPreservingSet&& other) = default;

    bool operator==(const ProjectionNameOrderPreservingSet& other) const;
    ProjectionNameOrderPreservingSet& operator=(const ProjectionNameOrderPreservingSet& other) =
        default;

    std::pair<size_t, bool> emplace_back(ProjectionName projectionName);
    boost::optional<size_t> find(const ProjectionName& projectionName) const;
    bool erase(const ProjectionName& projectionName);

    bool isEqualIgnoreOrder(const ProjectionNameOrderPreservingSet& other) const;

    const ProjectionNameVector& getVector() const;

private:
    ProjectionNameMap<size_t> _map;
    ProjectionNameVector _vector;
};

#define INDEXREQTARGET_NAMES(F) \
    F(Index)                    \
    F(Seek)                     \
    F(Complete)

QUERY_UTIL_NAMED_ENUM_DEFINE(IndexReqTarget, INDEXREQTARGET_NAMES);
#undef INDEXREQTARGET_NAMES

#define DISTRIBUTIONTYPE_NAMES(F) \
    F(Centralized)                \
    F(Replicated)                 \
    F(RoundRobin)                 \
    F(HashPartitioning)           \
    F(RangePartitioning)          \
    F(UnknownPartitioning)

QUERY_UTIL_NAMED_ENUM_DEFINE(DistributionType, DISTRIBUTIONTYPE_NAMES);
#undef DISTRIBUTIONTYPE_NAMES

// In case of covering scan, index, or fetch, specify names of bound projections for each field.
// Also optionally specify if applicable the rid and record (root) projections.
struct FieldProjectionMap {
    boost::optional<ProjectionName> _ridProjection;
    boost::optional<ProjectionName> _rootProjection;
    std::map<FieldNameType, ProjectionName> _fieldProjections;

    bool operator==(const FieldProjectionMap& other) const;
};

// Used to generate field names encoding index keys for covered indexes.
static constexpr const char* kIndexKeyPrefix = "<indexKey>";

/**
 * Function that replaces parameterized constants in a MatchExpression with their corresponding
 * param id's in ABT.
 *
 * Represented by an ABT FunctionCall node with two children:
 * (1) parameter id (int) that maps to the constant value
 * (2) enum/int representation of the constant's sbe type tag
 */
static constexpr auto kParameterFunctionName = "getParam";

/**
 * Memo-related types.
 */
using GroupIdType = int64_t;

// Logical node id.
struct MemoLogicalNodeId {
    GroupIdType _groupId;
    size_t _index;

    bool operator==(const MemoLogicalNodeId& other) const;
};

struct NodeIdHash {
    size_t operator()(const MemoLogicalNodeId& id) const;
};
using NodeIdSet = opt::unordered_set<MemoLogicalNodeId, NodeIdHash>;

// Physical node id.
struct MemoPhysicalNodeId {
    GroupIdType _groupId;
    size_t _index;

    bool operator==(const MemoPhysicalNodeId& other) const;
};

class DebugInfo {
public:
    static const int kIterationLimitForTests = 10000;
    static const int kDefaultDebugLevelForTests = 1;

    static DebugInfo kDefaultForTests;
    static DebugInfo kDefaultForProd;

    DebugInfo(bool debugMode, int debugLevel, int iterationLimit);

    bool isDebugMode() const;

    bool hasDebugLevel(int debugLevel) const;

    bool exceedsIterationLimit(int iterations) const;

private:
    // Are we in debug mode? Can we do additional logging, etc?
    const bool _debugMode;

    const int _debugLevel;

    // Maximum number of rewrite iterations.
    const int _iterationLimit;
};


struct SelectivityTag {
    // Selectivity does not have units, it is a simple ratio.
    static constexpr bool kUnitless = true;
    static constexpr double kMaxValue = 1.0;
    static constexpr double kMinValue = 0.0;
};
using SelectivityType = StrongDoubleAlias<SelectivityTag>;

struct CETag {
    // Cardinality has units: it is measured in documents.
    static constexpr bool kUnitless = false;
    static constexpr double kMaxValue = std::numeric_limits<double>::max();
    static constexpr double kMinValue = 0.0;
};
using CEType = StrongDoubleAlias<CETag>;

// We can multiply a cardinality and a selectivity to obtain a cardinality.
constexpr CEType operator*(const CEType v1, const SelectivityType v2) {
    return {v1._value * v2._value};
}
constexpr CEType operator*(const SelectivityType v1, const CEType v2) {
    return {v1._value * v2._value};
}
constexpr CEType& operator*=(CEType& v1, const SelectivityType v2) {
    v1._value *= v2._value;
    return v1;
}

// Holds a CE and the estimation method used to derive it.
struct CERecord {
    CEType _ce;
    std::string _mode;

    bool operator==(const CERecord& other) const = default;
};

// We can divide two cardinalities to obtain a selectivity.
constexpr SelectivityType operator/(const CEType v1, const CEType v2) {
    return {v1._value / v2._value};
}

// Constant to correct for the difference between CE estimates which don't contain orphans and
// physical execution of plans which will encounter orphans if shard filtering has not occurred.
inline constexpr double kOrphansCardinalityFudgeFactor = 1.001;

class CostType {
    static constexpr double kPrecision = 0.00000001;

public:
    static CostType kInfinity;
    static CostType kZero;

    static CostType fromDouble(double cost);

    CostType() = default;
    CostType(const CostType& other) = default;
    CostType(CostType&& other) = default;
    CostType& operator=(const CostType& other) = default;

    bool operator==(const CostType& other) = delete;
    bool operator!=(const CostType& other) = delete;
    bool operator<(const CostType& other) const;

    CostType operator+(const CostType& other) const;
    CostType operator-(const CostType& other) const;
    CostType& operator+=(const CostType& other);

    std::string toString() const;

    /**
     * Returns the cost as a double, or asserts if infinite.
     */
    double getCost() const;

    bool isInfinite() const;

private:
    CostType(bool isInfinite, double cost);

    bool _isInfinite{false};
    double _cost{0.0};
};

struct CostAndCE {
    CostType _cost;
    CEType _ce;
};

/**
 * Note: Ascending and Descending sorts are performed according to the semantics of BinaryOp
 * comparisons: gt, lt, etc where for examples arrays sort after all numbers, as opposed to sort
 * semantics where arrays sort relative to numbers and one another based on their smallest/largest
 * element as defined by the sort path.
 */
#define COLLATIONOP_OPNAMES(F) \
    F(Ascending)               \
    F(Descending)              \
    F(Clustered)

QUERY_UTIL_NAMED_ENUM_DEFINE(CollationOp, COLLATIONOP_OPNAMES);
#undef COLLATIONOP_OPNAMES

using ProjectionCollationEntry = std::pair<ProjectionName, CollationOp>;
using ProjectionCollationSpec = std::vector<ProjectionCollationEntry>;

CollationOp reverseCollationOp(CollationOp op);

bool collationOpsCompatible(CollationOp availableOp, CollationOp requiredOp);
bool collationsCompatible(const ProjectionCollationSpec& available,
                          const ProjectionCollationSpec& required);

enum class DisableIndexOptions {
    Enabled,            // All types of indexes are enabled.
    DisableAll,         // Disable all indexes.
    DisablePartialOnly  // Only disable partial indexes.
};

struct QueryHints {
    // Disable full collection scans.
    bool _disableScan = false;

    // Disable index scans.
    DisableIndexOptions _disableIndexes = DisableIndexOptions::Enabled;

    // Disable placing a hash-join during RIDIntersect implementation.
    bool _disableHashJoinRIDIntersect = false;

    // Disable placing a merge-join during RIDIntersect implementation.
    bool _disableMergeJoinRIDIntersect = false;

    // Disable placing a group-by and union based RIDIntersect implementation.
    bool _disableGroupByAndUnionRIDIntersect = false;

    // Force an index scan for eligible sargable predicate. Prevent their execution as residual.
    bool _forceIndexScanForPredicates = false;

    // If set keep track of rejected plans in the memo.
    bool _keepRejectedPlans = false;

    // Disable Cascades branch-and-bound strategy, and fully evaluate all plans. Used in conjunction
    // with keeping rejected plans.
    bool _disableBranchAndBound = false;

    // Controls if we prefer to cover queries which may return nulls with indexes, even though we
    // may not distinguish between null and missing. Alternatively we always fetch (slower).
    bool _fastIndexNullHandling = false;

    // Controls if we prefer to insert redundant index predicates on the Seek side in order to
    // prevent issues arising from yielding.
    bool _disableYieldingTolerantPlans = true;

    // Controls if we permit the optimization to remove Not operators by pushing them
    // down toward the leaves of an ABT.
    bool _enableNotPushdown = false;

    // Controls if we force sampling CE to fall back on heuristic for filter node.
    bool _forceSamplingCEFallBackForFilterNode = true;

    // Controls the minimum and maximum number of equalityPrefixes we generate for a candidate
    // index. The minimum bound is only used for testing and in production should remain set to 1.
    size_t _minIndexEqPrefixes = 1;
    size_t _maxIndexEqPrefixes = 1;

    // Rather than sampling a fully random set of documents, sample N documents (10 by default)
    // randomly and scan sequentially from each of them for the rest.
    size_t _numSamplingChunks = 10;

    // If the collection size falls within this range, sampling is a valid estimation method.
    size_t _samplingCollectionSizeMin = 100;
    size_t _samplingCollectionSizeMax = 10000;

    // Controls if we exclusively sample indexed fields.
    bool _sampleIndexedFields = true;

    // Controls if we sample the two most common indexed fields together.
    bool _sampleTwoFields = true;

    // If enabled, take the square root of numDocs for sample size.
    bool _sqrtSampleSizeEnabled = true;
};

#define SCAN_ORDER(F) \
    F(Forward)        \
    F(Reverse)        \
    F(Random)  // Uses a random cursor.

QUERY_UTIL_NAMED_ENUM_DEFINE(ScanOrder, SCAN_ORDER);
#undef SCAN_ORDER

/*
 * Type for storing mapping between query parameter IDs and Constants. Parameters always have a
 * mapping to their associated Constant and will never be Nothing.
 */
using QueryParameterMap = opt::unordered_map<int32_t, Constant>;

}  // namespace mongo::optimizer
