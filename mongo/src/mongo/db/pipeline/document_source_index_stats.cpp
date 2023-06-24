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

#include "mongo/db/pipeline/document_source_index_stats.h"

#include <boost/smart_ptr/intrusive_ptr.hpp>

#include "mongo/bson/bsonobj.h"
#include "mongo/bson/bsontypes.h"
#include "mongo/db/cluster_role.h"
#include "mongo/db/pipeline/process_interface/mongo_process_interface.h"
#include "mongo/db/query/allowed_contexts.h"
#include "mongo/db/server_options.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/intrusive_counter.h"
#include "mongo/util/net/socket_utils.h"

namespace mongo {

using boost::intrusive_ptr;

REGISTER_DOCUMENT_SOURCE(indexStats,
                         DocumentSourceIndexStats::LiteParsed::parse,
                         DocumentSourceIndexStats::createFromBson,
                         AllowedWithApiStrict::kNeverInVersion1);

const char* DocumentSourceIndexStats::getSourceName() const {
    return kStageName.rawData();
}

DocumentSource::GetNextResult DocumentSourceIndexStats::doGetNext() {
    if (_indexStats.empty()) {
        _indexStats = pExpCtx->mongoProcessInterface->getIndexStats(
            pExpCtx->opCtx,
            pExpCtx->ns,
            _processName,
            !serverGlobalParams.clusterRole.has(ClusterRole::None));
        _indexStatsIter = _indexStats.cbegin();
    }

    if (_indexStatsIter != _indexStats.cend()) {
        Document doc{std::move(*_indexStatsIter)};
        ++_indexStatsIter;
        return doc;
    }

    return GetNextResult::makeEOF();
}

DocumentSourceIndexStats::DocumentSourceIndexStats(const intrusive_ptr<ExpressionContext>& pExpCtx)
    : DocumentSource(kStageName, pExpCtx), _processName(getHostNameCachedAndPort()) {}

intrusive_ptr<DocumentSource> DocumentSourceIndexStats::createFromBson(
    BSONElement elem, const intrusive_ptr<ExpressionContext>& pExpCtx) {
    uassert(28803,
            "The $indexStats stage specification must be an empty object",
            elem.type() == Object && elem.Obj().isEmpty());
    return new DocumentSourceIndexStats(pExpCtx);
}

Value DocumentSourceIndexStats::serialize(SerializationOptions opts) const {
    return Value(DOC(getSourceName() << Document()));
}
}  // namespace mongo
