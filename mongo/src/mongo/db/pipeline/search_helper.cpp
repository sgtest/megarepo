/**
 *    Copyright (C) 2022-present MongoDB, Inc.
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

#include "mongo/db/pipeline/search_helper.h"

#include <boost/preprocessor/control/iif.hpp>
#include <list>
#include <set>
#include <string>
#include <utility>

#include <boost/smart_ptr/intrusive_ptr.hpp>

#include "mongo/db/pipeline/document_source.h"
#include "mongo/db/pipeline/variables.h"
#include "mongo/util/assert_util.h"

namespace mongo {
ServiceContext::Decoration<std::unique_ptr<SearchDefaultHelperFunctions>> getSearchHelpers =
    ServiceContext::declareDecoration<std::unique_ptr<SearchDefaultHelperFunctions>>();

void SearchDefaultHelperFunctions::assertSearchMetaAccessValid(
    const Pipeline::SourceContainer& pipeline, ExpressionContext* expCtx) {
    // Any access of $$SEARCH_META is invalid.
    for (const auto& source : pipeline) {
        std::set<Variables::Id> stageRefs;
        source->addVariableRefs(&stageRefs);
        uassert(6347903,
                "Can't access $$SEARCH_META without a $search stage earlier in the pipeline",
                !Variables::hasVariableReferenceTo(stageRefs, {Variables::kSearchMetaId}));
    }
}

ServiceContext::ConstructorActionRegisterer searchQueryHelperRegisterer{
    "searchQueryHelperRegisterer", [](ServiceContext* context) {
        invariant(context);
        getSearchHelpers(context) = std::make_unique<SearchDefaultHelperFunctions>();
    }};
}  // namespace mongo
