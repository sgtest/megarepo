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

#include <boost/optional.hpp>
#include <set>
#include <type_traits>

#include <absl/container/node_hash_map.h>
#include <boost/move/utility_core.hpp>
#include <boost/optional/optional.hpp>

#include "mongo/db/exec/sbe/values/value.h"
#include "mongo/db/query/optimizer/algebra/polyvalue.h"
#include "mongo/db/query/optimizer/containers.h"
#include "mongo/db/query/optimizer/node.h"  // IWYU pragma: keep
#include "mongo/db/query/optimizer/syntax/expr.h"
#include "mongo/db/query/optimizer/utils/path_utils.h"
#include "mongo/db/query/optimizer/utils/strong_alias.h"
#include "mongo/db/query/optimizer/utils/utils.h"
#include "mongo/util/str.h"

namespace mongo::optimizer {

/**
 * A simple helper that creates a vector of Sources and binds names.
 */
static ABT buildSimpleBinder(ProjectionNameVector names) {
    ABTVector sources;
    for (size_t idx = 0; idx < names.size(); ++idx) {
        sources.emplace_back(make<Source>());
    }

    return make<ExpressionBinder>(std::move(names), std::move(sources));
}

/**
 * Builds References from the provided projection names. Equality of References is sensitive
 * to order, so the projections are sorted first.
 */
static ABT buildReferences(const ProjectionNameSet& projections) {
    ABTVector variables;
    ProjectionNameOrderedSet ordered{projections.cbegin(), projections.cend()};
    for (const ProjectionName& projection : ordered) {
        variables.emplace_back(make<Variable>(projection));
    }
    return make<References>(std::move(variables));
}

ScanNode::ScanNode(ProjectionName projectionName, std::string scanDefName)
    : Base(buildSimpleBinder({std::move(projectionName)})), _scanDefName(std::move(scanDefName)) {}

const ProjectionName& ScanNode::getProjectionName() const {
    return binder().names()[0];
}

const std::string& ScanNode::getScanDefName() const {
    return _scanDefName;
}

bool ScanNode::operator==(const ScanNode& other) const {
    return getProjectionName() == other.getProjectionName() && _scanDefName == other._scanDefName;
}

static ProjectionNameVector extractProjectionNamesForScan(
    const FieldProjectionMap& fieldProjectionMap) {
    ProjectionNameVector result;

    if (const auto& projName = fieldProjectionMap._ridProjection) {
        result.push_back(*projName);
    }
    if (const auto& projName = fieldProjectionMap._rootProjection) {
        result.push_back(*projName);
    }
    for (const auto& entry : fieldProjectionMap._fieldProjections) {
        result.push_back(entry.second);
    }

    return result;
}

PhysicalScanNode::PhysicalScanNode(FieldProjectionMap fieldProjectionMap,
                                   std::string scanDefName,
                                   bool useParallelScan,
                                   ScanOrder scanOrder)
    : Base(buildSimpleBinder(extractProjectionNamesForScan(fieldProjectionMap))),
      _fieldProjectionMap(std::move(fieldProjectionMap)),
      _scanDefName(std::move(scanDefName)),
      _useParallelScan(useParallelScan),
      _scanOrder(scanOrder) {}

bool PhysicalScanNode::operator==(const PhysicalScanNode& other) const {
    return _fieldProjectionMap == other._fieldProjectionMap && _scanDefName == other._scanDefName &&
        _useParallelScan == other._useParallelScan;
}

const FieldProjectionMap& PhysicalScanNode::getFieldProjectionMap() const {
    return _fieldProjectionMap;
}

const std::string& PhysicalScanNode::getScanDefName() const {
    return _scanDefName;
}

bool PhysicalScanNode::useParallelScan() const {
    return _useParallelScan;
}

ScanOrder PhysicalScanNode::getScanOrder() const {
    return _scanOrder;
}

ValueScanNode::ValueScanNode(ProjectionNameVector projections,
                             boost::optional<properties::LogicalProps> props)
    : ValueScanNode(
          std::move(projections), std::move(props), Constant::emptyArray(), false /*hasRID*/) {}

ValueScanNode::ValueScanNode(ProjectionNameVector projections,
                             boost::optional<properties::LogicalProps> props,
                             ABT valueArray,
                             const bool hasRID)
    : Base(buildSimpleBinder(std::move(projections))),
      _props(std::move(props)),
      _valueArray(std::move(valueArray)),
      _hasRID(hasRID) {
    const auto constPtr = _valueArray.cast<Constant>();
    tassert(6624081, "ValueScan must be initialized with a constant", constPtr != nullptr);

    const auto [tag, val] = constPtr->get();
    tassert(
        6624082, "ValueScan must be initialized with an array", tag == sbe::value::TypeTags::Array);

    const auto arr = sbe::value::getArrayView(val);
    _arraySize = arr->size();
    const size_t projectionCount = binder().names().size();
    for (size_t i = 0; i < _arraySize; i++) {
        const auto [tag1, val1] = arr->getAt(i);
        tassert(6624083,
                "ValueScan must be initialized with an array of arrays: each subarray is a row, "
                "with one element per projection",
                tag1 == sbe::value::TypeTags::Array);

        const auto innerArray = sbe::value::getArrayView(val1);
        size_t expectedSize = projectionCount + (_hasRID ? 1 : 0);
        tassert(6624084,
                str::stream() << "ValueScanNode expected " << expectedSize
                              << " elements in each subarray (one per projection) but got "
                              << innerArray->size(),
                innerArray->size() == expectedSize);
        tassert(6624177,
                "First element must be a RecordId",
                !_hasRID || innerArray->getAt(0).first == sbe::value::TypeTags::RecordId);
    }
}

bool ValueScanNode::operator==(const ValueScanNode& other) const {
    return binder() == other.binder() && _props == other._props && _arraySize == other._arraySize &&
        _valueArray == other._valueArray && _hasRID == other._hasRID;
}

const boost::optional<properties::LogicalProps>& ValueScanNode::getProps() const {
    return _props;
}

const ABT& ValueScanNode::getValueArray() const {
    return _valueArray;
}

size_t ValueScanNode::getArraySize() const {
    return _arraySize;
}

bool ValueScanNode::getHasRID() const {
    return _hasRID;
}

CoScanNode::CoScanNode() : Base() {}

bool CoScanNode::operator==(const CoScanNode& other) const {
    return true;
}

IndexScanNode::IndexScanNode(FieldProjectionMap fieldProjectionMap,
                             std::string scanDefName,
                             std::string indexDefName,
                             CompoundIntervalRequirement indexInterval,
                             bool isIndexReverseOrder)
    : Base(buildSimpleBinder(extractProjectionNamesForScan(fieldProjectionMap))),
      _fieldProjectionMap(std::move(fieldProjectionMap)),
      _scanDefName(std::move(scanDefName)),
      _indexDefName(std::move(indexDefName)),
      _indexInterval(std::move(indexInterval)),
      _isIndexReverseOrder(isIndexReverseOrder) {}

bool IndexScanNode::operator==(const IndexScanNode& other) const {
    // Scan spec does not participate, the indexSpec by itself should determine equality.
    return _fieldProjectionMap == other._fieldProjectionMap && _scanDefName == other._scanDefName &&
        _indexDefName == other._indexDefName && _indexInterval == other._indexInterval &&
        _isIndexReverseOrder == other._isIndexReverseOrder;
}

const FieldProjectionMap& IndexScanNode::getFieldProjectionMap() const {
    return _fieldProjectionMap;
}

const std::string& IndexScanNode::getScanDefName() const {
    return _scanDefName;
}

const std::string& IndexScanNode::getIndexDefName() const {
    return _indexDefName;
}

const CompoundIntervalRequirement& IndexScanNode::getIndexInterval() const {
    return _indexInterval;
}

bool IndexScanNode::isIndexReverseOrder() const {
    return _isIndexReverseOrder;
}

SeekNode::SeekNode(ProjectionName ridProjectionName,
                   FieldProjectionMap fieldProjectionMap,
                   std::string scanDefName)
    : Base(buildSimpleBinder(extractProjectionNamesForScan(fieldProjectionMap)),
           make<References>(ProjectionNameVector{std::move(ridProjectionName)})),
      _fieldProjectionMap(std::move(fieldProjectionMap)),
      _scanDefName(std::move(scanDefName)) {}

bool SeekNode::operator==(const SeekNode& other) const {
    return getRIDProjectionName() == other.getRIDProjectionName() &&
        _fieldProjectionMap == other._fieldProjectionMap && _scanDefName == other._scanDefName;
}

const FieldProjectionMap& SeekNode::getFieldProjectionMap() const {
    return _fieldProjectionMap;
}

const std::string& SeekNode::getScanDefName() const {
    return _scanDefName;
}

const ProjectionName& SeekNode::getRIDProjectionName() const {
    return get<1>().cast<References>()->nodes()[0].cast<Variable>()->name();
}

MemoLogicalDelegatorNode::MemoLogicalDelegatorNode(const GroupIdType groupId)
    : Base(), _groupId(groupId) {}

GroupIdType MemoLogicalDelegatorNode::getGroupId() const {
    return _groupId;
}

bool MemoLogicalDelegatorNode::operator==(const MemoLogicalDelegatorNode& other) const {
    return _groupId == other._groupId;
}

MemoPhysicalDelegatorNode::MemoPhysicalDelegatorNode(const MemoPhysicalNodeId nodeId)
    : Base(), _nodeId(nodeId) {}

bool MemoPhysicalDelegatorNode::operator==(const MemoPhysicalDelegatorNode& other) const {
    return _nodeId == other._nodeId;
}

MemoPhysicalNodeId MemoPhysicalDelegatorNode::getNodeId() const {
    return _nodeId;
}

FilterNode::FilterNode(FilterType filter, ABT child) : Base(std::move(child), std::move(filter)) {
    assertExprSort(getFilter());
    assertNodeSort(getChild());
}

bool FilterNode::operator==(const FilterNode& other) const {
    return getFilter() == other.getFilter() && getChild() == other.getChild();
}

const FilterType& FilterNode::getFilter() const {
    return get<1>();
}

FilterType& FilterNode::getFilter() {
    return get<1>();
}

const ABT& FilterNode::getChild() const {
    return get<0>();
}

ABT& FilterNode::getChild() {
    return get<0>();
}

EvaluationNode::EvaluationNode(ProjectionName projectionName, ProjectionType projection, ABT child)
    : Base(std::move(child),
           make<ExpressionBinder>(std::move(projectionName), std::move(projection))) {
    assertNodeSort(getChild());
}

bool EvaluationNode::operator==(const EvaluationNode& other) const {
    return binder() == other.binder() && getProjection() == other.getProjection() &&
        getChild() == other.getChild();
}

RIDIntersectNode::RIDIntersectNode(ProjectionName scanProjectionName, ABT leftChild, ABT rightChild)
    : Base(std::move(leftChild), std::move(rightChild)),
      _scanProjectionName(std::move(scanProjectionName)) {
    assertNodeSort(getLeftChild());
    assertNodeSort(getRightChild());
}

const ABT& RIDIntersectNode::getLeftChild() const {
    return get<0>();
}

ABT& RIDIntersectNode::getLeftChild() {
    return get<0>();
}

const ABT& RIDIntersectNode::getRightChild() const {
    return get<1>();
}

ABT& RIDIntersectNode::getRightChild() {
    return get<1>();
}

bool RIDIntersectNode::operator==(const RIDIntersectNode& other) const {
    return _scanProjectionName == other._scanProjectionName &&
        getLeftChild() == other.getLeftChild() && getRightChild() == other.getRightChild();
}

const ProjectionName& RIDIntersectNode::getScanProjectionName() const {
    return _scanProjectionName;
}

/**
 * A helper that builds References object of UnionNode or SortedMergeNode for reference tracking
 * purposes.
 *
 * Example: union outputs 3 projections: A,B,C and it has 4 children. Then the References object is
 * a vector of variables A,B,C,A,B,C,A,B,C,A,B,C. One group of variables per child.
 */
static ABT buildUnionTypeReferences(const ProjectionNameVector& names, const size_t numOfChildren) {
    ABTVector variables;
    for (size_t outerIdx = 0; outerIdx < numOfChildren; ++outerIdx) {
        for (size_t idx = 0; idx < names.size(); ++idx) {
            variables.emplace_back(make<Variable>(names[idx]));
        }
    }

    return make<References>(std::move(variables));
}

RIDUnionNode::RIDUnionNode(ProjectionName scanProjectionName,
                           ProjectionNameVector unionProjectionNames,
                           ABT leftChild,
                           ABT rightChild)
    : Base(std::move(leftChild),
           std::move(rightChild),
           buildSimpleBinder(unionProjectionNames),
           buildUnionTypeReferences(unionProjectionNames, 2)),
      _scanProjectionName(std::move(scanProjectionName)) {
    tassert(7858803,
            "Scan projection must exist in the RIDUnionNode projection list",
            std::find(unionProjectionNames.cbegin(),
                      unionProjectionNames.cend(),
                      _scanProjectionName) != unionProjectionNames.cend());
    assertNodeSort(getLeftChild());
    assertNodeSort(getRightChild());
}

const ABT& RIDUnionNode::getLeftChild() const {
    return get<0>();
}

ABT& RIDUnionNode::getLeftChild() {
    return get<0>();
}

const ABT& RIDUnionNode::getRightChild() const {
    return get<1>();
}

ABT& RIDUnionNode::getRightChild() {
    return get<1>();
}

const ExpressionBinder& RIDUnionNode::binder() const {
    const ABT& result = get<2>();
    tassert(7858801, "Invalid binder type", result.is<ExpressionBinder>());
    return *result.cast<ExpressionBinder>();
}

bool RIDUnionNode::operator==(const RIDUnionNode& other) const {
    return _scanProjectionName == other._scanProjectionName &&
        getLeftChild() == other.getLeftChild() && getRightChild() == other.getRightChild();
}

const ProjectionName& RIDUnionNode::getScanProjectionName() const {
    return _scanProjectionName;
}

static ProjectionNameVector createSargableBindings(const PSRExpr::Node& reqMap) {
    ProjectionNameVector result;
    PSRExpr::visitDNF(reqMap, [&](const PartialSchemaEntry& e, const PSRExpr::VisitorContext&) {
        if (auto binding = e.second.getBoundProjectionName()) {
            result.push_back(*binding);
        }
    });
    return result;
}

static ProjectionNameVector createSargableReferences(const PSRExpr::Node& reqMap) {
    ProjectionNameOrderPreservingSet result;
    PSRExpr::visitDNF(reqMap, [&](const PartialSchemaEntry& e, const PSRExpr::VisitorContext&) {
        result.emplace_back(*e.first._projectionName);
    });
    return result.getVector();
}

SargableNode::SargableNode(PSRExpr::Node reqMap,
                           CandidateIndexes candidateIndexes,
                           boost::optional<ScanParams> scanParams,
                           const IndexReqTarget target,
                           ABT child)
    : Base(std::move(child),
           buildSimpleBinder(createSargableBindings(reqMap)),
           make<References>(createSargableReferences(reqMap))),
      _reqMap(std::move(reqMap)),
      _candidateIndexes(std::move(candidateIndexes)),
      _scanParams(std::move(scanParams)),
      _target(target) {
    assertNodeSort(getChild());
    tassert(6624085, "SargableNode requires at least one predicate", !psr::isNoop(_reqMap));
    tassert(7447500, "SargableNode requirements should be in DNF", PSRExpr::isDNF(_reqMap));
    if (const size_t numLeaves = PSRExpr::numLeaves(_reqMap); numLeaves > kMaxPartialSchemaReqs) {
        tasserted(6624086,
                  str::stream() << "SargableNode has too many predicates: " << numLeaves
                                << ". We allow at most " << kMaxPartialSchemaReqs);
    }

    auto bindings = createSargableBindings(_reqMap);
    tassert(7410100,
            "SargableNode with top-level OR cannot bind",
            bindings.empty() || PSRExpr::isSingletonDisjunction(_reqMap));

    ProjectionNameSet boundsProjectionNameSet(bindings.begin(), bindings.end());

    // Assert there are no perf-only binding requirements, references to internally bound
    // projections, or non-trivial multikey requirements which also bind. Further assert that under
    // a conjunction 1) non-multikey paths have at most one req and 2) there are no duplicate bound
    // projection names.
    PSRExpr::visitDisjuncts(
        _reqMap, [&](const PSRExpr::Node& disjunct, const PSRExpr::VisitorContext&) {
            PartialSchemaKeySet seenKeys;
            ProjectionNameSet seenProjNames;
            PSRExpr::visitConjuncts(
                disjunct, [&](const PSRExpr::Node& conjunct, const PSRExpr::VisitorContext&) {
                    PSRExpr::visitAtom(
                        conjunct, [&](const PartialSchemaEntry& e, const PSRExpr::VisitorContext&) {
                            const auto& [key, req] = e;
                            if (auto projName = req.getBoundProjectionName()) {
                                tassert(6624094,
                                        "SargableNode has a multikey requirement with a "
                                        "non-trivial interval which "
                                        "also binds",
                                        isIntervalReqFullyOpenDNF(req.getIntervals()) ||
                                            !checkPathContainsTraverse(key._path));
                                tassert(6624095,
                                        "SargableNode has a perf only binding requirement",
                                        !req.getIsPerfOnly());

                                auto insertedBoundProj = seenProjNames.insert(*projName).second;
                                tassert(6624087,
                                        "PartialSchemaRequirements has duplicate bound projection "
                                        "names in "
                                        "a conjunction",
                                        insertedBoundProj);
                            }

                            tassert(6624088,
                                    "SargableNode cannot reference an internally bound projection",
                                    boundsProjectionNameSet.count(*key._projectionName) == 0);

                            if (!checkPathContainsTraverse(key._path)) {
                                auto insertedKey = seenKeys.insert(key).second;
                                tassert(7155020,
                                        "PartialSchemaRequirements has two predicates on the same "
                                        "non-multikey "
                                        "path in a conjunction",
                                        insertedKey);
                            }
                        });
                });
        });
}

bool SargableNode::operator==(const SargableNode& other) const {
    // Specifically not comparing the candidate indexes and ScanParams. Those are derivative of the
    // requirements, and can have temp projection names.
    return _reqMap == other._reqMap && _target == other._target && getChild() == other.getChild();
}

const PSRExpr::Node& SargableNode::getReqMap() const {
    return _reqMap;
}

const CandidateIndexes& SargableNode::getCandidateIndexes() const {
    return _candidateIndexes;
}

const boost::optional<ScanParams>& SargableNode::getScanParams() const {
    return _scanParams;
}

IndexReqTarget SargableNode::getTarget() const {
    return _target;
}

BinaryJoinNode::BinaryJoinNode(JoinType joinType,
                               ProjectionNameSet correlatedProjectionNames,
                               FilterType filter,
                               ABT leftChild,
                               ABT rightChild)
    : Base(std::move(leftChild), std::move(rightChild), std::move(filter)),
      _joinType(joinType),
      _correlatedProjectionNames(std::move(correlatedProjectionNames)) {
    assertExprSort(getFilter());
    assertNodeSort(getLeftChild());
    assertNodeSort(getRightChild());
}

JoinType BinaryJoinNode::getJoinType() const {
    return _joinType;
}

const ProjectionNameSet& BinaryJoinNode::getCorrelatedProjectionNames() const {
    return _correlatedProjectionNames;
}

bool BinaryJoinNode::operator==(const BinaryJoinNode& other) const {
    return _joinType == other._joinType &&
        _correlatedProjectionNames == other._correlatedProjectionNames &&
        getLeftChild() == other.getLeftChild() && getRightChild() == other.getRightChild();
}

const ABT& BinaryJoinNode::getLeftChild() const {
    return get<0>();
}

ABT& BinaryJoinNode::getLeftChild() {
    return get<0>();
}

const ABT& BinaryJoinNode::getRightChild() const {
    return get<1>();
}

ABT& BinaryJoinNode::getRightChild() {
    return get<1>();
}

const ABT& BinaryJoinNode::getFilter() const {
    return get<2>();
}

static ABT buildHashJoinReferences(const ProjectionNameVector& leftKeys,
                                   const ProjectionNameVector& rightKeys) {
    ABTVector variables;
    for (const ProjectionName& projection : leftKeys) {
        variables.emplace_back(make<Variable>(projection));
    }
    for (const ProjectionName& projection : rightKeys) {
        variables.emplace_back(make<Variable>(projection));
    }

    return make<References>(std::move(variables));
}

HashJoinNode::HashJoinNode(JoinType joinType,
                           ProjectionNameVector leftKeys,
                           ProjectionNameVector rightKeys,
                           ABT leftChild,
                           ABT rightChild)
    : Base(std::move(leftChild),
           std::move(rightChild),
           buildHashJoinReferences(leftKeys, rightKeys)),
      _joinType(joinType),
      _leftKeys(std::move(leftKeys)),
      _rightKeys(std::move(rightKeys)) {
    tassert(6624089,
            "Mismatched number of left and right join keys",
            !_leftKeys.empty() && _leftKeys.size() == _rightKeys.size());
    assertNodeSort(getLeftChild());
    assertNodeSort(getRightChild());
}

bool HashJoinNode::operator==(const HashJoinNode& other) const {
    return _joinType == other._joinType && _leftKeys == other._leftKeys &&
        _rightKeys == other._rightKeys && getLeftChild() == other.getLeftChild() &&
        getRightChild() == other.getRightChild();
}

JoinType HashJoinNode::getJoinType() const {
    return _joinType;
}

const ProjectionNameVector& HashJoinNode::getLeftKeys() const {
    return _leftKeys;
}

const ProjectionNameVector& HashJoinNode::getRightKeys() const {
    return _rightKeys;
}

const ABT& HashJoinNode::getLeftChild() const {
    return get<0>();
}

ABT& HashJoinNode::getLeftChild() {
    return get<0>();
}

const ABT& HashJoinNode::getRightChild() const {
    return get<1>();
}

ABT& HashJoinNode::getRightChild() {
    return get<1>();
}

MergeJoinNode::MergeJoinNode(ProjectionNameVector leftKeys,
                             ProjectionNameVector rightKeys,
                             std::vector<CollationOp> collation,
                             ABT leftChild,
                             ABT rightChild)
    : Base(std::move(leftChild),
           std::move(rightChild),
           buildHashJoinReferences(leftKeys, rightKeys)),
      _collation(std::move(collation)),
      _leftKeys(std::move(leftKeys)),
      _rightKeys(std::move(rightKeys)) {
    tassert(6624090,
            "Mismatched number of left and right join keys",
            !_leftKeys.empty() && _leftKeys.size() == _rightKeys.size());
    tassert(
        6624091, "Mismatched collation and join key size", _collation.size() == _leftKeys.size());
    assertNodeSort(getLeftChild());
    assertNodeSort(getRightChild());
    for (const auto& collReq : _collation) {
        tassert(7063704,
                "MergeJoin collation requirement must be ascending or descending",
                collReq == CollationOp::Ascending || collReq == CollationOp::Descending);
    }
}

bool MergeJoinNode::operator==(const MergeJoinNode& other) const {
    return _leftKeys == other._leftKeys && _rightKeys == other._rightKeys &&
        _collation == other._collation && getLeftChild() == other.getLeftChild() &&
        getRightChild() == other.getRightChild();
}

const ProjectionNameVector& MergeJoinNode::getLeftKeys() const {
    return _leftKeys;
}

const ProjectionNameVector& MergeJoinNode::getRightKeys() const {
    return _rightKeys;
}

const std::vector<CollationOp>& MergeJoinNode::getCollation() const {
    return _collation;
}

const ABT& MergeJoinNode::getLeftChild() const {
    return get<0>();
}

ABT& MergeJoinNode::getLeftChild() {
    return get<0>();
}

const ABT& MergeJoinNode::getRightChild() const {
    return get<1>();
}

ABT& MergeJoinNode::getRightChild() {
    return get<1>();
}

NestedLoopJoinNode::NestedLoopJoinNode(JoinType joinType,
                                       ProjectionNameSet correlatedProjectionNames,
                                       FilterType filter,
                                       ABT leftChild,
                                       ABT rightChild)
    : Base(std::move(leftChild), std::move(rightChild), std::move(filter)),
      _joinType(joinType),
      _correlatedProjectionNames(std::move(correlatedProjectionNames)) {
    assertExprSort(getFilter());
    assertNodeSort(getLeftChild());
    assertNodeSort(getRightChild());
}

JoinType NestedLoopJoinNode::getJoinType() const {
    return _joinType;
}

const ProjectionNameSet& NestedLoopJoinNode::getCorrelatedProjectionNames() const {
    return _correlatedProjectionNames;
}

bool NestedLoopJoinNode::operator==(const NestedLoopJoinNode& other) const {
    return _joinType == other._joinType &&
        _correlatedProjectionNames == other._correlatedProjectionNames &&
        getLeftChild() == other.getLeftChild() && getRightChild() == other.getRightChild();
}

const ABT& NestedLoopJoinNode::getLeftChild() const {
    return get<0>();
}

ABT& NestedLoopJoinNode::getLeftChild() {
    return get<0>();
}

const ABT& NestedLoopJoinNode::getRightChild() const {
    return get<1>();
}

ABT& NestedLoopJoinNode::getRightChild() {
    return get<1>();
}

const ABT& NestedLoopJoinNode::getFilter() const {
    return get<2>();
}

// Helper function to get the projection names from a CollationRequirement as a vector instead of a
// set, since we would like to keep the order.
static ProjectionNameVector getAffectedProjectionNamesOrdered(
    const properties::CollationRequirement& collReq) {
    ProjectionNameVector result;
    for (const auto& entry : collReq.getCollationSpec()) {
        result.push_back(entry.first);
    }
    return result;
}

SortedMergeNode::SortedMergeNode(properties::CollationRequirement collReq, ABTVector children)
    : SortedMergeNode(std::move(collReq), NodeChildrenHolder{std::move(children)}) {}

SortedMergeNode::SortedMergeNode(properties::CollationRequirement collReq,
                                 NodeChildrenHolder children)
    : Base(std::move(children._nodes),
           buildSimpleBinder(getAffectedProjectionNamesOrdered(collReq)),
           buildUnionTypeReferences(getAffectedProjectionNamesOrdered(collReq),
                                    children._numOfNodes)),
      _collationReq(collReq) {
    for (auto& n : nodes()) {
        assertNodeSort(n);
    }
    for (const auto& collReq : _collationReq.getCollationSpec()) {
        tassert(7063703,
                "SortedMerge collation requirement must be ascending or descending",
                collReq.second == CollationOp::Ascending ||
                    collReq.second == CollationOp::Descending);
    }
}

const properties::CollationRequirement& SortedMergeNode::getCollationReq() const {
    return _collationReq;
}

bool SortedMergeNode::operator==(const SortedMergeNode& other) const {
    return _collationReq == other._collationReq && binder() == other.binder() &&
        nodes() == other.nodes();
}

UnionNode::UnionNode(ProjectionNameVector unionProjectionNames, ABTVector children)
    : UnionNode(std::move(unionProjectionNames), NodeChildrenHolder{std::move(children)}) {}

UnionNode::UnionNode(ProjectionNameVector unionProjectionNames, NodeChildrenHolder children)
    : Base(std::move(children._nodes),
           buildSimpleBinder(unionProjectionNames),
           buildUnionTypeReferences(unionProjectionNames, children._numOfNodes)) {
    tassert(
        6624007, "UnionNode must have a non-empty projection list", !unionProjectionNames.empty());

    for (auto& n : nodes()) {
        assertNodeSort(n);
    }
}

bool UnionNode::operator==(const UnionNode& other) const {
    return binder() == other.binder() && nodes() == other.nodes();
}

GroupByNode::GroupByNode(ProjectionNameVector groupByProjectionNames,
                         ProjectionNameVector aggregationProjectionNames,
                         ABTVector aggregationExpressions,
                         ABT child)
    : GroupByNode(std::move(groupByProjectionNames),
                  std::move(aggregationProjectionNames),
                  std::move(aggregationExpressions),
                  GroupNodeType::Complete,
                  std::move(child)) {}

GroupByNode::GroupByNode(ProjectionNameVector groupByProjectionNames,
                         ProjectionNameVector aggregationProjectionNames,
                         ABTVector aggregationExpressions,
                         GroupNodeType type,
                         ABT child)
    : Base(std::move(child),
           buildSimpleBinder(std::move(aggregationProjectionNames)),
           make<References>(std::move(aggregationExpressions)),
           buildSimpleBinder(groupByProjectionNames),
           make<References>(groupByProjectionNames)),
      _type(type) {
    assertNodeSort(getChild());
    tassert(6624300,
            "Mismatched number of agg expressions and projection names",
            getAggregationExpressions().size() == getAggregationProjectionNames().size());
}

bool GroupByNode::operator==(const GroupByNode& other) const {
    return getAggregationProjectionNames() == other.getAggregationProjectionNames() &&
        getAggregationProjections() == other.getAggregationProjections() &&
        getGroupByProjectionNames() == other.getGroupByProjectionNames() && _type == other._type &&
        getChild() == other.getChild();
}

const ABTVector& GroupByNode::getAggregationExpressions() const {
    return get<2>().cast<References>()->nodes();
}

const ABT& GroupByNode::getChild() const {
    return get<0>();
}

ABT& GroupByNode::getChild() {
    return get<0>();
}

GroupNodeType GroupByNode::getType() const {
    return _type;
}

UnwindNode::UnwindNode(ProjectionName projectionName,
                       ProjectionName pidProjectionName,
                       const bool retainNonArrays,
                       ABT child)
    : Base(std::move(child),
           buildSimpleBinder(ProjectionNameVector{projectionName, std::move(pidProjectionName)}),
           make<References>(ProjectionNameVector{projectionName})),
      _retainNonArrays(retainNonArrays) {
    assertNodeSort(getChild());
}

bool UnwindNode::getRetainNonArrays() const {
    return _retainNonArrays;
}

const ABT& UnwindNode::getChild() const {
    return get<0>();
}

ABT& UnwindNode::getChild() {
    return get<0>();
}

bool UnwindNode::operator==(const UnwindNode& other) const {
    return binder() == other.binder() && _retainNonArrays == other._retainNonArrays &&
        getChild() == other.getChild();
}

UniqueNode::UniqueNode(ProjectionNameVector projections, ABT child)
    : Base(std::move(child), make<References>(ProjectionNameVector{projections})),
      _projections(std::move(projections)) {
    assertNodeSort(getChild());
    tassert(6624092, "UniqueNode must have a non-empty projection list", !_projections.empty());
}

bool UniqueNode::operator==(const UniqueNode& other) const {
    return _projections == other._projections;
}

const ProjectionNameVector& UniqueNode::getProjections() const {
    return _projections;
}

const ABT& UniqueNode::getChild() const {
    return get<0>();
}

ABT& UniqueNode::getChild() {
    return get<0>();
}

SpoolProducerNode::SpoolProducerNode(const SpoolProducerType type,
                                     const int64_t spoolId,
                                     ProjectionNameVector projections,
                                     ABT filter,
                                     ABT child)
    : Base(std::move(child),
           std::move(filter),
           buildSimpleBinder(projections),
           make<References>(projections)),
      _type(type),
      _spoolId(spoolId) {
    assertNodeSort(getChild());
    assertExprSort(getFilter());
    tassert(
        6624155, "Spool producer must have a non-empty projection list", !binder().names().empty());
    tassert(6624120,
            "Invalid combination of spool producer type and spool filter",
            _type == SpoolProducerType::Lazy || getFilter() == Constant::boolean(true));
}

bool SpoolProducerNode::operator==(const SpoolProducerNode& other) const {
    return _type == other._type && _spoolId == other._spoolId && getFilter() == other.getFilter() &&
        binder() == other.binder();
}

SpoolProducerType SpoolProducerNode::getType() const {
    return _type;
}

int64_t SpoolProducerNode::getSpoolId() const {
    return _spoolId;
}

const ABT& SpoolProducerNode::getFilter() const {
    return get<1>();
}

const ABT& SpoolProducerNode::getChild() const {
    return get<0>();
}

ABT& SpoolProducerNode::getChild() {
    return get<0>();
}

SpoolConsumerNode::SpoolConsumerNode(const SpoolConsumerType type,
                                     const int64_t spoolId,
                                     ProjectionNameVector projections)
    : Base(buildSimpleBinder(std::move(projections))), _type(type), _spoolId(spoolId) {
    tassert(
        6624125, "Spool consumer must have a non-empty projection list", !binder().names().empty());
}

bool SpoolConsumerNode::operator==(const SpoolConsumerNode& other) const {
    return _type == other._type && _spoolId == other._spoolId && binder() == other.binder();
}

SpoolConsumerType SpoolConsumerNode::getType() const {
    return _type;
}

int64_t SpoolConsumerNode::getSpoolId() const {
    return _spoolId;
}

CollationNode::CollationNode(properties::CollationRequirement property, ABT child)
    : Base(std::move(child),
           buildReferences(extractReferencedColumns(properties::makePhysProps(property)))),
      _property(std::move(property)) {
    assertNodeSort(getChild());
}

const properties::CollationRequirement& CollationNode::getProperty() const {
    return _property;
}

properties::CollationRequirement& CollationNode::getProperty() {
    return _property;
}

bool CollationNode::operator==(const CollationNode& other) const {
    return _property == other._property && getChild() == other.getChild();
}

const ABT& CollationNode::getChild() const {
    return get<0>();
}

ABT& CollationNode::getChild() {
    return get<0>();
}

LimitSkipNode::LimitSkipNode(properties::LimitSkipRequirement property, ABT child)
    : Base(std::move(child)), _property(std::move(property)) {
    assertNodeSort(getChild());
}

const properties::LimitSkipRequirement& LimitSkipNode::getProperty() const {
    return _property;
}

properties::LimitSkipRequirement& LimitSkipNode::getProperty() {
    return _property;
}

bool LimitSkipNode::operator==(const LimitSkipNode& other) const {
    return _property == other._property && getChild() == other.getChild();
}

const ABT& LimitSkipNode::getChild() const {
    return get<0>();
}

ABT& LimitSkipNode::getChild() {
    return get<0>();
}

ExchangeNode::ExchangeNode(properties::DistributionRequirement distribution, ABT child)
    : Base(std::move(child), buildReferences(distribution.getAffectedProjectionNames())),
      _distribution(std::move(distribution)) {
    assertNodeSort(getChild());
    tassert(6624008,
            "Cannot exchange towards an unknown distribution",
            _distribution.getDistributionAndProjections()._type !=
                DistributionType::UnknownPartitioning);
}

bool ExchangeNode::operator==(const ExchangeNode& other) const {
    return _distribution == other._distribution && getChild() == other.getChild();
}

const ABT& ExchangeNode::getChild() const {
    return get<0>();
}

ABT& ExchangeNode::getChild() {
    return get<0>();
}

const properties::DistributionRequirement& ExchangeNode::getProperty() const {
    return _distribution;
}

properties::DistributionRequirement& ExchangeNode::getProperty() {
    return _distribution;
}

RootNode::RootNode(properties::ProjectionRequirement property, ABT child)
    : Base(std::move(child), buildReferences(property.getAffectedProjectionNames())),
      _property(std::move(property)) {
    assertNodeSort(getChild());
}

bool RootNode::operator==(const RootNode& other) const {
    return getChild() == other.getChild() && _property == other._property;
}

const properties::ProjectionRequirement& RootNode::getProperty() const {
    return _property;
}

const ABT& RootNode::getChild() const {
    return get<0>();
}

ABT& RootNode::getChild() {
    return get<0>();
}

}  // namespace mongo::optimizer
