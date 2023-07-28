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

#include <memory>
#include <set>
#include <string>
#include <utility>
#include <vector>

#include <boost/none.hpp>
#include <boost/optional/optional.hpp>
#include <boost/smart_ptr/intrusive_ptr.hpp>

#include "mongo/base/string_data.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/db/auth/action_type.h"
#include "mongo/db/auth/privilege.h"
#include "mongo/db/auth/resource_pattern.h"
#include "mongo/db/exec/document_value/value.h"
#include "mongo/db/namespace_string.h"
#include "mongo/db/pipeline/document_source.h"
#include "mongo/db/pipeline/document_source_match.h"
#include "mongo/db/pipeline/expression_context.h"
#include "mongo/db/pipeline/lite_parsed_document_source.h"
#include "mongo/db/pipeline/pipeline.h"
#include "mongo/db/pipeline/stage_constraints.h"
#include "mongo/db/pipeline/variables.h"
#include "mongo/db/query/serialization_options.h"
#include "mongo/db/read_concern_support_result.h"
#include "mongo/db/repl/read_concern_level.h"
#include "mongo/stdx/unordered_set.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/intrusive_counter.h"

namespace mongo {

class DocumentSourcePlanCacheStats final : public DocumentSource {
public:
    static constexpr StringData kStageName = "$planCacheStats"_sd;

    class LiteParsed final : public LiteParsedDocumentSource {
    public:
        static std::unique_ptr<LiteParsed> parse(const NamespaceString& nss,
                                                 const BSONElement& spec) {
            return std::make_unique<LiteParsed>(spec.fieldName(), nss);
        }

        explicit LiteParsed(std::string parseTimeName, NamespaceString nss)
            : LiteParsedDocumentSource(std::move(parseTimeName)), _nss(std::move(nss)) {}

        stdx::unordered_set<NamespaceString> getInvolvedNamespaces() const override {
            // There are no foreign collections.
            return stdx::unordered_set<NamespaceString>();
        }

        PrivilegeVector requiredPrivileges(bool isMongos,
                                           bool bypassDocumentValidation) const override {
            return {Privilege(ResourcePattern::forExactNamespace(_nss), ActionType::planCacheRead)};
        }

        bool isInitialSource() const final {
            return true;
        }

        bool allowedToPassthroughFromMongos() const override {
            // $planCacheStats must be run locally on a mongod.
            return false;
        }

        ReadConcernSupportResult supportsReadConcern(repl::ReadConcernLevel level,
                                                     bool isImplicitDefault) const {
            return onlyReadConcernLocalSupported(kStageName, level, isImplicitDefault);
        }

        void assertSupportsMultiDocumentTransaction() const {
            transactionNotSupported(DocumentSourcePlanCacheStats::kStageName);
        }

    private:
        const NamespaceString _nss;
    };

    static boost::intrusive_ptr<DocumentSource> createFromBson(
        BSONElement elem, const boost::intrusive_ptr<ExpressionContext>& pExpCtx);

    virtual ~DocumentSourcePlanCacheStats() = default;

    StageConstraints constraints(
        Pipeline::SplitState = Pipeline::SplitState::kUnsplit) const override {
        StageConstraints constraints{StreamType::kStreaming,
                                     PositionRequirement::kFirst,
                                     _allHosts ? HostTypeRequirement::kAllShardServers
                                               : HostTypeRequirement::kAnyShard,
                                     DiskUseRequirement::kNoDiskUse,
                                     FacetRequirement::kNotAllowed,
                                     TransactionRequirement::kNotAllowed,
                                     LookupRequirement::kAllowed,
                                     UnionRequirement::kAllowed};

        constraints.requiresInputDocSource = false;
        return constraints;
    }

    boost::optional<DistributedPlanLogic> distributedPlanLogic() final {
        return boost::none;
    }

    const char* getSourceName() const override {
        return DocumentSourcePlanCacheStats::kStageName.rawData();
    }

    /**
     * Absorbs a subsequent $match, in order to avoid copying the entire contents of the plan cache
     * prior to filtering.
     */
    Pipeline::SourceContainer::iterator doOptimizeAt(Pipeline::SourceContainer::iterator itr,
                                                     Pipeline::SourceContainer* container) override;

    void serializeToArray(
        std::vector<Value>& array,
        const SerializationOptions& opts = SerializationOptions{}) const final override;

    void addVariableRefs(std::set<Variables::Id>* refs) const final {}

private:
    DocumentSourcePlanCacheStats(const boost::intrusive_ptr<ExpressionContext>& expCtx,
                                 bool allHosts);

    GetNextResult doGetNext() final;

    Value serialize(
        const SerializationOptions& opts = SerializationOptions{}) const final override {
        MONGO_UNREACHABLE_TASSERT(7484303);  // Should call serializeToArray instead.
    }

    // If true, requests plan cache stats from all data-bearing nodes, primary and secondary.
    // Otherwise, follows read preference.
    const bool _allHosts;

    // If running through mongos in a sharded cluster, stores the shard name so that it can be
    // appended to each plan cache entry document.
    std::string _shardName;

    // If running through mongos in a sharded cluster, stores the "host:port" string so that it can
    // be appended to each plan cache entry document.
    std::string _hostAndPort;

    // The result set for this change is produced through the mongo process interface on the first
    // call to getNext(), and then held by this data member.
    std::vector<BSONObj> _results;

    // Whether '_results' has been populated yet.
    bool _haveRetrievedStats = false;

    // Used to spool out '_results' as calls to getNext() are made.
    std::vector<BSONObj>::iterator _resultsIter;

    // $planCacheStats can push a match down into the plan cache layer, in order to avoid copying
    // the entire contents of the cache.
    boost::intrusive_ptr<DocumentSourceMatch> _absorbedMatch;
};

}  // namespace mongo
