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

#include <limits>

#include "mongo/bson/bsonelement.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/db/exec/sbe/makeobj_spec.h"
#include "mongo/db/exec/sbe/values/util.h"
#include "mongo/db/exec/sbe/vm/makeobj_input_fields_cursors.h"
#include "mongo/db/exec/sbe/vm/vm.h"

namespace mongo::sbe::vm {

void ByteCode::traverseAndProduceBsonObj(const ProduceObjContextAndSpec& ctx,
                                         value::TypeTags tag,
                                         value::Value val,
                                         int64_t maxDepth,
                                         UniqueBSONArrayBuilder& bab) {
    constexpr int64_t maxInt64 = std::numeric_limits<int64_t>::max();

    // Loop over each element in the array.
    value::arrayForEach(tag, val, [&](value::TypeTags elemTag, value::Value elemVal) {
        if (maxDepth > 0 && value::isArray(elemTag)) {
            // If 'tag' is an array and we have not exceeded the maximum depth yet, traverse
            // the array.
            auto newMaxDepth = maxDepth == maxInt64 ? maxDepth : maxDepth - 1;

            UniqueBSONArrayBuilder nestedBab(bab.subarrayStart());
            traverseAndProduceBsonObj(ctx, elemTag, elemVal, newMaxDepth, nestedBab);
        } else if (ctx.spec->nonObjInputBehavior != MakeObjSpec::NonObjInputBehavior::kNewObj &&
                   !value::isObject(elemTag)) {
            // Otherwise, if 'tag' is not an object and 'nonObjInputBehavior' is not 'kNewObj',
            // then we either append 'tag'/'val' if ('nonObjInputBehavior' is 'kReturnInput') or we
            // skip 'tag'/'val' (if 'nonObjInputBehavior' is 'kReturnNothing').
            if (ctx.spec->nonObjInputBehavior == MakeObjSpec::NonObjInputBehavior::kReturnInput) {
                bson::appendValueToBsonArr(bab, elemTag, elemVal);
            }
        } else {
            // For all other cases, call produceBsonObject().
            UniqueBSONObjBuilder bob(bab.subobjStart());
            produceBsonObject(ctx.produceObjCtx, ctx.spec, bob, elemTag, elemVal);
        }
    });
}

void ByteCode::traverseAndProduceBsonObj(const ProduceObjContextAndSpec& ctx,
                                         value::TypeTags tag,
                                         value::Value val,
                                         StringData fieldName,
                                         UniqueBSONObjBuilder& bob) {
    constexpr int64_t maxInt64 = std::numeric_limits<int64_t>::max();

    auto maxDepth = ctx.spec->traversalDepth ? *ctx.spec->traversalDepth : maxInt64;

    if (value::isArray(tag) && maxDepth > 0) {
        // If 'tag' is an array and we have not exceeded the maximum depth yet, traverse the array.
        auto newMaxDepth = maxDepth == maxInt64 ? maxDepth : maxDepth - 1;

        UniqueBSONArrayBuilder bab(bob.subarrayStart(fieldName));
        traverseAndProduceBsonObj(ctx, tag, val, newMaxDepth, bab);
    } else if (ctx.spec->nonObjInputBehavior != MakeObjSpec::NonObjInputBehavior::kNewObj &&
               !value::isObject(tag)) {
        // Otherwise, if 'tag' is not an object and 'nonObjInputBehavior' is not 'kNewObj',
        // then we either append 'tag'/'val' if ('nonObjInputBehavior' is 'kReturnInput') or we
        // skip 'tag'/'val' (if 'nonObjInputBehavior' is 'kReturnNothing').
        if (ctx.spec->nonObjInputBehavior == MakeObjSpec::NonObjInputBehavior::kReturnInput) {
            bson::appendValueToBsonObj(bob, fieldName, tag, val);
        }
    } else {
        // For all other cases, call produceBsonObject().
        UniqueBSONObjBuilder nestedBob(bob.subobjStart(fieldName));
        produceBsonObject(ctx.produceObjCtx, ctx.spec, nestedBob, tag, val);
    }
}

template <typename CursorT>
void ByteCode::produceBsonObject(const ProduceObjContext& ctx,
                                 const MakeObjSpec* spec,
                                 UniqueBSONObjBuilder& bob,
                                 CursorT cursor) {
    using TypeTags = value::TypeTags;
    using Value = value::Value;
    using InputFields = MakeObjCursorInputFields;

    const auto& fields = spec->fields;
    const auto* actions = !spec->actions.empty() ? &spec->actions[0] : nullptr;

    const bool isClosed = spec->fieldsScopeIsClosed();
    const bool recordVisits = !spec->mandatoryFields.empty();
    const auto defActionType =
        isClosed ? MakeObjSpec::ActionType::kDrop : MakeObjSpec::ActionType::kKeep;

    // The 'visited' array keeps track of which computed fields have been visited so far so
    // that later we can append the non-visited computed fields to the end of the object.
    //
    // Note that we only use the 'visited' array when 'recordVisits' is true.
    char* visited = nullptr;
    absl::InlinedVector<char, 128> visitedVec;

    if (recordVisits) {
        size_t visitedSize = fields.size();
        visitedVec.resize(visitedSize);
        visited = visitedVec.data();
    }

    size_t fieldsLeft = spec->numFieldsToSearchFor;

    for (; !cursor.atEnd() && fieldsLeft > 0; cursor.moveNext(fields)) {
        // Get the idx of the current field and the corresponding action.
        const auto fieldIdx = cursor.fieldIdx();
        auto t = fieldIdx != StringListSet::npos ? actions[fieldIdx].type() : defActionType;

        if (t == MakeObjSpec::ActionType::kDrop) {
            fieldsLeft -= static_cast<uint8_t>(!isClosed);
            continue;
        } else if (t == MakeObjSpec::ActionType::kKeep) {
            fieldsLeft -= static_cast<uint8_t>(isClosed);
            cursor.appendTo(bob);
            continue;
        } else if (t == MakeObjSpec::ActionType::kAddArg) {
            fieldsLeft -= static_cast<uint8_t>(!isClosed);
            continue;
        }

        --fieldsLeft;
        if (recordVisits) {
            visited[fieldIdx] = 1;
        }

        auto& action = actions[fieldIdx];

        if (t == MakeObjSpec::ActionType::kSetArg) {
            auto fieldName = cursor.fieldName();

            performSetArgAction(ctx, action, fieldName, bob);
        } else if (t == MakeObjSpec::ActionType::kLambdaArg) {
            auto fieldName = cursor.fieldName();
            auto [tag, val] = cursor.value();

            performLambdaArgAction(ctx, action, tag, val, fieldName, bob);
        } else if (t == MakeObjSpec::ActionType::kMakeObj) {
            auto fieldName = cursor.fieldName();
            auto [tag, val] = cursor.value();

            performMakeObjAction(ctx, action, tag, val, fieldName, bob);
        } else {
            MONGO_UNREACHABLE_TASSERT(7103502);
        }
    }

    // If 'isClosed' is false and 'cursor' has not reached the end of the input object, copy over
    // the remaining fields from the input object to the output object.
    if (!isClosed) {
        for (; !cursor.atEnd(); cursor.moveNext(fields)) {
            cursor.appendTo(bob);
        }
    }

    // If there are remaining fields in 'fields' that need to be processed, take care of processing
    // those remaining fields here.
    if (recordVisits) {
        // Loop over 'fields'.
        const auto& mandatoryFields = spec->mandatoryFields;
        for (size_t fieldIdx : mandatoryFields) {
            // Skip fields that have already been visited.
            if (visited[fieldIdx]) {
                continue;
            }

            // Get the field name for this field, and then consult 'action' to see what
            // action should be taken.
            StringData fieldName = fields[fieldIdx];
            const auto& action = actions[fieldIdx];

            const auto tag = TypeTags::Nothing;
            const auto val = Value{0u};

            if (action.isSetArg()) {
                performSetArgAction(ctx, action, fieldName, bob);
            } else if (action.isAddArg()) {
                performAddArgAction(ctx, action, fieldName, bob);
            } else if (action.isLambdaArg()) {
                performLambdaArgAction(ctx, action, tag, val, fieldName, bob);
            } else if (action.isMakeObj()) {
                performMakeObjAction(ctx, action, tag, val, fieldName, bob);
            } else {
                MONGO_UNREACHABLE_TASSERT(7103503);
            }
        }
    }
}

template void ByteCode::produceBsonObject<BsonObjCursor>(const ProduceObjContext& ctx,
                                                         const MakeObjSpec* spec,
                                                         UniqueBSONObjBuilder& bob,
                                                         BsonObjCursor cursor);

template void ByteCode::produceBsonObject<ObjectCursor>(const ProduceObjContext& ctx,
                                                        const MakeObjSpec* spec,
                                                        UniqueBSONObjBuilder& bob,
                                                        ObjectCursor cursor);

template void ByteCode::produceBsonObject<InputFieldsOnlyCursor>(const ProduceObjContext& ctx,
                                                                 const MakeObjSpec* spec,
                                                                 UniqueBSONObjBuilder& bob,
                                                                 InputFieldsOnlyCursor cursor);

template void ByteCode::produceBsonObject<BsonObjWithInputFieldsCursor>(
    const ProduceObjContext& ctx,
    const MakeObjSpec* spec,
    UniqueBSONObjBuilder& bob,
    BsonObjWithInputFieldsCursor cursor);

template void ByteCode::produceBsonObject<ObjWithInputFieldsCursor>(
    const ProduceObjContext& ctx,
    const MakeObjSpec* spec,
    UniqueBSONObjBuilder& bob,
    ObjWithInputFieldsCursor cursor);

void ByteCode::produceBsonObjectWithInputFields(const ProduceObjContext& ctx,
                                                const MakeObjSpec* spec,
                                                UniqueBSONObjBuilder& bob,
                                                value::TypeTags objTag,
                                                value::Value objVal) {
    using TypeTags = value::TypeTags;
    using InputFields = MakeObjCursorInputFields;

    const auto& fields = spec->fields;
    const size_t numInputFields = spec->numInputFields ? *spec->numInputFields : 0;

    auto inputFields = InputFields(*this, ctx.fieldsStackOffset, numInputFields);

    auto bsonObjCursor = objTag == TypeTags::bsonObject
        ? boost::make_optional(BsonObjCursor(fields, value::bitcastTo<const char*>(objVal)))
        : boost::none;
    auto objCursor = objTag == TypeTags::Object
        ? boost::make_optional(ObjectCursor(fields, value::getObjectView(objVal)))
        : boost::none;

    // Invoke the produceBsonObject() lambda with the appropriate cursor type.
    if (objTag == TypeTags::Null) {
        produceBsonObject(ctx, spec, bob, InputFieldsOnlyCursor(fields, inputFields));
    } else if (objTag == TypeTags::bsonObject) {
        produceBsonObject(
            ctx, spec, bob, BsonObjWithInputFieldsCursor(fields, inputFields, *bsonObjCursor));
    } else if (objTag == TypeTags::Object) {
        produceBsonObject(
            ctx, spec, bob, ObjWithInputFieldsCursor(fields, inputFields, *objCursor));
    } else {
        MONGO_UNREACHABLE_TASSERT(8146602);
    }
}

}  // namespace mongo::sbe::vm
