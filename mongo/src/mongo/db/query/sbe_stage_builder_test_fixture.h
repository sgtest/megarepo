/**
 *    Copyright (C) 2020-present MongoDB, Inc.
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

#include <boost/smart_ptr/intrusive_ptr.hpp>
#include <memory>
#include <tuple>
#include <utility>

#include "mongo/db/catalog/collection.h"
#include "mongo/db/exec/sbe/sbe_plan_stage_test.h"
#include "mongo/db/exec/sbe/stages/stages.h"
#include "mongo/db/exec/sbe/values/slot.h"
#include "mongo/db/exec/sbe/values/value.h"
#include "mongo/db/namespace_string.h"
#include "mongo/db/pipeline/expression_context.h"
#include "mongo/db/query/collation/collator_interface.h"
#include "mongo/db/query/multiple_collection_accessor.h"
#include "mongo/db/query/query_solution.h"
#include "mongo/db/query/sbe_stage_builder.h"
#include "mongo/db/query/shard_filterer_factory_interface.h"
#include "mongo/unittest/unittest.h"

namespace mongo {
/**
 * SbeStageBuilderTestFixture is a unittest fixture that can be used to facilitate testing the
 * translation of a QuerySolution tree to an sbe PlanStage tree.
 *
 * The main mechanism that enables the whole sbe::PlanStage tree to be exercised under unittests is
 * the use of a VirtualScanNode. This virtual node can be created with a vector of BSONArray
 * documents and used as a replacement for a CollectionScanNode in the QuerySolution tree. A testing
 * client would manually build a QuerySolution tree containing this VirtualScanNode and
 * then transform it to an sbe::PlanStage by calling buildPlanStage(). The buildPlanStage()
 * method will do the QuerySolution to sbe::PlanStage translation, and return a vector of result
 * slots, the prepared sub-tree and a PlanStageData that carries the sbe::CompileCtx needed to
 * prepare the sbe::PlanStage tree. The PlanStageData returned from buildPlanStage() must be
 * kept alive across buildPlanStage(), prepareTree() and execution of the plan.
 */
class SbeStageBuilderTestFixture : public sbe::PlanStageTestFixture {
public:
    SbeStageBuilderTestFixture() = default;

    /**
     * Makes a QuerySolution from a QuerySolutionNode.
     */
    std::unique_ptr<QuerySolution> makeQuerySolution(std::unique_ptr<QuerySolutionNode> root);

    /**
     * Builds an sbe::PlanStage tree from a QuerySolution that can be executed by repeatedly calling
     * getNext() on the root. Results from the PlanStage are exposed through the returned
     * SlotVector. If hasRecordId is 'true' then the returned SlotVector will carry a SlotId in the
     * 0th position for the RecordId and a SlotId for the BSONObj representation of the document in
     * the 1st position. Otherwise, if hasRecordId is 'false', the SlotVector will contain a single
     * SlotId for the BSONObj representation of the document. A real or mock
     * ShardFiltererFactoryInterface must be provided so the sbe SlotBasedStageBuilder can build and
     * utilize a ShardFilterer instance during translation of a ShardingFilterNode.
     */
    std::tuple<sbe::value::SlotVector,
               std::unique_ptr<sbe::PlanStage>,
               stage_builder::PlanStageData,
               boost::intrusive_ptr<ExpressionContext>>
    buildPlanStage(std::unique_ptr<QuerySolution> querySolution,
                   MultipleCollectionAccessor& colls,
                   bool hasRecordId,
                   std::unique_ptr<ShardFiltererFactoryInterface> shardFiltererFactoryInterface,
                   std::unique_ptr<CollatorInterface> collator = nullptr);

    std::tuple<sbe::value::SlotVector,
               std::unique_ptr<sbe::PlanStage>,
               stage_builder::PlanStageData,
               boost::intrusive_ptr<ExpressionContext>>
    buildPlanStage(std::unique_ptr<QuerySolution> querySolution,
                   bool hasRecordId,
                   std::unique_ptr<ShardFiltererFactoryInterface> shardFiltererFactoryInterface,
                   std::unique_ptr<CollatorInterface> collator = nullptr) {
        auto nullColl = MultipleCollectionAccessor(CollectionPtr::null);
        return buildPlanStage(std::move(querySolution),
                              nullColl,
                              hasRecordId,
                              std::move(shardFiltererFactoryInterface),
                              std::move(collator));
    }

protected:
    const NamespaceString _nss =
        NamespaceString::createNamespaceString_forTest("testdb.sbe_stage_builder");
};

}  // namespace mongo
