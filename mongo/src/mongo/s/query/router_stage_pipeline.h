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

#include <boost/smart_ptr/intrusive_ptr.hpp>
#include <cstddef>
#include <memory>

#include "mongo/base/status.h"
#include "mongo/base/status_with.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/db/exec/document_value/document.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/pipeline/document_source.h"
#include "mongo/db/pipeline/pipeline.h"
#include "mongo/s/query/cluster_query_result.h"
#include "mongo/s/query/document_source_merge_cursors.h"
#include "mongo/s/query/router_exec_stage.h"
#include "mongo/util/duration.h"
#include "mongo/util/intrusive_counter.h"

namespace mongo {

/**
 * Inserts a pipeline into the router execution tree, drawing results from the input stage, feeding
 * them through the pipeline, and outputting the results of the pipeline.
 */
class RouterStagePipeline final : public RouterExecStage {
public:
    RouterStagePipeline(std::unique_ptr<Pipeline, PipelineDeleter> mergePipeline);

    StatusWith<ClusterQueryResult> next() final;

    void kill(OperationContext* opCtx) final;

    bool remotesExhausted() final;

    std::size_t getNumRemotes() const final;

    BSONObj getPostBatchResumeToken() final;

    boost::optional<query_stats::DataBearingNodeMetrics> takeRemoteMetrics() final {
        if (_mergeCursorsStage) {
            return _mergeCursorsStage->takeRemoteMetrics();
        }
        return boost::none;
    }

protected:
    Status doSetAwaitDataTimeout(Milliseconds awaitDataTimeout) final;

    void doReattachToOperationContext() final;

    void doDetachFromOperationContext() final;

private:
    BSONObj _validateAndConvertToBSON(const Document& event);

    std::unique_ptr<Pipeline, PipelineDeleter> _mergePipeline;

    // May be null if this pipeline runs exclusively on mongos without contacting the shards at all.
    boost::intrusive_ptr<DocumentSourceMergeCursors> _mergeCursorsStage;
};
}  // namespace mongo
