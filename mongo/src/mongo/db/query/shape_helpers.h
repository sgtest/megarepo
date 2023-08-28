/**
 *    Copyright (C) 2023-present MongoDB, Inc.
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

#include "mongo/db/query/query_shape.h"

namespace mongo::shape_helpers {

/**
 * Serializes the given 'hintObj' in accordance with the options. Assumes the hint is correct and
 * contains field names. It is possible that this hint doesn't actually represent an index, but we
 * can't detect that here.
 */
BSONObj extractHintShape(BSONObj hintObj, const SerializationOptions& opts);
BSONObj extractMinOrMaxShape(BSONObj obj, const SerializationOptions& opts);

NamespaceStringOrUUID parseNamespaceShape(BSONElement cmdNsElt);
void appendNamespaceShape(BSONObjBuilder& bob,
                          const NamespaceString& nss,
                          const SerializationOptions& opts);

}  // namespace mongo::shape_helpers
