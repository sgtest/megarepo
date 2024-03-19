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
#include <string>

#include <boost/none.hpp>
#include <boost/optional/optional.hpp>
#include <boost/smart_ptr/intrusive_ptr.hpp>

#include "mongo/base/string_data.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/db/exec/document_value/document.h"
#include "mongo/db/exec/document_value/value.h"
#include "mongo/db/pipeline/dependencies.h"
#include "mongo/db/pipeline/document_source.h"
#include "mongo/db/pipeline/document_source_change_stream.h"
#include "mongo/db/pipeline/document_source_change_stream_gen.h"
#include "mongo/db/pipeline/expression_context.h"
#include "mongo/db/pipeline/pipeline.h"
#include "mongo/db/pipeline/stage_constraints.h"
#include "mongo/db/pipeline/variables.h"
#include "mongo/db/query/query_shape/serialization_options.h"
#include "mongo/util/assert_util_core.h"

namespace mongo {

/**
 * Part of the change stream API machinery used to look up the pre-image of a document.
 *
 * The identifier of pre-image is in "preImageId" field of the incoming document. The pre-image is
 * set to "fullDocumentBeforeChange" field of the returned document.
 */
class DocumentSourceChangeStreamAddPreImage final : public DocumentSource {
public:
    static constexpr StringData kStageName = "$_internalChangeStreamAddPreImage"_sd;
    static constexpr StringData kFullDocumentBeforeChangeFieldName =
        DocumentSourceChangeStream::kFullDocumentBeforeChangeField;
    static constexpr StringData kPreImageIdFieldName = DocumentSourceChangeStream::kPreImageIdField;

    /**
     * Creates a DocumentSourceChangeStreamAddPreImage stage.
     */
    static boost::intrusive_ptr<DocumentSourceChangeStreamAddPreImage> create(
        const boost::intrusive_ptr<ExpressionContext>& expCtx,
        const DocumentSourceChangeStreamSpec& spec);

    static boost::intrusive_ptr<DocumentSourceChangeStreamAddPreImage> createFromBson(
        BSONElement elem, const boost::intrusive_ptr<ExpressionContext>& expCtx);

    // Retrieves the pre-image document given the specified 'preImageId'. Returns boost::none if no
    // such pre-image is available.
    static boost::optional<Document> lookupPreImage(boost::intrusive_ptr<ExpressionContext> pExpCtx,
                                                    const Document& preImageId);

    // Removes the internal fields from the event and returns the string representation of it.
    static std::string makePreImageNotFoundErrorMsg(const Document& event);

    DocumentSourceChangeStreamAddPreImage(const boost::intrusive_ptr<ExpressionContext>& expCtx,
                                          FullDocumentBeforeChangeModeEnum mode)
        : DocumentSource(kStageName, expCtx), _fullDocumentBeforeChangeMode(mode) {
        // This stage should never be created with FullDocumentBeforeChangeMode::kOff.
        invariant(_fullDocumentBeforeChangeMode != FullDocumentBeforeChangeModeEnum::kOff);
    }

    /**
     * Only modifies: "fullDocumentBeforeChange" and "preImageId".
     */
    GetModPathsReturn getModifiedPaths() const final {
        return {GetModPathsReturn::Type::kFiniteSet,
                {kFullDocumentBeforeChangeFieldName.toString(), kPreImageIdFieldName.toString()},
                {}};
    }

    StageConstraints constraints(Pipeline::SplitState pipeState) const final {
        invariant(pipeState != Pipeline::SplitState::kSplitForShards);
        StageConstraints constraints(StreamType::kStreaming,
                                     PositionRequirement::kNone,
                                     HostTypeRequirement::kAnyShard,
                                     DiskUseRequirement::kNoDiskUse,
                                     FacetRequirement::kNotAllowed,
                                     TransactionRequirement::kNotAllowed,
                                     LookupRequirement::kNotAllowed,
                                     UnionRequirement::kNotAllowed,
                                     ChangeStreamRequirement::kChangeStreamStage);
        constraints.canSwapWithMatch = true;
        return constraints;
    }

    boost::optional<DistributedPlanLogic> distributedPlanLogic() final {
        return boost::none;
    }

    DepsTracker::State getDependencies(DepsTracker* deps) const override {
        deps->fields.insert(DocumentSourceChangeStream::kPreImageIdField.toString());
        // This stage does not restrict the output fields to a finite set, and has no impact on
        // whether metadata is available or needed.
        return DepsTracker::State::SEE_NEXT;
    }

    void addVariableRefs(std::set<Variables::Id>* refs) const final {}

    Value serialize(const SerializationOptions& opts = SerializationOptions{}) const final;

    const char* getSourceName() const final {
        return kStageName.rawData();
    }

private:
    /**
     * Performs the lookup to retrieve the full pre-image document for applicable operations.
     */
    GetNextResult doGetNext() final;

    // Determines whether pre-images are strictly required or may be included only when available.
    FullDocumentBeforeChangeModeEnum _fullDocumentBeforeChangeMode =
        FullDocumentBeforeChangeModeEnum::kOff;
};

}  // namespace mongo
