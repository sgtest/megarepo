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

#include <benchmark/benchmark.h>
#include <memory>

#include "mongo/bson/bsonobj.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/db/bonsai_query_bm_fixture.h"
#include "mongo/db/namespace_string.h"
#include "mongo/db/pipeline/expression_context.h"
#include "mongo/db/pipeline/expression_context_for_test.h"
#include "mongo/db/pipeline/pipeline.h"
#include "mongo/db/query/cqf_command_utils.h"
#include "mongo/db/query/query_test_service_context.h"
#include "mongo/util/intrusive_counter.h"

namespace mongo::optimizer {
namespace {
/**
 * Benchmarks encoding of CanonicalQuery to SBE PlanCacheKey.
 */
class PipelineEncodeSBE : public BonsaiQueryBenchmarkFixture {
public:
    PipelineEncodeSBE() {}

    void benchmarkQueryMatchProject(benchmark::State& state,
                                    BSONObj matchSpec,
                                    BSONObj projectSpec) override final {
        std::vector<BSONObj> pipeline;
        if (!matchSpec.isEmpty()) {
            pipeline.push_back(BSON("$match" << matchSpec));
        }
        if (!projectSpec.isEmpty()) {
            pipeline.push_back(BSON("$project" << projectSpec));
        }
        benchmarkPipeline(state, pipeline);
    }

    void benchmarkPipeline(benchmark::State& state,
                           const std::vector<BSONObj>& pipeline) override final {
        QueryTestServiceContext testServiceContext;
        auto opCtx = testServiceContext.makeOperationContext();
        auto expCtx = make_intrusive<ExpressionContextForTest>(
            opCtx.get(), NamespaceString::createNamespaceString_forTest("test.bm"));

        std::unique_ptr<Pipeline, PipelineDeleter> parsedPipeline =
            Pipeline::parse(pipeline, expCtx);
        parsedPipeline->optimizePipeline();
        parsedPipeline->parameterize();

        std::vector<boost::intrusive_ptr<DocumentSource>> pipelineStages;
        for (auto&& source : parsedPipeline->getSources()) {
            pipelineStages.emplace_back(source);
        }

        // This is where recording starts.
        for (auto keepRunning : state) {
            benchmark::DoNotOptimize(canonical_query_encoder::encodePipeline(
                expCtx.get(), pipelineStages, canonical_query_encoder::Optimizer::kBonsai));
            benchmark::ClobberMemory();
        }
    }
};

BENCHMARK_QUERY_ENCODING(PipelineEncodeSBE);
BENCHMARK_PIPELINE_QUERY_ENCODING(PipelineEncodeSBE);
}  // namespace
}  // namespace mongo::optimizer
