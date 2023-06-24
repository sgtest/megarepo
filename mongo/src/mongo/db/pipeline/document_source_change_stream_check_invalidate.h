/**
 *    Copyright (C) 2021-present MongoDB, Inc.
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

#include <set>
#include <utility>

#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>
#include <boost/optional/optional.hpp>
#include <boost/preprocessor/control/iif.hpp>
#include <boost/smart_ptr/intrusive_ptr.hpp>

#include "mongo/base/string_data.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/db/exec/document_value/document.h"
#include "mongo/db/exec/document_value/value.h"
#include "mongo/db/pipeline/change_stream_invalidation_info.h"
#include "mongo/db/pipeline/document_source.h"
#include "mongo/db/pipeline/document_source_change_stream_gen.h"
#include "mongo/db/pipeline/expression_context.h"
#include "mongo/db/pipeline/pipeline.h"
#include "mongo/db/pipeline/resume_token.h"
#include "mongo/db/pipeline/stage_constraints.h"
#include "mongo/db/pipeline/variables.h"
#include "mongo/db/query/serialization_options.h"
#include "mongo/util/assert_util_core.h"

namespace mongo {

/**
 * This stage is used internally for change stream notifications to artifically generate an
 * "invalidate" entry for commands that should invalidate the change stream (e.g. collection drop
 * for a single-collection change stream). It is not intended to be created by the user.
 */
class DocumentSourceChangeStreamCheckInvalidate final : public DocumentSource {
public:
    static constexpr StringData kStageName = "$_internalChangeStreamCheckInvalidate"_sd;

    const char* getSourceName() const final {
        // This is used in error reporting.
        return DocumentSourceChangeStreamCheckInvalidate::kStageName.rawData();
    }

    StageConstraints constraints(Pipeline::SplitState pipeState) const final {
        return {StreamType::kStreaming,
                PositionRequirement::kNone,
                HostTypeRequirement::kNone,
                DiskUseRequirement::kNoDiskUse,
                FacetRequirement::kNotAllowed,
                TransactionRequirement::kNotAllowed,
                LookupRequirement::kNotAllowed,
                UnionRequirement::kNotAllowed,
                ChangeStreamRequirement::kChangeStreamStage};
    }

    boost::optional<DistributedPlanLogic> distributedPlanLogic() final {
        return boost::none;
    }

    Value serialize(SerializationOptions opts = SerializationOptions()) const final override;

    void addVariableRefs(std::set<Variables::Id>* refs) const final {}

    static boost::intrusive_ptr<DocumentSourceChangeStreamCheckInvalidate> createFromBson(
        BSONElement spec, const boost::intrusive_ptr<ExpressionContext>& expCtx);

    static boost::intrusive_ptr<DocumentSourceChangeStreamCheckInvalidate> create(
        const boost::intrusive_ptr<ExpressionContext>& expCtx,
        const DocumentSourceChangeStreamSpec& spec);

private:
    /**
     * Use the create static method to create a DocumentSourceChangeStreamCheckInvalidate.
     */
    DocumentSourceChangeStreamCheckInvalidate(const boost::intrusive_ptr<ExpressionContext>& expCtx,
                                              boost::optional<ResumeTokenData> startAfterInvalidate)
        : DocumentSource(kStageName, expCtx),
          _startAfterInvalidate(std::move(startAfterInvalidate)) {
        invariant(!_startAfterInvalidate ||
                  _startAfterInvalidate->fromInvalidate == ResumeTokenData::kFromInvalidate);
    }

    GetNextResult doGetNext() final;

    boost::optional<ResumeTokenData> _startAfterInvalidate;
    boost::optional<Document> _queuedInvalidate;
    boost::optional<ChangeStreamInvalidationInfo> _queuedException;
};

}  // namespace mongo
