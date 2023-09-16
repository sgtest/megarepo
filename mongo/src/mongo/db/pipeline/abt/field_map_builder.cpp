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

#include "mongo/db/pipeline/abt/field_map_builder.h"

#include <absl/container/node_hash_map.h>
#include <absl/meta/type_traits.h>
#include <boost/move/utility_core.hpp>
#include <cstddef>
#include <map>
#include <set>

#include <boost/optional/optional.hpp>

#include "mongo/db/query/optimizer/syntax/expr.h"
#include "mongo/db/query/optimizer/syntax/path.h"
#include "mongo/db/query/optimizer/utils/path_utils.h"


namespace mongo::optimizer {

void FieldMapBuilder::integrateFieldPath(
    const FieldPath& fieldPath, const std::function<void(const bool, FieldMapEntry&)>& fn) {
    std::string path = kRootElement;
    auto it = _fieldMap.emplace(path, FieldNameType{StringData(kRootElement)});
    const size_t fieldPathLength = fieldPath.getPathLength();

    for (size_t i = 0; i < fieldPathLength; i++) {
        const std::string& fieldName = fieldPath.getFieldName(i).toString();
        path += '.' + fieldName;

        it.first->second._childPaths.insert(path);
        it = _fieldMap.emplace(path, FieldNameType{fieldName});
        fn(i == fieldPathLength - 1, it.first->second);
    }
}

boost::optional<ABT> FieldMapBuilder::generateABT() const {
    auto it = _fieldMap.find(kRootElement);
    if (it == _fieldMap.cend()) {
        return {};
    }
    return generateABTForField(it->second);
}

ABT FieldMapBuilder::generateABTForField(const FieldMapEntry& entry) const {
    const bool isRootEntry = entry._fieldName.value() == kRootElement;

    bool hasLeadingObj = false;
    bool hasTrailingDefault = false;
    FieldNameOrderedSet keepSet;
    FieldNameOrderedSet dropSet;
    std::map<FieldNameType, ProjectionName> varMap;

    for (const std::string& childField : entry._childPaths) {
        const FieldMapEntry& childEntry = _fieldMap.at(childField);
        const FieldNameType childFieldName = childEntry._fieldName;

        if (childEntry._hasKeep) {
            keepSet.insert(FieldNameType{childFieldName});
        }
        if (childEntry._hasDrop) {
            dropSet.insert(FieldNameType{childFieldName});
        }
        if (childEntry._hasLeadingObj) {
            hasLeadingObj = true;
        }
        if (childEntry._hasTrailingDefault) {
            hasTrailingDefault = true;
        }
        if (const auto& constVarName = childEntry._constVarName) {
            varMap.emplace(childFieldName, *constVarName);
        }
    }

    ABT result = make<PathIdentity>();
    if (hasLeadingObj && (!isRootEntry || !_isRootSameAsScanProj)) {
        // We do not need a leading Obj if we are using the scan projection directly (scan
        // delivers Objects).
        maybeComposePath(result, make<PathObj>());
    }
    if (!keepSet.empty()) {
        maybeComposePath(result, make<PathKeep>(std::move(keepSet)));
    }
    if (!dropSet.empty()) {
        maybeComposePath(result, make<PathDrop>(std::move(dropSet)));
    }

    for (const auto& varMapEntry : varMap) {
        maybeComposePath(result,
                         make<PathField>(varMapEntry.first,
                                         make<PathConstant>(make<Variable>(varMapEntry.second))));
    }

    // By this point we have constructed an ABT which contains the appropriate keep/drop logic up to
    // and including the child paths of 'entry'. For example, if 'entry' represents path 'a' with
    // children 'b' and 'c', paths 'a.b' and 'a.c' are appropriately kept or dropped.
    for (const std::string& childPath : entry._childPaths) {
        const FieldMapEntry& childEntry = _fieldMap.at(childPath);

        // Recursively construct ABTs for the paths below each child entry.
        ABT childResult = generateABTForField(childEntry);
        if (!childResult.is<PathIdentity>()) {
            maybeComposePath(result,
                             make<PathField>(FieldNameType{childEntry._fieldName},
                                             make<PathTraverse>(PathTraverse::kUnlimited,
                                                                std::move(childResult))));
        }
    }

    if (hasTrailingDefault) {
        maybeComposePath(result, make<PathDefault>(Constant::emptyObject()));
    }
    if (!isRootEntry) {
        return result;
    }
    return make<EvalPath>(std::move(result), make<Variable>(_rootProjName));
}

}  // namespace mongo::optimizer
