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

// IWYU pragma: no_include "ext/alloc_traits.h"
#include <utility>
#include <vector>

#include "mongo/base/string_data.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/bson/bsonmisc.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/bson/json.h"
#include "mongo/db/catalog/collection.h"
#include "mongo/db/exec/document_value/document.h"
#include "mongo/db/namespace_string.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/pipeline/expression_context_for_test.h"
#include "mongo/db/query/classic_stage_builder.h"
#include "mongo/db/query/find_command.h"
#include "mongo/db/query/query_solution.h"
#include "mongo/db/service_context.h"
#include "mongo/db/service_context_d_test_fixture.h"
#include "mongo/db/storage/snapshot.h"
#include "mongo/unittest/assert.h"
#include "mongo/unittest/bson_test_util.h"
#include "mongo/unittest/framework.h"
#include "mongo/util/intrusive_counter.h"

namespace mongo {

const static NamespaceString kNss = NamespaceString::createNamespaceString_forTest("db.dummy");

class ClassicStageBuilderTest : public ServiceContextMongoDTest {
public:
    ClassicStageBuilderTest() : ServiceContextMongoDTest(Options{}.useMockClock(true)) {}

    void setUp() override {
        _opCtx = makeOperationContext();
        _workingSet = std::make_unique<WorkingSet>();
    }

    void tearDown() override {
        _opCtx.reset();
        _workingSet.reset();
    }

    /**
     * Converts a 'QuerySolutionNode' to a 'QuerySolution'.
     */
    std::unique_ptr<QuerySolution> makeQuerySolution(std::unique_ptr<QuerySolutionNode> root) {
        auto querySoln = std::make_unique<QuerySolution>();
        querySoln->setRoot(std::move(root));
        return querySoln;
    }

    /**
     * Builds a PlanStage using the given WorkingSet and QuerySolution.
     */
    std::unique_ptr<PlanStage> buildPlanStage(std::unique_ptr<QuerySolution> querySolution) {
        auto findCommand = std::make_unique<FindCommandRequest>(kNss);
        auto cq = std::make_unique<CanonicalQuery>(
            CanonicalQueryParams{.expCtx = makeExpressionContext(opCtx(), *findCommand),
                                 .parsedFind = ParsedFindCommandParams{std::move(findCommand)}});

        stage_builder::ClassicStageBuilder builder{
            opCtx(), &CollectionPtr::null, *cq, *querySolution, workingSet()};
        return builder.build(querySolution->root());
    }

    /**
     * A helper to repeatedly call work() until the stage returns a PlanStage::IS_EOF state and
     * returns the resulting documents as a vector of BSONObj.
     */
    std::vector<BSONObj> collectResults(std::unique_ptr<PlanStage> stage) {
        WorkingSetID id;
        std::vector<BSONObj> results;
        auto state = PlanStage::ADVANCED;

        while (state != PlanStage::IS_EOF) {
            state = stage->work(&id);
            if (state == PlanStage::ADVANCED) {
                auto member = workingSet()->get(id);
                auto doc = member->doc.value().toBson();
                results.push_back(doc);
            }
        }
        return results;
    }

    OperationContext* opCtx() {
        return _opCtx.get();
    }

    WorkingSet* workingSet() {
        return _workingSet.get();
    }

private:
    ServiceContext::UniqueOperationContext _opCtx;
    std::unique_ptr<WorkingSet> _workingSet;
};


/**
 * Verify that a VirtualScanNode can be translated to a MockStage and produce a filtered data
 * stream.
 */
TEST_F(ClassicStageBuilderTest, VirtualScanTranslation) {
    static const std::vector<BSONArray> kFilteredDocs = std::vector<BSONArray>{
        BSON_ARRAY(BSON("a" << 1 << "b" << 2)), BSON_ARRAY(BSON("a" << 3 << "b" << 2))};
    auto filter = uassertStatusOK(MatchExpressionParser::parse(
        fromjson("{a: {$ne: 2}}"), make_intrusive<ExpressionContextForTest>(opCtx(), kNss)));

    std::vector<BSONArray> allDocs = kFilteredDocs;
    allDocs.insert(allDocs.begin() + 1, BSON_ARRAY(BSON("a" << 2 << "b" << 2)));
    allDocs.insert(allDocs.end(), BSON_ARRAY(BSON("a" << 2 << "b" << 2)));

    // Construct a QuerySolution consisting of a single VirtualScanNode to test if a stream of
    // documents can be produced and filtered, according to the provided filter.
    auto virtScan = std::make_unique<VirtualScanNode>(
        std::move(allDocs), VirtualScanNode::ScanType::kCollScan, false);
    virtScan->filter = std::move(filter);
    // Make a QuerySolution from the root virtual scan node.
    auto querySolution = makeQuerySolution(std::move(virtScan));
    ASSERT_EQ(querySolution->root()->nodeId(), 1);

    // Translate the QuerySolution to a classic PlanStage.
    auto stage = buildPlanStage(std::move(querySolution));

    // Work the stage and collect the results.
    auto results = collectResults(std::move(stage));
    ASSERT_EQ(results.size(), kFilteredDocs.size());

    // Check that the results produced from the translated VirtualScanNode meet expectation.
    for (size_t i = 0; i < kFilteredDocs.size(); ++i) {
        BSONObjIterator arrIt{kFilteredDocs[i]};
        auto firstElt = arrIt.next();
        ASSERT_BSONOBJ_EQ(firstElt.embeddedObject(), results[i]);
    }
}
}  // namespace mongo
