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

#include <cstddef>
#include <memory>
#include <set>
#include <string>
#include <vector>

#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>
#include <boost/optional/optional.hpp>
#include <boost/smart_ptr/intrusive_ptr.hpp>

#include "mongo/base/string_data.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/db/exec/document_value/document.h"
#include "mongo/db/exec/document_value/document_internal.h"
#include "mongo/db/exec/document_value/value.h"
#include "mongo/db/pipeline/dependencies.h"
#include "mongo/db/pipeline/document_source.h"
#include "mongo/db/pipeline/document_source_limit.h"
#include "mongo/db/pipeline/document_source_sort.h"
#include "mongo/db/pipeline/expression_context.h"
#include "mongo/db/pipeline/field_path.h"
#include "mongo/db/pipeline/pipeline.h"
#include "mongo/db/pipeline/stage_constraints.h"
#include "mongo/db/pipeline/variables.h"
#include "mongo/db/query/serialization_options.h"

namespace mongo {

class DocumentSourceUnwind final : public DocumentSource {
public:
    static constexpr StringData kStageName = "$unwind"_sd;

    // virtuals from DocumentSource
    const char* getSourceName() const final;

    Value serialize(SerializationOptions opts = SerializationOptions()) const final override;

    /**
     * Returns the unwound path, and the 'includeArrayIndex' path, if specified.
     */
    GetModPathsReturn getModifiedPaths() const final;

    StageConstraints constraints(Pipeline::SplitState pipeState) const final {
        StageConstraints constraints(StreamType::kStreaming,
                                     PositionRequirement::kNone,
                                     HostTypeRequirement::kNone,
                                     DiskUseRequirement::kNoDiskUse,
                                     FacetRequirement::kAllowed,
                                     TransactionRequirement::kAllowed,
                                     LookupRequirement::kAllowed,
                                     UnionRequirement::kAllowed);

        constraints.canSwapWithMatch = true;
        return constraints;
    }

    boost::optional<DistributedPlanLogic> distributedPlanLogic() final {
        return boost::none;
    }

    DepsTracker::State getDependencies(DepsTracker* deps) const final;

    void addVariableRefs(std::set<Variables::Id>* refs) const final {}

    /**
     * Creates a new $unwind DocumentSource from a BSON specification.
     */
    static boost::intrusive_ptr<DocumentSource> createFromBson(
        BSONElement elem, const boost::intrusive_ptr<ExpressionContext>& pExpCtx);

    static boost::intrusive_ptr<DocumentSourceUnwind> create(
        const boost::intrusive_ptr<ExpressionContext>& expCtx,
        const std::string& path,
        bool includeNullIfEmptyOrMissing,
        const boost::optional<std::string>& includeArrayIndex,
        bool strict = false);

    std::string getUnwindPath() const {
        return _unwindPath.fullPath();
    }

    bool preserveNullAndEmptyArrays() const {
        return _preserveNullAndEmptyArrays;
    }

    const boost::optional<FieldPath>& indexPath() const {
        return _indexPath;
    }

protected:
    /**
     * Attempts to swap with a subsequent $sort stage if the $sort is on a different field.
     */
    Pipeline::SourceContainer::iterator doOptimizeAt(Pipeline::SourceContainer::iterator itr,
                                                     Pipeline::SourceContainer* container) final;

private:
    DocumentSourceUnwind(const boost::intrusive_ptr<ExpressionContext>& pExpCtx,
                         const FieldPath& fieldPath,
                         bool includeNullIfEmptyOrMissing,
                         const boost::optional<FieldPath>& includeArrayIndex,
                         bool strict);

    GetNextResult doGetNext() final;

    // Checks if a sort is eligible to be moved before the unwind.
    bool canPushSortBack(const DocumentSourceSort* sort) const;

    // Checks if a limit is eligible to be moved before the unwind.
    bool canPushLimitBack(const DocumentSourceLimit* limit) const;

    // Configuration state.
    const FieldPath _unwindPath;
    // Documents that have a nullish value, or an empty array for the field '_unwindPath', will pass
    // through the $unwind stage unmodified if '_preserveNullAndEmptyArrays' is true.
    const bool _preserveNullAndEmptyArrays;
    // If set, the $unwind stage will include the array index in the specified path, overwriting any
    // existing value, setting to null when the value was a non-array or empty array.
    const boost::optional<FieldPath> _indexPath;

    // Iteration state.
    class Unwinder;
    std::unique_ptr<Unwinder> _unwinder;

    // If preserveNullAndEmptyArrays is true and unwind is followed by a limit, we can duplicate
    // the limit before the unwind. We only want to do this if we've found a limit smaller than the
    // one we already pushed down. boost::none means no push down has occurred yet.
    boost::optional<long long> _smallestLimitPushedDown;
};

/** Helper class to unwind array from a single document. */
class DocumentSourceUnwind::Unwinder {
public:
    Unwinder(const FieldPath& unwindPath,
             bool preserveNullAndEmptyArrays,
             const boost::optional<FieldPath>& indexPath,
             bool strict);
    /** Reset the unwinder to unwind a new document. */
    void resetDocument(const Document& document);

    /**
     * @return the next document unwound from the document provided to resetDocument(), using
     * the current value in the array located at the provided unwindPath.
     *
     * Returns boost::none if the array is exhausted.
     */
    DocumentSource::GetNextResult getNext();

private:
    // Tracks whether or not we can possibly return any more documents. Note we may return
    // boost::none even if this is true.
    bool _haveNext = false;

    // Path to the array to unwind.
    const FieldPath _unwindPath;

    // Documents that have a nullish value, or an empty array for the field '_unwindPath', will pass
    // through the $unwind stage unmodified if '_preserveNullAndEmptyArrays' is true.
    const bool _preserveNullAndEmptyArrays;

    // If set, the $unwind stage will include the array index in the specified path, overwriting any
    // existing value, setting to null when the value was a non-array or empty array.
    const boost::optional<FieldPath> _indexPath;
    // Specifies if input to $unwind is required to be an array.
    const bool _strict;

    Value _inputArray;

    MutableDocument _output;

    // Document indexes of the field path components.
    std::vector<Position> _unwindPathFieldIndexes;

    // Index into the _inputArray to return next.
    size_t _index = 0;
};

}  // namespace mongo
