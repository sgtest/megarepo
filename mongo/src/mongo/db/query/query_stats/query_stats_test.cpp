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

#include "mongo/bson/bsonobj.h"
#include "mongo/db/collection_type.h"
#include "mongo/db/namespace_string.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/pipeline/expression_context.h"
#include "mongo/db/pipeline/expression_context_for_test.h"
#include "mongo/db/query/find_command.h"
#include "mongo/db/query/parsed_find_command.h"
#include "mongo/db/query/query_stats/find_key.h"
#include "mongo/db/query/query_stats/query_stats.h"
#include "mongo/db/service_context_test_fixture.h"
#include "mongo/idl/server_parameter_test_util.h"
#include "mongo/unittest/assert.h"
#include "mongo/unittest/framework.h"
#include "mongo/util/assert_util.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kQueryStats

namespace mongo::query_stats {
class QueryStatsTest : public ServiceContextTest {};

TEST_F(QueryStatsTest, TwoRegisterRequestsWithSameOpCtxRateLimitedFirstCall) {
    // This test simulates what happens with queries over views where two calls to registerRequest()
    // can be made with the same opCtx.

    // Make query for query stats.
    const NamespaceString nss = NamespaceString::createNamespaceString_forTest("testDB.testColl");
    FindCommandRequest fcr((NamespaceStringOrUUID(nss)));
    fcr.setFilter(BSONObj());

    auto fcrCopy = std::make_unique<FindCommandRequest>(fcr);
    auto opCtx = makeOperationContext();
    auto expCtx = makeExpressionContext(opCtx.get(), *fcrCopy);
    auto parsedFind = uassertStatusOK(parsed_find_command::parse(expCtx, {std::move(fcrCopy)}));

    RAIIServerParameterControllerForTest controller("featureFlagQueryStats", true);
    auto& opDebug = CurOp::get(*opCtx)->debug();
    ASSERT_EQ(opDebug.queryStatsInfo.wasRateLimited, false);

    // First call to registerRequest() should be rate limited.
    QueryStatsStoreManager::getRateLimiter(opCtx->getServiceContext()) =
        std::make_unique<RateLimiting>(0, Seconds{1});
    ASSERT_DOES_NOT_THROW(query_stats::registerRequest(opCtx.get(), nss, [&]() {
        return std::make_unique<query_stats::FindKey>(
            expCtx, *parsedFind, query_shape::CollectionType::kCollection);
    }));

    // Since the query was rate limited, no key should have been created.
    ASSERT(opDebug.queryStatsInfo.key == nullptr);
    ASSERT_EQ(opDebug.queryStatsInfo.wasRateLimited, true);

    // Second call should not be rate limited.
    QueryStatsStoreManager::getRateLimiter(opCtx->getServiceContext())
        .get()
        ->setSamplingRate(INT_MAX);

    ASSERT_DOES_NOT_THROW(query_stats::registerRequest(opCtx.get(), nss, [&]() {
        return std::make_unique<query_stats::FindKey>(
            expCtx, *parsedFind, query_shape::CollectionType::kCollection);
    }));

    // queryStatsKey should not be created for previously rate limited query.
    ASSERT(opDebug.queryStatsInfo.key == nullptr);
    ASSERT_EQ(opDebug.queryStatsInfo.wasRateLimited, true);
    ASSERT_FALSE(opDebug.queryStatsInfo.keyHash.has_value());
}

TEST_F(QueryStatsTest, TwoRegisterRequestsWithSameOpCtxDisabledBetween) {
    // This test simulates an observed bug where an opCtx is used for two requests, and between the
    // first and the second the query stats store is emptied/disabled.

    // Make query for query stats.
    const NamespaceString nss = NamespaceString::createNamespaceString_forTest("testDB.testColl");
    FindCommandRequest fcr((NamespaceStringOrUUID(nss)));
    fcr.setFilter(BSONObj());

    auto serviceCtx = getServiceContext();
    auto opCtx = makeOperationContext();

    auto& opDebug = CurOp::get(*opCtx)->debug();
    ASSERT(opDebug.queryStatsInfo.key == nullptr);
    ASSERT_FALSE(opDebug.queryStatsInfo.keyHash.has_value());
    QueryStatsStoreManager::get(serviceCtx) =
        std::make_unique<QueryStatsStoreManager>(16 * 1024 * 1024, 1);

    QueryStatsStoreManager::getRateLimiter(serviceCtx) =
        std::make_unique<RateLimiting>(-1, Seconds{1});

    {
        auto fcrCopy = std::make_unique<FindCommandRequest>(fcr);
        auto expCtx = makeExpressionContext(opCtx.get(), *fcrCopy);
        auto parsedFind = uassertStatusOK(parsed_find_command::parse(expCtx, {std::move(fcrCopy)}));
        ASSERT_DOES_NOT_THROW(query_stats::registerRequest(opCtx.get(), nss, [&]() {
            return std::make_unique<query_stats::FindKey>(
                expCtx, *parsedFind, query_shape::CollectionType::kCollection);
        }));

        ASSERT(opDebug.queryStatsInfo.key != nullptr);
        ASSERT(opDebug.queryStatsInfo.keyHash.has_value());

        ASSERT_DOES_NOT_THROW(query_stats::writeQueryStats(opCtx.get(),
                                                           opDebug.queryStatsInfo.keyHash,
                                                           std::move(opDebug.queryStatsInfo.key),
                                                           QueryStatsSnapshot{}));
    }

    // Second call should see that query stats are now disabled.
    {
        // To reproduce SERVER-84730 we need to clear out the query stats store so that writing the
        // stats at the end will attempt to insert a new entry.
        QueryStatsStoreManager::get(serviceCtx)->resetSize(0);

        auto fcrCopy = std::make_unique<FindCommandRequest>(fcr);
        fcrCopy->setFilter(BSON("x" << 1));
        fcrCopy->setSort(BSON("x" << 1));
        auto expCtx = makeExpressionContext(opCtx.get(), *fcrCopy);
        auto parsedFind = uassertStatusOK(parsed_find_command::parse(expCtx, {std::move(fcrCopy)}));

        ASSERT_DOES_NOT_THROW(query_stats::registerRequest(opCtx.get(), nss, [&]() {
            return std::make_unique<query_stats::FindKey>(
                expCtx, *parsedFind, query_shape::CollectionType::kCollection);
        }));

        // queryStatsKey should not be created since we have a size budget of 0.
        ASSERT(opDebug.queryStatsInfo.key == nullptr);
        // This is not a rate limit, but rather a lack of space rendering it entirely disabled.
        ASSERT_FALSE(opDebug.queryStatsInfo.wasRateLimited);

        // Interestingly, we purposefully leave the hash value around on the OperationContext after
        // the previous operation finishes. This is because we think it may have value in being
        // logged in the future, even after query stats have been written. Excepting obscure
        // internal use-cases, most OperationContexts will die shortly after the query stats are
        // written, so this isn't expected to be a large issue.
        ASSERT(opDebug.queryStatsInfo.keyHash.has_value());

        QueryStatsStoreManager::get(serviceCtx)->resetSize(16 * 1024 * 1024);
        // SERVER-84730 this assertion used to throw since there is no key, but there is a hash.
        ASSERT_DOES_NOT_THROW(query_stats::writeQueryStats(opCtx.get(),
                                                           opDebug.queryStatsInfo.keyHash,
                                                           std::move(opDebug.queryStatsInfo.key),
                                                           QueryStatsSnapshot{}));
    }
}
}  // namespace mongo::query_stats
