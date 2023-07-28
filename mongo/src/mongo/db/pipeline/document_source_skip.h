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

#include <set>

#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>
#include <boost/optional/optional.hpp>
#include <boost/smart_ptr/intrusive_ptr.hpp>

#include "mongo/base/string_data.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/db/exec/document_value/value.h"
#include "mongo/db/pipeline/dependencies.h"
#include "mongo/db/pipeline/document_source.h"
#include "mongo/db/pipeline/expression_context.h"
#include "mongo/db/pipeline/pipeline.h"
#include "mongo/db/pipeline/stage_constraints.h"
#include "mongo/db/pipeline/variables.h"
#include "mongo/db/query/serialization_options.h"
#include "mongo/util/intrusive_counter.h"

namespace mongo {

class DocumentSourceSkip final : public DocumentSource {
public:
    static constexpr StringData kStageName = "$skip"_sd;

    /**
     * Convenience method for creating a $skip stage.
     */
    static boost::intrusive_ptr<DocumentSourceSkip> create(
        const boost::intrusive_ptr<ExpressionContext>& pExpCtx, long long nToSkip);

    /**
     * Parses the user-supplied BSON into a $skip stage.
     *
     * Throws a AssertionException if 'elem' is an invalid $skip specification.
     */
    static boost::intrusive_ptr<DocumentSource> createFromBson(
        BSONElement elem, const boost::intrusive_ptr<ExpressionContext>& pExpCtx);

    StageConstraints constraints(Pipeline::SplitState pipeState) const final {
        return {StreamType::kStreaming,
                PositionRequirement::kNone,
                HostTypeRequirement::kNone,
                DiskUseRequirement::kNoDiskUse,
                FacetRequirement::kAllowed,
                TransactionRequirement::kAllowed,
                LookupRequirement::kAllowed,
                UnionRequirement::kAllowed};
    }

    const char* getSourceName() const final {
        return kStageName.rawData();
    }

    /**
     * Attempts to move a subsequent $limit before the skip, potentially allowing for forther
     * optimizations earlier in the pipeline.
     */
    Pipeline::SourceContainer::iterator doOptimizeAt(Pipeline::SourceContainer::iterator itr,
                                                     Pipeline::SourceContainer* container) final;

    Value serialize(const SerializationOptions& opts = SerializationOptions{}) const final override;

    boost::intrusive_ptr<DocumentSource> optimize() final;

    DepsTracker::State getDependencies(DepsTracker* deps) const final {
        return DepsTracker::State::SEE_NEXT;  // This doesn't affect needed fields
    }

    void addVariableRefs(std::set<Variables::Id>* refs) const final {}

    /**
     * The $skip stage must run on the merging half of the pipeline.
     */
    boost::optional<DistributedPlanLogic> distributedPlanLogic() final {
        // {shardsStage, mergingStage, sortPattern}
        return DistributedPlanLogic{nullptr, this, boost::none};
    }

    long long getSkip() const {
        return _nToSkip;
    }
    void setSkip(long long newSkip) {
        _nToSkip = newSkip;
    }

private:
    explicit DocumentSourceSkip(const boost::intrusive_ptr<ExpressionContext>& pExpCtx,
                                long long nToSkip);

    GetNextResult doGetNext() final;

    long long _nToSkip = 0;
    long long _nSkippedSoFar = 0;
};

}  // namespace mongo
