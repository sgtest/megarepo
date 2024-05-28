/**
 *    Copyright (C) 2024-present MongoDB, Inc.
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

#include "mongo/base/error_codes.h"
#include "mongo/base/string_data.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/bson/bsonmisc.h"
#include "mongo/db/query/search/mongot_cursor_getmore_strategy.h"
#include "mongo/db/service_context.h"
#include "mongo/db/session/logical_session_id.h"
#include "mongo/executor/task_executor_cursor.h"
#include "mongo/executor/task_executor_cursor_test_fixture.h"
#include "mongo/idl/server_parameter_test_util.h"
#include "mongo/unittest/assert.h"
#include "mongo/unittest/bson_test_util.h"
#include "mongo/unittest/framework.h"
#include "mongo/unittest/thread_assertion_monitor.h"
#include "mongo/util/net/hostandport.h"

namespace mongo {
namespace executor {
namespace {
/**
 * Follows the same pattern as task_executor_cursor_test.cpp, where this MongotCursorTestFixture
 * can be specialized with either pinned or unpinned cursor mechanics.
 */
template <typename Base>
class MongotCursorTestFixture : public Base {
public:
    void setUp() override {
        Base::setUp();
        client = serviceCtx->getService()->makeClient("TaskExecutorCursorTest");
        opCtx = client->makeOperationContext();
        Base::postSetUp();
    }

    void tearDown() override {
        opCtx.reset();
        client.reset();

        Base::tearDown();
    }

    BSONObj scheduleSuccessfulCursorResponse(StringData fieldName,
                                             size_t start,
                                             size_t end,
                                             size_t cursorId,
                                             bool expectedPrefetch = true) {
        return Base::scheduleSuccessfulCursorResponse(
            fieldName, start, end, cursorId, expectedPrefetch);
    }

    BSONObj scheduleSuccessfulMultiCursorResponse(StringData fieldName,
                                                  size_t start,
                                                  size_t end,
                                                  std::vector<size_t> cursorIds,
                                                  bool expectedPrefetch = true) {
        return Base::scheduleSuccessfulMultiCursorResponse(
            fieldName, start, end, cursorIds, expectedPrefetch);
    }

    void scheduleErrorResponse(Status error) {
        return Base::scheduleErrorResponse(error);
    }
    void blackHoleNextOutgoingRequest() {
        return Base::blackHoleNextOutgoingRequest();
    }

    BSONObj scheduleSuccessfulKillCursorResponse(size_t cursorId, bool expectedPrefetch = true) {
        return Base::scheduleSuccessfulKillCursorResponse(cursorId, expectedPrefetch);
    }

    std::unique_ptr<TaskExecutorCursor> makeMongotCursor(
        RemoteCommandRequest rcr,
        std::function<boost::optional<long long>()> calcDocsNeededFn = nullptr,
        boost::optional<long long> startingBatchSize = boost::none,
        DocsNeededBounds minDocsNeededBounds = docs_needed_bounds::Unknown(),
        DocsNeededBounds maxDocsNeededBounds = docs_needed_bounds::Unknown()) {
        std::unique_ptr<MongotTaskExecutorCursorGetMoreStrategy> mongotGetMoreStrategy;

        // If calcDocsNeededFn is provided, that enables use of the docsRequested option. Otherwise,
        // enable use of batchSize option.
        if (calcDocsNeededFn != nullptr) {
            mongotGetMoreStrategy = std::make_unique<MongotTaskExecutorCursorGetMoreStrategy>(
                calcDocsNeededFn,
                /*startingBatchSize*/ boost::none,
                minDocsNeededBounds,
                maxDocsNeededBounds);
        } else if (startingBatchSize.has_value()) {
            mongotGetMoreStrategy = std::make_unique<MongotTaskExecutorCursorGetMoreStrategy>(
                /*calcDocsNeededFn*/ nullptr,
                startingBatchSize,
                minDocsNeededBounds,
                maxDocsNeededBounds);
        } else {
            // Use the default startingBatchSize.
            mongotGetMoreStrategy = std::make_unique<MongotTaskExecutorCursorGetMoreStrategy>();
        }
        return Base::makeTec(rcr, {std::move(mongotGetMoreStrategy)});
    }

    bool hasReadyRequests() {
        return Base::hasReadyRequests();
    }

    bool tryWaitUntilReadyRequests() {
        return Base::tryWaitUntilReadyRequests();
    }

    /**
     * Tests that the TaskExecutorCursor with mongot options applies the calcDocsNeededFn to add
     * docsRequested option on getMore requests.
     */
    void BasicDocsRequestedTest() {
        // Asserting within a spawned thread could crash the unit test due to an uncaught exception.
        // We wrap the test with the threadAssertionMonitoredTest, which will do the work to track
        // assertions not in the main thread and propogate errors.
        unittest::threadAssertionMonitoredTest([&](auto& monitor) {
            CursorId cursorId = 1;
            RemoteCommandRequest rcr(HostAndPort("localhost"),
                                     DatabaseName::createDatabaseName_forTest(boost::none, "test"),
                                     BSON("search"
                                          << "foo"),
                                     opCtx.get());

            // Construction of the TaskExecutorCursor enqueues a request in the
            // NetworkInterfaceMock.
            auto calcDocsNeededFn = []() {
                return 10;
            };
            auto tec = makeMongotCursor(rcr,
                                        calcDocsNeededFn,
                                        /*startingBatchSize*/ boost::none,
                                        /*minDocsNeededBounds*/ 10,
                                        /*maxDocsNeededBounds*/ 10);

            // Mock the response for the first batch.
            scheduleSuccessfulCursorResponse(
                "firstBatch", 1, 2, cursorId, /*expectedPrefetch*/ false);

            // Exhaust the first batch.
            ASSERT_EQUALS(tec->getNext(opCtx.get()).value()["x"].Int(), 1);
            ASSERT_EQUALS(tec->getNext(opCtx.get()).value()["x"].Int(), 2);

            // Assert that the TaskExecutorCursor has not pre-fetched a GetMore.
            ASSERT_FALSE(hasReadyRequests());

            // As soon as 'getNext()' is invoked, the TaskExecutorCursor will try to send a GetMore
            // and that will block this thread in the NetworkInterfaceMock until there is a
            // scheduled response. However, we cannot schedule the cursor response on the main
            // thread before we call 'getNext()' as that will cause the NetworkInterfaceMock to
            // block until there is request enqueued ('getNext()' is the function which will enqueue
            // such as request). To avoid this deadlock, we start a new thread which will schedule a
            // response on the NetworkInterfaceMock.
            auto responseSchedulerThread = monitor.spawn([&] {
                auto recievedGetMoreCmd = scheduleSuccessfulCursorResponse(
                    "nextBatch", 3, 4, 0, /*expectedPrefetch*/ false);

                // Assert that the command processed for the above response matches with the
                // lambda to augment the getMore command used during construction of the TEC
                // above.
                const auto expectedGetMoreCmd =
                    BSON("getMore" << 1LL << "collection"
                                   << "test"
                                   << "cursorOptions" << BSON("docsRequested" << 10));
                ASSERT_BSONOBJ_EQ(expectedGetMoreCmd, recievedGetMoreCmd);
            });

            // Schedules the GetMore request and exhausts the cursor.
            ASSERT_EQUALS(tec->getNext(opCtx.get()).value()["x"].Int(), 3);
            ASSERT_EQUALS(tec->getNext(opCtx.get()).value()["x"].Int(), 4);
            ASSERT_FALSE(tec->getNext(opCtx.get()));

            // Joining the thread which schedules the cursor response for the GetMore here forces
            // the destructor of NetworkInterfaceMock::InNetworkGuard to run, which ensures that the
            // 'NetworkInterfaceMock' stops executing as the network thread. This is required before
            // we invoke 'hasReadyRequests()' which enters the network again.
            responseSchedulerThread.join();

            // Assert no GetMore is requested.
            ASSERT_FALSE(hasReadyRequests());
        });
    }

    /**
     * Tests that the TaskExecutorCursor applies the calcDocsNeededFn to add docsRequested option
     * on getMore requests, where the function will return different values across getMores.
     */
    void DecreasingDocsRequestedTest() {
        // See comments in "BasicDocsRequestedTest" for why this thread monitor setup is necessary
        // throughout the test.
        unittest::threadAssertionMonitoredTest([&](auto& monitor) {
            CursorId cursorId = 1;
            RemoteCommandRequest rcr(HostAndPort("localhost"),
                                     DatabaseName::createDatabaseName_forTest(boost::none, "test"),
                                     BSON("search"
                                          << "foo"),
                                     opCtx.get());

            // Construction of the TaskExecutorCursor enqueues a request in the
            // NetworkInterfaceMock.
            long long docsRequested = 50;
            auto calcDocsNeededFn = [&docsRequested]() {
                docsRequested -= 20;
                return docsRequested;
            };
            auto tec = makeMongotCursor(rcr,
                                        calcDocsNeededFn,
                                        /*startingBatchSize*/ boost::none,
                                        /*minDocsNeededBounds*/ 50,
                                        /*maxDocsNeededBounds*/ 100);
            // Mock the response for the first batch.
            scheduleSuccessfulCursorResponse(
                "firstBatch", 1, 2, cursorId, /*expectedPrefetch*/ false);

            // Exhaust the first batch.
            ASSERT_EQUALS(tec->getNext(opCtx.get()).value()["x"].Int(), 1);
            ASSERT_EQUALS(tec->getNext(opCtx.get()).value()["x"].Int(), 2);

            // Assert that the TaskExecutorCursor has not pre-fetched a GetMore.
            ASSERT_FALSE(hasReadyRequests());

            // Schedule another batch, where docsRequested should be set to 50 - 20 = 30;
            auto responseSchedulerThread = monitor.spawn([&] {
                auto recievedGetMoreCmd = scheduleSuccessfulCursorResponse(
                    "nextBatch", 3, 4, cursorId, /*expectedPrefetch*/ false);
                const auto expectedGetMoreCmd =
                    BSON("getMore" << 1LL << "collection"
                                   << "test"
                                   << "cursorOptions" << BSON("docsRequested" << 30));
                ASSERT_BSONOBJ_EQ(expectedGetMoreCmd, recievedGetMoreCmd);
            });

            // Schedules the GetMore request and exhausts the cursor.
            ASSERT_EQUALS(tec->getNext(opCtx.get()).value()["x"].Int(), 3);
            ASSERT_EQUALS(tec->getNext(opCtx.get()).value()["x"].Int(), 4);
            responseSchedulerThread.join();

            // Schedule another batch, where docsRequested should be set to 30 - 20 = 10;
            responseSchedulerThread = monitor.spawn([&] {
                auto recievedGetMoreCmd = scheduleSuccessfulCursorResponse(
                    "nextBatch", 5, 5, 0, /*expectedPrefetch*/ false);
                const auto expectedGetMoreCmd =
                    BSON("getMore" << 1LL << "collection"
                                   << "test"
                                   << "cursorOptions" << BSON("docsRequested" << 10));
                ASSERT_BSONOBJ_EQ(expectedGetMoreCmd, recievedGetMoreCmd);
            });

            // Schedules the GetMore request and exhausts the cursor.
            ASSERT_EQUALS(tec->getNext(opCtx.get()).value()["x"].Int(), 5);
            ASSERT_FALSE(tec->getNext(opCtx.get()));
            responseSchedulerThread.join();

            // Assert no GetMore is requested.
            ASSERT_FALSE(hasReadyRequests());
        });
    }

    void BatchSizeGrowsExponentiallyFromDefaultStartingSizeTest() {
        // See comments in "BasicDocsRequestedTest" for why this thread monitor setup is necessary
        // throughout the test.
        unittest::threadAssertionMonitoredTest([&](auto& monitor) {
            CursorId cursorId = 1;
            RemoteCommandRequest rcr(HostAndPort("localhost"),
                                     DatabaseName::createDatabaseName_forTest(boost::none, "test"),
                                     BSON("search"
                                          << "foo"),
                                     opCtx.get());
            // Construction of the TaskExecutorCursor enqueues a request in the
            // NetworkInterfaceMock.
            auto tec = makeMongotCursor(rcr);

            // Mock the response for the first batch.
            scheduleSuccessfulCursorResponse(
                "firstBatch", 1, 101, cursorId, /*expectedPrefetch*/ false);

            // Exhaust the first batch.
            for (int docNum = 1; docNum <= 101; docNum++) {
                ASSERT_EQUALS(tec->getNext(opCtx.get()).value()["x"].Int(), docNum);
            }

            // Assert that the TaskExecutorCursor has not pre-fetched a GetMore.
            ASSERT_FALSE(hasReadyRequests());

            // Schedule another batch, where the batchSize should have exponentially increased from
            // the first batchSize. The batchSize should be set to kDefaultMongotBatchSize *
            // kInternalSearchBatchSizeGrowthFactor = 202.
            auto responseSchedulerThread = monitor.spawn([&] {
                auto receivedGetMoreCmd = scheduleSuccessfulCursorResponse(
                    "nextBatch", 102, 303, cursorId, /*expectedPrefetch*/ false);
                const auto expectedGetMoreCmd =
                    BSON("getMore" << 1LL << "collection"
                                   << "test"
                                   << "cursorOptions" << BSON("batchSize" << 202));
                ASSERT_BSONOBJ_EQ(expectedGetMoreCmd, receivedGetMoreCmd);
            });

            // Schedules the GetMore request and exhausts the cursor.
            for (int docNum = 102; docNum <= 303; docNum++) {
                ASSERT_EQUALS(tec->getNext(opCtx.get()).value()["x"].Int(), docNum);
            }
            responseSchedulerThread.join();

            // Schedule the final batch, where batchSize should have exponentially increased from
            // the batchSize in the last GetMore request. The batchSize should be set to
            // kDefaultMongotBatchSize * (kInternalSearchBatchSizeGrowthFactor)^2 = 404.
            responseSchedulerThread = monitor.spawn([&] {
                auto receivedGetMoreCmd = scheduleSuccessfulCursorResponse(
                    "nextBatch", 303, 303, 0, /*expectedPrefetch*/ false);
                const auto expectedGetMoreCmd =
                    BSON("getMore" << 1LL << "collection"
                                   << "test"
                                   << "cursorOptions" << BSON("batchSize" << 404));
                ASSERT_BSONOBJ_EQ(expectedGetMoreCmd, receivedGetMoreCmd);
            });

            // Schedules the GetMore request and exhausts the cursor.
            ASSERT_EQUALS(tec->getNext(opCtx.get()).value()["x"].Int(), 303);
            ASSERT_FALSE(tec->getNext(opCtx.get()));
            responseSchedulerThread.join();

            // Assert no GetMore is requested.
            ASSERT_FALSE(hasReadyRequests());
        });
    }

    void BatchSizeGrowsExponentiallyFromCustomStartingSizeTest() {
        // See comments in "BasicDocsRequestedTest" for why this thread monitor setup is necessary
        // throughout the test.
        unittest::threadAssertionMonitoredTest([&](auto& monitor) {
            CursorId cursorId = 1;
            RemoteCommandRequest rcr(HostAndPort("localhost"),
                                     DatabaseName::createDatabaseName_forTest(boost::none, "test"),
                                     BSON("search"
                                          << "foo"),
                                     opCtx.get());
            // Construction of the TaskExecutorCursor enqueues a request in the
            // NetworkInterfaceMock.
            auto tec = makeMongotCursor(rcr,
                                        /*calcDocsNeededFn*/ nullptr,
                                        /*startingBatchSize*/ 3);
            // Mock the response for the first batch.
            scheduleSuccessfulCursorResponse(
                "firstBatch", 1, 3, cursorId, /*expectedPrefetch*/ false);

            // Exhaust the first batch.
            for (int docNum = 1; docNum <= 3; docNum++) {
                ASSERT_EQUALS(tec->getNext(opCtx.get()).value()["x"].Int(), docNum);
            }

            // Assert that the TaskExecutorCursor has not pre-fetched a GetMore.
            ASSERT_FALSE(hasReadyRequests());

            // Schedule another batch, where the batchSize should have exponentially increased from
            // the first batchSize. The batchSize should be set to startingBatchSize *
            // kInternalSearchBatchSizeGrowthFactor.
            auto responseSchedulerThread = monitor.spawn([&] {
                auto receivedGetMoreCmd = scheduleSuccessfulCursorResponse(
                    "nextBatch", 3, 8, cursorId, /*expectedPrefetch*/ false);
                const auto expectedGetMoreCmd =
                    BSON("getMore" << 1LL << "collection"
                                   << "test"
                                   << "cursorOptions" << BSON("batchSize" << 6));
                ASSERT_BSONOBJ_EQ(expectedGetMoreCmd, receivedGetMoreCmd);
            });

            // Schedules the GetMore request and exhausts the cursor.
            for (int docNum = 3; docNum <= 8; docNum++) {
                ASSERT_EQUALS(tec->getNext(opCtx.get()).value()["x"].Int(), docNum);
            }
            responseSchedulerThread.join();

            // Schedule another batch, where batchSize should have exponentially increased from the
            // batchSize in the last GetMore request. The batchSize should be set to
            // startingBatchSize * (kInternalSearchBatchSizeGrowthFactor)^2.
            responseSchedulerThread = monitor.spawn([&] {
                auto receivedGetMoreCmd = scheduleSuccessfulCursorResponse(
                    "nextBatch", 9, 9, 0, /*expectedPrefetch*/ false);
                const auto expectedGetMoreCmd =
                    BSON("getMore" << 1LL << "collection"
                                   << "test"
                                   << "cursorOptions" << BSON("batchSize" << 12));
                ASSERT_BSONOBJ_EQ(expectedGetMoreCmd, receivedGetMoreCmd);
            });

            // Schedules the GetMore request and exhausts the cursor.
            ASSERT_EQUALS(tec->getNext(opCtx.get()).value()["x"].Int(), 9);
            ASSERT_FALSE(tec->getNext(opCtx.get()));
            responseSchedulerThread.join();

            // Assert no GetMore is requested.
            ASSERT_FALSE(hasReadyRequests());
        });
    }

    void BatchSizePausesGrowthWhenBatchNotFilledTest() {
        // See comments in "BasicDocsRequestedTest" for why this thread monitor setup is necessary
        // throughout the test.
        unittest::threadAssertionMonitoredTest([&](auto& monitor) {
            CursorId cursorId = 1;
            RemoteCommandRequest rcr(HostAndPort("localhost"),
                                     DatabaseName::createDatabaseName_forTest(boost::none, "test"),
                                     BSON("search"
                                          << "foo"),
                                     opCtx.get());
            // Construction of the TaskExecutorCursor enqueues a request in the
            // NetworkInterfaceMock.
            auto tec = makeMongotCursor(rcr,
                                        /*calcDocsNeededFn*/ nullptr,
                                        /*startingBatchSize*/ 20);
            // Mock the response for the first batch, which only returns 15 documents, rather than
            // the requested 20.
            scheduleSuccessfulCursorResponse(
                "firstBatch", 1, 15, cursorId, /*expectedPrefetch*/ false);

            // Exhaust the first batch.
            for (int docNum = 1; docNum <= 15; docNum++) {
                ASSERT_EQUALS(tec->getNext(opCtx.get()).value()["x"].Int(), docNum);
            }

            // Assert that the TaskExecutorCursor has not pre-fetched a GetMore.
            ASSERT_FALSE(hasReadyRequests());

            // Schedule another batch, where the batchSize should remain at 20 since the previous
            // batchSize requested wasn't fulfilled. This batch will only return 10 documents.
            auto responseSchedulerThread = monitor.spawn([&] {
                auto receivedGetMoreCmd = scheduleSuccessfulCursorResponse(
                    "nextBatch", 16, 25, cursorId, /*expectedPrefetch*/ false);
                const auto expectedGetMoreCmd =
                    BSON("getMore" << 1LL << "collection"
                                   << "test"
                                   << "cursorOptions" << BSON("batchSize" << 20));
                ASSERT_BSONOBJ_EQ(expectedGetMoreCmd, receivedGetMoreCmd);
            });

            // Schedules the GetMore request and exhausts the cursor.
            for (int docNum = 16; docNum <= 25; docNum++) {
                ASSERT_EQUALS(tec->getNext(opCtx.get()).value()["x"].Int(), docNum);
            }
            responseSchedulerThread.join();

            // Schedule another batch, where the batchSize again remains at 20 since the previous
            // batchSize requested wasn't fulfilled again.
            responseSchedulerThread = monitor.spawn([&] {
                auto receivedGetMoreCmd = scheduleSuccessfulCursorResponse(
                    "nextBatch", 26, 26, 0, /*expectedPrefetch*/ false);
                const auto expectedGetMoreCmd =
                    BSON("getMore" << 1LL << "collection"
                                   << "test"
                                   << "cursorOptions" << BSON("batchSize" << 20));
                ASSERT_BSONOBJ_EQ(expectedGetMoreCmd, receivedGetMoreCmd);
            });

            // Schedules the GetMore request and exhausts the cursor.
            ASSERT_EQUALS(tec->getNext(opCtx.get()).value()["x"].Int(), 26);
            ASSERT_FALSE(tec->getNext(opCtx.get()));
            responseSchedulerThread.join();

            // Assert no GetMore is requested.
            ASSERT_FALSE(hasReadyRequests());
        });
    }

    void BatchSizeGrowthPausesThenResumesTest() {
        // See comments in "BasicDocsRequestedTest" for why this thread monitor setup is necessary
        // throughout the test.
        unittest::threadAssertionMonitoredTest([&](auto& monitor) {
            CursorId cursorId = 1;
            RemoteCommandRequest rcr(HostAndPort("localhost"),
                                     DatabaseName::createDatabaseName_forTest(boost::none, "test"),
                                     BSON("search"
                                          << "foo"),
                                     opCtx.get());
            // Construction of the TaskExecutorCursor enqueues a request in the
            // NetworkInterfaceMock.
            auto tec = makeMongotCursor(rcr,
                                        /*calcDocsNeededFn*/ nullptr,
                                        /*startingBatchSize*/ 5);
            // Mock the response for the first batch, which fulfills the requested batchSize of 5.
            scheduleSuccessfulCursorResponse(
                "firstBatch", 1, 5, cursorId, /*expectedPrefetch*/ false);

            // Exhaust the first batch.
            for (int docNum = 1; docNum <= 5; docNum++) {
                ASSERT_EQUALS(tec->getNext(opCtx.get()).value()["x"].Int(), docNum);
            }

            // Assert that the TaskExecutorCursor has not pre-fetched a GetMore.
            ASSERT_FALSE(hasReadyRequests());

            // Schedule another batch, where the batchSize requested has doubled to 10, but it will
            // only return 8.
            auto responseSchedulerThread = monitor.spawn([&] {
                auto receivedGetMoreCmd = scheduleSuccessfulCursorResponse(
                    "nextBatch", 6, 13, cursorId, /*expectedPrefetch*/ false);
                const auto expectedGetMoreCmd =
                    BSON("getMore" << 1LL << "collection"
                                   << "test"
                                   << "cursorOptions" << BSON("batchSize" << 10));
                ASSERT_BSONOBJ_EQ(expectedGetMoreCmd, receivedGetMoreCmd);
            });

            // Schedules the GetMore request and exhausts the cursor.
            for (int docNum = 6; docNum <= 13; docNum++) {
                ASSERT_EQUALS(tec->getNext(opCtx.get()).value()["x"].Int(), docNum);
            }
            responseSchedulerThread.join();

            // Schedule another batch, where the batchSize remains at 10 and returns a filled batch
            // of 10.
            responseSchedulerThread = monitor.spawn([&] {
                auto receivedGetMoreCmd = scheduleSuccessfulCursorResponse(
                    "nextBatch", 14, 23, cursorId, /*expectedPrefetch*/ false);
                const auto expectedGetMoreCmd =
                    BSON("getMore" << 1LL << "collection"
                                   << "test"
                                   << "cursorOptions" << BSON("batchSize" << 10));
                ASSERT_BSONOBJ_EQ(expectedGetMoreCmd, receivedGetMoreCmd);
            });

            // Schedules the GetMore request and exhausts the cursor.
            for (int docNum = 14; docNum <= 23; docNum++) {
                ASSERT_EQUALS(tec->getNext(opCtx.get()).value()["x"].Int(), docNum);
            }
            responseSchedulerThread.join();

            // Schedule the final batch, where the batchSize doubling should've resumed and will now
            // request 20.
            responseSchedulerThread = monitor.spawn([&] {
                auto receivedGetMoreCmd = scheduleSuccessfulCursorResponse(
                    "nextBatch", 24, 40, 0, /*expectedPrefetch*/ false);
                const auto expectedGetMoreCmd =
                    BSON("getMore" << 1LL << "collection"
                                   << "test"
                                   << "cursorOptions" << BSON("batchSize" << 20));
                ASSERT_BSONOBJ_EQ(expectedGetMoreCmd, receivedGetMoreCmd);
            });

            // Schedules the GetMore request and exhausts the cursor.
            for (int docNum = 24; docNum <= 40; docNum++) {
                ASSERT_EQUALS(tec->getNext(opCtx.get()).value()["x"].Int(), docNum);
            }
            ASSERT_FALSE(tec->getNext(opCtx.get()));
            responseSchedulerThread.join();

            // Assert no GetMore is requested.
            ASSERT_FALSE(hasReadyRequests());
        });
    }

    void PrefetchAllGetMoresTest() {
        CursorId cursorId = 1;
        RemoteCommandRequest rcr(HostAndPort("localhost"),
                                 DatabaseName::createDatabaseName_forTest(boost::none, "test"),
                                 BSON("search"
                                      << "foo"),
                                 opCtx.get());
        // Use NeedAll bounds to trigger pre-fetching for all batches.
        auto tec = makeMongotCursor(rcr,
                                    /*calcDocsNeededFn*/ nullptr,
                                    /*startingBatchSize*/ 5,
                                    /*minDocsNeededBounds*/ docs_needed_bounds::NeedAll(),
                                    /*maxDocsNeededBounds*/ docs_needed_bounds::NeedAll());
        // Assert the initial request is received.
        ASSERT_TRUE(tryWaitUntilReadyRequests());
        scheduleSuccessfulCursorResponse("firstBatch", 1, 5, cursorId, /*expectedPrefetch*/ true);

        // Populate the cursor to process the initial batch, which should dispatch the pre-fetched
        // request for the first getMore, even before any getNexts.
        tec->populateCursor(opCtx.get());
        // Assert the pre-fetched GetMore was recevied.
        ASSERT_TRUE(tryWaitUntilReadyRequests());
        // Mock the response for the first getMore.
        scheduleSuccessfulCursorResponse("nextBatch", 6, 10, cursorId, /*expectedPrefetch*/ true);

        // Exhaust the first batch, then request the first result of the first getMore, prompting
        // another pre-fetched batch.
        for (int docNum = 1; docNum <= 6; docNum++) {
            ASSERT_EQUALS(tec->getNext(opCtx.get()).value()["x"].Int(), docNum);
        }
        // Assert another pre-fetched GetMore was recevied.
        ASSERT_TRUE(tryWaitUntilReadyRequests());
        // Mock the response for the second getMore, which returns a closed cursor.
        scheduleSuccessfulCursorResponse(
            "nextBatch", 11, 15, /*cursorId*/ 0, /*expectedPrefetch*/ true);

        // Exhaust the second batch, then request the first result of the second getMore, to ensure
        // no requests are sent since the cursor was closed.
        for (int docNum = 7; docNum <= 11; docNum++) {
            ASSERT_EQUALS(tec->getNext(opCtx.get()).value()["x"].Int(), docNum);
        }

        // Assert that the TaskExecutorCursor has not pre-fetched a GetMore since the cursor was
        // exhausted.
        ASSERT_FALSE(hasReadyRequests());
    }

    void DefaultStartPrefetchAfterThreeBatchesTest() {
        // See comments in "BasicDocsRequestedTest" for why this thread monitor setup is necessary
        // throughout the test.
        unittest::threadAssertionMonitoredTest([&](auto& monitor) {
            CursorId cursorId = 1;
            RemoteCommandRequest rcr(HostAndPort("localhost"),
                                     DatabaseName::createDatabaseName_forTest(boost::none, "test"),
                                     BSON("search"
                                          << "foo"),
                                     opCtx.get());
            // Use the default mongot cursor behavior, which should only start pre-fetching after
            // the third batch is received.
            auto tec = makeMongotCursor(rcr);
            // Mock and exhaust the response for the first batch.
            scheduleSuccessfulCursorResponse(
                "firstBatch", 1, 101, cursorId, /*expectedPrefetch*/ false);
            for (int docNum = 1; docNum <= 101; docNum++) {
                ASSERT_EQUALS(tec->getNext(opCtx.get()).value()["x"].Int(), docNum);
            }
            // Assert that the TaskExecutorCursor has not pre-fetched a GetMore.
            ASSERT_FALSE(hasReadyRequests());

            // Schedule and exhaust the second batch.
            auto responseSchedulerThread = monitor.spawn([&] {
                scheduleSuccessfulCursorResponse(
                    "nextBatch", 102, 303, cursorId, /*expectedPrefetch*/ false);
            });
            for (int docNum = 102; docNum <= 303; docNum++) {
                ASSERT_EQUALS(tec->getNext(opCtx.get()).value()["x"].Int(), docNum);
            }
            responseSchedulerThread.join();

            // Assert that the TaskExecutorCursor has not pre-fetched a GetMore.
            ASSERT_FALSE(hasReadyRequests());

            // Schedule the third batch, and request just the first document from that batch. Upon
            // receipt of the third batch, a request to pre-fetch the fourth batch should be sent.
            responseSchedulerThread = monitor.spawn([&] {
                scheduleSuccessfulCursorResponse(
                    "nextBatch", 304, 707, cursorId, /*expectedPrefetch*/ false);
            });
            ASSERT_EQUALS(tec->getNext(opCtx.get()).value()["x"].Int(), 304);
            // Assert the pre-fetched GetMore was recevied.
            ASSERT_TRUE(tryWaitUntilReadyRequests());
            // Black hole the pre-fetched fourth batch since it won't be necessary.
            blackHoleNextOutgoingRequest();
            responseSchedulerThread.join();
        });
    }

    ServiceContext::UniqueServiceContext serviceCtx = ServiceContext::make();
    ServiceContext::UniqueClient client;
    ServiceContext::UniqueOperationContext opCtx;
};

using NonPinningMongotCursorTestFixture =
    MongotCursorTestFixture<NonPinningTaskExecutorCursorTestFixture>;
using PinnedConnMongotCursorTestFixture =
    MongotCursorTestFixture<PinnedConnTaskExecutorCursorTestFixture>;

TEST_F(NonPinningMongotCursorTestFixture, BasicDocsRequestedTest) {
    BasicDocsRequestedTest();
}

TEST_F(PinnedConnMongotCursorTestFixture, BasicDocsRequestedTest) {
    BasicDocsRequestedTest();
}

TEST_F(NonPinningMongotCursorTestFixture, DecreasingDocsRequestedTest) {
    DecreasingDocsRequestedTest();
}

TEST_F(PinnedConnMongotCursorTestFixture, DecreasingDocsRequestedTest) {
    DecreasingDocsRequestedTest();
}

TEST_F(PinnedConnMongotCursorTestFixture, BatchSizeGrowsExponentiallyFromDefaultStartingSizeTest) {
    BatchSizeGrowsExponentiallyFromDefaultStartingSizeTest();
}

TEST_F(NonPinningMongotCursorTestFixture, BatchSizeGrowsExponentiallyFromDefaultStartingSizeTest) {
    BatchSizeGrowsExponentiallyFromDefaultStartingSizeTest();
}

TEST_F(PinnedConnMongotCursorTestFixture, BatchSizeGrowsExponentiallyFromCustomStartingSizeTest) {
    BatchSizeGrowsExponentiallyFromCustomStartingSizeTest();
}

TEST_F(NonPinningMongotCursorTestFixture, BatchSizeGrowsExponentiallyFromCustomStartingSizeTest) {
    BatchSizeGrowsExponentiallyFromCustomStartingSizeTest();
}

TEST_F(PinnedConnMongotCursorTestFixture, BatchSizePausesGrowthWhenBatchNotFilledTest) {
    BatchSizePausesGrowthWhenBatchNotFilledTest();
}

TEST_F(NonPinningMongotCursorTestFixture, BatchSizePausesGrowthWhenBatchNotFilledTest) {
    BatchSizePausesGrowthWhenBatchNotFilledTest();
}

TEST_F(PinnedConnMongotCursorTestFixture, BatchSizeGrowthPausesThenResumesTest) {
    BatchSizeGrowthPausesThenResumesTest();
}

TEST_F(NonPinningMongotCursorTestFixture, BatchSizeGrowthPausesThenResumesTest) {
    BatchSizeGrowthPausesThenResumesTest();
}

TEST_F(PinnedConnMongotCursorTestFixture, PrefetchAllGetMoresTest) {
    PrefetchAllGetMoresTest();
}

TEST_F(NonPinningMongotCursorTestFixture, PrefetchAllGetMoresTest) {
    PrefetchAllGetMoresTest();
}

TEST_F(PinnedConnMongotCursorTestFixture, DefaultStartPrefetchAfterThreeBatchesTest) {
    DefaultStartPrefetchAfterThreeBatchesTest();
}

TEST_F(NonPinningMongotCursorTestFixture, DefaultStartPrefetchAfterThreeBatchesTest) {
    DefaultStartPrefetchAfterThreeBatchesTest();
}
}  // namespace
}  // namespace executor
}  // namespace mongo
