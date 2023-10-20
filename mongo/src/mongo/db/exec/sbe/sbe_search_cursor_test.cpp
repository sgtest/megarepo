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

/**
 * This file contains tests for sbe::SearchCursorStage.
 */

#include <memory>
#include <utility>
#include <vector>

#include "mongo/base/string_data.h"
#include "mongo/bson/bsonmisc.h"
#include "mongo/db/exec/sbe/expressions/expression.h"
#include "mongo/db/exec/sbe/sbe_plan_stage_test.h"
#include "mongo/db/exec/sbe/stages/search_cursor.h"
#include "mongo/db/exec/sbe/stages/spool.h"
#include "mongo/db/exec/sbe/stages/stages.h"
#include "mongo/db/exec/sbe/values/slot.h"
#include "mongo/db/exec/sbe/values/value.h"
#include "mongo/db/pipeline/expression_context_for_test.h"
#include "mongo/db/query/sbe_stage_builder_helpers.h"
#include "mongo/db/query/stage_types.h"
#include "mongo/executor/network_interface_mock.h"
#include "mongo/executor/remote_command_request.h"
#include "mongo/executor/thread_pool_task_executor.h"
#include "mongo/executor/thread_pool_task_executor_test_fixture.h"
#include "mongo/logv2/log.h"
#include "mongo/unittest/assert.h"
#include "mongo/unittest/framework.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kQuery
namespace mongo::sbe {

using SearchCursorStageTest = PlanStageTestFixture;

const BSONArray resultArray = BSON_ARRAY(fromjson(R"(
{
    "_id" : 0,
    "metaA" : 0,
    "metaB" : 1,
    "fieldA" : 200,
    "fieldB" : 300
})") << fromjson(R"(
{
    "_id" : 1,
    "metaA" : 2,
    "metaB" : 3,
    "fieldA" : 4,
    "fieldB" : 5
})"));

const BSONObj query = fromjson(R"(
{
})");

const BSONObj queryStoredSource = fromjson(R"(
{
    "returnStoredSource": true
})");

const BSONArray resultStoredSource = BSON_ARRAY(fromjson(R"(
{
    "storedSource" : {
        "fieldA" : 200,
        "fieldB" : 300
    },
    "metaA" : 0,
    "metaB" : 1
})") << fromjson(R"(
{
    "storedSource" : {
        "fieldA" : 4,
        "fieldB" : 5
    },
    "metaA" : 2,
    "metaB" : 3
})"));

std::unique_ptr<executor::TaskExecutorCursor> mockTaskExecutorCursor(OperationContext* opCtx,
                                                                     CursorId cursorId,
                                                                     const BSONArray& firstBatch) {
    auto networkInterface = std::make_unique<executor::NetworkInterfaceMock>();
    auto testExecutor = executor::makeSharedThreadPoolTestExecutor(std::move(networkInterface));
    executor::RemoteCommandRequest req = executor::RemoteCommandRequest();
    req.opCtx = opCtx;

    std::vector<BSONObj> batchVec;
    for (const auto& ele : firstBatch) {
        batchVec.push_back(ele.Obj());
    }

    executor::TaskExecutorCursor::Options opts;
    opts.preFetchNextBatch = false;
    return std::make_unique<executor::TaskExecutorCursor>(
        testExecutor,
        nullptr /* underlyingExec */,
        CursorResponse{NamespaceString::kEmpty, cursorId, batchVec},
        req,
        std::move(opts));
}

std::unique_ptr<PlanStage> makeSearchCursorStage(value::SlotId resultSlot,
                                                 std::vector<std::string> metadataNames,
                                                 value::SlotVector metadataSlots,
                                                 std::vector<std::string> fieldNames,
                                                 value::SlotVector fieldSlots,
                                                 bool isStoredSource,
                                                 value::SlotId limitSlot) {
    const boost::intrusive_ptr<ExpressionContextForTest> expCtx(new ExpressionContextForTest());
    expCtx->uuid = UUID::gen();
    return makeS<SearchCursorStage>(NamespaceString::kEmpty,
                                    expCtx->uuid,
                                    resultSlot,
                                    metadataNames,
                                    metadataSlots,
                                    fieldNames,
                                    fieldSlots,
                                    0,
                                    isStoredSource,
                                    boost::none, /* sortSpecSlot */
                                    limitSlot,
                                    boost::none, /* sortKeySlot */
                                    boost::none, /* collatorSlot */
                                    expCtx->explain,
                                    nullptr /* yieldPolicy */,
                                    kEmptyPlanNodeId);
}

TEST_F(SearchCursorStageTest, SearchTestOutputs) {
    auto env = std::make_unique<RuntimeEnvironment>();

    // Register and fill input slots in the runtime environment.
    auto limitSlot = env->registerSlot("limit"_sd,
                                       sbe::value::TypeTags::NumberInt64,
                                       10 /* val */,
                                       true /* owned */,
                                       getSlotIdGenerator());

    // Generate slots for the outputs.
    auto resultSlot = generateSlotId();
    std::vector<std::string> metadataNames = {"metaA", "metaB"};
    auto metadataSlots = generateMultipleSlotIds(2);
    std::vector<std::string> fieldNames = {"fieldA", "fieldB"};
    auto fieldSlots = generateMultipleSlotIds(2);

    const boost::intrusive_ptr<ExpressionContextForTest> expCtx(new ExpressionContextForTest());
    expCtx->uuid = UUID::gen();

    // Build and prepare for execution of the search cursor stage.
    auto searchCursor = makeSearchCursorStage(
        resultSlot, metadataNames, metadataSlots, fieldNames, fieldSlots, false, limitSlot);

    auto ctx = makeCompileCtx(std::move(env));
    auto remoteCursors = std::make_unique<RemoteCursorMap>();
    remoteCursors->insert({0, mockTaskExecutorCursor(expCtx->opCtx, 0, resultArray)});
    ctx->remoteCursors = remoteCursors.get();

    prepareTree(ctx.get(), searchCursor.get());

    // Test that all of the output slots are filled correctly.
    int i = 0;
    for (auto st = searchCursor->getNext(); st == PlanState::ADVANCED;
         st = searchCursor->getNext(), i++) {
        auto curElem = resultArray[i].Obj();

        auto [actualTag, actualVal] = searchCursor->getAccessor(*ctx, resultSlot)->getViewOfValue();
        auto [expectedTag, expectedVal] = stage_builder::makeValue(curElem);
        value::ValueGuard guard(expectedTag, expectedVal);
        assertValuesEqual(actualTag, actualVal, expectedTag, expectedVal);

        for (size_t p = 0; p < metadataNames.size(); ++p) {
            ASSERT(curElem.hasField(metadataNames[p]));
            auto [tag, val] = searchCursor->getAccessor(*ctx, metadataSlots[p])->getViewOfValue();
            auto expVal = curElem[metadataNames[p]].Int();
            ASSERT_EQ(tag, sbe::value::TypeTags::NumberInt32);
            ASSERT_EQ(value::bitcastTo<int32_t>(val), expVal);
        }

        for (size_t p = 0; p < fieldNames.size(); ++p) {
            ASSERT(curElem.hasField(fieldNames[p]));
            auto [tag, val] = searchCursor->getAccessor(*ctx, fieldSlots[p])->getViewOfValue();
            auto expVal = curElem[fieldNames[p]].Int();
            ASSERT_EQ(tag, sbe::value::TypeTags::NumberInt32);
            ASSERT_EQ(value::bitcastTo<int32_t>(val), expVal);
        }
    }
}

TEST_F(SearchCursorStageTest, SearchTestLimit) {
    auto env = std::make_unique<RuntimeEnvironment>();

    // Register and fill input slots in the runtime environment.
    auto limitSlot = env->registerSlot("limit"_sd,
                                       sbe::value::TypeTags::NumberInt64,
                                       1 /* val */,
                                       true /* owned */,
                                       getSlotIdGenerator());

    // Generate slots for the outputs.
    auto resultSlot = generateSlotId();
    std::vector<std::string> metadataNames = {"metaA", "metaB"};
    auto metadataSlots = generateMultipleSlotIds(2);
    std::vector<std::string> fieldNames = {"fieldA", "fieldB"};
    auto fieldSlots = generateMultipleSlotIds(2);

    const boost::intrusive_ptr<ExpressionContextForTest> expCtx(new ExpressionContextForTest());
    expCtx->uuid = UUID::gen();


    // Build and prepare for execution of the search cursor stage.
    auto searchCursor = makeSearchCursorStage(
        resultSlot, metadataNames, metadataSlots, fieldNames, fieldSlots, true, limitSlot);

    auto ctx = makeCompileCtx(std::move(env));
    auto remoteCursors = std::make_unique<RemoteCursorMap>();
    remoteCursors->insert({0, mockTaskExecutorCursor(expCtx->opCtx, 0, resultStoredSource)});
    ctx->remoteCursors = remoteCursors.get();

    prepareTree(ctx.get(), searchCursor.get());

    // Test that the limit of the query is 1 and the second doc will not be returned.
    int i = 0;
    for (auto st = searchCursor->getNext(); st == PlanState::ADVANCED;
         st = searchCursor->getNext(), i++) {
    }
    ASSERT_EQ(i, 1);
}

TEST_F(SearchCursorStageTest, SearchTestStoredSource) {
    auto env = std::make_unique<RuntimeEnvironment>();

    // Register and fill input slots in the runtime environment.
    auto limitSlot = env->registerSlot("limit"_sd,
                                       sbe::value::TypeTags::NumberInt64,
                                       10 /* val */,
                                       true /* owned */,
                                       getSlotIdGenerator());

    // Generate slots for the outputs.
    auto resultSlot = generateSlotId();
    std::vector<std::string> metadataNames = {"metaA", "metaB"};
    auto metadataSlots = generateMultipleSlotIds(2);
    std::vector<std::string> fieldNames = {"fieldA", "fieldB"};
    auto fieldSlots = generateMultipleSlotIds(2);

    const boost::intrusive_ptr<ExpressionContextForTest> expCtx(new ExpressionContextForTest());
    expCtx->uuid = UUID::gen();

    // Build and prepare for execution of the search cursor stage.
    auto searchCursor = makeSearchCursorStage(
        resultSlot, metadataNames, metadataSlots, fieldNames, fieldSlots, true, limitSlot);

    auto ctx = makeCompileCtx(std::move(env));
    auto remoteCursors = std::make_unique<RemoteCursorMap>();
    remoteCursors->insert({0, mockTaskExecutorCursor(expCtx->opCtx, 0, resultStoredSource)});
    ctx->remoteCursors = remoteCursors.get();

    prepareTree(ctx.get(), searchCursor.get());

    // Test that all of the output slots are filled correctly.
    int i = 0;
    for (auto st = searchCursor->getNext(); st == PlanState::ADVANCED;
         st = searchCursor->getNext(), i++) {
        auto curElem = resultStoredSource[i].Obj();

        auto [actualTag, actualVal] = searchCursor->getAccessor(*ctx, resultSlot)->getViewOfValue();
        auto [expectedTag, expectedVal] = stage_builder::makeValue(curElem["storedSource"].Obj());
        value::ValueGuard guard(expectedTag, expectedVal);
        assertValuesEqual(actualTag, actualVal, expectedTag, expectedVal);

        for (size_t p = 0; p < metadataNames.size(); ++p) {
            ASSERT(curElem.hasField(metadataNames[p]));
            auto [tag, val] = searchCursor->getAccessor(*ctx, metadataSlots[p])->getViewOfValue();
            auto expVal = curElem[metadataNames[p]].Int();
            ASSERT_EQ(tag, sbe::value::TypeTags::NumberInt32);
            ASSERT_EQ(value::bitcastTo<int32_t>(val), expVal);
        }

        for (size_t p = 0; p < fieldNames.size(); ++p) {
            ASSERT(curElem["storedSource"].Obj().hasField(fieldNames[p]));
            auto [tag, val] = searchCursor->getAccessor(*ctx, fieldSlots[p])->getViewOfValue();
            auto expVal = curElem["storedSource"][fieldNames[p]].Int();
            ASSERT_EQ(tag, sbe::value::TypeTags::NumberInt32);
            ASSERT_EQ(value::bitcastTo<int32_t>(val), expVal);
        }
    }
}
}  // namespace mongo::sbe
