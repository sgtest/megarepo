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

#include <string>

#include "mongo/base/error_codes.h"
#include "mongo/db/pipeline/abt/document_source_visitor.h"
#include "mongo/db/pipeline/visitors/document_source_visitor_registry_mongos.h"
#include "mongo/db/query/cqf_command_utils.h"
#include "mongo/db/service_context.h"
#include "mongo/s/query/document_source_merge_cursors.h"
#include "mongo/util/assert_util.h"

namespace mongo::optimizer {

void visit(ABTUnsupportedDocumentSourceVisitorContext* ctx, const DocumentSourceMergeCursors&) {
    ctx->eligible = false;
}

const ServiceContext::ConstructorActionRegisterer abtUnsupportedRegisterer{
    "ABTUnsupportedRegistererMongoS", [](ServiceContext* service) {
        registerMongosVisitor<ABTUnsupportedDocumentSourceVisitorContext>(service);
    }};

void visit(ABTDocumentSourceTranslationVisitorContext* ctx, const DocumentSourceMergeCursors&) {
    uasserted(ErrorCodes::InternalErrorNotSupported,
              "DocumentSourceMergeCursors is not supported in CQF");
}

const ServiceContext::ConstructorActionRegisterer abtTranslationRegisterer{
    "ABTTranslationRegistererMongoS", [](ServiceContext* service) {
        registerMongosVisitor<ABTDocumentSourceTranslationVisitorContext>(service);
    }};

}  // namespace mongo::optimizer
