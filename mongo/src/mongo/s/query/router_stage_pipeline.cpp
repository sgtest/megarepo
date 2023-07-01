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

#include <boost/move/utility_core.hpp>
#include <boost/optional/optional.hpp>
#include <boost/preprocessor/control/iif.hpp>
#include <boost/smart_ptr.hpp>
#include <utility>

#include <boost/smart_ptr/intrusive_ptr.hpp>

#include "mongo/base/error_codes.h"
#include "mongo/base/string_data.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/bson/bsonmisc.h"
#include "mongo/bson/bsontypes.h"
#include "mongo/db/exec/document_value/document_metadata_fields.h"
#include "mongo/db/exec/document_value/value.h"
#include "mongo/db/pipeline/document_source.h"
#include "mongo/db/pipeline/expression_context.h"
#include "mongo/s/query/document_source_merge_cursors.h"
#include "mongo/s/query/router_stage_pipeline.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/str.h"

namespace mongo {

RouterStagePipeline::RouterStagePipeline(std::unique_ptr<Pipeline, PipelineDeleter> mergePipeline)
    : RouterExecStage(mergePipeline->getContext()->opCtx),
      _mergePipeline(std::move(mergePipeline)) {
    invariant(!_mergePipeline->getSources().empty());
    _mergeCursorsStage =
        dynamic_cast<DocumentSourceMergeCursors*>(_mergePipeline->getSources().front().get());
}

StatusWith<ClusterQueryResult> RouterStagePipeline::next() {
    // Pipeline::getNext will return a boost::optional<Document> or boost::none if EOF.
    if (auto result = _mergePipeline->getNext()) {
        return _validateAndConvertToBSON(*result);
    }

    // If we reach this point, we have hit EOF.
    if (!_mergePipeline->getContext()->isTailableAwaitData()) {
        _mergePipeline.get_deleter().dismissDisposal();
        _mergePipeline->dispose(getOpCtx());
    }

    return {ClusterQueryResult()};
}

void RouterStagePipeline::doReattachToOperationContext() {
    _mergePipeline->reattachToOperationContext(getOpCtx());
}

void RouterStagePipeline::doDetachFromOperationContext() {
    _mergePipeline->detachFromOperationContext();
}

void RouterStagePipeline::kill(OperationContext* opCtx) {
    _mergePipeline.get_deleter().dismissDisposal();
    _mergePipeline->dispose(opCtx);
}

std::size_t RouterStagePipeline::getNumRemotes() const {
    if (_mergeCursorsStage) {
        return _mergeCursorsStage->getNumRemotes();
    }
    return 0;
}

BSONObj RouterStagePipeline::getPostBatchResumeToken() {
    return _mergeCursorsStage ? _mergeCursorsStage->getHighWaterMark() : BSONObj();
}

BSONObj RouterStagePipeline::_validateAndConvertToBSON(const Document& event) {
    // If this is not a change stream pipeline, we have nothing to do except return the BSONObj.
    if (!_mergePipeline->getContext()->isTailableAwaitData()) {
        return event.toBson();
    }
    // Confirm that the document _id field matches the original resume token in the sort key field.
    auto eventBSON = event.toBson();
    auto resumeToken = event.metadata().getSortKey();
    auto idField = eventBSON.getObjectField("_id");
    invariant(!resumeToken.missing());
    uassert(ErrorCodes::ChangeStreamFatalError,
            str::stream() << "Encountered an event whose _id field, which contains the resume "
                             "token, was modified by the pipeline. Modifying the _id field of an "
                             "event makes it impossible to resume the stream from that point. Only "
                             "transformations that retain the unmodified _id field are allowed. "
                             "Expected: "
                          << BSON("_id" << resumeToken) << " but found: "
                          << (eventBSON["_id"] ? BSON("_id" << eventBSON["_id"]) : BSONObj()),
            (resumeToken.getType() == BSONType::Object) &&
                idField.binaryEqual(resumeToken.getDocument().toBson()));

    // Return the event in BSONObj form, minus the $sortKey metadata.
    return eventBSON;
}

bool RouterStagePipeline::remotesExhausted() {
    return !_mergeCursorsStage || _mergeCursorsStage->remotesExhausted();
}

Status RouterStagePipeline::doSetAwaitDataTimeout(Milliseconds awaitDataTimeout) {
    invariant(_mergeCursorsStage,
              "The only cursors which should be tailable are those with remote cursors.");
    return _mergeCursorsStage->setAwaitDataTimeout(awaitDataTimeout);
}

}  // namespace mongo
