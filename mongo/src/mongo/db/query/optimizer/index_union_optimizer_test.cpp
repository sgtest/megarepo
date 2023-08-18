/**
 *    Copyright (C) 2023-present MongoDB, Inc.
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

#include <string>
#include <utility>
#include <vector>

#include <absl/container/node_hash_map.h>
#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>
#include <boost/optional/optional.hpp>

#include "mongo/base/string_data.h"
#include "mongo/db/pipeline/abt/utils.h"
#include "mongo/db/query/cost_model/cost_model_gen.h"
#include "mongo/db/query/optimizer/bool_expression.h"
#include "mongo/db/query/optimizer/defs.h"
#include "mongo/db/query/optimizer/index_bounds.h"
#include "mongo/db/query/optimizer/metadata.h"
#include "mongo/db/query/optimizer/metadata_factory.h"
#include "mongo/db/query/optimizer/node.h"  // IWYU pragma: keep
#include "mongo/db/query/optimizer/opt_phase_manager.h"
#include "mongo/db/query/optimizer/partial_schema_requirements.h"
#include "mongo/db/query/optimizer/props.h"
#include "mongo/db/query/optimizer/syntax/path.h"
#include "mongo/db/query/optimizer/syntax/syntax.h"
#include "mongo/db/query/optimizer/utils/abt_hash.h"
#include "mongo/db/query/optimizer/utils/physical_plan_builder.h"
#include "mongo/db/query/optimizer/utils/unit_test_abt_literals.h"
#include "mongo/db/query/optimizer/utils/unit_test_utils.h"
#include "mongo/db/query/optimizer/utils/utils.h"
#include "mongo/unittest/framework.h"

using namespace mongo::optimizer::unit_test_abt_literals;

namespace mongo::optimizer {
namespace {

TEST(LogicalRewriter, MakeSargableNodeWithTopLevelDisjunction) {
    // Hand-build SargableNode with top-level disjunction.
    auto req = PartialSchemaRequirement(
        boost::none, _disj(_conj(_interval(_incl("1"_cint32), _incl("1"_cint32)))), false);

    auto makeKey = [](std::string pathName) {
        return PartialSchemaKey("ptest",
                                make<PathGet>(FieldNameType{pathName}, make<PathIdentity>()));
    };
    PSRExprBuilder builder;
    builder.pushDisj()
        .pushConj()
        .atom({makeKey("a"), req})
        .atom({makeKey("b"), req})
        .pop()
        .pushConj()
        .atom({makeKey("c"), req})
        .atom({makeKey("d"), req})
        .pop();
    auto reqs = builder.finish().get();

    ABT scanNode = make<ScanNode>("ptest", "test");
    ABT sargableNode = make<SargableNode>(
        reqs, CandidateIndexes(), boost::none, IndexReqTarget::Index, std::move(scanNode));
    ABT rootNode = make<RootNode>(properties::ProjectionRequirement{ProjectionNameVector{"ptest"}},
                                  std::move(sargableNode));
    ASSERT_EXPLAIN_V2_AUTO(
        "Root [{ptest}]\n"
        "Sargable [Index]\n"
        "|   requirements: \n"
        "|       {\n"
        "|           {\n"
        "|               {ptest, 'PathGet [a] PathIdentity []', {{{=Const [1]}}}}\n"
        "|            ^ \n"
        "|               {ptest, 'PathGet [b] PathIdentity []', {{{=Const [1]}}}}\n"
        "|           }\n"
        "|        U \n"
        "|           {\n"
        "|               {ptest, 'PathGet [c] PathIdentity []', {{{=Const [1]}}}}\n"
        "|            ^ \n"
        "|               {ptest, 'PathGet [d] PathIdentity []', {{{=Const [1]}}}}\n"
        "|           }\n"
        "|       }\n"
        "Scan [test, {ptest}]\n",
        rootNode);

    // Show that hashing a top-level disjunction doesn't throw.
    ABTHashGenerator::generate(rootNode);
}

TEST(LogicalRewriter, ToplevelDisjunctionConversion) {
    // When we have a Filter with a top-level disjunction,
    // it gets translated to a Sargable node with top-level disjunction.

    // {$or: [ {a: 2}, {b: 3} ]}
    ABT rootNode = NodeBuilder{}
                       .root("scan_0")
                       .filter(_evalf(_composea(_get("a", _cmp("Eq", "2"_cint64)),
                                                _get("b", _cmp("Eq", "3"_cint64))),
                                      "scan_0"_var))
                       .finish(_scan("scan_0", "coll"));

    auto prefixId = PrefixId::createForTests();
    auto phaseManager = makePhaseManager({OptPhase::MemoSubstitutionPhase},
                                         prefixId,
                                         {{{"coll", createScanDef({}, {})}}},
                                         boost::none /*costModel*/,
                                         DebugInfo::kDefaultForTests,
                                         QueryHints{});

    ABT optimized = rootNode;
    phaseManager.optimize(optimized);

    ASSERT_EXPLAIN_V2_AUTO(
        "Root [{scan_0}]\n"
        "Sargable [Complete]\n"
        "|   |   requirements: \n"
        "|   |       {\n"
        "|   |           {{scan_0, 'PathGet [a] PathIdentity []', {{{=Const [2]}}}}}\n"
        "|   |        U \n"
        "|   |           {{scan_0, 'PathGet [b] PathIdentity []', {{{=Const [3]}}}}}\n"
        "|   |       }\n"
        "|   scanParams: \n"
        "|       {'a': evalTemp_0, 'b': evalTemp_1}\n"
        "|           residualReqs: \n"
        "|               {\n"
        "|                   {{evalTemp_0, 'PathIdentity []', {{{=Const [2]}}}, entryIndex: 0}}\n"
        "|                U \n"
        "|                   {{evalTemp_1, 'PathIdentity []', {{{=Const [3]}}}, entryIndex: 1}}\n"
        "|               }\n"
        "Scan [coll, {scan_0}]\n",
        optimized);
}

TEST(LogicalRewriter, ToplevelNestedDisjunctionConversion) {
    // When we have a Filter with a top-level disjunction,
    // it gets translated to a Sargable node with top-level disjunction,
    // even if it's a nested disjunction.

    // {$or: [{$or: [{a: 2}. {b: 3}]}, {$or: [{c: 4}, {b: 5}]}]}
    ABT rootNode = NodeBuilder{}
                       .root("scan_0")
                       .filter(_evalf(_composea(_composea(_get("a", _cmp("Eq", "2"_cint64)),
                                                          _get("b", _cmp("Eq", "3"_cint64))),
                                                _composea(_get("c", _cmp("Eq", "4"_cint64)),
                                                          _get("d", _cmp("Eq", "5"_cint64)))),
                                      "scan_0"_var))
                       .finish(_scan("scan_0", "coll"));

    auto prefixId = PrefixId::createForTests();
    auto phaseManager = makePhaseManager({OptPhase::MemoSubstitutionPhase},
                                         prefixId,
                                         {{{"coll", createScanDef({}, {})}}},
                                         boost::none /*costModel*/,
                                         DebugInfo::kDefaultForTests,
                                         QueryHints{});

    ABT optimized = rootNode;
    phaseManager.optimize(optimized);

    ASSERT_EXPLAIN_V2_AUTO(
        "Root [{scan_0}]\n"
        "Sargable [Complete]\n"
        "|   |   requirements: \n"
        "|   |       {\n"
        "|   |           {{scan_0, 'PathGet [a] PathIdentity []', {{{=Const [2]}}}}}\n"
        "|   |        U \n"
        "|   |           {{scan_0, 'PathGet [b] PathIdentity []', {{{=Const [3]}}}}}\n"
        "|   |        U \n"
        "|   |           {{scan_0, 'PathGet [c] PathIdentity []', {{{=Const [4]}}}}}\n"
        "|   |        U \n"
        "|   |           {{scan_0, 'PathGet [d] PathIdentity []', {{{=Const [5]}}}}}\n"
        "|   |       }\n"
        "|   scanParams: \n"
        "|       {'a': evalTemp_0, 'b': evalTemp_1, 'c': evalTemp_2, 'd': evalTemp_3}\n"
        "|           residualReqs: \n"
        "|               {\n"
        "|                   {{evalTemp_0, 'PathIdentity []', {{{=Const [2]}}}, entryIndex: 0}}\n"
        "|                U \n"
        "|                   {{evalTemp_1, 'PathIdentity []', {{{=Const [3]}}}, entryIndex: 1}}\n"
        "|                U \n"
        "|                   {{evalTemp_2, 'PathIdentity []', {{{=Const [4]}}}, entryIndex: 2}}\n"
        "|                U \n"
        "|                   {{evalTemp_3, 'PathIdentity []', {{{=Const [5]}}}, entryIndex: 3}}\n"
        "|               }\n"
        "Scan [coll, {scan_0}]\n",
        optimized);
}

TEST(LogicalRewriter, ComplexBooleanConversion) {

    auto leaf0 = _get("a", _cmp("Eq", "0"_cint64));
    auto leaf1 = _get("b", _cmp("Eq", "1"_cint64));
    auto leaf2 = _get("c", _cmp("Eq", "2"_cint64));
    auto leaf3 = _get("d", _cmp("Eq", "3"_cint64));
    auto leaf4 = _get("e", _cmp("Eq", "4"_cint64));
    auto leaf5 = _get("f", _cmp("Eq", "5"_cint64));
    auto path = _composem(
        leaf0, _composea(leaf1, _composem(leaf2, _composea(leaf3, _composem(leaf4, leaf5)))));
    ABT rootNode = NodeBuilder{}
                       .root("scan_0")
                       .filter(_evalf(path, "scan_0"_var))
                       .finish(_scan("scan_0", "coll"));

    auto prefixId = PrefixId::createForTests();
    auto phaseManager = makePhaseManager({OptPhase::MemoSubstitutionPhase},
                                         prefixId,
                                         {{{"coll", createScanDef({}, {})}}},
                                         boost::none /*costModel*/,
                                         DebugInfo::kDefaultForTests,
                                         QueryHints{});

    ABT optimized = rootNode;
    phaseManager.optimize(optimized);

    // For now PSR conversion fails because the result would not be DNF.
    ASSERT_EXPLAIN_V2_AUTO(
        "Root [{scan_0}]\n"
        "Filter []\n"
        "|   EvalFilter []\n"
        "|   |   Variable [scan_0]\n"
        "|   PathComposeA []\n"
        "|   |   PathComposeM []\n"
        "|   |   |   PathComposeA []\n"
        "|   |   |   |   PathComposeM []\n"
        "|   |   |   |   |   PathGet [f]\n"
        "|   |   |   |   |   PathCompare [Eq]\n"
        "|   |   |   |   |   Const [5]\n"
        "|   |   |   |   PathGet [e]\n"
        "|   |   |   |   PathCompare [Eq]\n"
        "|   |   |   |   Const [4]\n"
        "|   |   |   PathGet [d]\n"
        "|   |   |   PathCompare [Eq]\n"
        "|   |   |   Const [3]\n"
        "|   |   PathGet [c]\n"
        "|   |   PathCompare [Eq]\n"
        "|   |   Const [2]\n"
        "|   PathGet [b]\n"
        "|   PathCompare [Eq]\n"
        "|   Const [1]\n"
        "Sargable [Complete]\n"
        "|   |   requirements: \n"
        "|   |       {{{scan_0, 'PathGet [a] PathIdentity []', {{{=Const [0]}}}}}}\n"
        "|   scanParams: \n"
        "|       {'a': evalTemp_0}\n"
        "|           residualReqs: \n"
        "|               {{{evalTemp_0, 'PathIdentity []', {{{=Const [0]}}}, entryIndex: 0}}}\n"
        "Scan [coll, {scan_0}]\n",
        optimized);
}

TEST(LogicalRewriter, DisjunctionProjectionConversion) {

    auto leaf0 = _get("a", _cmp("Eq", "0"_cint64));
    auto leaf1 = _get("b", _cmp("Eq", "1"_cint64));
    auto path = _composea(leaf0, leaf1);
    ABT rootNode = NodeBuilder{}
                       .root("doc")
                       .eval("doc", _evalp(_keep(FieldNameType{"x"}), "scan_0"_var))
                       .filter(_evalf(path, "scan_0"_var))
                       .finish(_scan("scan_0", "coll"));

    auto prefixId = PrefixId::createForTests();
    auto phaseManager = makePhaseManager({OptPhase::MemoSubstitutionPhase},
                                         prefixId,
                                         {{{"coll", createScanDef({}, {})}}},
                                         boost::none /*costModel*/,
                                         DebugInfo::kDefaultForTests,
                                         QueryHints{});

    ABT optimized = rootNode;
    phaseManager.optimize(optimized);

    // We get two Sargable nodes, but they aren't combined, because converting to DNF would
    // distribute the projection into both disjuncts, and for now we don't want to have
    // projections inside a (nontrivial) disjunction.
    ASSERT_EXPLAIN_V2_AUTO(
        "Root [{doc}]\n"
        "Evaluation [{doc}]\n"
        "|   EvalPath []\n"
        "|   |   Const [{}]\n"
        "|   PathField [x]\n"
        "|   PathConstant []\n"
        "|   Variable [fieldProj_0]\n"
        "Sargable [Complete]\n"
        "|   |   requirements: \n"
        "|   |       {\n"
        "|   |           {{scan_0, 'PathGet [a] PathIdentity []', {{{=Const [0]}}}}}\n"
        "|   |        U \n"
        "|   |           {{scan_0, 'PathGet [b] PathIdentity []', {{{=Const [1]}}}}}\n"
        "|   |       }\n"
        "|   scanParams: \n"
        "|       {'a': evalTemp_0, 'b': evalTemp_1}\n"
        "|           residualReqs: \n"
        "|               {\n"
        "|                   {{evalTemp_0, 'PathIdentity []', {{{=Const [0]}}}, entryIndex: 0}}\n"
        "|                U \n"
        "|                   {{evalTemp_1, 'PathIdentity []', {{{=Const [1]}}}, entryIndex: 1}}\n"
        "|               }\n"
        "Sargable [Complete]\n"
        "|   |   requirements: \n"
        "|   |       {{{scan_0, 'PathGet [x] PathIdentity []', fieldProj_0, {{{<fully open>}}}}}}\n"
        "|   scanParams: \n"
        "|       {'x': fieldProj_0}\n"
        "Scan [coll, {scan_0}]\n",
        optimized);
}

TEST(LogicalRewriter, DisjunctionConversionDedup) {

    auto leaf0 = _get("a", _cmp("Eq", "0"_cint64));
    auto leaf1 = _get("b", _cmp("Eq", "1"_cint64));
    auto path = _composea(_composea(leaf0, leaf1), _composea(leaf0, leaf0));
    ABT rootNode = NodeBuilder{}
                       .root("scan_0")
                       .filter(_evalf(path, "scan_0"_var))
                       .finish(_scan("scan_0", "coll"));

    auto prefixId = PrefixId::createForTests();
    auto phaseManager = makePhaseManager({OptPhase::MemoSubstitutionPhase},
                                         prefixId,
                                         {{{"coll", createScanDef({}, {})}}},
                                         boost::none /*costModel*/,
                                         DebugInfo::kDefaultForTests,
                                         QueryHints{});

    ABT optimized = rootNode;
    phaseManager.optimize(optimized);

    // We should see everything get reordered and deduped,
    // so each of the leaf predicates appears once.
    ASSERT_EXPLAIN_V2_AUTO(
        "Root [{scan_0}]\n"
        "Sargable [Complete]\n"
        "|   |   requirements: \n"
        "|   |       {\n"
        "|   |           {{scan_0, 'PathGet [a] PathIdentity []', {{{=Const [0]}}}}}\n"
        "|   |        U \n"
        "|   |           {{scan_0, 'PathGet [b] PathIdentity []', {{{=Const [1]}}}}}\n"
        "|   |       }\n"
        "|   scanParams: \n"
        "|       {'a': evalTemp_0, 'b': evalTemp_1}\n"
        "|           residualReqs: \n"
        "|               {\n"
        "|                   {{evalTemp_0, 'PathIdentity []', {{{=Const [0]}}}, entryIndex: 0}}\n"
        "|                U \n"
        "|                   {{evalTemp_1, 'PathIdentity []', {{{=Const [1]}}}, entryIndex: 1}}\n"
        "|               }\n"
        "Scan [coll, {scan_0}]\n",
        optimized);
}

TEST(PhysRewriter, LowerRequirementsWithTopLevelDisjunction) {
    auto req =
        PartialSchemaRequirement(boost::none,
                                 _disj(_conj(_interval(_incl("1"_cint32), _incl("1"_cint32)))),
                                 false /*perfOnly*/);

    auto makeKey = [](std::string pathName) {
        return PartialSchemaKey("ptest",
                                make<PathGet>(FieldNameType{pathName}, make<PathIdentity>()));
    };

    CEType scanGroupCE{10.0};
    FieldProjectionMap fieldProjectionMap;
    fieldProjectionMap._rootProjection = "ptest";
    std::vector<SelectivityType> indexPredSels;

    PhysPlanBuilder builder;
    builder.make<PhysicalScanNode>(
        scanGroupCE, fieldProjectionMap, "test" /* scanDefName */, false /* parallelScan */);

    BoolExprBuilder<ResidualRequirementWithOptionalCE> residReqsBuilder;
    residReqsBuilder.pushDisj()
        .pushConj()
        .atom({makeKey("a"), req, CEType{2.0}})
        .atom({makeKey("b"), req, CEType{3.0}})
        .pop()
        .pushConj()
        .atom({makeKey("c"), req, CEType{5.0}})
        .atom({makeKey("d"), req, CEType{4.0}})
        .pop();
    auto residReqs = residReqsBuilder.finish().get();
    lowerPartialSchemaRequirements(
        scanGroupCE, scanGroupCE, indexPredSels, residReqs, defaultConvertPathToInterval, builder);

    ASSERT_EXPLAIN_V2_AUTO(
        "Filter []\n"
        "|   BinaryOp [Or]\n"
        "|   |   BinaryOp [And]\n"
        "|   |   |   EvalFilter []\n"
        "|   |   |   |   Variable [ptest]\n"
        "|   |   |   PathGet [c]\n"
        "|   |   |   PathCompare [Eq]\n"
        "|   |   |   Const [1]\n"
        "|   |   EvalFilter []\n"
        "|   |   |   Variable [ptest]\n"
        "|   |   PathGet [d]\n"
        "|   |   PathCompare [Eq]\n"
        "|   |   Const [1]\n"
        "|   BinaryOp [And]\n"
        "|   |   EvalFilter []\n"
        "|   |   |   Variable [ptest]\n"
        "|   |   PathGet [b]\n"
        "|   |   PathCompare [Eq]\n"
        "|   |   Const [1]\n"
        "|   EvalFilter []\n"
        "|   |   Variable [ptest]\n"
        "|   PathGet [a]\n"
        "|   PathCompare [Eq]\n"
        "|   Const [1]\n"
        "PhysicalScan [{'<root>': ptest}, test]\n",
        builder._node);
}

TEST(PhysRewriter, OptimizeSargableNodeWithTopLevelDisjunction) {
    auto req =
        PartialSchemaRequirement(boost::none,
                                 _disj(_conj(_interval(_incl("1"_cint32), _incl("1"_cint32)))),
                                 false /*perfOnly*/);

    auto makeKey = [](std::string pathName) {
        return PartialSchemaKey("ptest",
                                make<PathGet>(FieldNameType{pathName}, make<PathIdentity>()));
    };

    // Create three SargableNodes with top-level disjunctions.
    PSRExprBuilder builder;
    builder.pushDisj()
        .pushConj()
        .atom({makeKey("a"), req})
        .atom({makeKey("b"), req})
        .pop()
        .pushConj()
        .atom({makeKey("c"), req})
        .atom({makeKey("d"), req})
        .pop();
    auto reqs1 = builder.finish().get();

    builder.pushDisj()
        .pushConj()
        .atom({makeKey("e"), req})
        .pop()
        .pushConj()
        .atom({makeKey("f"), req})
        .pop();
    auto reqs2 = builder.finish().get();

    ABT scanNode = make<ScanNode>("ptest", "test");
    ABT sargableNode1 = make<SargableNode>(
        reqs1, CandidateIndexes(), boost::none, IndexReqTarget::Complete, std::move(scanNode));
    ABT sargableNode2 = make<SargableNode>(
        reqs2, CandidateIndexes(), boost::none, IndexReqTarget::Complete, std::move(sargableNode1));
    ABT rootNode = make<RootNode>(properties::ProjectionRequirement{ProjectionNameVector{"ptest"}},
                                  std::move(sargableNode2));

    auto prefixId = PrefixId::createForTests();
    auto phaseManager = makePhaseManager(
        {
            OptPhase::MemoSubstitutionPhase,
            OptPhase::MemoExplorationPhase,
            OptPhase::MemoImplementationPhase,
        },
        prefixId,
        {{{"test",
           createScanDef(
               {},
               {
                   {"ab",
                    IndexDefinition{{{makeNonMultikeyIndexPath("a"), CollationOp::Ascending},
                                     {makeNonMultikeyIndexPath("b"), CollationOp::Ascending}},
                                    false /*isMultiKey*/}},
                   {"cd",
                    IndexDefinition{{{makeNonMultikeyIndexPath("c"), CollationOp::Ascending},
                                     {makeNonMultikeyIndexPath("d"), CollationOp::Ascending}},
                                    false /*isMultiKey*/}},
                   {"e", makeIndexDefinition("e", CollationOp::Ascending, false /*isMultiKey*/)},
                   {"f", makeIndexDefinition("f", CollationOp::Ascending, false /*isMultiKey*/)},
                   {"g", makeIndexDefinition("g", CollationOp::Ascending, false /*isMultiKey*/)},
               })}}},
        boost::none /*costModel*/,
        DebugInfo::kDefaultForTests,
        QueryHints{
            ._disableScan = true,
        });
    phaseManager.optimize(rootNode);

    // We should get an index union between 'ab' and 'cd'.
    ASSERT_EXPLAIN_V2Compact_AUTO(
        "Root [{ptest}]\n"
        "Filter []\n"
        "|   BinaryOp [Or]\n"
        "|   |   EvalFilter []\n"
        "|   |   |   Variable [ptest]\n"
        "|   |   PathGet [f] PathCompare [Eq] Const [1]\n"
        "|   EvalFilter []\n"
        "|   |   Variable [ptest]\n"
        "|   PathGet [e] PathCompare [Eq] Const [1]\n"
        "NestedLoopJoin [joinType: Inner, {rid_0}]\n"
        "|   |   Const [true]\n"
        "|   LimitSkip [limit: 1, skip: 0]\n"
        "|   Seek [ridProjection: rid_0, {'<root>': ptest}, test]\n"
        "Unique [{rid_0}]\n"
        "Union [{rid_0}]\n"
        "|   IndexScan [{'<rid>': rid_0}, scanDefName: test, indexDefName: cd, interval: {=Const "
        "[1 | 1]}]\n"
        "IndexScan [{'<rid>': rid_0}, scanDefName: test, indexDefName: ab, interval: {=Const [1 | "
        "1]}]\n",
        rootNode);
}

TEST(PhysRewriter, ThreeWayIndexUnion) {
    auto req =
        PartialSchemaRequirement(boost::none,
                                 _disj(_conj(_interval(_incl("1"_cint32), _incl("1"_cint32)))),
                                 false /*perfOnly*/);

    auto makeKey = [](std::string pathName) {
        return PartialSchemaKey("ptest",
                                make<PathGet>(FieldNameType{pathName}, make<PathIdentity>()));
    };

    // Create three SargableNodes with a 3-argument disjunction.
    PSRExprBuilder builder;
    builder.pushDisj()
        .pushConj()
        .atom({makeKey("a"), req})
        .pop()
        .pushConj()
        .atom({makeKey("b"), req})
        .pop()
        .pushConj()
        .atom({makeKey("c"), req})
        .pop();
    auto reqs = builder.finish().get();

    ABT scanNode = make<ScanNode>("ptest", "test");
    ABT sargableNode = make<SargableNode>(
        reqs, CandidateIndexes(), boost::none, IndexReqTarget::Complete, std::move(scanNode));
    ABT rootNode = make<RootNode>(properties::ProjectionRequirement{ProjectionNameVector{"ptest"}},
                                  std::move(sargableNode));

    // Show that the optimization of the SargableNode does not throw, and that all three
    // SargableNodes are correctly lowered to FilterNodes.
    auto prefixId = PrefixId::createForTests();
    auto phaseManager = makePhaseManager(
        {
            OptPhase::MemoSubstitutionPhase,
            OptPhase::MemoExplorationPhase,
            OptPhase::MemoImplementationPhase,
        },
        prefixId,
        {{{"test",
           createScanDef(
               {},
               {
                   {"a",
                    IndexDefinition{{{makeNonMultikeyIndexPath("a"), CollationOp::Ascending}},
                                    false /*isMultiKey*/}},
                   {"b",
                    IndexDefinition{{{makeNonMultikeyIndexPath("b"), CollationOp::Ascending}},
                                    false /*isMultiKey*/}},
                   {"c",
                    IndexDefinition{{{makeNonMultikeyIndexPath("c"), CollationOp::Ascending}},
                                    false /*isMultiKey*/}},
               })}}},
        boost::none /*costModel*/,
        DebugInfo::kDefaultForTests,
        QueryHints{
            ._disableScan = true,
        });
    phaseManager.optimize(rootNode);

    // We should get a union of three index scans.
    ASSERT_EXPLAIN_V2Compact_AUTO(
        "Root [{ptest}]\n"
        "NestedLoopJoin [joinType: Inner, {rid_0}]\n"
        "|   |   Const [true]\n"
        "|   LimitSkip [limit: 1, skip: 0]\n"
        "|   Seek [ridProjection: rid_0, {'<root>': ptest}, test]\n"
        "Unique [{rid_0}]\n"
        "Union [{rid_0}]\n"
        "|   Union [{rid_0}]\n"
        "|   |   IndexScan [{'<rid>': rid_0}, scanDefName: test, indexDefName: c, interval: "
        "{=Const [1]}]\n"
        "|   IndexScan [{'<rid>': rid_0}, scanDefName: test, indexDefName: b, interval: {=Const "
        "[1]}]\n"
        "IndexScan [{'<rid>': rid_0}, scanDefName: test, indexDefName: a, interval: {=Const "
        "[1]}]\n",
        rootNode);
}

}  // namespace
}  // namespace mongo::optimizer
