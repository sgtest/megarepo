/**
 *    Copyright (C) 2019-present MongoDB, Inc.
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

#include <absl/container/inlined_vector.h>
#include <absl/meta/type_traits.h>
#include <algorithm>
#include <boost/algorithm/string/case_conv.hpp>
#include <boost/move/utility_core.hpp>
#include <chrono>
#include <functional>
#include <initializer_list>
#include <iosfwd>
#include <memory>
#include <queue>
#include <ratio>
#include <string_view>
#include <system_error>
#include <vector>

#include <absl/container/flat_hash_map.h>
#include <boost/optional/optional.hpp>

#include "mongo/base/data_view.h"
#include "mongo/base/error_codes.h"
#include "mongo/base/parse_number.h"
#include "mongo/base/status.h"
#include "mongo/base/status_with.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/bson/bsontypes.h"
#include "mongo/bson/bsontypes_util.h"
#include "mongo/bson/oid.h"
#include "mongo/bson/ordering.h"
#include "mongo/bson/timestamp.h"
#include "mongo/bson/util/builder.h"
#include "mongo/bson/util/builder_fwd.h"
#include "mongo/config.h"  // IWYU pragma: keep
#include "mongo/db/exec/js_function.h"
#include "mongo/db/exec/sbe/accumulator_sum_value_enum.h"
#include "mongo/db/exec/sbe/column_store_encoder.h"
#include "mongo/db/exec/sbe/columnar.h"
#include "mongo/db/exec/sbe/expressions/expression.h"
#include "mongo/db/exec/sbe/expressions/runtime_environment.h"
#include "mongo/db/exec/sbe/makeobj_spec.h"
#include "mongo/db/exec/sbe/sbe_pattern_value_cmp.h"
#include "mongo/db/exec/sbe/sort_spec.h"
#include "mongo/db/exec/sbe/util/pcre.h"
#include "mongo/db/exec/sbe/values/arith_common.h"
#include "mongo/db/exec/sbe/values/bson.h"
#include "mongo/db/exec/sbe/values/row.h"
#include "mongo/db/exec/sbe/values/util.h"
#include "mongo/db/exec/sbe/values/value.h"
#include "mongo/db/exec/sbe/vm/datetime.h"
#include "mongo/db/exec/sbe/vm/vm.h"
#include "mongo/db/exec/sbe/vm/vm_printer.h"
#include "mongo/db/exec/shard_filterer.h"
#include "mongo/db/fts/fts_matcher.h"
#include "mongo/db/hasher.h"
#include "mongo/db/index/btree_key_generator.h"
#include "mongo/db/matcher/in_list_data.h"
#include "mongo/db/query/collation/collation_index_key.h"
#include "mongo/db/query/datetime/date_time_support.h"
#include "mongo/db/query/query_knobs_gen.h"
#include "mongo/db/query/str_trim_utils.h"
#include "mongo/db/storage/column_store.h"
#include "mongo/db/storage/key_string.h"
#include "mongo/logv2/log.h"
#include "mongo/logv2/log_attr.h"
#include "mongo/logv2/log_component.h"
#include "mongo/platform/atomic_word.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/errno_util.h"
#include "mongo/util/pcre.h"
#include "mongo/util/shared_buffer.h"
#include "mongo/util/str.h"
#include "mongo/util/string_listset.h"
#include "mongo/util/string_map.h"
#include "mongo/util/summation.h"
#include "mongo/util/time_support.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kQuery

namespace mongo {
namespace sbe {
namespace vm {

/*
 * This table must be kept in sync with Instruction::Tags. It encodes how the instruction affects
 * the stack; i.e. push(+1), pop(-1), or no effect.
 */
int Instruction::stackOffset[Instruction::Tags::lastInstruction] = {
    1,   // pushConstVal
    1,   // pushAccessVal
    1,   // pushOwnedAccessorVal
    1,   // pushEnvAccessorVal
    1,   // pushMoveVal
    1,   // pushLocalVal
    1,   // pushMoveLocalVal
    1,   // pushLocalLambda
    -1,  // pop
    0,   // swap

    -1,  // add
    -1,  // sub
    -1,  // mul
    -1,  // div
    -1,  // idiv
    -1,  // mod
    0,   // negate
    0,   // numConvert

    0,  // logicNot

    -1,  // less
    -1,  // lessEq
    -1,  // greater
    -1,  // greaterEq
    -1,  // eq
    -1,  // neq
    -1,  // cmp3w

    -2,  // collLess
    -2,  // collLessEq
    -2,  // collGreater
    -2,  // collGreaterEq
    -2,  // collEq
    -2,  // collNeq
    -2,  // collCmp3w

    -1,  // fillEmpty
    0,   // fillEmptyImm
    -1,  // getField
    0,   // getFieldImm
    -1,  // getElement
    -1,  // collComparisonKey
    -1,  // getFieldOrElement
    -2,  // traverseP
    0,   // traversePImm
    -2,  // traverseF
    0,   // traverseFImm
    -4,  // magicTraverseF
    0,   // traverseCsiCellValues
    0,   // traverseCsiCellTypes
    -2,  // setField
    0,   // getArraySize

    -1,  // aggSum
    -1,  // aggMin
    -1,  // aggMax
    -1,  // aggFirst
    -1,  // aggLast

    -1,  // aggCollMin
    -1,  // aggCollMax

    0,  // exists
    0,  // isNull
    0,  // isObject
    0,  // isArray
    0,  // isInListData
    0,  // isString
    0,  // isNumber
    0,  // isBinData
    0,  // isDate
    0,  // isNaN
    0,  // isInfinity
    0,  // isRecordId
    0,  // isMinKey
    0,  // isMaxKey
    0,  // isTimestamp
    0,  // typeMatchImm

    0,  // function is special, the stack offset is encoded in the instruction itself
    0,  // functionSmall is special, the stack offset is encoded in the instruction itself

    0,   // jmp
    -1,  // jmpTrue
    -1,  // jmpFalse
    0,   // jmpNothing
    0,   // jmpNotNothing
    0,   // ret
    0,   // allocStack does not affect the top of stack

    -1,  // fail

    0,  // dateTruncImm

    -1,  // valueBlockApplyLambda
};

void ByteCode::allocStackImpl(size_t newSizeDelta) noexcept {
    invariant(newSizeDelta > 0);

    auto oldSize = _argStackEnd - _argStack;
    auto oldTop = _argStackTop - _argStack;

    _argStack = reinterpret_cast<uint8_t*>(mongoRealloc(_argStack, oldSize + newSizeDelta));
    _argStackEnd = _argStack + oldSize + newSizeDelta;
    _argStackTop = _argStack + oldTop;
}

std::string CodeFragment::toString() const {
    std::ostringstream ss;
    vm::CodeFragmentPrinter printer(vm::CodeFragmentPrinter::PrintFormat::Debug);
    printer.print(ss, *this);
    return ss.str();
}
template <typename... Ts>
void CodeFragment::adjustStackSimple(const Instruction& i, Ts&&... params) {
    // Get the stack delta from a table.
    auto delta = Instruction::stackOffset[i.tag];
    // And adjust it by parameters coming from frames.
    ((delta += params.frameId ? 1 : 0), ...);

    _stackSize += delta;

    // Only if we grow the stack can we affect the maximum size.
    if (delta > 0) {
        _maxStackSize = std::max(_maxStackSize, _stackSize);
    }
}

void CodeFragment::declareFrame(FrameId frameId) {
    declareFrame(frameId, 0);
}

void CodeFragment::declareFrame(FrameId frameId, int stackOffset) {
    FrameInfo& frame = getOrDeclareFrame(frameId);
    tassert(7239101,
            str::stream() << "Frame stackPosition is already defined. frameId: " << frameId,
            frame.stackPosition == FrameInfo::kPositionNotSet);
    frame.stackPosition = _stackSize + stackOffset;
    if (!frame.fixupOffsets.empty()) {
        fixupFrame(frame);
    }
}

void CodeFragment::removeFrame(FrameId frameId) {
    auto p = _frames.find(frameId);
    if (p == _frames.end()) {
        return;
    }

    tassert(7239103,
            str::stream() << "Can't remove frame that has outstanding fixups. frameId:" << frameId,
            p->second.fixupOffsets.empty());

    _frames.erase(frameId);
}

bool CodeFragment::hasFrames() const {
    return !_frames.empty();
}

CodeFragment::FrameInfo& CodeFragment::getOrDeclareFrame(FrameId frameId) {
    auto [it, r] = _frames.try_emplace(frameId);
    return it->second;
}

void CodeFragment::fixupFrame(FrameInfo& frame) {
    tassert(7239105,
            "Frame must have defined stackPosition",
            frame.stackPosition != FrameInfo::kPositionNotSet);

    for (auto fixupOffset : frame.fixupOffsets) {
        int stackOffset = readFromMemory<int>(_instrs.data() + fixupOffset);
        writeToMemory(_instrs.data() + fixupOffset,
                      stackOffset - static_cast<int>(frame.stackPosition));
    }

    frame.fixupOffsets.clear();
}

void CodeFragment::fixupStackOffsets(int stackOffsetDelta) {
    if (stackOffsetDelta == 0) {
        return;
    }

    for (auto& p : _frames) {
        auto& frame = p.second;
        if (frame.stackPosition != FrameInfo::kPositionNotSet) {
            frame.stackPosition = frame.stackPosition + stackOffsetDelta;
        }

        for (auto& fixupOffset : frame.fixupOffsets) {
            int stackOffset = readFromMemory<int>(_instrs.data() + fixupOffset);
            writeToMemory<int>(_instrs.data() + fixupOffset, stackOffset + stackOffsetDelta);
        }
    }
}

void CodeFragment::removeLabel(LabelId labelId) {
    auto p = _labels.find(labelId);
    if (p == _labels.end()) {
        return;
    }

    tassert(7134601,
            str::stream() << "Can't remove label that has outstanding fixups. labelId:" << labelId,
            p->second.fixupOffsets.empty());

    _labels.erase(labelId);
}

void CodeFragment::appendLabel(LabelId labelId) {
    auto& label = getOrDeclareLabel(labelId);
    tassert(7134602,
            str::stream() << "Label definitionOffset is already defined. labelId: " << labelId,
            label.definitionOffset == LabelInfo::kOffsetNotSet);
    label.definitionOffset = _instrs.size();
    if (!label.fixupOffsets.empty()) {
        fixupLabel(label);
    }
}

void CodeFragment::fixupLabel(LabelInfo& label) {
    tassert(7134603,
            "Label must have defined definitionOffset",
            label.definitionOffset != LabelInfo::kOffsetNotSet);

    for (auto fixupOffset : label.fixupOffsets) {
        int jumpOffset = readFromMemory<int>(_instrs.data() + fixupOffset);
        writeToMemory(_instrs.data() + fixupOffset,
                      jumpOffset + static_cast<int>(label.definitionOffset - fixupOffset));
    }

    label.fixupOffsets.clear();
}

CodeFragment::LabelInfo& CodeFragment::getOrDeclareLabel(LabelId labelId) {
    auto [it, r] = _labels.try_emplace(labelId);
    return it->second;
}

void CodeFragment::validate() {
    if constexpr (kDebugBuild) {
        for (auto& p : _frames) {
            auto& frame = p.second;
            tassert(7134606,
                    str::stream() << "Unresolved frame fixup offsets. frameId: " << p.first,
                    frame.fixupOffsets.empty());
        }

        for (auto& p : _labels) {
            auto& label = p.second;
            tassert(7134607,
                    str::stream() << "Unresolved label fixup offsets. labelId: " << p.first,
                    label.fixupOffsets.empty());
        }
    }
}

void CodeFragment::copyCodeAndFixup(CodeFragment&& from) {
    auto instrsSize = _instrs.size();

    if (_instrs.empty()) {
        _instrs = std::move(from._instrs);
    } else {
        _instrs.insert(_instrs.end(), from._instrs.begin(), from._instrs.end());
    }

    for (auto& p : from._frames) {
        auto& fromFrame = p.second;
        for (auto& fixupOffset : fromFrame.fixupOffsets) {
            fixupOffset += instrsSize;
        }
        auto it = _frames.find(p.first);
        if (it != _frames.end()) {
            auto& frame = it->second;
            if (fromFrame.stackPosition != FrameInfo::kPositionNotSet) {
                tassert(7239104,
                        "Duplicate frame stackPosition",
                        frame.stackPosition == FrameInfo::kPositionNotSet);
                frame.stackPosition = fromFrame.stackPosition;
            }
            frame.fixupOffsets.insert(frame.fixupOffsets.end(),
                                      fromFrame.fixupOffsets.begin(),
                                      fromFrame.fixupOffsets.end());
            if (frame.stackPosition != FrameInfo::kPositionNotSet) {
                fixupFrame(frame);
            }
        } else {
            _frames.emplace(p.first, std::move(fromFrame));
        }
    }

    for (auto& p : from._labels) {
        auto& fromLabel = p.second;
        if (fromLabel.definitionOffset != LabelInfo::kOffsetNotSet) {
            fromLabel.definitionOffset += instrsSize;
        }
        for (auto& fixupOffset : fromLabel.fixupOffsets) {
            fixupOffset += instrsSize;
        }
        auto it = _labels.find(p.first);
        if (it != _labels.end()) {
            auto& label = it->second;
            if (fromLabel.definitionOffset != LabelInfo::kOffsetNotSet) {
                tassert(7134605,
                        "Duplicate label definitionOffset",
                        label.definitionOffset == LabelInfo::kOffsetNotSet);
                label.definitionOffset = fromLabel.definitionOffset;
            }
            label.fixupOffsets.insert(label.fixupOffsets.end(),
                                      fromLabel.fixupOffsets.begin(),
                                      fromLabel.fixupOffsets.end());
            if (label.definitionOffset != LabelInfo::kOffsetNotSet) {
                fixupLabel(label);
            }
        } else {
            _labels.emplace(p.first, std::move(fromLabel));
        }
    }
}

template <typename... Ts>
size_t CodeFragment::appendParameters(uint8_t* ptr, Ts&&... params) {
    int popCompensation = 0;
    ((popCompensation += params.frameId ? 0 : -1), ...);

    size_t size = 0;
    ((size += appendParameter(ptr + size, params, popCompensation)), ...);
    return size;
}

size_t CodeFragment::appendParameter(uint8_t* ptr,
                                     Instruction::Parameter param,
                                     int& popCompensation) {
    // 'pop' means that the location we're reading from is a temporary value on the VM stack
    // (i.e. not a local variable) and that it needs to be popped off the stack immediately
    // after we read it.
    bool pop = !param.frameId;

    // 'moveFrom' means that the location we're reading from is eligible to be the right hand
    // side of a "move assignment" (i.e. it's an "rvalue reference"). If 'pop' is true, then
    // 'moveFrom' must always be true as well.
    bool moveFrom = pop || param.moveFrom;

    // If the parameter is not coming from a frame then we have to pop it off the stack once the
    // instruction is done.
    uint8_t flags = static_cast<uint8_t>(pop) | (static_cast<uint8_t>(moveFrom) << 1);

    ptr += writeToMemory(ptr, flags);

    if (param.frameId) {
        auto& frame = getOrDeclareFrame(*param.frameId);

        // Compute the absolute variable stack offset based on the current stack depth and pop
        // compensation.
        int stackOffset = varToOffset(param.variable) + popCompensation + _stackSize;

        // If frame has stackPositiion defined, then compute the final relative stack offset.
        // Otherwise, register a fixup to compute the relative stack offset later.
        if (frame.stackPosition != FrameInfo::kPositionNotSet) {
            stackOffset -= frame.stackPosition;
        } else {
            size_t fixUpOffset = ptr - _instrs.data();
            frame.fixupOffsets.push_back(fixUpOffset);
        }

        ptr += writeToMemory(ptr, stackOffset);
    } else {
        ++popCompensation;
    }


    return param.size();
}

void CodeFragment::append(CodeFragment&& code) {
    // Fixup all stack offsets before copying.
    code.fixupStackOffsets(_stackSize);

    _maxStackSize = std::max(_maxStackSize, _stackSize + code._maxStackSize);
    _stackSize += code._stackSize;

    copyCodeAndFixup(std::move(code));
}

void CodeFragment::appendNoStack(CodeFragment&& code) {
    copyCodeAndFixup(std::move(code));
}

void CodeFragment::append(CodeFragment&& lhs, CodeFragment&& rhs) {
    invariant(lhs.stackSize() == rhs.stackSize());

    // Fixup all stack offsets before copying.
    lhs.fixupStackOffsets(_stackSize);
    rhs.fixupStackOffsets(_stackSize);

    _maxStackSize = std::max(_maxStackSize, _stackSize + lhs._maxStackSize);
    _maxStackSize = std::max(_maxStackSize, _stackSize + rhs._maxStackSize);
    _stackSize += lhs._stackSize;

    copyCodeAndFixup(std::move(lhs));
    copyCodeAndFixup(std::move(rhs));
}

void CodeFragment::appendConstVal(value::TypeTags tag, value::Value val) {
    Instruction i;
    i.tag = Instruction::pushConstVal;

    auto offset = allocateSpace(sizeof(Instruction) + sizeof(tag) + sizeof(val));

    offset += writeToMemory(offset, i);
    offset += writeToMemory(offset, tag);
    offset += writeToMemory(offset, val);

    adjustStackSimple(i);
}

void CodeFragment::appendAccessVal(value::SlotAccessor* accessor) {
    Instruction i;
    i.tag = [](value::SlotAccessor* accessor) {
        if (accessor->is<value::OwnedValueAccessor>()) {
            return Instruction::pushOwnedAccessorVal;
        } else if (accessor->is<RuntimeEnvironment::Accessor>()) {
            return Instruction::pushEnvAccessorVal;
        }

        return Instruction::pushAccessVal;
    }(accessor);
    auto offset = allocateSpace(sizeof(Instruction) + sizeof(accessor));

    offset += writeToMemory(offset, i);
    offset += writeToMemory(offset, accessor);

    adjustStackSimple(i);
}

void CodeFragment::appendMoveVal(value::SlotAccessor* accessor) {
    Instruction i;
    i.tag = Instruction::pushMoveVal;

    auto offset = allocateSpace(sizeof(Instruction) + sizeof(accessor));

    offset += writeToMemory(offset, i);
    offset += writeToMemory(offset, accessor);

    adjustStackSimple(i);
}

void CodeFragment::appendLocalVal(FrameId frameId, int variable, bool moveFrom) {
    Instruction i;
    i.tag = moveFrom ? Instruction::pushMoveLocalVal : Instruction::pushLocalVal;

    auto& frame = getOrDeclareFrame(frameId);

    // Compute the absolute variable stack offset based on the current stack depth
    int stackOffset = varToOffset(variable) + _stackSize;

    // If frame has stackPositiion defined, then compute the final relative stack offset.
    // Otherwise, register a fixup to compute the relative stack offset later.
    if (frame.stackPosition != FrameInfo::kPositionNotSet) {
        stackOffset -= frame.stackPosition;
    } else {
        auto fixUpOffset = _instrs.size() + sizeof(Instruction);
        frame.fixupOffsets.push_back(fixUpOffset);
    }

    auto offset = allocateSpace(sizeof(Instruction) + sizeof(stackOffset));

    offset += writeToMemory(offset, i);
    offset += writeToMemory(offset, stackOffset);

    adjustStackSimple(i);
}

void CodeFragment::appendLocalLambda(int codePosition) {
    Instruction i;
    i.tag = Instruction::pushLocalLambda;

    auto size = sizeof(Instruction) + sizeof(codePosition);
    auto offset = allocateSpace(size);

    int codeOffset = codePosition - _instrs.size();

    offset += writeToMemory(offset, i);
    offset += writeToMemory(offset, codeOffset);

    adjustStackSimple(i);
}

void CodeFragment::appendPop() {
    appendSimpleInstruction(Instruction::pop);
}

void CodeFragment::appendSwap() {
    appendSimpleInstruction(Instruction::swap);
}

void CodeFragment::appendCmp3w(Instruction::Parameter lhs, Instruction::Parameter rhs) {
    appendSimpleInstruction(Instruction::cmp3w, lhs, rhs);
}

void CodeFragment::appendAdd(Instruction::Parameter lhs, Instruction::Parameter rhs) {
    appendSimpleInstruction(Instruction::add, lhs, rhs);
}

void CodeFragment::appendNumericConvert(value::TypeTags targetTag) {
    Instruction i;
    i.tag = Instruction::numConvert;

    auto offset = allocateSpace(sizeof(Instruction) + sizeof(targetTag));

    offset += writeToMemory(offset, i);
    offset += writeToMemory(offset, targetTag);
    adjustStackSimple(i);
}

void CodeFragment::appendSub(Instruction::Parameter lhs, Instruction::Parameter rhs) {
    appendSimpleInstruction(Instruction::sub, lhs, rhs);
}

void CodeFragment::appendMul(Instruction::Parameter lhs, Instruction::Parameter rhs) {
    appendSimpleInstruction(Instruction::mul, lhs, rhs);
}

void CodeFragment::appendDiv(Instruction::Parameter lhs, Instruction::Parameter rhs) {
    appendSimpleInstruction(Instruction::div, lhs, rhs);
}

void CodeFragment::appendIDiv(Instruction::Parameter lhs, Instruction::Parameter rhs) {
    appendSimpleInstruction(Instruction::idiv, lhs, rhs);
}

void CodeFragment::appendMod(Instruction::Parameter lhs, Instruction::Parameter rhs) {
    appendSimpleInstruction(Instruction::mod, lhs, rhs);
}

void CodeFragment::appendNegate(Instruction::Parameter input) {
    appendSimpleInstruction(Instruction::negate, input);
}

void CodeFragment::appendNot(Instruction::Parameter input) {
    appendSimpleInstruction(Instruction::logicNot, input);
}

void CodeFragment::appendLess(Instruction::Parameter lhs, Instruction::Parameter rhs) {
    appendSimpleInstruction(Instruction::less, lhs, rhs);
}

void CodeFragment::appendLessEq(Instruction::Parameter lhs, Instruction::Parameter rhs) {
    appendSimpleInstruction(Instruction::lessEq, lhs, rhs);
}

void CodeFragment::appendGreater(Instruction::Parameter lhs, Instruction::Parameter rhs) {
    appendSimpleInstruction(Instruction::greater, lhs, rhs);
}

void CodeFragment::appendGreaterEq(Instruction::Parameter lhs, Instruction::Parameter rhs) {
    appendSimpleInstruction(Instruction::greaterEq, lhs, rhs);
}

void CodeFragment::appendEq(Instruction::Parameter lhs, Instruction::Parameter rhs) {
    appendSimpleInstruction(Instruction::eq, lhs, rhs);
}

void CodeFragment::appendNeq(Instruction::Parameter lhs, Instruction::Parameter rhs) {
    appendSimpleInstruction(Instruction::neq, lhs, rhs);
}

template <typename... Ts>
void CodeFragment::appendSimpleInstruction(Instruction::Tags tag, Ts&&... params) {
    Instruction i;
    i.tag = tag;

    // For every parameter that is popped (i.e. not coming from a frame) we have to compensate frame
    // offsets.
    size_t paramSize = 0;

    ((paramSize += params.size()), ...);

    auto offset = allocateSpace(sizeof(Instruction) + paramSize);

    offset += writeToMemory(offset, i);
    offset += appendParameters(offset, params...);

    adjustStackSimple(i, params...);
}

void CodeFragment::appendCollLess(Instruction::Parameter lhs,
                                  Instruction::Parameter rhs,
                                  Instruction::Parameter collator) {
    appendSimpleInstruction(Instruction::collLess, collator, lhs, rhs);
}

void CodeFragment::appendCollLessEq(Instruction::Parameter lhs,
                                    Instruction::Parameter rhs,
                                    Instruction::Parameter collator) {
    appendSimpleInstruction(Instruction::collLessEq, collator, lhs, rhs);
}

void CodeFragment::appendCollGreater(Instruction::Parameter lhs,
                                     Instruction::Parameter rhs,
                                     Instruction::Parameter collator) {
    appendSimpleInstruction(Instruction::collGreater, collator, lhs, rhs);
}

void CodeFragment::appendCollGreaterEq(Instruction::Parameter lhs,
                                       Instruction::Parameter rhs,
                                       Instruction::Parameter collator) {
    appendSimpleInstruction(Instruction::collGreaterEq, collator, lhs, rhs);
}

void CodeFragment::appendCollEq(Instruction::Parameter lhs,
                                Instruction::Parameter rhs,
                                Instruction::Parameter collator) {
    appendSimpleInstruction(Instruction::collEq, collator, lhs, rhs);
}

void CodeFragment::appendCollNeq(Instruction::Parameter lhs,
                                 Instruction::Parameter rhs,
                                 Instruction::Parameter collator) {
    appendSimpleInstruction(Instruction::collNeq, collator, lhs, rhs);
}

void CodeFragment::appendCollCmp3w(Instruction::Parameter lhs,
                                   Instruction::Parameter rhs,
                                   Instruction::Parameter collator) {
    appendSimpleInstruction(Instruction::collCmp3w, collator, lhs, rhs);
}

void CodeFragment::appendFillEmpty() {
    appendSimpleInstruction(Instruction::fillEmpty);
}

void CodeFragment::appendFillEmpty(Instruction::Constants k) {
    Instruction i;
    i.tag = Instruction::fillEmptyImm;

    auto offset = allocateSpace(sizeof(Instruction) + sizeof(k));

    offset += writeToMemory(offset, i);
    offset += writeToMemory(offset, k);

    adjustStackSimple(i);
}

void CodeFragment::appendGetField(Instruction::Parameter lhs, Instruction::Parameter rhs) {
    appendSimpleInstruction(Instruction::getField, lhs, rhs);
}

void CodeFragment::appendGetField(Instruction::Parameter input, StringData fieldName) {
    auto size = fieldName.size();
    invariant(size < Instruction::kMaxInlineStringSize);

    Instruction i;
    i.tag = Instruction::getFieldImm;

    auto offset = allocateSpace(sizeof(Instruction) + input.size() + sizeof(uint8_t) + size);

    offset += writeToMemory(offset, i);
    offset += appendParameters(offset, input);
    offset += writeToMemory(offset, static_cast<uint8_t>(size));
    for (auto ch : fieldName) {
        offset += writeToMemory(offset, ch);
    }

    adjustStackSimple(i, input);
}

void CodeFragment::appendGetElement(Instruction::Parameter lhs, Instruction::Parameter rhs) {
    appendSimpleInstruction(Instruction::getElement, lhs, rhs);
}

void CodeFragment::appendCollComparisonKey(Instruction::Parameter lhs, Instruction::Parameter rhs) {
    appendSimpleInstruction(Instruction::collComparisonKey, lhs, rhs);
}

void CodeFragment::appendGetFieldOrElement(Instruction::Parameter lhs, Instruction::Parameter rhs) {
    appendSimpleInstruction(Instruction::getFieldOrElement, lhs, rhs);
}

void CodeFragment::appendGetArraySize(Instruction::Parameter input) {
    appendSimpleInstruction(Instruction::getArraySize, input);
}

void CodeFragment::appendSetField() {
    appendSimpleInstruction(Instruction::setField);
}

void CodeFragment::appendSum() {
    appendSimpleInstruction(Instruction::aggSum);
}

void CodeFragment::appendMin() {
    appendSimpleInstruction(Instruction::aggMin);
}

void CodeFragment::appendMax() {
    appendSimpleInstruction(Instruction::aggMax);
}

void CodeFragment::appendFirst() {
    appendSimpleInstruction(Instruction::aggFirst);
}

void CodeFragment::appendLast() {
    appendSimpleInstruction(Instruction::aggLast);
}

void CodeFragment::appendCollMin() {
    appendSimpleInstruction(Instruction::aggCollMin);
}

void CodeFragment::appendCollMax() {
    appendSimpleInstruction(Instruction::aggCollMax);
}

void CodeFragment::appendExists(Instruction::Parameter input) {
    appendSimpleInstruction(Instruction::exists, input);
}

void CodeFragment::appendIsNull(Instruction::Parameter input) {
    appendSimpleInstruction(Instruction::isNull, input);
}

void CodeFragment::appendIsObject(Instruction::Parameter input) {
    appendSimpleInstruction(Instruction::isObject, input);
}

void CodeFragment::appendIsArray(Instruction::Parameter input) {
    appendSimpleInstruction(Instruction::isArray, input);
}

void CodeFragment::appendIsInListData(Instruction::Parameter input) {
    appendSimpleInstruction(Instruction::isInListData, input);
}

void CodeFragment::appendIsString(Instruction::Parameter input) {
    appendSimpleInstruction(Instruction::isString, input);
}

void CodeFragment::appendIsNumber(Instruction::Parameter input) {
    appendSimpleInstruction(Instruction::isNumber, input);
}

void CodeFragment::appendIsBinData(Instruction::Parameter input) {
    appendSimpleInstruction(Instruction::isBinData, input);
}

void CodeFragment::appendIsDate(Instruction::Parameter input) {
    appendSimpleInstruction(Instruction::isDate, input);
}

void CodeFragment::appendIsNaN(Instruction::Parameter input) {
    appendSimpleInstruction(Instruction::isNaN, input);
}

void CodeFragment::appendIsInfinity(Instruction::Parameter input) {
    appendSimpleInstruction(Instruction::isInfinity, input);
}

void CodeFragment::appendIsRecordId(Instruction::Parameter input) {
    appendSimpleInstruction(Instruction::isRecordId, input);
}

void CodeFragment::appendIsMinKey(Instruction::Parameter input) {
    appendSimpleInstruction(Instruction::isMinKey, input);
}

void CodeFragment::appendIsMaxKey(Instruction::Parameter input) {
    appendSimpleInstruction(Instruction::isMaxKey, input);
}

void CodeFragment::appendIsTimestamp(Instruction::Parameter input) {
    appendSimpleInstruction(Instruction::isTimestamp, input);
}

void CodeFragment::appendTraverseP() {
    appendSimpleInstruction(Instruction::traverseP);
}

void CodeFragment::appendTraverseP(int codePosition, Instruction::Constants k) {
    Instruction i;
    i.tag = Instruction::traversePImm;

    auto size = sizeof(Instruction) + sizeof(codePosition) + sizeof(k);
    auto offset = allocateSpace(size);

    int codeOffset = codePosition - _instrs.size();

    offset += writeToMemory(offset, i);
    offset += writeToMemory(offset, k);
    offset += writeToMemory(offset, codeOffset);

    adjustStackSimple(i);
}

void CodeFragment::appendMagicTraverseF() {
    appendSimpleInstruction(Instruction::magicTraverseF);
}
void CodeFragment::appendTraverseF() {
    appendSimpleInstruction(Instruction::traverseF);
}

void CodeFragment::appendTraverseF(int codePosition, Instruction::Constants k) {
    Instruction i;
    i.tag = Instruction::traverseFImm;

    auto size = sizeof(Instruction) + sizeof(codePosition) + sizeof(k);
    auto offset = allocateSpace(size);

    int codeOffset = codePosition - _instrs.size();

    offset += writeToMemory(offset, i);
    offset += writeToMemory(offset, k);
    offset += writeToMemory(offset, codeOffset);

    adjustStackSimple(i);
}

void CodeFragment::appendTraverseCellValues() {
    appendSimpleInstruction(Instruction::traverseCsiCellValues);
}

void CodeFragment::appendTraverseCellValues(int codePosition) {
    Instruction i;
    i.tag = Instruction::traverseCsiCellValues;

    auto size = sizeof(Instruction) + sizeof(codePosition);
    auto offset = allocateSpace(size);

    int codeOffset = codePosition - _instrs.size();

    offset += writeToMemory(offset, i);
    offset += writeToMemory(offset, codeOffset);

    adjustStackSimple(i);
}

void CodeFragment::appendTraverseCellTypes() {
    appendSimpleInstruction(Instruction::traverseCsiCellTypes);
}

void CodeFragment::appendTraverseCellTypes(int codePosition) {
    Instruction i;
    i.tag = Instruction::traverseCsiCellTypes;

    auto size = sizeof(Instruction) + sizeof(codePosition);
    auto offset = allocateSpace(size);

    int codeOffset = codePosition - _instrs.size();

    offset += writeToMemory(offset, i);
    offset += writeToMemory(offset, codeOffset);

    adjustStackSimple(i);
}

void CodeFragment::appendTypeMatch(Instruction::Parameter input, uint32_t mask) {
    Instruction i;
    i.tag = Instruction::typeMatchImm;

    auto size = sizeof(Instruction) + input.size() + sizeof(mask);
    auto offset = allocateSpace(size);

    offset += writeToMemory(offset, i);
    offset += appendParameters(offset, input);
    offset += writeToMemory(offset, mask);

    adjustStackSimple(i, input);
}

void CodeFragment::appendDateTrunc(TimeUnit unit,
                                   int64_t binSize,
                                   TimeZone timezone,
                                   DayOfWeek startOfWeek) {
    Instruction i;
    i.tag = Instruction::dateTruncImm;

    auto offset = allocateSpace(sizeof(Instruction) + sizeof(unit) + sizeof(binSize) +
                                sizeof(timezone) + sizeof(startOfWeek));

    offset += writeToMemory(offset, i);
    offset += writeToMemory(offset, unit);
    offset += writeToMemory(offset, binSize);
    offset += writeToMemory(offset, timezone);
    offset += writeToMemory(offset, startOfWeek);

    adjustStackSimple(i);
}

void CodeFragment::appendValueBlockApplyLambda() {
    appendSimpleInstruction(Instruction::valueBlockApplyLambda);
}

void CodeFragment::appendFunction(Builtin f, ArityType arity) {
    Instruction i;
    const bool isSmallArity = (arity <= std::numeric_limits<SmallArityType>::max());
    const bool isSmallBuiltin =
        (f <= static_cast<Builtin>(std::numeric_limits<SmallBuiltinType>::max()));
    const bool isSmallFunction = isSmallArity && isSmallBuiltin;
    i.tag = isSmallFunction ? Instruction::functionSmall : Instruction::function;

    _maxStackSize = std::max(_maxStackSize, _stackSize + 1);
    // Account for consumed arguments
    _stackSize -= arity;
    // and the return value.
    _stackSize += 1;

    auto offset = allocateSpace(sizeof(Instruction) +
                                (isSmallFunction ? sizeof(SmallBuiltinType) : sizeof(Builtin)) +
                                (isSmallFunction ? sizeof(SmallArityType) : sizeof(ArityType)));

    offset += writeToMemory(offset, i);
    if (isSmallFunction) {
        SmallBuiltinType smallBuiltin = static_cast<SmallBuiltinType>(f);
        offset += writeToMemory(offset, smallBuiltin);
    } else {
        offset += writeToMemory(offset, f);
    }
    offset += isSmallFunction ? writeToMemory(offset, static_cast<SmallArityType>(arity))
                              : writeToMemory(offset, arity);
}

void CodeFragment::appendLabelJump(LabelId labelId) {
    appendLabelJumpInstruction(labelId, Instruction::jmp);
}

void CodeFragment::appendLabelJumpTrue(LabelId labelId) {
    appendLabelJumpInstruction(labelId, Instruction::jmpTrue);
}

void CodeFragment::appendLabelJumpFalse(LabelId labelId) {
    appendLabelJumpInstruction(labelId, Instruction::jmpFalse);
}

void CodeFragment::appendLabelJumpNothing(LabelId labelId) {
    appendLabelJumpInstruction(labelId, Instruction::jmpNothing);
}

void CodeFragment::appendLabelJumpNotNothing(LabelId labelId) {
    appendLabelJumpInstruction(labelId, Instruction::jmpNotNothing);
}

void CodeFragment::appendLabelJumpInstruction(LabelId labelId, Instruction::Tags tag) {
    auto& label = getOrDeclareLabel(labelId);

    Instruction i;
    i.tag = tag;

    int jumpOffset;
    auto offset = allocateSpace(sizeof(Instruction) + sizeof(jumpOffset));

    if (label.definitionOffset != LabelInfo::kOffsetNotSet) {
        jumpOffset = label.definitionOffset - _instrs.size();
    } else {
        // Fixup will compute the relative jump as if it was done from the fixup offset itself,
        // so initialize jumpOffset with the difference between jump offset and the end of
        // instruction.
        jumpOffset = -static_cast<int>(sizeof(jumpOffset));
        label.fixupOffsets.push_back(offset + sizeof(Instruction) - _instrs.data());
    }

    offset += writeToMemory(offset, i);
    offset += writeToMemory(offset, jumpOffset);

    adjustStackSimple(i);
}

void CodeFragment::appendRet() {
    appendSimpleInstruction(Instruction::ret);
}

void CodeFragment::appendAllocStack(uint32_t size) {
    Instruction i;
    i.tag = Instruction::allocStack;

    auto offset = allocateSpace(sizeof(Instruction) + sizeof(size));

    offset += writeToMemory(offset, i);
    offset += writeToMemory(offset, size);

    adjustStackSimple(i);
}

void CodeFragment::appendFail() {
    appendSimpleInstruction(Instruction::fail);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::getField(value::TypeTags objTag,
                                                                  value::Value objValue,
                                                                  value::TypeTags fieldTag,
                                                                  value::Value fieldValue) {
    if (!value::isString(fieldTag)) {
        return {false, value::TypeTags::Nothing, 0};
    }

    auto fieldStr = value::getStringView(fieldTag, fieldValue);

    return getField(objTag, objValue, fieldStr);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::getField(value::TypeTags objTag,
                                                                  value::Value objValue,
                                                                  StringData fieldStr) {
    if (objTag == value::TypeTags::Object) {
        auto [tag, val] = value::getObjectView(objValue)->getField(fieldStr);
        return {false, tag, val};
    } else if (objTag == value::TypeTags::bsonObject) {
        auto be = value::bitcastTo<const char*>(objValue);
        const auto end = be + ConstDataView(be).read<LittleEndian<uint32_t>>();
        // Skip document length.
        be += 4;
        while (be != end - 1) {
            auto sv = bson::fieldNameAndLength(be);

            if (sv == fieldStr) {
                auto [tag, val] = bson::convertFrom<true>(be, end, fieldStr.size());
                return {false, tag, val};
            }

            be = bson::advance(be, sv.size());
        }
    }
    return {false, value::TypeTags::Nothing, 0};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::getElement(value::TypeTags arrTag,
                                                                    value::Value arrValue,
                                                                    value::TypeTags idxTag,
                                                                    value::Value idxValue) {
    // We need to ensure that 'size_t' is wide enough to store 32-bit index.
    static_assert(sizeof(size_t) >= sizeof(int32_t), "size_t must be at least 32-bits");

    if (!value::isArray(arrTag)) {
        return {false, value::TypeTags::Nothing, 0};
    }

    if (idxTag != value::TypeTags::NumberInt32) {
        return {false, value::TypeTags::Nothing, 0};
    }

    const auto idxInt32 = value::bitcastTo<int32_t>(idxValue);
    const bool isNegative = idxInt32 < 0;

    size_t idx = 0;
    if (isNegative) {
        // Upcast 'idxInt32' to 'int64_t' prevent overflow during the sign change.
        idx = static_cast<size_t>(-static_cast<int64_t>(idxInt32));
    } else {
        idx = static_cast<size_t>(idxInt32);
    }

    if (arrTag == value::TypeTags::Array) {
        // If 'arr' is an SBE array, use Array::getAt() to retrieve the element at index 'idx'.
        auto arrayView = value::getArrayView(arrValue);

        size_t convertedIdx = idx;
        if (isNegative) {
            if (idx > arrayView->size()) {
                return {false, value::TypeTags::Nothing, 0};
            }
            convertedIdx = arrayView->size() - idx;
        }

        auto [tag, val] = value::getArrayView(arrValue)->getAt(convertedIdx);
        return {false, tag, val};
    } else if (arrTag == value::TypeTags::bsonArray || arrTag == value::TypeTags::ArraySet ||
               arrTag == value::TypeTags::ArrayMultiSet) {
        value::ArrayEnumerator enumerator(arrTag, arrValue);

        if (!isNegative) {
            // Loop through array until we meet element at position 'idx'.
            size_t i = 0;
            while (i < idx && !enumerator.atEnd()) {
                i++;
                enumerator.advance();
            }
            // If the array didn't have an element at index 'idx', return Nothing.
            if (enumerator.atEnd()) {
                return {false, value::TypeTags::Nothing, 0};
            }
            auto [tag, val] = enumerator.getViewOfValue();
            return {false, tag, val};
        }

        // For negative indexes we use two pointers approach. We start two array enumerators at the
        // distance of 'idx' and move them at the same time. Once one of the enumerators reaches the
        // end of the array, the second one points to the element at position '-idx'.
        //
        // First, move one of the enumerators 'idx' elements forward.
        size_t i = 0;
        while (i < idx && !enumerator.atEnd()) {
            enumerator.advance();
            i++;
        }

        if (i != idx) {
            // Array is too small to have an element at the requested index.
            return {false, value::TypeTags::Nothing, 0};
        }

        // Initiate second enumerator at the start of the array. Now the distance between
        // 'enumerator' and 'windowEndEnumerator' is exactly 'idx' elements. Move both enumerators
        // until the first one reaches the end of the array.
        value::ArrayEnumerator windowEndEnumerator(arrTag, arrValue);
        while (!enumerator.atEnd() && !windowEndEnumerator.atEnd()) {
            enumerator.advance();
            windowEndEnumerator.advance();
        }
        invariant(enumerator.atEnd());
        invariant(!windowEndEnumerator.atEnd());

        auto [tag, val] = windowEndEnumerator.getViewOfValue();
        return {false, tag, val};
    } else {
        // Earlier in this function we bailed out if the 'arrTag' wasn't Array, ArraySet or
        // bsonArray, so it should be impossible to reach this point.
        MONGO_UNREACHABLE
    }
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::getFieldOrElement(
    value::TypeTags objTag,
    value::Value objValue,
    value::TypeTags fieldTag,
    value::Value fieldValue) {
    // If this is an array and we can convert the "field name" to a reasonable number then treat
    // this as getElement call.
    if (value::isArray(objTag) && value::isString(fieldTag)) {
        int idx;
        auto status = NumberParser{}(value::getStringView(fieldTag, fieldValue), &idx);
        if (!status.isOK()) {
            return {false, value::TypeTags::Nothing, 0};
        }
        return getElement(
            objTag, objValue, value::TypeTags::NumberInt32, value::bitcastFrom<int>(idx));
    } else {
        return getField(objTag, objValue, fieldTag, fieldValue);
    }
}

void ByteCode::traverseP(const CodeFragment* code) {
    // Traverse a projection path - evaluate the input lambda on every element of the input array.
    // The traversal is recursive; i.e. we visit nested arrays if any.
    auto [maxDepthOwn, maxDepthTag, maxDepthVal] = getFromStack(0);
    popAndReleaseStack();
    auto [lamOwn, lamTag, lamVal] = getFromStack(0);
    popAndReleaseStack();

    if ((maxDepthTag != value::TypeTags::Nothing && maxDepthTag != value::TypeTags::NumberInt32) ||
        lamTag != value::TypeTags::LocalLambda) {
        popAndReleaseStack();
        pushStack(false, value::TypeTags::Nothing, 0);
        return;
    }

    int64_t lamPos = value::bitcastTo<int64_t>(lamVal);
    int64_t maxDepth = maxDepthTag == value::TypeTags::NumberInt32
        ? value::bitcastTo<int32_t>(maxDepthVal)
        : std::numeric_limits<int64_t>::max();

    traverseP(code, lamPos, maxDepth);
}

void ByteCode::traverseP(const CodeFragment* code, int64_t position, int64_t maxDepth) {
    auto [own, tag, val] = getFromStack(0);

    if (value::isArray(tag) && maxDepth > 0) {
        value::ValueGuard input(own, tag, val);
        popStack();

        if (maxDepth != std::numeric_limits<int64_t>::max()) {
            --maxDepth;
        }

        traverseP_nested(code, position, tag, val, maxDepth);
    } else {
        runLambdaInternal(code, position);
    }
}

void ByteCode::traverseP_nested(const CodeFragment* code,
                                int64_t position,
                                value::TypeTags tagInput,
                                value::Value valInput,
                                int64_t maxDepth) {
    auto decrement = [](int64_t d) {
        return d == std::numeric_limits<int64_t>::max() ? d : d - 1;
    };

    auto [tagArrOutput, valArrOutput] = value::makeNewArray();
    auto arrOutput = value::getArrayView(valArrOutput);
    value::ValueGuard guard{tagArrOutput, valArrOutput};

    value::arrayForEach(tagInput, valInput, [&](value::TypeTags elemTag, value::Value elemVal) {
        if (maxDepth > 0 && value::isArray(elemTag)) {
            traverseP_nested(code, position, elemTag, elemVal, decrement(maxDepth));
        } else {
            pushStack(false, elemTag, elemVal);
            runLambdaInternal(code, position);
        }

        auto [retOwn, retTag, retVal] = getFromStack(0);
        popStack();
        if (!retOwn) {
            auto [copyTag, copyVal] = value::copyValue(retTag, retVal);
            retTag = copyTag;
            retVal = copyVal;
        }
        arrOutput->push_back(retTag, retVal);
    });
    guard.reset();
    pushStack(true, tagArrOutput, valArrOutput);
}

void ByteCode::magicTraverseF(const CodeFragment* code) {
    // A combined filter traversal (i.e. non-recursive visit of both array elements and the array
    // itself) with getField/getElement to simulate numeric paths.
    // The semantics are controlled by 2 runtime conditions:
    // 1. is a value to be examined coming from an object (i.e. getField) or from an array (i.e.
    // getElement)? Values originating from objects are further traversed whereas array values are
    // not.
    // 2. is this traversal at the leaf position of the path? If so then the further object
    // traversals are followed. Otherwise there is no further traversals.
    auto [ownFlag, tagFlag, valFlag] = getFromStack(0, true);
    value::ValueGuard firstGuard{ownFlag, tagFlag, valFlag};
    auto [lamOwn, lamTag, lamVal] = getFromStack(0, true);
    value::ValueGuard lamGuard{lamOwn, lamTag, lamVal};
    auto arrayIndex = getFromStack(0, true);
    value::ValueGuard indexGuard{arrayIndex};
    auto fieldName = getFromStack(0, true);
    value::ValueGuard fieldGuard{fieldName};
    auto [ownInput, tagInput, valInput] = getFromStack(0, true);
    value::ValueGuard inputGuard{ownInput, tagInput, valInput};

    const bool preTraverse = value::bitcastTo<int32_t>(valFlag) & MagicTraverse::kPreTraverse;
    const bool postTraverse = value::bitcastTo<int32_t>(valFlag) & MagicTraverse::kPostTraverse;

    auto lambdaPtr = value::bitcastTo<int64_t>(lamVal);

    enum class Traverse { document, array };
    auto innerTraverse = [&](value::TypeTags tagElem,
                             value::Value valElem,
                             Traverse type,
                             bool nested) {
        auto [ownArrayIndex, tagArrayIndex, valArrayIndex] = arrayIndex;
        auto [ownFieldName, tagFieldName, valFieldName] = fieldName;

        auto [ownInner, tagInner, valInner] = type == Traverse::document
            ? getField(tagElem, valElem, tagFieldName, valFieldName)
            : getElement(tagElem, valElem, tagArrayIndex, valArrayIndex);

        // Follow on with a traversal only if the flag is set.
        if (value::isArray(tagInner) && nested) {
            const bool passed = value::arrayAny(
                tagInner, valInner, [&](value::TypeTags tagElem, value::Value valElem) {
                    pushStack(false, tagElem, valElem);
                    if (runLambdaPredicate(code, lambdaPtr)) {
                        pushStack(false, value::TypeTags::Boolean, value::bitcastFrom<bool>(true));
                        return true;
                    }
                    return false;
                });
            if (passed) {
                return passed;
            }
        }
        pushStack(ownInner, tagInner, valInner);
        if (runLambdaPredicate(code, lambdaPtr)) {
            pushStack(false, value::TypeTags::Boolean, value::bitcastFrom<bool>(true));
            return true;
        }
        return false;
    };

    if (value::isArray(tagInput)) {
        const bool passed =
            value::arrayAny(tagInput, valInput, [&](value::TypeTags tagElem, value::Value valElem) {
                return innerTraverse(tagElem, valElem, Traverse::document, preTraverse);
            });

        if (passed) {
            return;
        }

        // For values originating from arrays we do not run the inner traversal unless the flag is
        // set.
        if (!innerTraverse(tagInput, valInput, Traverse::array, postTraverse)) {
            pushStack(false, value::TypeTags::Boolean, value::bitcastFrom<bool>(false));
        }
        return;
    } else {
        if (!innerTraverse(tagInput, valInput, Traverse::document, preTraverse)) {
            pushStack(false, value::TypeTags::Boolean, value::bitcastFrom<bool>(false));
        }
        return;
    }
}

void ByteCode::traverseF(const CodeFragment* code) {
    // Traverse a filter path - evaluate the input lambda (predicate) on every element of the input
    // array without recursion.
    auto [numberOwn, numberTag, numberVal] = getFromStack(0);
    popAndReleaseStack();
    auto [lamOwn, lamTag, lamVal] = getFromStack(0);
    popAndReleaseStack();

    if (lamTag != value::TypeTags::LocalLambda) {
        popAndReleaseStack();
        pushStack(false, value::TypeTags::Nothing, 0);
        return;
    }
    int64_t lamPos = value::bitcastTo<int64_t>(lamVal);

    bool compareArray = numberTag == value::TypeTags::Boolean && value::bitcastTo<bool>(numberVal);

    traverseF(code, lamPos, compareArray);
}

void ByteCode::traverseF(const CodeFragment* code, int64_t position, bool compareArray) {
    auto [ownInput, tagInput, valInput] = getFromStack(0);

    if (value::isArray(tagInput)) {
        traverseFInArray(code, position, compareArray);
    } else {
        runLambdaInternal(code, position);
    }
}

bool ByteCode::runLambdaPredicate(const CodeFragment* code, int64_t position) {
    runLambdaInternal(code, position);
    auto [retOwn, retTag, retVal] = getFromStack(0);
    popStack();

    bool isTrue = (retTag == value::TypeTags::Boolean) && value::bitcastTo<bool>(retVal);
    if (retOwn) {
        value::releaseValue(retTag, retVal);
    }
    return isTrue;
}

void ByteCode::traverseFInArray(const CodeFragment* code, int64_t position, bool compareArray) {
    auto [ownInput, tagInput, valInput] = getFromStack(0);

    value::ValueGuard input(ownInput, tagInput, valInput);
    popStack();

    const bool passed =
        value::arrayAny(tagInput, valInput, [&](value::TypeTags tag, value::Value val) {
            pushStack(false, tag, val);
            if (runLambdaPredicate(code, position)) {
                pushStack(false, value::TypeTags::Boolean, value::bitcastFrom<bool>(true));
                return true;
            }
            return false;
        });

    if (passed) {
        return;
    }

    // If this is a filter over a number path then run over the whole array. More details in
    // SERVER-27442.
    if (compareArray) {
        // Transfer the ownership to the lambda
        pushStack(ownInput, tagInput, valInput);
        input.reset();
        runLambdaInternal(code, position);
        return;
    }

    pushStack(false, value::TypeTags::Boolean, value::bitcastFrom<bool>(false));
}

void ByteCode::traverseCsiCellValues(const CodeFragment* code, int64_t position) {
    auto [ownCsiCell, tagCsiCell, valCsiCell] = getFromStack(0);
    invariant(!ownCsiCell);
    popStack();

    invariant(tagCsiCell == value::TypeTags::csiCell);
    auto csiCell = value::getCsiCellView(valCsiCell);
    bool isTrue = false;

    // If there are no doubly-nested arrays, we can avoid parsing the array info and use the simple
    // cursor over all values in the cell.
    if (!csiCell->splitCellView->hasDoubleNestedArrays) {
        SplitCellView::Cursor<ColumnStoreEncoder> cellCursor =
            csiCell->splitCellView->subcellValuesGenerator<ColumnStoreEncoder>(csiCell->encoder);

        while (cellCursor.hasNext() && !isTrue) {
            const auto& val = cellCursor.nextValue();
            pushStack(false, val->first, val->second);
            isTrue = runLambdaPredicate(code, position);
        }
    } else {
        SplitCellView::CursorWithArrayDepth<ColumnStoreEncoder> cellCursor{
            csiCell->pathDepth,
            csiCell->splitCellView->firstValuePtr,
            csiCell->splitCellView->arrInfo,
            csiCell->encoder};

        while (cellCursor.hasNext() && !isTrue) {
            const auto& val = cellCursor.nextValue();

            if (val.depthWithinDirectlyNestedArraysOnPath > 0 || val.depthAtLeaf > 1) {
                // The value is too deep.
                continue;
            }

            if (val.isObject) {
                continue;
            } else {
                pushStack(false, val.value->first, val.value->second);
                isTrue = runLambdaPredicate(code, position);
            }
        }
    }
    pushStack(false, value::TypeTags::Boolean, value::bitcastFrom<bool>(isTrue));
}

void ByteCode::traverseCsiCellTypes(const CodeFragment* code, int64_t position) {
    using namespace value;

    auto [ownCsiCell, tagCsiCell, valCsiCell] = getFromStack(0);
    invariant(!ownCsiCell);
    popStack();

    invariant(tagCsiCell == TypeTags::csiCell);
    auto csiCell = getCsiCellView(valCsiCell);

    // When traversing cell types cannot use the simple cursor even if the cell doesn't contain
    // doubly-nested arrays because must report types of objects and arrays and for that need to
    // parse the array info.
    SplitCellView::CursorWithArrayDepth<ColumnStoreEncoder> cellCursor{
        csiCell->pathDepth,
        csiCell->splitCellView->firstValuePtr,
        csiCell->splitCellView->arrInfo,
        csiCell->encoder};

    // The dummy array/object are needed when running lambda on the type on non-empty arrays and
    // objects. We allocate them on the stack because these values are only used in the scope of
    // this traversal and discarded after evaluating the lambda.
    const auto dummyArray = Array{};
    const auto dummyObject = Object{};

    bool shouldProcessArray = true;
    bool isTrue = false;
    while (cellCursor.hasNext() && !isTrue) {
        const auto& val = cellCursor.nextValue();

        if (val.depthWithinDirectlyNestedArraysOnPath > 0) {
            // There is nesting on the path.
            continue;
        }

        if (val.depthAtLeaf > 0) {
            // Empty arrays are stored in columnstore cells as values and don't require special
            // handling. All other arrays can be detected when their first value is seen. To apply
            // the lambda to the leaf array type we inject a "fake" array here as the caller should
            // only look at the returned type. Note, that we might still need to process the values
            // inside the array.
            if (shouldProcessArray) {
                shouldProcessArray = false;

                pushStack(false, TypeTags::Array, bitcastFrom<const Array*>(&dummyArray));
                isTrue = runLambdaPredicate(code, position);
                if (isTrue) {
                    break;
                }
            }

            if (val.depthAtLeaf > 1) {
                // The value is inside a nested array at the leaf.
                continue;
            }
        } else {
            shouldProcessArray = true;
        }

        // Apply lambda to the types of values at the leaf.
        if (val.isObject) {
            pushStack(false, TypeTags::Object, bitcastFrom<const Object*>(&dummyObject));
        } else {
            pushStack(false, val.value->first, val.value->second);
        }
        isTrue = runLambdaPredicate(code, position);
    }
    pushStack(false, TypeTags::Boolean, bitcastFrom<bool>(isTrue));
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::setField() {
    auto [newOwn, newTag, newVal] = moveFromStack(0);
    value::ValueGuard guardNewElem{newTag, newVal};
    auto [fieldOwn, fieldTag, fieldVal] = getFromStack(1);
    // Consider using a moveFromStack optimization.
    auto [objOwn, objTag, objVal] = getFromStack(2);

    if (!value::isString(fieldTag)) {
        return {false, value::TypeTags::Nothing, 0};
    }

    auto fieldName = value::getStringView(fieldTag, fieldVal);

    if (newTag == value::TypeTags::Nothing) {
        // Setting a field value to nothing means removing the field.
        if (value::isObject(objTag)) {
            auto [tagOutput, valOutput] = value::makeNewObject();
            auto objOutput = value::getObjectView(valOutput);
            value::ValueGuard guard{tagOutput, valOutput};

            if (objTag == value::TypeTags::bsonObject) {
                auto be = value::bitcastTo<const char*>(objVal);
                const auto end = be + ConstDataView(be).read<LittleEndian<uint32_t>>();

                // Skip document length.
                be += 4;
                while (be != end - 1) {
                    auto sv = bson::fieldNameAndLength(be);

                    if (sv != fieldName) {
                        auto [tag, val] = bson::convertFrom<false>(be, end, sv.size());
                        objOutput->push_back(sv, tag, val);
                    }

                    be = bson::advance(be, sv.size());
                }
            } else {
                auto objRoot = value::getObjectView(objVal);
                for (size_t idx = 0; idx < objRoot->size(); ++idx) {
                    StringData sv(objRoot->field(idx));

                    if (sv != fieldName) {
                        auto [tag, val] = objRoot->getAt(idx);
                        auto [copyTag, copyVal] = value::copyValue(tag, val);
                        objOutput->push_back(sv, copyTag, copyVal);
                    }
                }
            }

            guard.reset();
            return {true, tagOutput, valOutput};
        } else {
            // Removing field from non-object value hardly makes any sense.
            return {false, value::TypeTags::Nothing, 0};
        }
    } else {
        // New value is not Nothing. We will be returning a new Object no matter what.
        auto [tagOutput, valOutput] = value::makeNewObject();
        auto objOutput = value::getObjectView(valOutput);
        value::ValueGuard guard{tagOutput, valOutput};

        if (objTag == value::TypeTags::bsonObject) {
            auto be = value::bitcastTo<const char*>(objVal);
            const auto end = be + ConstDataView(be).read<LittleEndian<uint32_t>>();

            // Skip document length.
            be += 4;
            while (be != end - 1) {
                auto sv = bson::fieldNameAndLength(be);

                if (sv != fieldName) {
                    auto [tag, val] = bson::convertFrom<false>(be, end, sv.size());
                    objOutput->push_back(sv, tag, val);
                }

                be = bson::advance(be, sv.size());
            }
        } else if (objTag == value::TypeTags::Object) {
            auto objRoot = value::getObjectView(objVal);
            for (size_t idx = 0; idx < objRoot->size(); ++idx) {
                StringData sv(objRoot->field(idx));

                if (sv != fieldName) {
                    auto [tag, val] = objRoot->getAt(idx);
                    auto [copyTag, copyVal] = value::copyValue(tag, val);
                    objOutput->push_back(sv, copyTag, copyVal);
                }
            }
        }
        guardNewElem.reset();
        if (!newOwn) {
            auto [copyTag, copyVal] = value::copyValue(newTag, newVal);
            newTag = copyTag;
            newVal = copyVal;
        }
        objOutput->push_back(fieldName, newTag, newVal);

        guard.reset();
        return {true, tagOutput, valOutput};
    }
    return {false, value::TypeTags::Nothing, 0};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::getArraySize(value::TypeTags tag,
                                                                      value::Value val) {
    size_t result = 0;

    switch (tag) {
        case value::TypeTags::Array: {
            result = value::getArrayView(val)->size();
            break;
        }
        case value::TypeTags::ArraySet: {
            result = value::getArraySetView(val)->size();
            break;
        }
        case value::TypeTags::ArrayMultiSet: {
            result = value::getArrayMultiSetView(val)->size();
            break;
        }
        case value::TypeTags::bsonArray: {
            value::arrayForEach(
                tag, val, [&](value::TypeTags t_unused, value::Value v_unused) { result++; });
            break;
        }
        default:
            return {false, value::TypeTags::Nothing, 0};
    }

    return {false, value::TypeTags::NumberInt64, value::bitcastFrom<int64_t>(result)};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::aggSum(value::TypeTags accTag,
                                                                value::Value accValue,
                                                                value::TypeTags fieldTag,
                                                                value::Value fieldValue) {
    // Skip aggregation step if we don't have the input.
    if (fieldTag == value::TypeTags::Nothing) {
        auto [tag, val] = value::copyValue(accTag, accValue);
        return {true, tag, val};
    }

    // Initialize the accumulator.
    if (accTag == value::TypeTags::Nothing) {
        accTag = value::TypeTags::NumberInt32;
        accValue = value::bitcastFrom<int32_t>(0);
    }

    return genericAdd(accTag, accValue, fieldTag, fieldValue);
}

void resetDoubleDoubleSumState(value::Array* state) {
    state->clear();
    // The order of the following three elements should match to 'AggSumValueElems'. An absent
    // 'kDecimalTotal' element means that we've not seen any decimal value. So, we're not adding
    // 'kDecimalTotal' element yet.
    state->push_back(value::TypeTags::NumberInt32, value::bitcastFrom<int32_t>(0));
    state->push_back(value::TypeTags::NumberDouble, value::bitcastFrom<double>(0.0));
    state->push_back(value::TypeTags::NumberDouble, value::bitcastFrom<double>(0.0));
}

std::pair<value::TypeTags, value::Value> initializeDoubleDoubleSumState() {
    auto [accTag, accValue] = value::makeNewArray();
    value::ValueGuard newArrGuard{accTag, accValue};
    auto arr = value::getArrayView(accValue);
    arr->reserve(AggSumValueElems::kMaxSizeOfArray);

    resetDoubleDoubleSumState(arr);

    newArrGuard.reset();
    return {accTag, accValue};
}

template <bool merging>
FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggDoubleDoubleSum(
    ArityType arity) {
    auto [_, fieldTag, fieldValue] = getFromStack(1);
    // Move the incoming accumulator state from the stack. Given that we are now the owner of the
    // state we are free to do any in-place update as we see fit.
    auto [accTag, accValue] = moveOwnedFromStack(0);

    // Initialize the accumulator.
    if (accTag == value::TypeTags::Nothing) {
        std::tie(accTag, accValue) = initializeDoubleDoubleSumState();
    }

    value::ValueGuard guard{accTag, accValue};
    tassert(5755317, "The result slot must be Array-typed", accTag == value::TypeTags::Array);
    auto accumulator = value::getArrayView(accValue);

    if constexpr (merging) {
        aggMergeDoubleDoubleSumsImpl(accumulator, fieldTag, fieldValue);
    } else {
        aggDoubleDoubleSumImpl(accumulator, fieldTag, fieldValue);
    }

    guard.reset();
    return {true, accTag, accValue};
}

// This function is necessary because 'aggDoubleDoubleSum()' result is 'Array' type but we need
// to produce a scalar value out of it.
FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinDoubleDoubleSumFinalize(
    ArityType arity) {
    auto [_, fieldTag, fieldValue] = getFromStack(0);
    auto arr = value::getArrayView(fieldValue);
    return aggDoubleDoubleSumFinalizeImpl(arr);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinDoubleDoublePartialSumFinalize(
    ArityType arity) {
    auto [_, fieldTag, fieldValue] = getFromStack(0);

    // For a count-like accumulator like {$sum: 1}, we use aggSum instruction. In this case, the
    // result type is guaranteed to be either 'NumberInt32', 'NumberInt64', or 'NumberDouble'. We
    // should transform the scalar result into an array which is the over-the-wire data format from
    // a shard to a merging side.
    if (fieldTag == value::TypeTags::NumberInt32 || fieldTag == value::TypeTags::NumberInt64 ||
        fieldTag == value::TypeTags::NumberDouble) {
        auto [tag, val] = value::makeNewArray();
        value::ValueGuard guard{tag, val};
        auto newArr = value::getArrayView(val);

        DoubleDoubleSummation res;
        BSONType resType = BSONType::NumberInt;
        switch (fieldTag) {
            case value::TypeTags::NumberInt32:
                res.addInt(value::bitcastTo<int32_t>(fieldValue));
                break;
            case value::TypeTags::NumberInt64:
                res.addLong(value::bitcastTo<long long>(fieldValue));
                resType = BSONType::NumberLong;
                break;
            case value::TypeTags::NumberDouble:
                res.addDouble(value::bitcastTo<double>(fieldValue));
                resType = BSONType::NumberDouble;
                break;
            default:
                MONGO_UNREACHABLE_TASSERT(6546500);
        }
        auto [sum, addend] = res.getDoubleDouble();

        // The merge-side expects that the first element is the BSON type, not internal slot type.
        newArr->push_back(value::TypeTags::NumberInt32, value::bitcastFrom<int>(resType));
        newArr->push_back(value::TypeTags::NumberDouble, value::bitcastFrom<double>(sum));
        newArr->push_back(value::TypeTags::NumberDouble, value::bitcastFrom<double>(addend));

        guard.reset();
        return {true, tag, val};
    }

    tassert(6546501, "The result slot must be an Array", fieldTag == value::TypeTags::Array);
    auto arr = value::getArrayView(fieldValue);
    tassert(6294000,
            str::stream() << "The result slot must have at least "
                          << AggSumValueElems::kMaxSizeOfArray - 1
                          << " elements but got: " << arr->size(),
            arr->size() >= AggSumValueElems::kMaxSizeOfArray - 1);

    auto [tag, val] = makeCopyArray(*arr);
    value::ValueGuard guard{tag, val};
    auto newArr = value::getArrayView(val);

    // Replaces the first element by the corresponding 'BSONType'.
    auto bsonType = [=]() -> int {
        switch (arr->getAt(AggSumValueElems::kNonDecimalTotalTag).first) {
            case value::TypeTags::NumberInt32:
                return static_cast<int>(BSONType::NumberInt);
            case value::TypeTags::NumberInt64:
                return static_cast<int>(BSONType::NumberLong);
            case value::TypeTags::NumberDouble:
                return static_cast<int>(BSONType::NumberDouble);
            default:
                MONGO_UNREACHABLE_TASSERT(6294001);
                return 0;
        }
    }();
    // The merge-side expects that the first element is the BSON type, not internal slot type.
    newArr->setAt(AggSumValueElems::kNonDecimalTotalTag,
                  value::TypeTags::NumberInt32,
                  value::bitcastFrom<int>(bsonType));

    guard.reset();
    return {true, tag, val};
}

template <bool merging>
FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggStdDev(ArityType arity) {
    auto [_, fieldTag, fieldValue] = getFromStack(1);
    // Move the incoming accumulator state from the stack. Given that we are now the owner of the
    // state we are free to do any in-place update as we see fit.
    auto [accTag, accValue] = moveOwnedFromStack(0);

    // Initialize the accumulator.
    if (accTag == value::TypeTags::Nothing) {
        std::tie(accTag, accValue) = value::makeNewArray();
        value::ValueGuard newArrGuard{accTag, accValue};
        auto arr = value::getArrayView(accValue);
        arr->reserve(AggStdDevValueElems::kSizeOfArray);

        // The order of the following three elements should match to 'AggStdDevValueElems'.
        arr->push_back(value::TypeTags::NumberInt64, value::bitcastFrom<int64_t>(0));
        arr->push_back(value::TypeTags::NumberDouble, value::bitcastFrom<double>(0.0));
        arr->push_back(value::TypeTags::NumberDouble, value::bitcastFrom<double>(0.0));
        newArrGuard.reset();
    }

    value::ValueGuard guard{accTag, accValue};
    tassert(5755210, "The result slot must be Array-typed", accTag == value::TypeTags::Array);
    auto accumulator = value::getArrayView(accValue);

    if constexpr (merging) {
        aggMergeStdDevsImpl(accumulator, fieldTag, fieldValue);
    } else {
        aggStdDevImpl(accumulator, fieldTag, fieldValue);
    }

    guard.reset();
    return {true, accTag, accValue};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinStdDevPopFinalize(ArityType arity) {
    auto [_, fieldTag, fieldValue] = getFromStack(0);

    return aggStdDevFinalizeImpl(fieldValue, false /* isSamp */);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinStdDevSampFinalize(
    ArityType arity) {
    auto [_, fieldTag, fieldValue] = getFromStack(0);

    return aggStdDevFinalizeImpl(fieldValue, true /* isSamp */);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::aggMin(value::TypeTags accTag,
                                                                value::Value accValue,
                                                                value::TypeTags fieldTag,
                                                                value::Value fieldValue,
                                                                CollatorInterface* collator) {
    // Skip aggregation step if we don't have the input.
    if (fieldTag == value::TypeTags::Nothing) {
        auto [tag, val] = value::copyValue(accTag, accValue);
        return {true, tag, val};
    }

    // Initialize the accumulator.
    if (accTag == value::TypeTags::Nothing) {
        auto [tag, val] = value::copyValue(fieldTag, fieldValue);
        return {true, tag, val};
    }

    auto [tag, val] = value::compare3way(accTag, accValue, fieldTag, fieldValue, collator);

    if (tag == value::TypeTags::NumberInt32 && value::bitcastTo<int>(val) < 0) {
        auto [tag, val] = value::copyValue(accTag, accValue);
        return {true, tag, val};
    } else {
        auto [tag, val] = value::copyValue(fieldTag, fieldValue);
        return {true, tag, val};
    }
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::aggMax(value::TypeTags accTag,
                                                                value::Value accValue,
                                                                value::TypeTags fieldTag,
                                                                value::Value fieldValue,
                                                                CollatorInterface* collator) {
    // Skip aggregation step if we don't have the input.
    if (fieldTag == value::TypeTags::Nothing) {
        auto [tag, val] = value::copyValue(accTag, accValue);
        return {true, tag, val};
    }

    // Initialize the accumulator.
    if (accTag == value::TypeTags::Nothing) {
        auto [tag, val] = value::copyValue(fieldTag, fieldValue);
        return {true, tag, val};
    }

    auto [tag, val] = value::compare3way(accTag, accValue, fieldTag, fieldValue, collator);

    if (tag == value::TypeTags::NumberInt32 && value::bitcastTo<int>(val) > 0) {
        auto [tag, val] = value::copyValue(accTag, accValue);
        return {true, tag, val};
    } else {
        auto [tag, val] = value::copyValue(fieldTag, fieldValue);
        return {true, tag, val};
    }
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::aggFirst(value::TypeTags accTag,
                                                                  value::Value accValue,
                                                                  value::TypeTags fieldTag,
                                                                  value::Value fieldValue) {
    // Skip aggregation step if we don't have the input.
    if (fieldTag == value::TypeTags::Nothing) {
        auto [tag, val] = value::copyValue(accTag, accValue);
        return {true, tag, val};
    }

    // Initialize the accumulator.
    if (accTag == value::TypeTags::Nothing) {
        auto [tag, val] = value::copyValue(fieldTag, fieldValue);
        return {true, tag, val};
    }

    // Disregard the next value, always return the first one.
    auto [tag, val] = value::copyValue(accTag, accValue);
    return {true, tag, val};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::aggLast(value::TypeTags accTag,
                                                                 value::Value accValue,
                                                                 value::TypeTags fieldTag,
                                                                 value::Value fieldValue) {
    // Skip aggregation step if we don't have the input.
    if (fieldTag == value::TypeTags::Nothing) {
        auto [tag, val] = value::copyValue(accTag, accValue);
        return {true, tag, val};
    }

    // Initialize the accumulator.
    if (accTag == value::TypeTags::Nothing) {
        auto [tag, val] = value::copyValue(fieldTag, fieldValue);
        return {true, tag, val};
    }

    // Disregard the accumulator, always return the next value.
    auto [tag, val] = value::copyValue(fieldTag, fieldValue);
    return {true, tag, val};
}


bool hasSeparatorAt(size_t idx, StringData input, StringData separator) {
    return (idx + separator.size() <= input.size()) &&
        input.substr(idx, separator.size()) == separator;
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinSplit(ArityType arity) {
    auto [ownedSeparator, tagSeparator, valSeparator] = getFromStack(1);
    auto [ownedInput, tagInput, valInput] = getFromStack(0);

    if (!value::isString(tagSeparator) || !value::isString(tagInput)) {
        return {false, value::TypeTags::Nothing, 0};
    }

    auto input = value::getStringView(tagInput, valInput);
    auto separator = value::getStringView(tagSeparator, valSeparator);

    auto [tag, val] = value::makeNewArray();
    auto arr = value::getArrayView(val);
    value::ValueGuard guard{tag, val};

    size_t splitPos;
    while ((splitPos = input.find(separator)) != std::string::npos) {
        auto [tag, val] = value::makeNewString(input.substr(0, splitPos));
        arr->push_back(tag, val);

        splitPos += separator.size();
        input = input.substr(splitPos);
    }

    // This is the last string.
    {
        auto [tag, val] = value::makeNewString(input);
        arr->push_back(tag, val);
    }

    guard.reset();
    return {true, tag, val};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinDropFields(ArityType arity) {
    auto [ownedSeparator, tagInObj, valInObj] = getFromStack(0);

    // We operate only on objects.
    if (!value::isObject(tagInObj)) {
        return {false, value::TypeTags::Nothing, 0};
    }

    // Build the set of fields to drop.
    StringSet restrictFieldsSet;
    for (ArityType idx = 1; idx < arity; ++idx) {
        auto [owned, tag, val] = getFromStack(idx);

        if (!value::isString(tag)) {
            return {false, value::TypeTags::Nothing, 0};
        }

        restrictFieldsSet.emplace(value::getStringView(tag, val));
    }

    auto [tag, val] = value::makeNewObject();
    auto obj = value::getObjectView(val);
    value::ValueGuard guard{tag, val};

    if (tagInObj == value::TypeTags::bsonObject) {
        auto be = value::bitcastTo<const char*>(valInObj);
        const auto end = be + ConstDataView(be).read<LittleEndian<uint32_t>>();
        // Skip document length.
        be += 4;
        while (be != end - 1) {
            auto sv = bson::fieldNameAndLength(be);

            if (restrictFieldsSet.count(sv) == 0) {
                auto [tag, val] = bson::convertFrom<false>(be, end, sv.size());
                obj->push_back(sv, tag, val);
            }

            be = bson::advance(be, sv.size());
        }
    } else if (tagInObj == value::TypeTags::Object) {
        auto objRoot = value::getObjectView(valInObj);
        for (size_t idx = 0; idx < objRoot->size(); ++idx) {
            StringData sv(objRoot->field(idx));

            if (restrictFieldsSet.count(sv) == 0) {

                auto [tag, val] = objRoot->getAt(idx);
                auto [copyTag, copyVal] = value::copyValue(tag, val);
                obj->push_back(sv, copyTag, copyVal);
            }
        }
    }

    guard.reset();
    return {true, tag, val};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinNewArray(ArityType arity) {
    auto [tag, val] = value::makeNewArray();
    value::ValueGuard guard{tag, val};

    auto arr = value::getArrayView(val);

    if (arity) {
        arr->reserve(arity);
        for (ArityType idx = 0; idx < arity; ++idx) {
            auto [tag, val] = moveOwnedFromStack(idx);
            arr->push_back(tag, val);
        }
    }

    guard.reset();
    return {true, tag, val};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinKeepFields(ArityType arity) {
    auto [ownedInObj, tagInObj, valInObj] = getFromStack(0);

    // We operate only on objects.
    if (!value::isObject(tagInObj)) {
        return {false, value::TypeTags::Nothing, 0};
    }

    // Build the set of fields to keep.
    StringSet keepFieldsSet;
    for (ArityType idx = 1; idx < arity; ++idx) {
        auto [owned, tag, val] = getFromStack(idx);

        if (!value::isString(tag)) {
            return {false, value::TypeTags::Nothing, 0};
        }

        keepFieldsSet.emplace(value::getStringView(tag, val));
    }

    auto [tag, val] = value::makeNewObject();
    auto obj = value::getObjectView(val);
    value::ValueGuard guard{tag, val};

    if (tagInObj == value::TypeTags::bsonObject) {
        auto be = value::bitcastTo<const char*>(valInObj);
        const auto end = be + ConstDataView(be).read<LittleEndian<uint32_t>>();
        // Skip document length.
        be += 4;
        while (be != end - 1) {
            auto sv = bson::fieldNameAndLength(be);

            if (keepFieldsSet.count(sv) == 1) {
                auto [tag, val] = bson::convertFrom<true>(be, end, sv.size());
                auto [copyTag, copyVal] = value::copyValue(tag, val);
                obj->push_back(sv, copyTag, copyVal);
            }

            be = bson::advance(be, sv.size());
        }
    } else if (tagInObj == value::TypeTags::Object) {
        auto objRoot = value::getObjectView(valInObj);
        for (size_t idx = 0; idx < objRoot->size(); ++idx) {
            StringData sv(objRoot->field(idx));

            if (keepFieldsSet.count(sv) == 1) {
                auto [tag, val] = objRoot->getAt(idx);
                auto [copyTag, copyVal] = value::copyValue(tag, val);
                obj->push_back(sv, copyTag, copyVal);
            }
        }
    }

    guard.reset();
    return {true, tag, val};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinNewArrayFromRange(ArityType arity) {
    auto [tag, val] = value::makeNewArray();
    value::ValueGuard guard{tag, val};

    auto arr = value::getArrayView(val);

    auto [startOwned, startTag, start] = getFromStack(0);
    auto [endOwned, endTag, end] = getFromStack(1);
    auto [stepOwned, stepTag, step] = getFromStack(2);

    for (auto& tag : {startTag, endTag, stepTag}) {
        if (value::TypeTags::NumberInt32 != tag) {
            return {false, value::TypeTags::Nothing, 0};
        }
    }

    // Cast to broader type 'int64_t' to prevent overflow during loop.
    auto startVal = value::numericCast<int64_t>(startTag, start);
    auto endVal = value::numericCast<int64_t>(endTag, end);
    auto stepVal = value::numericCast<int64_t>(stepTag, step);

    if (stepVal == 0) {
        return {false, value::TypeTags::Nothing, 0};
    }

    // Calculate how much memory is needed to generate the array and avoid going over the memLimit.
    auto steps = (endVal - startVal) / stepVal;
    // If steps not positive then no amount of steps can get you from start to end. For example
    // with start=5, end=7, step=-1 steps would be negative and in this case we would return an
    // empty array.
    auto length = steps >= 0 ? 1 + steps : 0;
    int64_t memNeeded = sizeof(value::Array) + length * value::getApproximateSize(startTag, start);
    auto memLimit = internalQueryMaxRangeBytes.load();
    uassert(ErrorCodes::ExceededMemoryLimit,
            str::stream() << "$range would use too much memory (" << memNeeded
                          << " bytes) and cannot spill to disk. Memory limit: " << memLimit
                          << " bytes",
            memNeeded < memLimit);

    arr->reserve(length);
    for (auto i = startVal; stepVal > 0 ? i < endVal : i > endVal; i += stepVal) {
        arr->push_back(value::TypeTags::NumberInt32, value::bitcastTo<int32_t>(i));
    }

    guard.reset();
    return {true, tag, val};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinNewObj(ArityType arity) {
    std::vector<value::TypeTags> typeTags;
    std::vector<value::Value> values;
    std::vector<std::string> names;

    size_t tmpVectorLen = arity >> 1;
    typeTags.reserve(tmpVectorLen);
    values.reserve(tmpVectorLen);
    names.reserve(tmpVectorLen);

    for (ArityType idx = 0; idx < arity; idx += 2) {
        {
            auto [owned, tag, val] = getFromStack(idx);

            if (!value::isString(tag)) {
                return {false, value::TypeTags::Nothing, 0};
            }

            names.emplace_back(value::getStringView(tag, val));
        }
        {
            auto [owned, tag, val] = getFromStack(idx + 1);
            typeTags.push_back(tag);
            values.push_back(val);
        }
    }

    auto [tag, val] = value::makeNewObject();
    auto obj = value::getObjectView(val);
    value::ValueGuard guard{tag, val};

    if (typeTags.size()) {
        obj->reserve(typeTags.size());
        for (size_t idx = 0; idx < typeTags.size(); ++idx) {
            auto [tagCopy, valCopy] = value::copyValue(typeTags[idx], values[idx]);
            obj->push_back(names[idx], tagCopy, valCopy);
        }
    }

    guard.reset();
    return {true, tag, val};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinNewBsonObj(ArityType arity) {
    UniqueBSONObjBuilder bob;

    for (ArityType idx = 0; idx < arity; idx += 2) {
        auto [_, nameTag, nameVal] = getFromStack(idx);
        auto [__, fieldTag, fieldVal] = getFromStack(idx + 1);
        if (!value::isString(nameTag)) {
            return {false, value::TypeTags::Nothing, 0};
        }

        auto name = value::getStringView(nameTag, nameVal);
        bson::appendValueToBsonObj(bob, name, fieldTag, fieldVal);
    }

    bob.doneFast();
    char* data = bob.bb().release().release();
    return {true, value::TypeTags::bsonObject, value::bitcastFrom<char*>(data)};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinKeyStringToString(ArityType arity) {
    auto [owned, tagInKey, valInKey] = getFromStack(0);

    // We operate only on keys.
    if (tagInKey != value::TypeTags::ksValue) {
        return {false, value::TypeTags::Nothing, 0};
    }

    auto key = value::getKeyStringView(valInKey);

    auto [tagStr, valStr] = value::makeNewString(key->toString());

    return {true, tagStr, valStr};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::genericNewKeyString(
    ArityType arity, CollatorInterface* collator) {
    auto [_, tagVersion, valVersion] = getFromStack(0);
    auto [__, tagOrdering, valOrdering] = getFromStack(1);
    auto [___, tagDiscriminator, valDiscriminator] = getFromStack(arity - 1u);
    if (!value::isNumber(tagVersion) || !value::isNumber(tagOrdering) ||
        !value::isNumber(tagDiscriminator)) {
        return {false, value::TypeTags::Nothing, 0};
    }

    auto version = value::numericCast<int64_t>(tagVersion, valVersion);
    auto discriminator = value::numericCast<int64_t>(tagDiscriminator, valDiscriminator);
    if ((version < 0 || version > 1) || (discriminator < 0 || discriminator > 2)) {
        return {false, value::TypeTags::Nothing, 0};
    }

    auto ksVersion = static_cast<key_string::Version>(version);
    auto ksDiscriminator = static_cast<key_string::Discriminator>(discriminator);

    uint32_t orderingBits = value::numericCast<int32_t>(tagOrdering, valOrdering);
    BSONObjBuilder bb;
    for (size_t i = 0; orderingBits != 0 && i < arity - 3u; ++i, orderingBits >>= 1) {
        bb.append(""_sd, (orderingBits & 1) ? -1 : 1);
    }

    key_string::HeapBuilder kb{ksVersion, Ordering::make(bb.done())};

    const auto stringTransformFn = [&](StringData stringData) {
        return collator->getComparisonString(stringData);
    };

    for (size_t idx = 2; idx < arity - 1u; ++idx) {
        auto [_, tag, val] = getFromStack(idx);
        // This is needed so that we can use 'tag' in the uassert() below without getting a
        // "Reference to local binding declared in enclosing function" compile error on clang.
        auto tagCopy = tag;

        switch (tag) {
            case value::TypeTags::Boolean:
                kb.appendBool(value::bitcastTo<bool>(val));
                break;
            case value::TypeTags::NumberInt32:
                kb.appendNumberInt(value::bitcastTo<int32_t>(val));
                break;
            case value::TypeTags::NumberInt64:
                kb.appendNumberLong(value::bitcastTo<int64_t>(val));
                break;
            case value::TypeTags::NumberDouble:
                kb.appendNumberDouble(value::bitcastTo<double>(val));
                break;
            case value::TypeTags::NumberDecimal:
                kb.appendNumberDecimal(value::bitcastTo<Decimal128>(val));
                break;
            case value::TypeTags::StringSmall:
            case value::TypeTags::StringBig:
            case value::TypeTags::bsonString:
                if (collator) {
                    kb.appendString(value::getStringView(tag, val), stringTransformFn);
                } else {
                    kb.appendString(value::getStringView(tag, val));
                }
                break;
            case value::TypeTags::Null:
                kb.appendNull();
                break;
            case value::TypeTags::bsonUndefined:
                kb.appendUndefined();
                break;
            case value::TypeTags::bsonJavascript:
                kb.appendCode(value::getBsonJavascriptView(val));
                break;
            case value::TypeTags::Date: {
                auto milliseconds = value::bitcastTo<int64_t>(val);
                auto duration = stdx::chrono::duration<int64_t, std::milli>(milliseconds);
                auto date = Date_t::fromDurationSinceEpoch(duration);
                kb.appendDate(date);
                break;
            }
            case value::TypeTags::Timestamp: {
                Timestamp ts{value::bitcastTo<uint64_t>(val)};
                kb.appendTimestamp(ts);
                break;
            }
            case value::TypeTags::MinKey: {
                BSONObjBuilder bob;
                bob.appendMinKey("");
                kb.appendBSONElement(bob.obj().firstElement());
                break;
            }
            case value::TypeTags::MaxKey: {
                BSONObjBuilder bob;
                bob.appendMaxKey("");
                kb.appendBSONElement(bob.obj().firstElement());
                break;
            }
            case value::TypeTags::bsonArray: {
                BSONObj bson{value::getRawPointerView(val)};
                if (collator) {
                    kb.appendArray(BSONArray(BSONObj(bson)), stringTransformFn);
                } else {
                    kb.appendArray(BSONArray(BSONObj(bson)));
                }
                break;
            }
            case value::TypeTags::Array:
            case value::TypeTags::ArraySet:
            case value::TypeTags::ArrayMultiSet: {
                value::ArrayEnumerator enumerator{tag, val};
                BSONArrayBuilder arrayBuilder;
                bson::convertToBsonArr(arrayBuilder, enumerator);
                if (collator) {
                    kb.appendArray(arrayBuilder.arr(), stringTransformFn);
                } else {
                    kb.appendArray(arrayBuilder.arr());
                }
                break;
            }
            case value::TypeTags::bsonObject: {
                BSONObj bson{value::getRawPointerView(val)};
                if (collator) {
                    kb.appendObject(bson, stringTransformFn);
                } else {
                    kb.appendObject(bson);
                }
                break;
            }
            case value::TypeTags::Object: {
                BSONObjBuilder objBuilder;
                bson::convertToBsonObj(objBuilder, value::getObjectView(val));
                if (collator) {
                    kb.appendObject(objBuilder.obj(), stringTransformFn);
                } else {
                    kb.appendObject(objBuilder.obj());
                }
                break;
            }
            case value::TypeTags::ObjectId: {
                auto oid = OID::from(value::getObjectIdView(val)->data());
                kb.appendOID(oid);
                break;
            }
            case value::TypeTags::bsonObjectId: {
                auto oid = OID::from(value::getRawPointerView(val));
                kb.appendOID(oid);
                break;
            }
            case value::TypeTags::bsonSymbol: {
                auto symbolView = value::getStringOrSymbolView(tag, val);
                kb.appendSymbol(symbolView);
                break;
            }
            case value::TypeTags::bsonBinData: {
                auto data = value::getBSONBinData(tag, val);
                auto length = static_cast<int>(value::getBSONBinDataSize(tag, val));
                auto type = value::getBSONBinDataSubtype(tag, val);
                BSONBinData binData{data, length, type};
                kb.appendBinData(binData);
                break;
            }
            case value::TypeTags::bsonRegex: {
                auto sbeRegex = value::getBsonRegexView(val);
                BSONRegEx regex{sbeRegex.pattern, sbeRegex.flags};
                kb.appendRegex(regex);
                break;
            }
            case value::TypeTags::bsonCodeWScope: {
                auto sbeCodeWScope = value::getBsonCodeWScopeView(val);
                BSONCodeWScope codeWScope{sbeCodeWScope.code, BSONObj(sbeCodeWScope.scope)};
                kb.appendCodeWString(codeWScope);
                break;
            }
            case value::TypeTags::bsonDBPointer: {
                auto dbPointer = value::getBsonDBPointerView(val);
                BSONDBRef dbRef{dbPointer.ns, OID::from(dbPointer.id)};
                kb.appendDBRef(dbRef);
                break;
            }
            default:
                uasserted(4822802, str::stream() << "Unsuppored key string type: " << tagCopy);
                break;
        }
    }

    kb.appendDiscriminator(ksDiscriminator);

    return {true,
            value::TypeTags::ksValue,
            value::bitcastFrom<key_string::Value*>(new key_string::Value(kb.release()))};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinNewKeyString(ArityType arity) {
    tassert(6333000,
            str::stream() << "Unsupported number of arguments passed to ks(): " << arity,
            arity >= 3 && arity <= Ordering::kMaxCompoundIndexKeys + 3);
    return genericNewKeyString(arity);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinCollNewKeyString(ArityType arity) {
    tassert(6511500,
            str::stream() << "Unsupported number of arguments passed to collKs(): " << arity,
            arity >= 4 && arity <= Ordering::kMaxCompoundIndexKeys + 4);

    auto [_, tagCollator, valCollator] = getFromStack(arity - 1u);
    if (tagCollator != value::TypeTags::collator) {
        return {false, value::TypeTags::Nothing, 0};
    }
    auto collator = value::getCollatorView(valCollator);
    return genericNewKeyString(arity - 1u, collator);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAbs(ArityType arity) {
    invariant(arity == 1);

    auto [_, tagOperand, valOperand] = getFromStack(0);

    return genericAbs(tagOperand, valOperand);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinCeil(ArityType arity) {
    invariant(arity == 1);

    auto [_, tagOperand, valOperand] = getFromStack(0);

    return genericCeil(tagOperand, valOperand);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinFloor(ArityType arity) {
    invariant(arity == 1);

    auto [_, tagOperand, valOperand] = getFromStack(0);

    return genericFloor(tagOperand, valOperand);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinExp(ArityType arity) {
    invariant(arity == 1);

    auto [_, tagOperand, valOperand] = getFromStack(0);

    return genericExp(tagOperand, valOperand);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinLn(ArityType arity) {
    invariant(arity == 1);

    auto [_, tagOperand, valOperand] = getFromStack(0);

    return genericLn(tagOperand, valOperand);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinLog10(ArityType arity) {
    invariant(arity == 1);

    auto [_, tagOperand, valOperand] = getFromStack(0);

    return genericLog10(tagOperand, valOperand);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinSqrt(ArityType arity) {
    invariant(arity == 1);

    auto [_, tagOperand, valOperand] = getFromStack(0);

    return genericSqrt(tagOperand, valOperand);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinPow(ArityType arity) {
    invariant(arity == 2);
    auto [baseOwned, baseTag, baseValue] = getFromStack(0);
    auto [exponentOwned, exponentTag, exponentValue] = getFromStack(1);

    return genericPow(baseTag, baseValue, exponentTag, exponentValue);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAddToArray(ArityType arity) {
    auto [ownAgg, tagAgg, valAgg] = getFromStack(0);
    auto [tagField, valField] = moveOwnedFromStack(1);
    value::ValueGuard guardField{tagField, valField};

    // Create a new array is it does not exist yet.
    if (tagAgg == value::TypeTags::Nothing) {
        ownAgg = true;
        std::tie(tagAgg, valAgg) = value::makeNewArray();
    } else {
        // Take ownership of the accumulator.
        topStack(false, value::TypeTags::Nothing, 0);
    }
    value::ValueGuard guard{tagAgg, valAgg};

    invariant(ownAgg && tagAgg == value::TypeTags::Array);
    auto arr = value::getArrayView(valAgg);

    // Push back the value. Note that array will ignore Nothing.
    arr->push_back(tagField, valField);
    guardField.reset();

    guard.reset();
    return {ownAgg, tagAgg, valAgg};
}

// The value being accumulated is an SBE array that contains an integer and the accumulated array,
// where the integer is the total size in bytes of the elements in the array.
FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAddToArrayCapped(ArityType arity) {
    auto [ownArr, tagArr, valArr] = getFromStack(0);
    auto [tagNewElem, valNewElem] = moveOwnedFromStack(1);
    value::ValueGuard guardNewElem{tagNewElem, valNewElem};
    auto [_, tagSizeCap, valSizeCap] = getFromStack(2);

    if (tagSizeCap != value::TypeTags::NumberInt32) {
        auto [ownArr, tagArr, valArr] = getFromStack(0);
        topStack(false, value::TypeTags::Nothing, 0);
        return {ownArr, tagArr, valArr};
    }
    const int32_t sizeCap = value::bitcastTo<int32_t>(valSizeCap);

    // Create a new array to hold size and added elements, if is it does not exist yet.
    if (tagArr == value::TypeTags::Nothing) {
        ownArr = true;
        std::tie(tagArr, valArr) = value::makeNewArray();
        auto arr = value::getArrayView(valArr);

        auto [tagAccArr, valAccArr] = value::makeNewArray();

        // The order is important! The accumulated array should be at index
        // AggArrayWithSize::kValues, and the size should be at index
        // AggArrayWithSize::kSizeOfValues.
        arr->push_back(tagAccArr, valAccArr);
        arr->push_back(value::TypeTags::NumberInt64, value::bitcastFrom<int64_t>(0));
    } else {
        // Take ownership of the accumulator.
        topStack(false, value::TypeTags::Nothing, 0);
    }
    value::ValueGuard guardArr{tagArr, valArr};

    invariant(ownArr && tagArr == value::TypeTags::Array);
    auto arr = value::getArrayView(valArr);
    invariant(arr->size() == static_cast<size_t>(AggArrayWithSize::kLast));

    // Check that the accumulated size of the array doesn't exceed the limit.
    int elemSize = value::getApproximateSize(tagNewElem, valNewElem);
    auto [tagAccSize, valAccSize] =
        arr->getAt(static_cast<size_t>(AggArrayWithSize::kSizeOfValues));
    invariant(tagAccSize == value::TypeTags::NumberInt64);
    const int64_t currentSize = value::bitcastTo<int64_t>(valAccSize);
    const int64_t newSize = currentSize + elemSize;

    auto [tagAccArr, valAccArr] = arr->getAt(static_cast<size_t>(AggArrayWithSize::kValues));
    auto accArr = value::getArrayView(valAccArr);
    if (newSize >= static_cast<int64_t>(sizeCap)) {
        uasserted(ErrorCodes::ExceededMemoryLimit,
                  str::stream() << "Used too much memory for a single array. Memory limit: "
                                << sizeCap << " bytes. The array contains " << accArr->size()
                                << " elements and is of size " << currentSize
                                << " bytes. The element being added has size " << elemSize
                                << " bytes.");
    }

    arr->setAt(static_cast<size_t>(AggArrayWithSize::kSizeOfValues),
               value::TypeTags::NumberInt64,
               value::bitcastFrom<int64_t>(newSize));

    // Push back the new value. Note that array will ignore Nothing.
    guardNewElem.reset();
    accArr->push_back(tagNewElem, valNewElem);

    guardArr.reset();
    return {ownArr, tagArr, valArr};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinMergeObjects(ArityType arity) {
    auto [_, tagField, valField] = getFromStack(1);
    // Move the incoming accumulator state from the stack. Given that we are now the owner of the
    // state we are free to do any in-place update as we see fit.
    auto [tagAgg, valAgg] = moveOwnedFromStack(0);

    value::ValueGuard guard{tagAgg, valAgg};
    // Create a new object if it does not exist yet.
    if (tagAgg == value::TypeTags::Nothing) {
        std::tie(tagAgg, valAgg) = value::makeNewObject();
    }

    invariant(tagAgg == value::TypeTags::Object);

    // If our field is nothing or null or it's not an object, return the accumulator state.
    if (tagField == value::TypeTags::Nothing || tagField == value::TypeTags::Null ||
        (tagField != value::TypeTags::Object && tagField != value::TypeTags::bsonObject)) {
        guard.reset();
        return {true, tagAgg, valAgg};
    }

    auto obj = value::getObjectView(valAgg);

    StringMap<std::pair<value::TypeTags, value::Value>> currObjMap;
    for (auto currObjEnum = value::ObjectEnumerator{tagField, valField}; !currObjEnum.atEnd();
         currObjEnum.advance()) {
        currObjMap[currObjEnum.getFieldName()] = currObjEnum.getViewOfValue();
    }

    // Process the accumulated fields and if a field within the current object already exists
    // within the existing accuultor, we set the value of that field within the accumuator to the
    // value contained within the current object. Preserves the order of existing fields in the
    // accumulator
    for (size_t idx = 0, numFields = obj->size(); idx < numFields; ++idx) {
        auto it = currObjMap.find(obj->field(idx));
        if (it != currObjMap.end()) {
            auto [currObjTag, currObjVal] = it->second;
            auto [currObjTagCopy, currObjValCopy] = value::copyValue(currObjTag, currObjVal);
            obj->setAt(idx, currObjTagCopy, currObjValCopy);
            currObjMap.erase(it);
        }
    }

    // Copy the remaining fields of the current object being processed to the
    // accumulator. Fields that were already present in the accumulated fields
    // have been set already. Preserves the relative order of the new fields
    for (auto currObjEnum = value::ObjectEnumerator{tagField, valField}; !currObjEnum.atEnd();
         currObjEnum.advance()) {
        auto it = currObjMap.find(currObjEnum.getFieldName());
        if (it != currObjMap.end()) {
            auto [currObjTag, currObjVal] = it->second;
            auto [currObjTagCopy, currObjValCopy] = value::copyValue(currObjTag, currObjVal);
            obj->push_back(currObjEnum.getFieldName(), currObjTagCopy, currObjValCopy);
        }
    }

    guard.reset();
    return {true, tagAgg, valAgg};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAddToSet(ArityType arity) {
    auto [ownAgg, tagAgg, valAgg] = getFromStack(0);
    auto [tagField, valField] = moveOwnedFromStack(1);
    value::ValueGuard guardField{tagField, valField};

    // Create a new array is it does not exist yet.
    if (tagAgg == value::TypeTags::Nothing) {
        ownAgg = true;
        std::tie(tagAgg, valAgg) = value::makeNewArraySet();
    } else {
        // Take ownership of the accumulator.
        topStack(false, value::TypeTags::Nothing, 0);
    }
    value::ValueGuard guard{tagAgg, valAgg};

    invariant(ownAgg && tagAgg == value::TypeTags::ArraySet);
    auto arr = value::getArraySetView(valAgg);

    // Push back the value. Note that array will ignore Nothing.
    guardField.reset();
    arr->push_back(tagField, valField);

    guard.reset();
    return {ownAgg, tagAgg, valAgg};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::addToSetCappedImpl(
    value::TypeTags tagNewElem,
    value::Value valNewElem,
    int32_t sizeCap,
    CollatorInterface* collator) {
    value::ValueGuard guardNewElem{tagNewElem, valNewElem};
    auto [ownArr, tagArr, valArr] = getFromStack(0);

    // Create a new array is it does not exist yet.
    if (tagArr == value::TypeTags::Nothing) {
        ownArr = true;
        std::tie(tagArr, valArr) = value::makeNewArray();
        auto arr = value::getArrayView(valArr);

        auto [tagAccSet, valAccSet] = value::makeNewArraySet(collator);

        // The order is important! The accumulated array should be at index
        // AggArrayWithSize::kValues, and the size should be at index
        // AggArrayWithSize::kSizeOfValues.
        arr->push_back(tagAccSet, valAccSet);
        arr->push_back(value::TypeTags::NumberInt64, value::bitcastFrom<int64_t>(0));
    } else {
        // Take ownership of the accumulator.
        topStack(false, value::TypeTags::Nothing, 0);
    }
    value::ValueGuard guardArr{tagArr, valArr};

    invariant(ownArr && tagArr == value::TypeTags::Array);
    auto arr = value::getArrayView(valArr);
    invariant(arr->size() == static_cast<size_t>(AggArrayWithSize::kLast));

    // Check that the accumulated size of the set won't exceed the limit after adding the new value,
    // and if so, add the value.
    auto [tagAccSet, valAccSet] = arr->getAt(static_cast<size_t>(AggArrayWithSize::kValues));
    invariant(tagAccSet == value::TypeTags::ArraySet);
    auto accSet = value::getArraySetView(valAccSet);
    if (!accSet->values().contains({tagNewElem, valNewElem})) {
        auto elemSize = value::getApproximateSize(tagNewElem, valNewElem);
        auto [tagAccSize, valAccSize] =
            arr->getAt(static_cast<size_t>(AggArrayWithSize::kSizeOfValues));
        invariant(tagAccSize == value::TypeTags::NumberInt64);
        const int64_t currentSize = value::bitcastTo<int64_t>(valAccSize);
        int64_t newSize = currentSize + elemSize;

        if (newSize >= static_cast<int64_t>(sizeCap)) {
            uasserted(ErrorCodes::ExceededMemoryLimit,
                      str::stream()
                          << "Used too much memory for a single set. Memory limit: " << sizeCap
                          << " bytes. The set contains " << accSet->size()
                          << " elements and is of size " << currentSize
                          << " bytes. The element being added has size " << elemSize << " bytes.");
        }

        arr->setAt(static_cast<size_t>(AggArrayWithSize::kSizeOfValues),
                   value::TypeTags::NumberInt64,
                   value::bitcastFrom<int64_t>(newSize));

        // Push back the new value. Note that array will ignore Nothing.
        guardNewElem.reset();
        accSet->push_back(tagNewElem, valNewElem);
    }

    guardArr.reset();
    return {ownArr, tagArr, valArr};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAddToSetCapped(ArityType arity) {
    auto [tagNewElem, valNewElem] = moveOwnedFromStack(1);
    value::ValueGuard guardNewElem{tagNewElem, valNewElem};
    auto [_, tagSizeCap, valSizeCap] = getFromStack(2);

    if (tagSizeCap != value::TypeTags::NumberInt32) {
        auto [ownArr, tagArr, valArr] = getFromStack(0);
        topStack(false, value::TypeTags::Nothing, 0);
        return {ownArr, tagArr, valArr};
    }

    guardNewElem.reset();
    return addToSetCappedImpl(
        tagNewElem, valNewElem, value::bitcastTo<int32_t>(valSizeCap), nullptr /*collator*/);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinCollAddToSet(ArityType arity) {
    auto [ownAgg, tagAgg, valAgg] = getFromStack(0);
    auto [ownColl, tagColl, valColl] = getFromStack(1);
    auto [tagField, valField] = moveOwnedFromStack(2);
    value::ValueGuard guardField{tagField, valField};

    // If the collator is Nothing or if it's some unexpected type, don't push back the value
    // and just return the accumulator.
    if (tagColl != value::TypeTags::collator) {
        topStack(false, value::TypeTags::Nothing, 0);
        return {ownAgg, tagAgg, valAgg};
    }

    // Create a new array is it does not exist yet.
    if (tagAgg == value::TypeTags::Nothing) {
        ownAgg = true;
        std::tie(tagAgg, valAgg) = value::makeNewArraySet(value::getCollatorView(valColl));
    } else {
        // Take ownership of the accumulator.
        topStack(false, value::TypeTags::Nothing, 0);
    }
    value::ValueGuard guard{tagAgg, valAgg};

    invariant(ownAgg && tagAgg == value::TypeTags::ArraySet);
    auto arr = value::getArraySetView(valAgg);

    // Push back the value. Note that array will ignore Nothing.
    guardField.reset();
    arr->push_back(tagField, valField);

    guard.reset();
    return {ownAgg, tagAgg, valAgg};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinCollAddToSetCapped(
    ArityType arity) {
    auto [_1, tagColl, valColl] = getFromStack(1);
    auto [tagNewElem, valNewElem] = moveOwnedFromStack(2);
    value::ValueGuard guardNewElem{tagNewElem, valNewElem};
    auto [_2, tagSizeCap, valSizeCap] = getFromStack(3);

    // If the collator is Nothing or if it's some unexpected type, don't push back the value
    // and just return the accumulator.
    if (tagColl != value::TypeTags::collator || tagSizeCap != value::TypeTags::NumberInt32) {
        auto [ownArr, tagArr, valArr] = getFromStack(0);
        topStack(false, value::TypeTags::Nothing, 0);
        return {ownArr, tagArr, valArr};
    }

    guardNewElem.reset();
    return addToSetCappedImpl(tagNewElem,
                              valNewElem,
                              value::bitcastTo<int32_t>(valSizeCap),
                              value::getCollatorView(valColl));
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinRunJsPredicate(ArityType arity) {
    invariant(arity == 2);

    auto [predicateOwned, predicateType, predicateValue] = getFromStack(0);
    auto [inputOwned, inputType, inputValue] = getFromStack(1);

    if (predicateType != value::TypeTags::jsFunction || !value::isObject(inputType)) {
        return {false, value::TypeTags::Nothing, value::bitcastFrom<int64_t>(0)};
    }

    BSONObj obj;
    if (inputType == value::TypeTags::Object) {
        BSONObjBuilder objBuilder;
        bson::convertToBsonObj(objBuilder, value::getObjectView(inputValue));
        obj = objBuilder.obj();
    } else if (inputType == value::TypeTags::bsonObject) {
        obj = BSONObj(value::getRawPointerView(inputValue));
    } else {
        MONGO_UNREACHABLE;
    }

    auto predicate = value::getJsFunctionView(predicateValue);
    auto predicateResult = predicate->runAsPredicate(obj);
    return {false, value::TypeTags::Boolean, value::bitcastFrom<bool>(predicateResult)};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinReplaceOne(ArityType arity) {
    invariant(arity == 3);

    auto [ownedInputStr, typeTagInputStr, valueInputStr] = getFromStack(0);
    auto [ownedFindStr, typeTagFindStr, valueFindStr] = getFromStack(1);
    auto [ownedReplacementStr, typeTagReplacementStr, valueReplacementStr] = getFromStack(2);

    if (!value::isString(typeTagInputStr) || !value::isString(typeTagFindStr) ||
        !value::isString(typeTagReplacementStr)) {
        return {false, value::TypeTags::Nothing, 0};
    }

    auto input = value::getStringView(typeTagInputStr, valueInputStr);
    auto find = value::getStringView(typeTagFindStr, valueFindStr);
    auto replacement = value::getStringView(typeTagReplacementStr, valueReplacementStr);

    // If find string is empty, return nothing, since an empty find will match every position in a
    // string.
    if (find.empty()) {
        return {false, value::TypeTags::Nothing, 0};
    }

    // If find string is not found, return the original string.
    size_t startIndex = input.find(find);
    if (startIndex == std::string::npos) {
        topStack(false, value::TypeTags::Nothing, 0);
        return {ownedInputStr, typeTagInputStr, valueInputStr};
    }

    StringBuilder output;
    size_t endIndex = startIndex + find.size();
    output << input.substr(0, startIndex);
    output << replacement;
    output << input.substr(endIndex);

    auto strData = output.stringData();
    auto [outputStrTypeTag, outputStrValue] = sbe::value::makeNewString(strData);
    return {true, outputStrTypeTag, outputStrValue};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinDoubleDoubleSum(ArityType arity) {
    invariant(arity >= 1);

    value::TypeTags resultTag = value::TypeTags::NumberInt32;
    bool haveDate = false;

    // Sweep across all tags and pick the result type.
    for (ArityType idx = 0; idx < arity; ++idx) {
        auto [own, tag, val] = getFromStack(idx);
        if (tag == value::TypeTags::Date) {
            if (haveDate) {
                uassert(4848404, "only one date allowed in an $add expression", !haveDate);
            }
            // Date is a simple 64 bit integer.
            haveDate = true;
            tag = value::TypeTags::NumberInt64;
        }
        if (value::isNumber(tag)) {
            resultTag = value::getWidestNumericalType(resultTag, tag);
        } else if (tag == value::TypeTags::Nothing || tag == value::TypeTags::Null) {
            // What to do about null and nothing?
            return {false, value::TypeTags::Nothing, 0};
        } else {
            // What to do about non-numeric types like arrays and objects?
            return {false, value::TypeTags::Nothing, 0};
        }
    }

    if (resultTag == value::TypeTags::NumberDecimal) {
        Decimal128 sum;
        for (ArityType idx = 0; idx < arity; ++idx) {
            auto [own, tag, val] = getFromStack(idx);
            if (tag == value::TypeTags::Date) {
                sum = sum.add(Decimal128(value::bitcastTo<int64_t>(val)));
            } else {
                sum = sum.add(value::numericCast<Decimal128>(tag, val));
            }
        }
        if (haveDate) {
            return {false, value::TypeTags::Date, value::bitcastFrom<int64_t>(sum.toLong())};
        } else {
            auto [tag, val] = value::makeCopyDecimal(sum);
            return {true, tag, val};
        }
    } else {
        DoubleDoubleSummation sum;
        for (ArityType idx = 0; idx < arity; ++idx) {
            auto [own, tag, val] = getFromStack(idx);
            if (tag == value::TypeTags::NumberInt32) {
                sum.addInt(value::numericCast<int32_t>(tag, val));
            } else if (tag == value::TypeTags::NumberInt64) {
                sum.addLong(value::numericCast<int64_t>(tag, val));
            } else if (tag == value::TypeTags::NumberDouble) {
                sum.addDouble(value::numericCast<double>(tag, val));
            } else if (tag == value::TypeTags::Date) {
                sum.addLong(value::bitcastTo<int64_t>(val));
            }
        }
        if (haveDate) {
            uassert(ErrorCodes::Overflow, "date overflow in $add", sum.fitsLong());
            return {false, value::TypeTags::Date, value::bitcastFrom<int64_t>(sum.getLong())};
        } else {
            switch (resultTag) {
                case value::TypeTags::NumberInt32: {
                    auto result = sum.getLong();
                    if (sum.fitsLong() && result >= std::numeric_limits<int32_t>::min() &&
                        result <= std::numeric_limits<int32_t>::max()) {
                        return {false,
                                value::TypeTags::NumberInt32,
                                value::bitcastFrom<int32_t>(result)};
                    }
                    [[fallthrough]];  // To the larger type
                }
                case value::TypeTags::NumberInt64: {
                    if (sum.fitsLong()) {
                        return {false,
                                value::TypeTags::NumberInt64,
                                value::bitcastFrom<int64_t>(sum.getLong())};
                    }
                    [[fallthrough]];  // To the larger type.
                }
                case value::TypeTags::NumberDouble: {
                    return {false,
                            value::TypeTags::NumberDouble,
                            value::bitcastFrom<double>(sum.getDouble())};
                }
                default:
                    MONGO_UNREACHABLE;
            }
        }
    }
    return {false, value::TypeTags::Nothing, 0};
}

/**
 * A helper for the builtinDate method. The formal parameters yearOrWeekYear and monthOrWeek carry
 * values depending on wether the date is a year-month-day or ISOWeekYear.
 */
using DateFn = std::function<Date_t(
    TimeZone, long long, long long, long long, long long, long long, long long, long long)>;
FastTuple<bool, value::TypeTags, value::Value> builtinDateHelper(
    DateFn computeDateFn,
    FastTuple<bool, value::TypeTags, value::Value> tzdb,
    FastTuple<bool, value::TypeTags, value::Value> yearOrWeekYear,
    FastTuple<bool, value::TypeTags, value::Value> monthOrWeek,
    FastTuple<bool, value::TypeTags, value::Value> day,
    FastTuple<bool, value::TypeTags, value::Value> hour,
    FastTuple<bool, value::TypeTags, value::Value> minute,
    FastTuple<bool, value::TypeTags, value::Value> second,
    FastTuple<bool, value::TypeTags, value::Value> millisecond,
    FastTuple<bool, value::TypeTags, value::Value> timezone) {

    auto [ownedTzdb, typeTagTzdb, valueTzdb] = tzdb;
    auto [ownedYearOrWeekYear, typeTagYearOrWeekYear, valueYearOrWeekYear] = yearOrWeekYear;
    auto [ownedMonthOrWeek, typeTagMonthOrWeek, valueMonthOrWeek] = monthOrWeek;
    auto [ownedDay, typeTagDay, valueDay] = day;
    auto [ownedHr, typeTagHr, valueHr] = hour;
    auto [ownedMin, typeTagMin, valueMin] = minute;
    auto [ownedSec, typeTagSec, valueSec] = second;
    auto [ownedMillis, typeTagMillis, valueMillis] = millisecond;
    auto [ownedTz, typeTagTz, valueTz] = timezone;

    if (typeTagTzdb != value::TypeTags::timeZoneDB || !value::isNumber(typeTagYearOrWeekYear) ||
        !value::isNumber(typeTagMonthOrWeek) || !value::isNumber(typeTagDay) ||
        !value::isNumber(typeTagHr) || !value::isNumber(typeTagMin) ||
        !value::isNumber(typeTagSec) || !value::isNumber(typeTagMillis) ||
        !value::isString(typeTagTz)) {
        return {false, value::TypeTags::Nothing, 0};
    }

    auto timeZoneDB = value::getTimeZoneDBView(valueTzdb);
    invariant(timeZoneDB);

    auto tzString = value::getStringView(typeTagTz, valueTz);
    const auto tz = tzString == "" ? timeZoneDB->utcZone() : timeZoneDB->getTimeZone(tzString);

    auto date =
        computeDateFn(tz,
                      value::numericCast<int64_t>(typeTagYearOrWeekYear, valueYearOrWeekYear),
                      value::numericCast<int64_t>(typeTagMonthOrWeek, valueMonthOrWeek),
                      value::numericCast<int64_t>(typeTagDay, valueDay),
                      value::numericCast<int64_t>(typeTagHr, valueHr),
                      value::numericCast<int64_t>(typeTagMin, valueMin),
                      value::numericCast<int64_t>(typeTagSec, valueSec),
                      value::numericCast<int64_t>(typeTagMillis, valueMillis));
    return {false, value::TypeTags::Date, value::bitcastFrom<int64_t>(date.asInt64())};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinDate(ArityType arity) {
    auto timeZoneDBTuple = getFromStack(0);
    auto yearTuple = getFromStack(1);
    auto monthTuple = getFromStack(2);
    auto dayTuple = getFromStack(3);
    auto hourTuple = getFromStack(4);
    auto minuteTuple = getFromStack(5);
    auto secondTuple = getFromStack(6);
    auto millisTuple = getFromStack(7);
    auto timezoneTuple = getFromStack(8);

    return builtinDateHelper(
        [](TimeZone tz,
           long long year,
           long long month,
           long long day,
           long long hour,
           long long min,
           long long sec,
           long long millis) -> Date_t {
            return tz.createFromDateParts(year, month, day, hour, min, sec, millis);
        },
        timeZoneDBTuple,
        yearTuple,
        monthTuple,
        dayTuple,
        hourTuple,
        minuteTuple,
        secondTuple,
        millisTuple,
        timezoneTuple);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinDateToString(ArityType arity) {
    invariant(arity == 4);

    auto [timezoneDBOwn, timezoneDBTag, timezoneDBValue] = getFromStack(0);
    if (timezoneDBTag != value::TypeTags::timeZoneDB) {
        return {false, value::TypeTags::Nothing, 0};
    }
    auto timezoneDB = value::getTimeZoneDBView(timezoneDBValue);

    // Get date.
    auto [dateOwn, dateTag, dateValue] = getFromStack(1);
    if (!coercibleToDate(dateTag)) {
        return {false, value::TypeTags::Nothing, 0};
    }
    auto date = getDate(dateTag, dateValue);

    // Get format.
    auto [formatOwn, formatTag, formatValue] = getFromStack(2);
    if (!value::isString(formatTag)) {
        return {false, value::TypeTags::Nothing, 0};
    }
    auto formatString = value::getStringView(formatTag, formatValue);
    if (!TimeZone::isValidToStringFormat(formatString)) {
        return {false, value::TypeTags::Nothing, 0};
    }

    // Get timezone.
    auto [timezoneOwn, timezoneTag, timezoneValue] = getFromStack(3);
    if (!isValidTimezone(timezoneTag, timezoneValue, timezoneDB)) {
        return {false, value::TypeTags::Nothing, 0};
    }
    auto timezone = getTimezone(timezoneTag, timezoneValue, timezoneDB);

    StringBuilder formatted;

    auto status = timezone.outputDateWithFormat(formatted, formatString, date);

    if (status != Status::OK()) {
        return {false, value::TypeTags::Nothing, 0};
    }

    auto [strTag, strValue] = sbe::value::makeNewString(formatted.str());
    return {true, strTag, strValue};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinDateFromString(ArityType arity) {
    auto [timezoneDBOwn, timezoneDBTag, timezoneDBValue] = getFromStack(0);
    if (timezoneDBTag != value::TypeTags::timeZoneDB) {
        return {false, value::TypeTags::Nothing, 0};
    }
    auto timezoneDB = value::getTimeZoneDBView(timezoneDBValue);

    // Get parameter tuples from stack.
    auto [dateStringOwn, dateStringTag, dateStringValue] = getFromStack(1);
    auto [timezoneOwn, timezoneTag, timezoneValue] = getFromStack(2);

    auto timezone = getTimezone(timezoneTag, timezoneValue, timezoneDB);

    // Attempt to get the date from the string. This may throw a ConversionFailure error.
    Date_t date;
    auto dateString = value::getStringView(dateStringTag, dateStringValue);
    if (arity == 3) {
        // Format wasn't specified, so we call fromString without it.
        date = timezoneDB->fromString(dateString, timezone);
    } else {
        // Fetch format from the stack, validate it, and call fromString with it.
        auto [formatOwn, formatTag, formatValue] = getFromStack(3);
        if (!value::isString(formatTag)) {
            return {false, value::TypeTags::Nothing, 0};
        }
        auto formatString = value::getStringView(formatTag, formatValue);
        if (!TimeZone::isValidFromStringFormat(formatString)) {
            return {false, value::TypeTags::Nothing, 0};
        }
        date = timezoneDB->fromString(dateString, timezone, formatString);
    }

    return {true, value::TypeTags::Date, value::bitcastFrom<int64_t>(date.toMillisSinceEpoch())};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinDateFromStringNoThrow(
    ArityType arity) {
    try {
        return builtinDateFromString(arity);
    } catch (const ExceptionFor<ErrorCodes::ConversionFailure>&) {
        // Upon error, we return Nothing and let the caller decide whether to raise an error.
        return {false, value::TypeTags::Nothing, 0};
    }
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::dateTrunc(value::TypeTags dateTag,
                                                                   value::Value dateValue,
                                                                   TimeUnit unit,
                                                                   int64_t binSize,
                                                                   TimeZone timezone,
                                                                   DayOfWeek startOfWeek) {
    // Get date.
    if (!coercibleToDate(dateTag)) {
        return {false, value::TypeTags::Nothing, 0};
    }
    auto date = getDate(dateTag, dateValue);

    auto truncatedDate = truncateDate(date, unit, binSize, timezone, startOfWeek);
    return {false,
            value::TypeTags::Date,
            value::bitcastFrom<int64_t>(truncatedDate.toMillisSinceEpoch())};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinDateWeekYear(ArityType arity) {
    auto timeZoneDBTuple = getFromStack(0);
    auto yearTuple = getFromStack(1);
    auto weekTuple = getFromStack(2);
    auto dayTuple = getFromStack(3);
    auto hourTuple = getFromStack(4);
    auto minuteTuple = getFromStack(5);
    auto secondTuple = getFromStack(6);
    auto millisTuple = getFromStack(7);
    auto timezoneTuple = getFromStack(8);

    return builtinDateHelper(
        [](TimeZone tz,
           long long year,
           long long month,
           long long day,
           long long hour,
           long long min,
           long long sec,
           long long millis) -> Date_t {
            return tz.createFromIso8601DateParts(year, month, day, hour, min, sec, millis);
        },
        timeZoneDBTuple,
        yearTuple,
        weekTuple,
        dayTuple,
        hourTuple,
        minuteTuple,
        secondTuple,
        millisTuple,
        timezoneTuple);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinDateToParts(ArityType arity) {
    auto [timezoneDBOwn, timezoneDBTag, timezoneDBVal] = getFromStack(0);
    if (timezoneDBTag != value::TypeTags::timeZoneDB) {
        return {false, value::TypeTags::Nothing, 0};
    }
    auto timezoneDB = value::getTimeZoneDBView(timezoneDBVal);
    auto [dateOwn, dateTag, dateVal] = getFromStack(1);

    // Get timezone.
    auto [timezoneOwn, timezoneTag, timezoneVal] = getFromStack(2);
    if (!value::isString(timezoneTag)) {
        return {false, value::TypeTags::Nothing, 0};
    }
    TimeZone timezone = getTimezone(timezoneTag, timezoneVal, timezoneDB);

    // Get date.
    if (dateTag != value::TypeTags::Date && dateTag != value::TypeTags::Timestamp &&
        dateTag != value::TypeTags::ObjectId && dateTag != value::TypeTags::bsonObjectId) {
        return {false, value::TypeTags::Nothing, 0};
    }
    Date_t date = getDate(dateTag, dateVal);

    // Get date parts.
    auto dateParts = timezone.dateParts(date);
    auto [dateObjTag, dateObjVal] = value::makeNewObject();
    value::ValueGuard guard{dateObjTag, dateObjVal};
    auto dateObj = value::getObjectView(dateObjVal);
    dateObj->reserve(7);
    dateObj->push_back("year", value::TypeTags::NumberInt32, dateParts.year);
    dateObj->push_back("month", value::TypeTags::NumberInt32, dateParts.month);
    dateObj->push_back("day", value::TypeTags::NumberInt32, dateParts.dayOfMonth);
    dateObj->push_back("hour", value::TypeTags::NumberInt32, dateParts.hour);
    dateObj->push_back("minute", value::TypeTags::NumberInt32, dateParts.minute);
    dateObj->push_back("second", value::TypeTags::NumberInt32, dateParts.second);
    dateObj->push_back("millisecond", value::TypeTags::NumberInt32, dateParts.millisecond);
    guard.reset();
    return {true, dateObjTag, dateObjVal};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinIsoDateToParts(ArityType arity) {
    auto [timezoneDBOwn, timezoneDBTag, timezoneDBVal] = getFromStack(0);
    if (timezoneDBTag != value::TypeTags::timeZoneDB) {
        return {false, value::TypeTags::Nothing, 0};
    }
    auto timezoneDB = value::getTimeZoneDBView(timezoneDBVal);
    auto [dateOwn, dateTag, dateVal] = getFromStack(1);

    // Get timezone.
    auto [timezoneOwn, timezoneTag, timezoneVal] = getFromStack(2);
    if (!value::isString(timezoneTag)) {
        return {false, value::TypeTags::Nothing, 0};
    }
    TimeZone timezone = getTimezone(timezoneTag, timezoneVal, timezoneDB);

    // Get date.
    if (dateTag != value::TypeTags::Date && dateTag != value::TypeTags::Timestamp &&
        dateTag != value::TypeTags::ObjectId && dateTag != value::TypeTags::bsonObjectId) {
        return {false, value::TypeTags::Nothing, 0};
    }
    Date_t date = getDate(dateTag, dateVal);

    // Get date parts.
    auto dateParts = timezone.dateIso8601Parts(date);
    auto [dateObjTag, dateObjVal] = value::makeNewObject();
    value::ValueGuard guard{dateObjTag, dateObjVal};
    auto dateObj = value::getObjectView(dateObjVal);
    dateObj->reserve(7);
    dateObj->push_back("isoWeekYear", value::TypeTags::NumberInt32, dateParts.year);
    dateObj->push_back("isoWeek", value::TypeTags::NumberInt32, dateParts.weekOfYear);
    dateObj->push_back("isoDayOfWeek", value::TypeTags::NumberInt32, dateParts.dayOfWeek);
    dateObj->push_back("hour", value::TypeTags::NumberInt32, dateParts.hour);
    dateObj->push_back("minute", value::TypeTags::NumberInt32, dateParts.minute);
    dateObj->push_back("second", value::TypeTags::NumberInt32, dateParts.second);
    dateObj->push_back("millisecond", value::TypeTags::NumberInt32, dateParts.millisecond);
    guard.reset();
    return {true, dateObjTag, dateObjVal};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinDayOfYear(ArityType arity) {
    invariant(arity == 3 || arity == 2);

    auto [dateOwn, dateTag, dateValue] = getFromStack(0);
    if (arity == 3) {
        auto [timezoneDBOwn, timezoneDBTag, timezoneDBValue] = getFromStack(1);
        auto [timezoneOwn, timezoneTag, timezoneValue] = getFromStack(2);
        return genericDayOfYear(
            timezoneDBTag, timezoneDBValue, dateTag, dateValue, timezoneTag, timezoneValue);
    } else {
        auto [timezoneOwn, timezoneTag, timezoneValue] = getFromStack(1);
        return genericDayOfYear(dateTag, dateValue, timezoneTag, timezoneValue);
    }
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinDayOfMonth(ArityType arity) {
    invariant(arity == 3 || arity == 2);

    auto [dateOwn, dateTag, dateValue] = getFromStack(0);
    if (arity == 3) {
        auto [timezoneDBOwn, timezoneDBTag, timezoneDBValue] = getFromStack(1);
        auto [timezoneOwn, timezoneTag, timezoneValue] = getFromStack(2);
        return genericDayOfMonth(
            timezoneDBTag, timezoneDBValue, dateTag, dateValue, timezoneTag, timezoneValue);
    } else {
        auto [timezoneOwn, timezoneTag, timezoneValue] = getFromStack(1);
        return genericDayOfMonth(dateTag, dateValue, timezoneTag, timezoneValue);
    }
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinDayOfWeek(ArityType arity) {
    invariant(arity == 3 || arity == 2);

    auto [dateOwn, dateTag, dateValue] = getFromStack(0);
    if (arity == 3) {
        auto [timezoneDBOwn, timezoneDBTag, timezoneDBValue] = getFromStack(1);
        auto [timezoneOwn, timezoneTag, timezoneValue] = getFromStack(2);
        return genericDayOfWeek(
            timezoneDBTag, timezoneDBValue, dateTag, dateValue, timezoneTag, timezoneValue);
    } else {
        auto [timezoneOwn, timezoneTag, timezoneValue] = getFromStack(1);
        return genericDayOfWeek(dateTag, dateValue, timezoneTag, timezoneValue);
    }
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinYear(ArityType arity) {
    invariant(arity == 3 || arity == 2);

    auto [dateOwn, dateTag, dateValue] = getFromStack(0);
    if (arity == 3) {
        auto [timezoneDBOwn, timezoneDBTag, timezoneDBValue] = getFromStack(1);
        auto [timezoneOwn, timezoneTag, timezoneValue] = getFromStack(2);
        return genericYear(
            timezoneDBTag, timezoneDBValue, dateTag, dateValue, timezoneTag, timezoneValue);
    } else {
        auto [timezoneOwn, timezoneTag, timezoneValue] = getFromStack(1);
        return genericYear(dateTag, dateValue, timezoneTag, timezoneValue);
    }
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinMonth(ArityType arity) {
    invariant(arity == 3 || arity == 2);

    auto [dateOwn, dateTag, dateValue] = getFromStack(0);
    if (arity == 3) {
        auto [timezoneDBOwn, timezoneDBTag, timezoneDBValue] = getFromStack(1);
        auto [timezoneOwn, timezoneTag, timezoneValue] = getFromStack(2);
        return genericMonth(
            timezoneDBTag, timezoneDBValue, dateTag, dateValue, timezoneTag, timezoneValue);
    } else {
        auto [timezoneOwn, timezoneTag, timezoneValue] = getFromStack(1);
        return genericMonth(dateTag, dateValue, timezoneTag, timezoneValue);
    }
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinHour(ArityType arity) {
    invariant(arity == 3 || arity == 2);

    auto [dateOwn, dateTag, dateValue] = getFromStack(0);
    if (arity == 3) {
        auto [timezoneDBOwn, timezoneDBTag, timezoneDBValue] = getFromStack(1);
        auto [timezoneOwn, timezoneTag, timezoneValue] = getFromStack(2);
        return genericHour(
            timezoneDBTag, timezoneDBValue, dateTag, dateValue, timezoneTag, timezoneValue);
    } else {
        auto [timezoneOwn, timezoneTag, timezoneValue] = getFromStack(1);
        return genericHour(dateTag, dateValue, timezoneTag, timezoneValue);
    }
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinMinute(ArityType arity) {
    invariant(arity == 3 || arity == 2);

    auto [dateOwn, dateTag, dateValue] = getFromStack(0);
    if (arity == 3) {
        auto [timezoneDBOwn, timezoneDBTag, timezoneDBValue] = getFromStack(1);
        auto [timezoneOwn, timezoneTag, timezoneValue] = getFromStack(2);
        return genericMinute(
            timezoneDBTag, timezoneDBValue, dateTag, dateValue, timezoneTag, timezoneValue);
    } else {
        auto [timezoneOwn, timezoneTag, timezoneValue] = getFromStack(1);
        return genericMinute(dateTag, dateValue, timezoneTag, timezoneValue);
    }
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinSecond(ArityType arity) {
    invariant(arity == 3 || arity == 2);

    auto [dateOwn, dateTag, dateValue] = getFromStack(0);
    if (arity == 3) {
        auto [timezoneDBOwn, timezoneDBTag, timezoneDBValue] = getFromStack(1);
        auto [timezoneOwn, timezoneTag, timezoneValue] = getFromStack(2);
        return genericSecond(
            timezoneDBTag, timezoneDBValue, dateTag, dateValue, timezoneTag, timezoneValue);
    } else {
        auto [timezoneOwn, timezoneTag, timezoneValue] = getFromStack(1);
        return genericSecond(dateTag, dateValue, timezoneTag, timezoneValue);
    }
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinMillisecond(ArityType arity) {
    invariant(arity == 3 || arity == 2);

    auto [dateOwn, dateTag, dateValue] = getFromStack(0);
    if (arity == 3) {
        auto [timezoneDBOwn, timezoneDBTag, timezoneDBValue] = getFromStack(1);
        auto [timezoneOwn, timezoneTag, timezoneValue] = getFromStack(2);
        return genericMillisecond(
            timezoneDBTag, timezoneDBValue, dateTag, dateValue, timezoneTag, timezoneValue);
    } else {
        auto [timezoneOwn, timezoneTag, timezoneValue] = getFromStack(1);
        return genericMillisecond(dateTag, dateValue, timezoneTag, timezoneValue);
    }
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinWeek(ArityType arity) {
    invariant(arity == 3 || arity == 2);

    auto [dateOwn, dateTag, dateValue] = getFromStack(0);
    if (arity == 3) {
        auto [timezoneDBOwn, timezoneDBTag, timezoneDBValue] = getFromStack(1);
        auto [timezoneOwn, timezoneTag, timezoneValue] = getFromStack(2);
        return genericWeek(
            timezoneDBTag, timezoneDBValue, dateTag, dateValue, timezoneTag, timezoneValue);
    } else {
        auto [timezoneOwn, timezoneTag, timezoneValue] = getFromStack(1);
        return genericWeek(dateTag, dateValue, timezoneTag, timezoneValue);
    }
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinISOWeekYear(ArityType arity) {
    invariant(arity == 3 || arity == 2);

    auto [dateOwn, dateTag, dateValue] = getFromStack(0);
    if (arity == 3) {
        auto [timezoneDBOwn, timezoneDBTag, timezoneDBValue] = getFromStack(1);
        auto [timezoneOwn, timezoneTag, timezoneValue] = getFromStack(2);
        return genericISOWeekYear(
            timezoneDBTag, timezoneDBValue, dateTag, dateValue, timezoneTag, timezoneValue);
    } else {
        auto [timezoneOwn, timezoneTag, timezoneValue] = getFromStack(1);
        return genericISOWeekYear(dateTag, dateValue, timezoneTag, timezoneValue);
    }
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinISODayOfWeek(ArityType arity) {
    invariant(arity == 3 || arity == 2);

    auto [dateOwn, dateTag, dateValue] = getFromStack(0);
    if (arity == 3) {
        auto [timezoneDBOwn, timezoneDBTag, timezoneDBValue] = getFromStack(1);
        auto [timezoneOwn, timezoneTag, timezoneValue] = getFromStack(2);
        return genericISODayOfWeek(
            timezoneDBTag, timezoneDBValue, dateTag, dateValue, timezoneTag, timezoneValue);
    } else {
        auto [timezoneOwn, timezoneTag, timezoneValue] = getFromStack(1);
        return genericISODayOfWeek(dateTag, dateValue, timezoneTag, timezoneValue);
    }
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinISOWeek(ArityType arity) {
    invariant(arity == 3 || arity == 2);

    auto [dateOwn, dateTag, dateValue] = getFromStack(0);
    if (arity == 3) {
        auto [timezoneDBOwn, timezoneDBTag, timezoneDBValue] = getFromStack(1);
        auto [timezoneOwn, timezoneTag, timezoneValue] = getFromStack(2);
        return genericISOWeek(
            timezoneDBTag, timezoneDBValue, dateTag, dateValue, timezoneTag, timezoneValue);
    } else {
        auto [timezoneOwn, timezoneTag, timezoneValue] = getFromStack(1);
        return genericISOWeek(dateTag, dateValue, timezoneTag, timezoneValue);
    }
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinBitTestPosition(ArityType arity) {
    invariant(arity == 3);

    auto [ownedMask, maskTag, maskValue] = getFromStack(0);
    auto [ownedInput, valueTag, value] = getFromStack(1);

    // Carries a flag to indicate the desired testing behavior this was invoked under. The testing
    // behavior is used to determine if we need to bail out of the bit position comparison early in
    // the depending if a bit is found to be set or unset.
    auto [_, tagBitTestBehavior, valueBitTestBehavior] = getFromStack(2);
    invariant(tagBitTestBehavior == value::TypeTags::NumberInt32);

    if (!value::isArray(maskTag) || !value::isBinData(valueTag)) {
        return {false, value::TypeTags::Nothing, 0};
    }

    auto bitPositions = value::getArrayView(maskValue);
    auto binDataSize = static_cast<int64_t>(value::getBSONBinDataSize(valueTag, value));
    auto binData = value::getBSONBinData(valueTag, value);
    auto bitTestBehavior = BitTestBehavior{value::bitcastTo<int32_t>(valueBitTestBehavior)};

    auto isBitSet = false;
    for (size_t idx = 0; idx < bitPositions->size(); ++idx) {
        auto [tagBitPosition, valueBitPosition] = bitPositions->getAt(idx);
        auto bitPosition = value::bitcastTo<int64_t>(valueBitPosition);
        if (bitPosition >= binDataSize * 8) {
            // If position to test is longer than the data to test against, zero-extend.
            isBitSet = false;
        } else {
            // Convert the bit position to a byte position within a byte. Note that byte positions
            // start at position 0 in the document's value BinData array representation, and bit
            // positions start at the least significant bit.
            auto byteIdx = bitPosition / 8;
            auto currentBit = bitPosition % 8;
            auto currentByte = binData[byteIdx];

            isBitSet = currentByte & (1 << currentBit);
        }

        // Bail out early if we succeed with the any case or fail with the all case. To do this, we
        // negate a test to determine if we need to continue looping over the bit position list. So
        // the first part of the disjunction checks when a bit is set and the test is invoked by the
        // AllSet or AnyClear expressions. The second test checks if a bit isn't set and we are
        // checking the AllClear or the AnySet cases.
        if (!((isBitSet &&
               (bitTestBehavior == BitTestBehavior::AllSet ||
                bitTestBehavior == BitTestBehavior::AnyClear)) ||
              (!isBitSet &&
               (bitTestBehavior == BitTestBehavior::AllClear ||
                bitTestBehavior == BitTestBehavior::AnySet)))) {
            return {false,
                    value::TypeTags::Boolean,
                    value::bitcastFrom<bool>(bitTestBehavior == BitTestBehavior::AnyClear ||
                                             bitTestBehavior == BitTestBehavior::AnySet)};
        }
    }
    return {false,
            value::TypeTags::Boolean,
            value::bitcastFrom<bool>(bitTestBehavior == BitTestBehavior::AllSet ||
                                     bitTestBehavior == BitTestBehavior::AllClear)};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinBitTestZero(ArityType arity) {
    invariant(arity == 2);
    auto [maskOwned, maskTag, maskValue] = getFromStack(0);
    auto [inputOwned, inputTag, inputValue] = getFromStack(1);

    if ((maskTag != value::TypeTags::NumberInt32 && maskTag != value::TypeTags::NumberInt64) ||
        (inputTag != value::TypeTags::NumberInt32 && inputTag != value::TypeTags::NumberInt64)) {
        return {false, value::TypeTags::Nothing, 0};
    }

    auto maskNum = value::numericCast<int64_t>(maskTag, maskValue);
    auto inputNum = value::numericCast<int64_t>(inputTag, inputValue);
    auto result = (maskNum & inputNum) == 0;
    return {false, value::TypeTags::Boolean, value::bitcastFrom<bool>(result)};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinBitTestMask(ArityType arity) {
    invariant(arity == 2);
    auto [maskOwned, maskTag, maskValue] = getFromStack(0);
    auto [inputOwned, inputTag, inputValue] = getFromStack(1);

    if ((maskTag != value::TypeTags::NumberInt32 && maskTag != value::TypeTags::NumberInt64) ||
        (inputTag != value::TypeTags::NumberInt32 && inputTag != value::TypeTags::NumberInt64)) {
        return {false, value::TypeTags::Nothing, 0};
    }

    auto maskNum = value::numericCast<int64_t>(maskTag, maskValue);
    auto inputNum = value::numericCast<int64_t>(inputTag, inputValue);
    auto result = (maskNum & inputNum) == maskNum;
    return {false, value::TypeTags::Boolean, value::bitcastFrom<bool>(result)};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinBsonSize(ArityType arity) {
    auto [_, tagOperand, valOperand] = getFromStack(0);

    if (tagOperand == value::TypeTags::Object) {
        BSONObjBuilder objBuilder;
        bson::convertToBsonObj(objBuilder, value::getObjectView(valOperand));
        int32_t sz = objBuilder.done().objsize();
        return {false, value::TypeTags::NumberInt32, value::bitcastFrom<int32_t>(sz)};
    } else if (tagOperand == value::TypeTags::bsonObject) {
        auto beginObj = value::getRawPointerView(valOperand);
        int32_t sz = ConstDataView(beginObj).read<LittleEndian<int32_t>>();
        return {false, value::TypeTags::NumberInt32, value::bitcastFrom<int32_t>(sz)};
    }
    return {false, value::TypeTags::Nothing, 0};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinStrLenBytes(ArityType arity) {
    invariant(arity == 1);

    auto [_, operandTag, operandVal] = getFromStack(0);

    if (value::isString(operandTag)) {
        auto str = value::getStringView(operandTag, operandVal);
        auto strLenBytes = str.size();
        uassert(5155801,
                "string length could not be represented as an int.",
                strLenBytes <= std::numeric_limits<int>::max());
        return {false, value::TypeTags::NumberInt32, strLenBytes};
    }
    return {false, value::TypeTags::Nothing, 0};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinToUpper(ArityType arity) {
    auto [_, operandTag, operandVal] = getFromStack(0);

    if (value::isString(operandTag)) {
        auto [strTag, strVal] = value::copyValue(operandTag, operandVal);
        auto buf = value::getRawStringView(strTag, strVal);
        auto range = std::make_pair(buf, buf + value::getStringLength(strTag, strVal));
        boost::to_upper(range);
        return {true, strTag, strVal};
    }
    return {false, value::TypeTags::Nothing, 0};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinToLower(ArityType arity) {
    auto [_, operandTag, operandVal] = getFromStack(0);

    if (value::isString(operandTag)) {
        auto [strTag, strVal] = value::copyValue(operandTag, operandVal);
        auto buf = value::getRawStringView(strTag, strVal);
        auto range = std::make_pair(buf, buf + value::getStringLength(strTag, strVal));
        boost::to_lower(range);
        return {true, strTag, strVal};
    }
    return {false, value::TypeTags::Nothing, 0};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinCoerceToBool(ArityType arity) {
    auto [operandOwned, operandTag, operandVal] = getFromStack(0);

    auto [tag, val] = value::coerceToBool(operandTag, operandVal);

    return {false, tag, val};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinCoerceToString(ArityType arity) {
    auto [operandOwn, operandTag, operandVal] = getFromStack(0);

    if (value::isString(operandTag)) {
        topStack(false, value::TypeTags::Nothing, 0);
        return {operandOwn, operandTag, operandVal};
    }

    if (operandTag == value::TypeTags::bsonSymbol) {
        // Values of type StringBig and Values of type bsonSymbol have identical representations,
        // so we can simply take ownership of the argument, change the type tag to StringBig, and
        // return it.
        topStack(false, value::TypeTags::Nothing, 0);
        return {operandOwn, value::TypeTags::StringBig, operandVal};
    }

    switch (operandTag) {
        case value::TypeTags::NumberInt32: {
            std::string str = str::stream() << value::bitcastTo<int32_t>(operandVal);
            auto [strTag, strVal] = value::makeNewString(str);
            return {true, strTag, strVal};
        }
        case value::TypeTags::NumberInt64: {
            std::string str = str::stream() << value::bitcastTo<int64_t>(operandVal);
            auto [strTag, strVal] = value::makeNewString(str);
            return {true, strTag, strVal};
        }
        case value::TypeTags::NumberDouble: {
            std::string str = str::stream() << value::bitcastTo<double>(operandVal);
            auto [strTag, strVal] = value::makeNewString(str);
            return {true, strTag, strVal};
        }
        case value::TypeTags::NumberDecimal: {
            std::string str = value::bitcastTo<Decimal128>(operandVal).toString();
            auto [strTag, strVal] = value::makeNewString(str);
            return {true, strTag, strVal};
        }
        case value::TypeTags::Date: {
            std::string str = str::stream()
                << TimeZoneDatabase::utcZone().formatDate(
                       kIsoFormatStringZ,
                       Date_t::fromMillisSinceEpoch(value::bitcastTo<int64_t>(operandVal)));
            auto [strTag, strVal] = value::makeNewString(str);
            return {true, strTag, strVal};
        }
        case value::TypeTags::Timestamp: {
            Timestamp ts{value::bitcastTo<uint64_t>(operandVal)};
            auto [strTag, strVal] = value::makeNewString(ts.toString());
            return {true, strTag, strVal};
        }
        case value::TypeTags::Null: {
            auto [strTag, strVal] = value::makeNewString("");
            return {true, strTag, strVal};
        }
        default:
            break;
    }
    return {false, value::TypeTags::Nothing, 0};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAcos(ArityType arity) {
    auto [_, operandTag, operandValue] = getFromStack(0);
    return genericAcos(operandTag, operandValue);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAcosh(ArityType arity) {
    auto [_, operandTag, operandValue] = getFromStack(0);
    return genericAcosh(operandTag, operandValue);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAsin(ArityType arity) {
    auto [_, operandTag, operandValue] = getFromStack(0);
    return genericAsin(operandTag, operandValue);
}
FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAsinh(ArityType arity) {
    auto [_, operandTag, operandValue] = getFromStack(0);
    return genericAsinh(operandTag, operandValue);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAtan(ArityType arity) {
    auto [_, operandTag, operandValue] = getFromStack(0);
    return genericAtan(operandTag, operandValue);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAtanh(ArityType arity) {
    auto [_, operandTag, operandValue] = getFromStack(0);
    return genericAtanh(operandTag, operandValue);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAtan2(ArityType arity) {
    auto [owned1, operandTag1, operandValue1] = getFromStack(0);
    auto [owned2, operandTag2, operandValue2] = getFromStack(1);
    return genericAtan2(operandTag1, operandValue1, operandTag2, operandValue2);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinCos(ArityType arity) {
    auto [_, operandTag, operandValue] = getFromStack(0);
    return genericCos(operandTag, operandValue);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinCosh(ArityType arity) {
    auto [_, operandTag, operandValue] = getFromStack(0);
    return genericCosh(operandTag, operandValue);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinDegreesToRadians(ArityType arity) {
    auto [_, operandTag, operandValue] = getFromStack(0);
    return genericDegreesToRadians(operandTag, operandValue);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinRadiansToDegrees(ArityType arity) {
    auto [_, operandTag, operandValue] = getFromStack(0);
    return genericRadiansToDegrees(operandTag, operandValue);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinSin(ArityType arity) {
    auto [_, operandTag, operandValue] = getFromStack(0);
    return genericSin(operandTag, operandValue);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinSinh(ArityType arity) {
    auto [_, operandTag, operandValue] = getFromStack(0);
    return genericSinh(operandTag, operandValue);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinTan(ArityType arity) {
    auto [_, operandTag, operandValue] = getFromStack(0);
    return genericTan(operandTag, operandValue);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinTanh(ArityType arity) {
    auto [_, operandTag, operandValue] = getFromStack(0);
    return genericTanh(operandTag, operandValue);
}

/**
 * Converts a number to int32 assuming the input fits the range. This is used for $round "place"
 * argument, which is checked to be a whole number between -20 and 100, but could still be a
 * non-int32 type.
 */
int32_t ByteCode::convertNumericToInt32(const value::TypeTags tag, const value::Value val) {
    switch (tag) {
        case value::TypeTags::NumberInt32: {
            return value::bitcastTo<int32_t>(val);
        }
        case value::TypeTags::NumberInt64: {
            return static_cast<int32_t>(value::bitcastTo<int64_t>(val));
        }
        case value::TypeTags::NumberDouble: {
            return static_cast<int32_t>(value::bitcastTo<double>(val));
        }
        case value::TypeTags::NumberDecimal: {
            Decimal128 dec = value::bitcastTo<Decimal128>(val);
            return dec.toInt(Decimal128::kRoundTiesToEven);
        }
        default:
            MONGO_UNREACHABLE;
    }
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::genericRoundTrunc(
    std::string funcName,
    Decimal128::RoundingMode roundingMode,
    int32_t place,
    value::TypeTags numTag,
    value::Value numVal) {

    // Construct 10^-precisionValue, which will be used as the quantize reference. This is passed to
    // decimal.quantize() to indicate the precision of our rounding.
    const auto quantum = Decimal128(0LL, Decimal128::kExponentBias - place, 0LL, 1LL);

    switch (numTag) {
        case value::TypeTags::NumberDecimal: {
            auto dec = value::bitcastTo<Decimal128>(numVal);
            if (!dec.isInfinite()) {
                dec = dec.quantize(quantum, roundingMode);
            }
            auto [resultTag, resultValue] = value::makeCopyDecimal(dec);
            return {true, resultTag, resultValue};
        }
        case value::TypeTags::NumberDouble: {
            auto asDec = Decimal128(value::bitcastTo<double>(numVal), Decimal128::kRoundTo34Digits);
            if (!asDec.isInfinite()) {
                asDec = asDec.quantize(quantum, roundingMode);
            }
            return {
                false, value::TypeTags::NumberDouble, value::bitcastFrom<double>(asDec.toDouble())};
        }
        case value::TypeTags::NumberInt32:
        case value::TypeTags::NumberInt64: {
            if (place >= 0) {
                return {false, numTag, numVal};
            }
            auto numericArgll = numTag == value::TypeTags::NumberInt32
                ? static_cast<int64_t>(value::bitcastTo<int32_t>(numVal))
                : value::bitcastTo<int64_t>(numVal);
            auto out = Decimal128(numericArgll).quantize(quantum, roundingMode);
            uint32_t flags = 0;
            auto outll = out.toLong(&flags);
            uassert(5155302,
                    "Invalid conversion to long during " + funcName + ".",
                    !Decimal128::hasFlag(flags, Decimal128::kInvalid));
            if (numTag == value::TypeTags::NumberInt64 ||
                outll > std::numeric_limits<int32_t>::max()) {
                // Even if the original was an int to begin with - it has to be a long now.
                return {false, value::TypeTags::NumberInt64, value::bitcastFrom<int64_t>(outll)};
            }
            return {false,
                    value::TypeTags::NumberInt32,
                    value::bitcastFrom<int32_t>(static_cast<int32_t>(outll))};
        }
        default:
            return {false, value::TypeTags::Nothing, 0};
    }
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::scalarRoundTrunc(
    std::string funcName, Decimal128::RoundingMode roundingMode, ArityType arity) {
    invariant(arity == 1 || arity == 2);
    int32_t place = 0;
    const auto [_, numTag, numVal] = getFromStack(0);
    if (arity == 2) {
        const auto [placeOwn, placeTag, placeVal] = getFromStack(1);
        if (!value::isNumber(placeTag)) {
            return {false, value::TypeTags::Nothing, 0};
        }
        place = convertNumericToInt32(placeTag, placeVal);
    }

    return genericRoundTrunc(funcName, roundingMode, place, numTag, numVal);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinTrunc(ArityType arity) {
    return scalarRoundTrunc("$trunc", Decimal128::kRoundTowardZero, arity);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinRound(ArityType arity) {
    return scalarRoundTrunc("$round", Decimal128::kRoundTiesToEven, arity);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinConcat(ArityType arity) {
    StringBuilder result;
    for (ArityType idx = 0; idx < arity; ++idx) {
        auto [_, tag, value] = getFromStack(idx);
        if (!value::isString(tag)) {
            return {false, value::TypeTags::Nothing, 0};
        }
        result << sbe::value::getStringView(tag, value);
    }

    auto [strTag, strValue] = sbe::value::makeNewString(result.str());
    return {true, strTag, strValue};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinConcatArrays(ArityType arity) {
    auto [resTag, resVal] = value::makeNewArray();
    value::ValueGuard resGuard{resTag, resVal};
    auto resView = value::getArrayView(resVal);

    for (ArityType idx = 0; idx < arity; ++idx) {
        auto [_, tag, val] = getFromStack(idx);
        if (!value::isArray(tag)) {
            return {false, value::TypeTags::Nothing, 0};
        }

        value::arrayForEach(tag, val, [&](value::TypeTags elTag, value::Value elVal) {
            auto [copyTag, copyVal] = value::copyValue(elTag, elVal);
            resView->push_back(copyTag, copyVal);
        });
    }

    resGuard.reset();

    return {true, resTag, resVal};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinTrim(ArityType arity,
                                                                     bool trimLeft,
                                                                     bool trimRight) {
    auto [ownedChars, tagChars, valChars] = getFromStack(1);
    auto [ownedInput, tagInput, valInput] = getFromStack(0);

    if (!value::isString(tagInput)) {
        return {false, value::TypeTags::Nothing, 0};
    }

    // Nullish 'chars' indicates that it was not provided and the default whitespace characters will
    // be used.
    auto replacementChars = !value::isNullish(tagChars)
        ? str_trim_utils::extractCodePointsFromChars(value::getStringView(tagChars, valChars))
        : str_trim_utils::kDefaultTrimWhitespaceChars;
    auto inputString = value::getStringView(tagInput, valInput);

    auto [strTag, strValue] = sbe::value::makeNewString(
        str_trim_utils::doTrim(inputString, replacementChars, trimLeft, trimRight));
    return {true, strTag, strValue};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggConcatArraysCapped(
    ArityType arity) {
    auto [ownArr, tagArr, valArr] = getFromStack(0);
    auto [tagNewElem, valNewElem] = moveOwnedFromStack(1);
    value::ValueGuard guardNewElem{tagNewElem, valNewElem};
    auto [_, tagSizeCap, valSizeCap] = getFromStack(2);

    tassert(7039508,
            "'cap' parameter must be a 32-bit int",
            tagSizeCap == value::TypeTags::NumberInt32);
    const int32_t sizeCap = value::bitcastTo<int32_t>(valSizeCap);

    // We expect the new value we are adding to the accumulator to be a two-element array where
    // the first element is the array to concatenate and the second value is the corresponding size.
    tassert(7039512, "expected value of type 'Array'", tagNewElem == value::TypeTags::Array);
    auto newArr = value::getArrayView(valNewElem);
    tassert(7039527,
            "array had unexpected size",
            newArr->size() == static_cast<size_t>(AggArrayWithSize::kLast));

    // Create a new array to hold size and added elements, if is it does not exist yet.
    if (tagArr == value::TypeTags::Nothing) {
        ownArr = true;
        std::tie(tagArr, valArr) = value::makeNewArray();
        auto arr = value::getArrayView(valArr);

        auto [tagAccArr, valAccArr] = value::makeNewArray();

        // The order is important! The accumulated array should be at index
        // AggArrayWithSize::kValues, and the size should be at index
        // AggArrayWithSize::kSizeOfValues.
        arr->push_back(tagAccArr, valAccArr);
        arr->push_back(value::TypeTags::NumberInt64, value::bitcastFrom<int64_t>(0));
    } else {
        // Take ownership of the accumulator.
        topStack(false, value::TypeTags::Nothing, 0);
    }

    tassert(7039513, "expected array to be owned", ownArr);
    value::ValueGuard accumulatorGuard{tagArr, valArr};
    tassert(7039514, "expected accumulator to have type 'Array'", tagArr == value::TypeTags::Array);
    auto arr = value::getArrayView(valArr);
    tassert(7039515,
            "accumulator was array of unexpected size",
            arr->size() == static_cast<size_t>(AggArrayWithSize::kLast));

    // Check that the accumulated size after concatentation won't exceed the limit.
    {
        auto [tagAccSize, valAccSize] =
            arr->getAt(static_cast<size_t>(AggArrayWithSize::kSizeOfValues));
        auto [tagNewSize, valNewSize] =
            newArr->getAt(static_cast<size_t>(AggArrayWithSize::kSizeOfValues));
        tassert(7039516, "expected 64-bit int", tagAccSize == value::TypeTags::NumberInt64);
        tassert(7039517, "expected 64-bit int", tagNewSize == value::TypeTags::NumberInt64);
        const int64_t currentSize = value::bitcastTo<int64_t>(valAccSize);
        const int64_t newSize = value::bitcastTo<int64_t>(valNewSize);
        const int64_t totalSize = currentSize + newSize;

        if (totalSize >= static_cast<int64_t>(sizeCap)) {
            uasserted(ErrorCodes::ExceededMemoryLimit,
                      str::stream() << "Used too much memory for a single array. Memory limit: "
                                    << sizeCap << ". Concatentating array of " << arr->size()
                                    << " elements and " << currentSize << " bytes with array of "
                                    << newArr->size() << " elements and " << newSize << " bytes.");
        }

        // We are still under the size limit. Set the new total size in the accumulator.
        arr->setAt(static_cast<size_t>(AggArrayWithSize::kSizeOfValues),
                   value::TypeTags::NumberInt64,
                   value::bitcastFrom<int64_t>(totalSize));
    }

    auto [tagAccArr, valAccArr] = arr->getAt(static_cast<size_t>(AggArrayWithSize::kValues));
    tassert(7039518, "expected value of type 'Array'", tagAccArr == value::TypeTags::Array);
    auto accArr = value::getArrayView(valAccArr);

    auto [tagNewArray, valNewArray] = newArr->getAt(static_cast<size_t>(AggArrayWithSize::kValues));
    tassert(7039519, "expected value of type 'Array'", tagNewArray == value::TypeTags::Array);

    value::arrayForEach<true>(
        tagNewArray, valNewArray, [&](value::TypeTags elTag, value::Value elVal) {
            accArr->push_back(elTag, elVal);
        });


    accumulatorGuard.reset();
    return {ownArr, tagArr, valArr};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinIsMember(ArityType arity) {
    invariant(arity == 2);

    auto [inputOwned, inputTag_, inputVal_] = getFromStack(0);
    auto [arrOwned, arrTag, arrVal] = getFromStack(1);

    auto inputTag = inputTag_;
    auto inputVal = inputVal_;

    if (!value::isArray(arrTag) && arrTag != value::TypeTags::inListData) {
        return {false, value::TypeTags::Nothing, 0};
    }

    if (arrTag == value::TypeTags::inListData) {
        if (inputTag == value::TypeTags::Nothing) {
            return {false, value::TypeTags::Boolean, value::bitcastFrom<bool>(false)};
        }

        auto inListData = value::getInListDataView(arrVal);
        const bool found = inListData->contains(inputTag, inputVal);

        return {false, value::TypeTags::Boolean, value::bitcastFrom<bool>(found)};
    } else if (arrTag == value::TypeTags::ArraySet) {
        auto arrSet = value::getArraySetView(arrVal);
        auto& values = arrSet->values();

        const bool found = values.find({inputTag, inputVal}) != values.end();

        return {false, value::TypeTags::Boolean, value::bitcastFrom<bool>(found)};
    }

    const bool found =
        value::arrayAny(arrTag, arrVal, [&](value::TypeTags elemTag, value::Value elemVal) {
            auto [tag, val] = value::compareValue(inputTag, inputVal, elemTag, elemVal);
            if (tag == value::TypeTags::NumberInt32 && value::bitcastTo<int32_t>(val) == 0) {
                return true;
            }
            return false;
        });

    return {false, value::TypeTags::Boolean, value::bitcastFrom<bool>(found)};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinIndexOfBytes(ArityType arity) {
    auto [strOwn, strTag, strVal] = getFromStack(0);
    auto [substrOwn, substrTag, substrVal] = getFromStack(1);
    if ((!value::isString(strTag)) || (!value::isString(substrTag))) {
        return {false, value::TypeTags::Nothing, 0};
    }
    auto str = value::getStringView(strTag, strVal);
    auto substring = value::getStringView(substrTag, substrVal);
    int64_t startIndex = 0, endIndex = str.size();

    if (arity >= 3) {
        auto [startOwn, startTag, startVal] = getFromStack(2);
        if (startTag != value::TypeTags::NumberInt64) {
            return {false, value::TypeTags::Nothing, 0};
        }
        startIndex = value::bitcastTo<int64_t>(startVal);
        // Check index is positive.
        if (startIndex < 0) {
            return {false, value::TypeTags::Nothing, 0};
        }
        // Check for valid bounds.
        if (static_cast<size_t>(startIndex) > str.size()) {
            return {false, value::TypeTags::NumberInt32, value::bitcastFrom<int32_t>(-1)};
        }
    }
    if (arity >= 4) {
        auto [endOwn, endTag, endVal] = getFromStack(3);
        if (endTag != value::TypeTags::NumberInt64) {
            return {false, value::TypeTags::Nothing, 0};
        }
        endIndex = value::bitcastTo<int64_t>(endVal);
        // Check index is positive.
        if (endIndex < 0) {
            return {false, value::TypeTags::Nothing, 0};
        }
        // Check for valid bounds.
        if (endIndex < startIndex) {
            return {false, value::TypeTags::NumberInt32, value::bitcastFrom<int32_t>(-1)};
        }
    }
    auto index = str.substr(startIndex, endIndex - startIndex).find(substring);
    if (index != std::string::npos) {
        return {
            false, value::TypeTags::NumberInt32, value::bitcastFrom<int32_t>(startIndex + index)};
    }
    return {false, value::TypeTags::NumberInt32, value::bitcastFrom<int32_t>(-1)};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinIndexOfCP(ArityType arity) {
    auto [strOwn, strTag, strVal] = getFromStack(0);
    auto [substrOwn, substrTag, substrVal] = getFromStack(1);
    if ((!value::isString(strTag)) || (!value::isString(substrTag))) {
        return {false, value::TypeTags::Nothing, 0};
    }
    auto str = value::getStringView(strTag, strVal);
    auto substr = value::getStringView(substrTag, substrVal);
    int64_t startCodePointIndex = 0, endCodePointIndexArg = str.size();

    if (arity >= 3) {
        auto [startOwn, startTag, startVal] = getFromStack(2);
        if (startTag != value::TypeTags::NumberInt64) {
            return {false, value::TypeTags::Nothing, 0};
        }
        startCodePointIndex = value::bitcastTo<int64_t>(startVal);
        // Check index is positive.
        if (startCodePointIndex < 0) {
            return {false, value::TypeTags::Nothing, 0};
        }
        // Check for valid bounds.
        if (static_cast<size_t>(startCodePointIndex) > str.size()) {
            return {false, value::TypeTags::NumberInt32, value::bitcastFrom<int32_t>(-1)};
        }
    }
    if (arity >= 4) {
        auto [endOwn, endTag, endVal] = getFromStack(3);
        if (endTag != value::TypeTags::NumberInt64) {
            return {false, value::TypeTags::Nothing, 0};
        }
        endCodePointIndexArg = value::bitcastTo<int64_t>(endVal);
        // Check index is positive.
        if (endCodePointIndexArg < 0) {
            return {false, value::TypeTags::Nothing, 0};
        }
        // Check for valid bounds.
        if (endCodePointIndexArg < startCodePointIndex) {
            return {false, value::TypeTags::NumberInt32, value::bitcastFrom<int32_t>(-1)};
        }
    }

    // Handle edge case if both string and substring are empty strings.
    if (startCodePointIndex == 0 && str.empty() && substr.empty()) {
        return {true, value::TypeTags::NumberInt32, value::bitcastFrom<int32_t>(0)};
    }

    // Need to get byte indexes for start and end indexes.
    int64_t startByteIndex = 0, byteIndex = 0, codePointIndex;
    for (codePointIndex = 0; static_cast<size_t>(byteIndex) < str.size(); codePointIndex++) {
        if (codePointIndex == startCodePointIndex) {
            startByteIndex = byteIndex;
        }
        uassert(5075307,
                "$indexOfCP found bad UTF-8 in the input",
                !str::isUTF8ContinuationByte(str[byteIndex]));
        byteIndex += str::getCodePointLength(str[byteIndex]);
    }

    int64_t endCodePointIndex = std::min(codePointIndex, endCodePointIndexArg);
    byteIndex = startByteIndex;
    for (codePointIndex = startCodePointIndex; codePointIndex < endCodePointIndex;
         ++codePointIndex) {
        if (str.substr(byteIndex, substr.size()) == substr) {
            return {
                false, value::TypeTags::NumberInt32, value::bitcastFrom<int32_t>(codePointIndex)};
        }
        byteIndex += str::getCodePointLength(str[byteIndex]);
    }
    return {false, value::TypeTags::NumberInt32, value::bitcastFrom<int32_t>(-1)};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinIsTimeUnit(ArityType arity) {
    invariant(arity == 1);
    auto [timeUnitOwn, timeUnitTag, timeUnitValue] = getFromStack(0);
    if (!value::isString(timeUnitTag)) {
        return {false, value::TypeTags::Nothing, 0};
    }
    return {false,
            value::TypeTags::Boolean,
            value::bitcastFrom<bool>(
                isValidTimeUnit(value::getStringView(timeUnitTag, timeUnitValue)))};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinIsDayOfWeek(ArityType arity) {
    invariant(arity == 1);
    auto [dayOfWeekOwn, dayOfWeekTag, dayOfWeekValue] = getFromStack(0);
    if (!value::isString(dayOfWeekTag)) {
        return {false, value::TypeTags::Nothing, 0};
    }
    return {false,
            value::TypeTags::Boolean,
            value::bitcastFrom<bool>(
                isValidDayOfWeek(value::getStringView(dayOfWeekTag, dayOfWeekValue)))};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinIsTimezone(ArityType arity) {
    auto [timezoneDBOwn, timezoneDBTag, timezoneDBVal] = getFromStack(0);
    if (timezoneDBTag != value::TypeTags::timeZoneDB) {
        return {false, value::TypeTags::Nothing, 0};
    }
    auto timezoneDB = value::getTimeZoneDBView(timezoneDBVal);
    auto [timezoneOwn, timezoneTag, timezoneVal] = getFromStack(1);
    if (!value::isString(timezoneTag)) {
        return {false, value::TypeTags::Boolean, false};
    }
    auto timezoneStr = value::getStringView(timezoneTag, timezoneVal);
    if (timezoneDB->isTimeZoneIdentifier(timezoneStr)) {
        return {false, value::TypeTags::Boolean, true};
    }
    return {false, value::TypeTags::Boolean, false};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinIsValidToStringFormat(
    ArityType arity) {
    auto [formatOwn, formatTag, formatVal] = getFromStack(0);
    if (!value::isString(formatTag)) {
        return {false, value::TypeTags::Boolean, false};
    }
    auto formatStr = value::getStringView(formatTag, formatVal);
    if (TimeZone::isValidToStringFormat(formatStr)) {
        return {false, value::TypeTags::Boolean, true};
    }
    return {false, value::TypeTags::Boolean, false};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinValidateFromStringFormat(
    ArityType arity) {
    auto [formatOwn, formatTag, formatVal] = getFromStack(0);
    if (!value::isString(formatTag)) {
        return {false, value::TypeTags::Boolean, false};
    }
    auto formatStr = value::getStringView(formatTag, formatVal);
    TimeZone::validateFromStringFormat(formatStr);
    return {false, value::TypeTags::Boolean, true};
}

namespace {
FastTuple<bool, value::TypeTags, value::Value> setUnion(
    const std::vector<value::TypeTags>& argTags,
    const std::vector<value::Value>& argVals,
    const CollatorInterface* collator = nullptr) {
    auto [resTag, resVal] = value::makeNewArraySet(collator);
    value::ValueGuard resGuard{resTag, resVal};
    auto resView = value::getArraySetView(resVal);

    for (size_t idx = 0; idx < argVals.size(); ++idx) {
        auto argTag = argTags[idx];
        auto argVal = argVals[idx];

        value::arrayForEach(argTag, argVal, [&](value::TypeTags elTag, value::Value elVal) {
            auto [copyTag, copyVal] = value::copyValue(elTag, elVal);
            resView->push_back(copyTag, copyVal);
        });
    }
    resGuard.reset();
    return {true, resTag, resVal};
}

FastTuple<bool, value::TypeTags, value::Value> setIntersection(
    const std::vector<value::TypeTags>& argTags,
    const std::vector<value::Value>& argVals,
    const CollatorInterface* collator = nullptr) {
    auto intersectionMap =
        value::ValueMapType<size_t>{0, value::ValueHash(collator), value::ValueEq(collator)};

    auto [resTag, resVal] = value::makeNewArraySet(collator);
    value::ValueGuard resGuard{resTag, resVal};

    for (size_t idx = 0; idx < argVals.size(); ++idx) {
        auto tag = argTags[idx];
        auto val = argVals[idx];

        bool atLeastOneCommonElement = false;
        value::arrayForEach(tag, val, [&](value::TypeTags elTag, value::Value elVal) {
            if (idx == 0) {
                intersectionMap[{elTag, elVal}] = 1;
            } else {
                if (auto it = intersectionMap.find({elTag, elVal}); it != intersectionMap.end()) {
                    if (it->second == idx) {
                        it->second++;
                        atLeastOneCommonElement = true;
                    }
                }
            }
        });

        if (idx > 0 && !atLeastOneCommonElement) {
            resGuard.reset();
            return {true, resTag, resVal};
        }
    }

    auto resView = value::getArraySetView(resVal);
    for (auto&& [item, counter] : intersectionMap) {
        if (counter == argVals.size()) {
            auto [elTag, elVal] = item;
            auto [copyTag, copyVal] = value::copyValue(elTag, elVal);
            resView->push_back(copyTag, copyVal);
        }
    }

    resGuard.reset();
    return {true, resTag, resVal};
}

value::ValueSetType valueToSetHelper(value::TypeTags tag,
                                     value::Value value,
                                     const CollatorInterface* collator) {
    value::ValueSetType setValues(0, value::ValueHash(collator), value::ValueEq(collator));
    value::arrayForEach(tag, value, [&](value::TypeTags elemTag, value::Value elemVal) {
        setValues.insert({elemTag, elemVal});
    });
    return setValues;
}

FastTuple<bool, value::TypeTags, value::Value> setDifference(
    value::TypeTags lhsTag,
    value::Value lhsVal,
    value::TypeTags rhsTag,
    value::Value rhsVal,
    const CollatorInterface* collator = nullptr) {
    auto [resTag, resVal] = value::makeNewArraySet(collator);
    value::ValueGuard resGuard{resTag, resVal};
    auto resView = value::getArraySetView(resVal);

    auto setValuesSecondArg = valueToSetHelper(rhsTag, rhsVal, collator);

    value::arrayForEach(lhsTag, lhsVal, [&](value::TypeTags elTag, value::Value elVal) {
        if (setValuesSecondArg.count({elTag, elVal}) == 0) {
            auto [copyTag, copyVal] = value::copyValue(elTag, elVal);
            resView->push_back(copyTag, copyVal);
        }
    });

    resGuard.reset();
    return {true, resTag, resVal};
}

FastTuple<bool, value::TypeTags, value::Value> setEquals(
    const std::vector<value::TypeTags>& argTags,
    const std::vector<value::Value>& argVals,
    const CollatorInterface* collator = nullptr) {
    auto setValuesFirstArg = valueToSetHelper(argTags[0], argVals[0], collator);

    for (size_t idx = 1; idx < argVals.size(); ++idx) {
        auto setValuesOtherArg = valueToSetHelper(argTags[idx], argVals[idx], collator);
        if (setValuesFirstArg != setValuesOtherArg) {
            return {false, value::TypeTags::Boolean, false};
        }
    }

    return {false, value::TypeTags::Boolean, true};
}

FastTuple<bool, value::TypeTags, value::Value> setIsSubset(
    value::TypeTags lhsTag,
    value::Value lhsVal,
    value::TypeTags rhsTag,
    value::Value rhsVal,
    const CollatorInterface* collator = nullptr) {

    if (!value::isArray(lhsTag) || !value::isArray(rhsTag)) {
        return {false, value::TypeTags::Nothing, 0};
    }

    auto setValuesSecondArg = valueToSetHelper(rhsTag, rhsVal, collator);

    bool isSubset = true;
    value::arrayAny(lhsTag, lhsVal, [&](value::TypeTags elTag, value::Value elVal) {
        isSubset = (setValuesSecondArg.count({elTag, elVal}) > 0);
        return !isSubset;
    });

    return {false, value::TypeTags::Boolean, isSubset};
}
}  // namespace

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinCollSetUnion(ArityType arity) {
    invariant(arity >= 1);

    auto [_, collTag, collVal] = getFromStack(0);
    if (collTag != value::TypeTags::collator) {
        return {false, value::TypeTags::Nothing, 0};
    }

    std::vector<value::TypeTags> argTags;
    std::vector<value::Value> argVals;
    for (size_t idx = 1; idx < arity; ++idx) {
        auto [owned, tag, val] = getFromStack(idx);
        if (!value::isArray(tag)) {
            return {false, value::TypeTags::Nothing, 0};
        }

        argTags.push_back(tag);
        argVals.push_back(val);
    }

    return setUnion(argTags, argVals, value::getCollatorView(collVal));
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinSetUnion(ArityType arity) {
    std::vector<value::TypeTags> argTags;
    std::vector<value::Value> argVals;

    for (size_t idx = 0; idx < arity; ++idx) {
        auto [_, tag, val] = getFromStack(idx);
        if (!value::isArray(tag)) {
            return {false, value::TypeTags::Nothing, 0};
        }

        argTags.push_back(tag);
        argVals.push_back(val);
    }

    return setUnion(argTags, argVals);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::aggSetUnionCappedImpl(
    value::TypeTags tagNewElem,
    value::Value valNewElem,
    int32_t sizeCap,
    CollatorInterface* collator) {
    value::ValueGuard guardNewElem{tagNewElem, valNewElem};
    auto [ownAcc, tagAcc, valAcc] = getFromStack(0);

    // We expect the new value we are adding to the accumulator to be a two-element array where
    // the first element is the new set of values and the second value is the corresponding size.
    tassert(7039526, "expected value of type 'Array'", tagNewElem == value::TypeTags::Array);
    auto newArr = value::getArrayView(valNewElem);
    tassert(7039528,
            "array had unexpected size",
            newArr->size() == static_cast<size_t>(AggArrayWithSize::kLast));

    // Create a new array is it does not exist yet.
    if (tagAcc == value::TypeTags::Nothing) {
        ownAcc = true;
        std::tie(tagAcc, valAcc) = value::makeNewArray();
        auto accArray = value::getArrayView(valAcc);

        auto [tagAccSet, valAccSet] = value::makeNewArraySet(collator);

        // The order is important! The accumulated array should be at index
        // AggArrayWithSize::kValues, and the size should be at index
        // AggArrayWithSize::kSizeOfValues.
        accArray->push_back(tagAccSet, valAccSet);
        accArray->push_back(value::TypeTags::NumberInt64, value::bitcastFrom<int64_t>(0));
    } else {
        // Take ownership of the accumulator.
        topStack(false, value::TypeTags::Nothing, 0);
    }

    tassert(7039520, "expected accumulator value to be owned", ownAcc);
    value::ValueGuard guardArr{tagAcc, valAcc};

    tassert(
        7039521, "expected accumulator to be of type 'Array'", tagAcc == value::TypeTags::Array);
    auto accArray = value::getArrayView(valAcc);
    tassert(7039522,
            "array had unexpected size",
            accArray->size() == static_cast<size_t>(AggArrayWithSize::kLast));

    auto [tagAccArrSet, valAccArrSet] =
        accArray->getAt(static_cast<size_t>(AggArrayWithSize::kValues));
    tassert(
        7039523, "expected value of type 'ArraySet'", tagAccArrSet == value::TypeTags::ArraySet);
    auto accArrSet = value::getArraySetView(valAccArrSet);

    // Extract the current size of the accumulator. As we add elements to the set, we will increment
    // the current size accordingly and throw an exception if we ever exceed the size limit. We
    // cannot simply sum the two sizes, since the two sets could have a substantial intersection.
    auto [tagAccSize, valAccSize] =
        accArray->getAt(static_cast<size_t>(AggArrayWithSize::kSizeOfValues));
    tassert(7039524, "expected 64-bit int", tagAccSize == value::TypeTags::NumberInt64);
    int64_t currentSize = value::bitcastTo<int64_t>(valAccSize);

    auto [tagNewValSet, valNewValSet] =
        newArr->getAt(static_cast<size_t>(AggArrayWithSize::kValues));
    tassert(
        7039525, "expected value of type 'ArraySet'", tagNewValSet == value::TypeTags::ArraySet);


    value::arrayForEach<true>(
        tagNewValSet, valNewValSet, [&](value::TypeTags elTag, value::Value elVal) {
            int elemSize = value::getApproximateSize(elTag, elVal);
            bool inserted = accArrSet->push_back(elTag, elVal);

            if (inserted) {
                currentSize += elemSize;
                if (currentSize >= static_cast<int64_t>(sizeCap)) {
                    uasserted(ErrorCodes::ExceededMemoryLimit,
                              str::stream()
                                  << "Used too much memory for a single array. Memory limit: "
                                  << sizeCap << ". Current set has " << accArrSet->size()
                                  << " elements and is " << currentSize << " bytes.");
                }
            }
        });

    // Update the accumulator with the new total size.
    accArray->setAt(static_cast<size_t>(AggArrayWithSize::kSizeOfValues),
                    value::TypeTags::NumberInt64,
                    value::bitcastFrom<int64_t>(currentSize));

    guardArr.reset();
    return {ownAcc, tagAcc, valAcc};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggSetUnion(ArityType arity) {
    auto [ownAcc, tagAcc, valAcc] = getFromStack(0);

    if (tagAcc == value::TypeTags::Nothing) {
        // Initialize the accumulator.
        ownAcc = true;
        std::tie(tagAcc, valAcc) = value::makeNewArraySet();
    } else {
        // Take ownership of the accumulator.
        topStack(false, value::TypeTags::Nothing, 0);
    }

    tassert(7039552, "accumulator must be owned", ownAcc);
    value::ValueGuard guardAcc{tagAcc, valAcc};
    tassert(7039553, "accumulator must be of type ArraySet", tagAcc == value::TypeTags::ArraySet);
    auto acc = value::getArraySetView(valAcc);

    auto [tagNewSet, valNewSet] = moveOwnedFromStack(1);
    value::ValueGuard guardNewSet{tagNewSet, valNewSet};
    if (!value::isArray(tagNewSet)) {
        return {false, value::TypeTags::Nothing, 0};
    }

    value::arrayForEach(tagNewSet, valNewSet, [&](value::TypeTags elTag, value::Value elVal) {
        auto [copyTag, copyVal] = value::copyValue(elTag, elVal);
        acc->push_back(copyTag, copyVal);
    });

    guardAcc.reset();
    return {ownAcc, tagAcc, valAcc};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggCollSetUnion(ArityType arity) {
    auto [ownAcc, tagAcc, valAcc] = getFromStack(0);

    if (tagAcc == value::TypeTags::Nothing) {
        auto [_, collatorTag, collatorVal] = getFromStack(1);
        tassert(
            7690402, "Expected value of type 'collator'", collatorTag == value::TypeTags::collator);
        CollatorInterface* collator = value::getCollatorView(collatorVal);

        // Initialize the accumulator.
        ownAcc = true;
        std::tie(tagAcc, valAcc) = value::makeNewArraySet(collator);
    } else {
        // Take ownership of the accumulator.
        topStack(false, value::TypeTags::Nothing, 0);
    }

    tassert(7690403, "Accumulator must be owned", ownAcc);
    value::ValueGuard guardAcc{tagAcc, valAcc};
    tassert(7690404, "Accumulator must be of type ArraySet", tagAcc == value::TypeTags::ArraySet);
    auto acc = value::getArraySetView(valAcc);

    auto [tagNewSet, valNewSet] = moveOwnedFromStack(2);
    value::ValueGuard guardNewSet{tagNewSet, valNewSet};
    if (!value::isArray(tagNewSet)) {
        return {false, value::TypeTags::Nothing, 0};
    }

    value::arrayForEach(tagNewSet, valNewSet, [&](value::TypeTags elTag, value::Value elVal) {
        auto [copyTag, copyVal] = value::copyValue(elTag, elVal);
        acc->push_back(copyTag, copyVal);
    });

    guardAcc.reset();
    return {ownAcc, tagAcc, valAcc};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggSetUnionCapped(ArityType arity) {
    auto [tagNewElem, valNewElem] = moveOwnedFromStack(1);
    value::ValueGuard guardNewElem{tagNewElem, valNewElem};

    auto [_, tagSizeCap, valSizeCap] = getFromStack(2);
    tassert(7039509,
            "'cap' parameter must be a 32-bit int",
            tagSizeCap == value::TypeTags::NumberInt32);
    const size_t sizeCap = value::bitcastTo<int32_t>(valSizeCap);

    guardNewElem.reset();
    return aggSetUnionCappedImpl(tagNewElem, valNewElem, sizeCap, nullptr /*collator*/);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggCollSetUnionCapped(
    ArityType arity) {
    auto [_1, tagColl, valColl] = getFromStack(1);
    auto [tagNewElem, valNewElem] = moveOwnedFromStack(2);
    value::ValueGuard guardNewElem{tagNewElem, valNewElem};
    auto [_2, tagSizeCap, valSizeCap] = getFromStack(3);

    tassert(7039510, "expected value of type 'collator'", tagColl == value::TypeTags::collator);
    tassert(7039511,
            "'cap' parameter must be a 32-bit int",
            tagSizeCap == value::TypeTags::NumberInt32);

    guardNewElem.reset();
    return aggSetUnionCappedImpl(tagNewElem,
                                 valNewElem,
                                 value::bitcastTo<int32_t>(valSizeCap),
                                 value::getCollatorView(valColl));
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinCollSetIntersection(
    ArityType arity) {
    invariant(arity >= 1);

    auto [_, collTag, collVal] = getFromStack(0);
    if (collTag != value::TypeTags::collator) {
        return {false, value::TypeTags::Nothing, 0};
    }

    std::vector<value::TypeTags> argTags;
    std::vector<value::Value> argVals;

    for (size_t idx = 1; idx < arity; ++idx) {
        auto [owned, tag, val] = getFromStack(idx);
        if (!value::isArray(tag)) {
            return {false, value::TypeTags::Nothing, 0};
        }

        argTags.push_back(tag);
        argVals.push_back(val);
    }

    return setIntersection(argTags, argVals, value::getCollatorView(collVal));
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinSetIntersection(ArityType arity) {
    std::vector<value::TypeTags> argTags;
    std::vector<value::Value> argVals;

    for (size_t idx = 0; idx < arity; ++idx) {
        auto [_, tag, val] = getFromStack(idx);
        if (!value::isArray(tag)) {
            return {false, value::TypeTags::Nothing, 0};
        }

        argTags.push_back(tag);
        argVals.push_back(val);
    }

    return setIntersection(argTags, argVals);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinCollSetDifference(ArityType arity) {
    invariant(arity == 3);

    auto [_, collTag, collVal] = getFromStack(0);
    if (collTag != value::TypeTags::collator) {
        return {false, value::TypeTags::Nothing, 0};
    }

    auto [lhsOwned, lhsTag, lhsVal] = getFromStack(1);
    auto [rhsOwned, rhsTag, rhsVal] = getFromStack(2);

    if (!value::isArray(lhsTag) || !value::isArray(rhsTag)) {
        return {false, value::TypeTags::Nothing, 0};
    }

    return setDifference(lhsTag, lhsVal, rhsTag, rhsVal, value::getCollatorView(collVal));
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinCollSetEquals(ArityType arity) {
    invariant(arity >= 3);

    auto [_, collTag, collVal] = getFromStack(0);
    if (collTag != value::TypeTags::collator) {
        return {false, value::TypeTags::Nothing, 0};
    }

    std::vector<value::TypeTags> argTags;
    std::vector<value::Value> argVals;

    for (size_t idx = 1; idx < arity; ++idx) {
        auto [owned, tag, val] = getFromStack(idx);
        if (!value::isArray(tag)) {
            return {false, value::TypeTags::Nothing, 0};
        }

        argTags.push_back(tag);
        argVals.push_back(val);
    }

    return setEquals(argTags, argVals, value::getCollatorView(collVal));
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinCollSetIsSubset(ArityType arity) {
    tassert(5154701, "$setIsSubset expects two sets and a collator", arity == 3);

    auto [_, collTag, collVal] = getFromStack(0);
    if (collTag != value::TypeTags::collator) {
        return {false, value::TypeTags::Nothing, 0};
    }

    auto [lhsOwned, lhsTag, lhsVal] = getFromStack(1);
    auto [rhsOwned, rhsTag, rhsVal] = getFromStack(2);

    return setIsSubset(lhsTag, lhsVal, rhsTag, rhsVal, value::getCollatorView(collVal));
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinSetDifference(ArityType arity) {
    invariant(arity == 2);

    auto [lhsOwned, lhsTag, lhsVal] = getFromStack(0);
    auto [rhsOwned, rhsTag, rhsVal] = getFromStack(1);

    if (!value::isArray(lhsTag) || !value::isArray(rhsTag)) {
        return {false, value::TypeTags::Nothing, 0};
    }

    return setDifference(lhsTag, lhsVal, rhsTag, rhsVal);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinSetEquals(ArityType arity) {
    invariant(arity >= 2);

    std::vector<value::TypeTags> argTags;
    std::vector<value::Value> argVals;

    for (size_t idx = 0; idx < arity; ++idx) {
        auto [_, tag, val] = getFromStack(idx);
        if (!value::isArray(tag)) {
            return {false, value::TypeTags::Nothing, 0};
        }

        argTags.push_back(tag);
        argVals.push_back(val);
    }

    return setEquals(argTags, argVals);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinSetIsSubset(ArityType arity) {
    tassert(5154702, "$setIsSubset expects two sets", arity == 2);

    auto [lhsOwned, lhsTag, lhsVal] = getFromStack(0);
    auto [rhsOwned, rhsTag, rhsVal] = getFromStack(1);

    return setIsSubset(lhsTag, lhsVal, rhsTag, rhsVal);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinSetToArray(ArityType arity) {
    invariant(arity == 1);

    auto [owned, tag, val] = getFromStack(0);

    if (tag != value::TypeTags::ArraySet && tag != value::TypeTags::ArrayMultiSet) {
        // passthrough if its not a set
        topStack(false, value::TypeTags::Nothing, 0);
        return {owned, tag, val};
    }

    auto [resTag, resVal] = value::makeNewArray();
    value::ValueGuard resGuard{resTag, resVal};
    auto resView = value::getArrayView(resVal);

    value::arrayForEach(tag, val, [&](value::TypeTags elTag, value::Value elVal) {
        resView->push_back(value::copyValue(elTag, elVal));
    });

    resGuard.reset();
    return {true, resTag, resVal};
}

namespace {
/**
 * A helper function to extract the next match in the subject string using the compiled regex
 * pattern.
 * - pcre: The wrapper object containing the compiled pcre expression
 * - inputString: The subject string.
 * - startBytePos: The position from where the search should start given in bytes.
 * - codePointPos: The same position in terms of code points.
 * - isMatch: Boolean flag to mark if the caller function is $regexMatch, in which case the result
 * returned is true/false.
 */
FastTuple<bool, value::TypeTags, value::Value> pcreNextMatch(pcre::Regex* pcre,
                                                             StringData inputString,
                                                             uint32_t& startBytePos,
                                                             uint32_t& codePointPos,
                                                             bool isMatch) {
    pcre::MatchData m = pcre->matchView(inputString, {}, startBytePos);
    if (!m && m.error() != pcre::Errc::ERROR_NOMATCH) {
        LOGV2_ERROR(5073414,
                    "Error occurred while executing regular expression.",
                    "execResult"_attr = errorMessage(m.error()));
        return {false, value::TypeTags::Nothing, 0};
    }

    if (isMatch) {
        // $regexMatch returns true or false.
        return {false, value::TypeTags::Boolean, value::bitcastFrom<bool>(!!m)};
    }
    // $regexFind and $regexFindAll build result object or return null.
    if (!m) {
        return {false, value::TypeTags::Null, 0};
    }

    // Create the result object {"match" : .., "idx" : ..., "captures" : ...}
    // from the pcre::MatchData.
    auto [matchedTag, matchedVal] = value::makeNewString(m[0]);
    value::ValueGuard matchedGuard{matchedTag, matchedVal};

    StringData precedesMatch = m.input().substr(m.startPos());
    precedesMatch = precedesMatch.substr(0, m[0].data() - precedesMatch.data());
    codePointPos += str::lengthInUTF8CodePoints(precedesMatch);
    startBytePos += precedesMatch.size();

    auto [arrTag, arrVal] = value::makeNewArray();
    value::ValueGuard arrGuard{arrTag, arrVal};
    auto arrayView = value::getArrayView(arrVal);
    arrayView->reserve(m.captureCount());
    for (size_t i = 0; i < m.captureCount(); ++i) {
        StringData cap = m[i + 1];
        if (!cap.rawData()) {
            arrayView->push_back(value::TypeTags::Null, 0);
        } else {
            auto [tag, val] = value::makeNewString(cap);
            arrayView->push_back(tag, val);
        }
    }

    auto [resTag, resVal] = value::makeNewObject();
    value::ValueGuard resGuard{resTag, resVal};
    auto resObjectView = value::getObjectView(resVal);
    resObjectView->reserve(3);
    matchedGuard.reset();
    resObjectView->push_back("match", matchedTag, matchedVal);
    resObjectView->push_back(
        "idx", value::TypeTags::NumberInt32, value::bitcastFrom<int32_t>(codePointPos));
    arrGuard.reset();
    resObjectView->push_back("captures", arrTag, arrVal);
    resGuard.reset();
    return {true, resTag, resVal};
}

/**
 * A helper function with common logic for $regexMatch and $regexFind functions. Both extract only
 * the first match to a regular expression, but return different result objects.
 */
FastTuple<bool, value::TypeTags, value::Value> genericPcreRegexSingleMatch(
    value::TypeTags typeTagPcreRegex,
    value::Value valuePcreRegex,
    value::TypeTags typeTagInputStr,
    value::Value valueInputStr,
    bool isMatch) {
    if (!value::isStringOrSymbol(typeTagInputStr) || !value::isPcreRegex(typeTagPcreRegex)) {
        return {false, value::TypeTags::Nothing, 0};
    }

    auto inputString = value::getStringOrSymbolView(typeTagInputStr, valueInputStr);
    auto pcreRegex = value::getPcreRegexView(valuePcreRegex);

    uint32_t startBytePos = 0;
    uint32_t codePointPos = 0;
    return pcreNextMatch(pcreRegex, inputString, startBytePos, codePointPos, isMatch);
}

MONGO_COMPILER_NOINLINE
std::pair<value::TypeTags, value::Value> collComparisonKey(value::TypeTags tag,
                                                           value::Value val,
                                                           const CollatorInterface* collator) {
    using namespace std::literals;

    // This function should only be called if 'collator' is non-null and 'tag' is a collatable type.
    invariant(collator);
    invariant(value::isCollatableType(tag));

    // For strings, call CollatorInterface::getComparisonKey() to obtain the comparison key.
    if (value::isString(tag)) {
        return value::makeNewString(
            collator->getComparisonKey(value::getStringView(tag, val)).getKeyData());
    }

    // For collatable types other than strings (such as arrays and objects), we take the slow
    // path and round-trip the value through BSON.
    BSONObjBuilder input;
    bson::appendValueToBsonObj<BSONObjBuilder>(input, ""_sd, tag, val);

    BSONObjBuilder output;
    CollationIndexKey::collationAwareIndexKeyAppend(input.obj().firstElement(), collator, &output);

    BSONObj outputView = output.done();
    auto ptr = outputView.objdata();
    auto be = ptr + 4;
    auto end = ptr + ConstDataView(ptr).read<LittleEndian<uint32_t>>();
    return bson::convertFrom<false>(be, end, 0);
}

}  // namespace

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinRegexCompile(ArityType arity) {
    invariant(arity == 2);

    auto [patternOwned, patternTypeTag, patternValue] = getFromStack(0);
    auto [optionsOwned, optionsTypeTag, optionsValue] = getFromStack(1);

    if (!value::isString(patternTypeTag) || !value::isString(optionsTypeTag)) {
        return {false, value::TypeTags::Nothing, 0};
    }

    auto pattern = value::getStringView(patternTypeTag, patternValue);
    auto options = value::getStringView(optionsTypeTag, optionsValue);

    if (pattern.find('\0', 0) != std::string::npos || options.find('\0', 0) != std::string::npos) {
        return {false, value::TypeTags::Nothing, 0};
    }

    auto [pcreTag, pcreValue] = makeNewPcreRegex(pattern, options);
    return {true, pcreTag, pcreValue};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinRegexMatch(ArityType arity) {
    invariant(arity == 2);
    auto [ownedPcreRegex, tagPcreRegex, valPcreRegex] = getFromStack(0);
    auto [ownedInputStr, tagInputStr, valInputStr] = getFromStack(1);

    if (value::isArray(tagPcreRegex)) {
        for (value::ArrayEnumerator ae(tagPcreRegex, valPcreRegex); !ae.atEnd(); ae.advance()) {
            auto [elemTag, elemVal] = ae.getViewOfValue();
            auto [ownedResult, tagResult, valResult] =
                genericPcreRegexSingleMatch(elemTag, elemVal, tagInputStr, valInputStr, true);

            if (tagResult == value::TypeTags::Boolean && value::bitcastTo<bool>(valResult)) {
                return {ownedResult, tagResult, valResult};
            }

            if (ownedResult) {
                value::releaseValue(tagResult, valResult);
            }
        }

        return {false, value::TypeTags::Boolean, value::bitcastFrom<bool>(false)};
    }

    return genericPcreRegexSingleMatch(tagPcreRegex, valPcreRegex, tagInputStr, valInputStr, true);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinRegexFind(ArityType arity) {
    invariant(arity == 2);
    auto [ownedPcreRegex, typeTagPcreRegex, valuePcreRegex] = getFromStack(0);
    auto [ownedInputStr, typeTagInputStr, valueInputStr] = getFromStack(1);

    return genericPcreRegexSingleMatch(
        typeTagPcreRegex, valuePcreRegex, typeTagInputStr, valueInputStr, false);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinRegexFindAll(ArityType arity) {
    invariant(arity == 2);
    auto [ownedPcre, typeTagPcreRegex, valuePcreRegex] = getFromStack(0);
    auto [ownedStr, typeTagInputStr, valueInputStr] = getFromStack(1);

    if (!value::isString(typeTagInputStr) || typeTagPcreRegex != value::TypeTags::pcreRegex) {
        return {false, value::TypeTags::Nothing, 0};
    }

    auto inputString = value::getStringView(typeTagInputStr, valueInputStr);
    auto pcre = value::getPcreRegexView(valuePcreRegex);

    uint32_t startBytePos = 0;
    uint32_t codePointPos = 0;

    // Prepare the result array of matching objects.
    auto [arrTag, arrVal] = value::makeNewArray();
    value::ValueGuard arrGuard{arrTag, arrVal};
    auto arrayView = value::getArrayView(arrVal);

    int resultSize = 0;
    do {
        auto [_, matchTag, matchVal] =
            pcreNextMatch(pcre, inputString, startBytePos, codePointPos, false);
        value::ValueGuard matchGuard{matchTag, matchVal};

        if (matchTag == value::TypeTags::Null) {
            break;
        }
        if (matchTag != value::TypeTags::Object) {
            return {false, value::TypeTags::Nothing, 0};
        }

        resultSize += getApproximateSize(matchTag, matchVal);
        uassert(5126606,
                "$regexFindAll: the size of buffer to store output exceeded the 64MB limit",
                resultSize <= mongo::BufferMaxSize);

        matchGuard.reset();
        arrayView->push_back(matchTag, matchVal);

        // Move indexes after the current matched string to prepare for the next search.
        auto [mstrTag, mstrVal] = value::getObjectView(matchVal)->getField("match");
        auto matchString = value::getStringView(mstrTag, mstrVal);
        if (matchString.empty()) {
            startBytePos += str::getCodePointLength(inputString[startBytePos]);
            ++codePointPos;
        } else {
            startBytePos += matchString.size();
            for (size_t byteIdx = 0; byteIdx < matchString.size(); ++codePointPos) {
                byteIdx += str::getCodePointLength(matchString[byteIdx]);
            }
        }
    } while (startBytePos < inputString.size());

    arrGuard.reset();
    return {true, arrTag, arrVal};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinShardFilter(ArityType arity) {
    invariant(arity == 2);

    auto [ownedFilter, filterTag, filterValue] = getFromStack(0);
    auto [ownedShardKey, shardKeyTag, shardKeyValue] = getFromStack(1);

    if (filterTag != value::TypeTags::shardFilterer || shardKeyTag != value::TypeTags::bsonObject) {
        if (filterTag == value::TypeTags::shardFilterer &&
            shardKeyTag == value::TypeTags::Nothing) {
            LOGV2_WARNING(5071200,
                          "No shard key found in document, it may have been inserted manually "
                          "into shard",
                          "keyPattern"_attr =
                              value::getShardFiltererView(filterValue)->getKeyPattern());
        }
        return {false, value::TypeTags::Nothing, 0};
    }

    BSONObj keyAsUnownedBson{sbe::value::bitcastTo<const char*>(shardKeyValue)};
    return {false,
            value::TypeTags::Boolean,
            value::bitcastFrom<bool>(
                value::getShardFiltererView(filterValue)->keyBelongsToMe(keyAsUnownedBson))};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinShardHash(ArityType arity) {
    invariant(arity == 1);

    auto [ownedShardKey, shardKeyTag, shardKeyValue] = getFromStack(0);

    // Compute the shard key hash value by round-tripping it through BSONObj as it is currently the
    // only way to do it if we do not want to duplicate the hash computation code.
    // TODO SERVER-55622
    BSONObjBuilder input;
    bson::appendValueToBsonObj<BSONObjBuilder>(input, ""_sd, shardKeyTag, shardKeyValue);
    auto hashVal =
        BSONElementHasher::hash64(input.obj().firstElement(), BSONElementHasher::DEFAULT_HASH_SEED);
    return {false, value::TypeTags::NumberInt64, value::bitcastFrom<decltype(hashVal)>(hashVal)};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinExtractSubArray(ArityType arity) {
    // We need to ensure that 'size_t' is wide enough to store 32-bit index.
    static_assert(sizeof(size_t) >= sizeof(int32_t), "size_t must be at least 32-bits");

    auto [arrayOwned, arrayTag, arrayValue] = getFromStack(0);
    auto [limitOwned, limitTag, limitValue] = getFromStack(1);

    if (!value::isArray(arrayTag) || limitTag != value::TypeTags::NumberInt32) {
        return {false, value::TypeTags::Nothing, 0};
    }

    auto limit = value::bitcastTo<int32_t>(limitValue);

    auto absWithSign = [](int32_t value) -> std::pair<bool, size_t> {
        if (value < 0) {
            // Upcast 'value' to 'int64_t' prevent overflow during the sign change.
            return {true, -static_cast<int64_t>(value)};
        }
        return {false, value};
    };

    size_t start = 0;
    bool isNegativeStart = false;
    size_t length = 0;
    if (arity == 2) {
        std::tie(isNegativeStart, start) = absWithSign(limit);
        length = start;
        if (!isNegativeStart) {
            start = 0;
        }
    } else {
        if (limit < 0) {
            return {false, value::TypeTags::Nothing, 0};
        }
        length = limit;

        auto [skipOwned, skipTag, skipValue] = getFromStack(2);
        if (skipTag != value::TypeTags::NumberInt32) {
            return {false, value::TypeTags::Nothing, 0};
        }

        auto skip = value::bitcastTo<int32_t>(skipValue);
        std::tie(isNegativeStart, start) = absWithSign(skip);
    }

    auto [resultTag, resultValue] = value::makeNewArray();
    value::ValueGuard resultGuard{resultTag, resultValue};
    auto resultView = value::getArrayView(resultValue);

    if (arrayTag == value::TypeTags::Array) {
        auto arrayView = value::getArrayView(arrayValue);
        auto arraySize = arrayView->size();

        auto convertedStart = [&]() -> size_t {
            if (isNegativeStart) {
                if (start > arraySize) {
                    return 0;
                } else {
                    return arraySize - start;
                }
            } else {
                return std::min(start, arraySize);
            }
        }();

        size_t end = convertedStart + std::min(length, arraySize - convertedStart);
        if (convertedStart < end) {
            resultView->reserve(end - convertedStart);

            for (size_t i = convertedStart; i < end; i++) {
                auto [tag, value] = arrayView->getAt(i);
                auto [copyTag, copyValue] = value::copyValue(tag, value);
                resultView->push_back(copyTag, copyValue);
            }
        }
    } else {
        auto advance = [](value::ArrayEnumerator& enumerator, size_t offset) {
            size_t i = 0;
            while (i < offset && !enumerator.atEnd()) {
                i++;
                enumerator.advance();
            }
        };

        value::ArrayEnumerator startEnumerator{arrayTag, arrayValue};
        if (isNegativeStart) {
            value::ArrayEnumerator windowEndEnumerator{arrayTag, arrayValue};
            advance(windowEndEnumerator, start);

            while (!startEnumerator.atEnd() && !windowEndEnumerator.atEnd()) {
                startEnumerator.advance();
                windowEndEnumerator.advance();
            }
            invariant(windowEndEnumerator.atEnd());
        } else {
            advance(startEnumerator, start);
        }

        size_t i = 0;
        while (i < length && !startEnumerator.atEnd()) {
            auto [tag, value] = startEnumerator.getViewOfValue();
            auto [copyTag, copyValue] = value::copyValue(tag, value);
            resultView->push_back(copyTag, copyValue);

            i++;
            startEnumerator.advance();
        }
    }

    resultGuard.reset();
    return {true, resultTag, resultValue};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinIsArrayEmpty(ArityType arity) {
    invariant(arity == 1);
    auto [arrayOwned, arrayType, arrayValue] = getFromStack(0);

    if (!value::isArray(arrayType)) {
        return {false, value::TypeTags::Nothing, 0};
    }

    if (arrayType == value::TypeTags::Array) {
        auto arrayView = value::getArrayView(arrayValue);
        return {false, value::TypeTags::Boolean, value::bitcastFrom<bool>(arrayView->size() == 0)};
    } else if (arrayType == value::TypeTags::bsonArray || arrayType == value::TypeTags::ArraySet) {
        value::ArrayEnumerator enumerator(arrayType, arrayValue);
        return {false, value::TypeTags::Boolean, value::bitcastFrom<bool>(enumerator.atEnd())};
    } else {
        // Earlier in this function we bailed out if the 'arrayType' wasn't Array, ArraySet or
        // bsonArray, so it should be impossible to reach this point.
        MONGO_UNREACHABLE
    }
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinHasNullBytes(ArityType arity) {
    invariant(arity == 1);
    auto [strOwned, strType, strValue] = getFromStack(0);

    if (!value::isString(strType)) {
        return {false, value::TypeTags::Nothing, 0};
    }

    auto stringView = value::getStringView(strType, strValue);
    auto hasNullBytes = stringView.find('\0') != std::string::npos;

    return {false, value::TypeTags::Boolean, value::bitcastFrom<bool>(hasNullBytes)};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinGetRegexPattern(ArityType arity) {
    invariant(arity == 1);
    auto [regexOwned, regexType, regexValue] = getFromStack(0);

    if (regexType != value::TypeTags::bsonRegex) {
        return {false, value::TypeTags::Nothing, 0};
    }

    auto regex = value::getBsonRegexView(regexValue);
    auto [strType, strValue] = value::makeNewString(regex.pattern);

    return {true, strType, strValue};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinGetRegexFlags(ArityType arity) {
    invariant(arity == 1);
    auto [regexOwned, regexType, regexValue] = getFromStack(0);

    if (regexType != value::TypeTags::bsonRegex) {
        return {false, value::TypeTags::Nothing, 0};
    }

    auto regex = value::getBsonRegexView(regexValue);
    auto [strType, strValue] = value::makeNewString(regex.flags);

    return {true, strType, strValue};
}

std::pair<SortSpec*, CollatorInterface*> ByteCode::generateSortKeyHelper(ArityType arity) {
    invariant(arity == 2 || arity == 3);

    auto [ssOwned, ssTag, ssVal] = getFromStack(0);
    auto [objOwned, objTag, objVal] = getFromStack(1);
    if (ssTag != value::TypeTags::sortSpec || !value::isObject(objTag)) {
        return {nullptr, nullptr};
    }

    CollatorInterface* collator{nullptr};
    if (arity == 3) {
        auto [collatorOwned, collatorTag, collatorVal] = getFromStack(2);
        if (collatorTag != value::TypeTags::collator) {
            return {nullptr, nullptr};
        }
        collator = value::getCollatorView(collatorVal);
    }

    auto ss = value::getSortSpecView(ssVal);
    return {ss, collator};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinGenerateCheapSortKey(
    ArityType arity) {
    auto [sortSpec, collator] = generateSortKeyHelper(arity);
    if (!sortSpec) {
        return {false, value::TypeTags::Nothing, 0};
    }

    // We "move" the object argument into the sort spec.
    auto sortKeyComponentVector =
        sortSpec->generateSortKeyComponentVector(moveFromStack(1), collator);

    return {false,
            value::TypeTags::sortKeyComponentVector,
            value::bitcastFrom<value::SortKeyComponentVector*>(sortKeyComponentVector)};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinGenerateSortKey(ArityType arity) {
    auto [sortSpec, collator] = generateSortKeyHelper(arity);
    if (!sortSpec) {
        return {false, value::TypeTags::Nothing, 0};
    }

    auto [objOwned, objTag, objVal] = getFromStack(1);
    auto bsonObj = [objTag = objTag, objVal = objVal]() {
        if (objTag == value::TypeTags::bsonObject) {
            return BSONObj{value::bitcastTo<const char*>(objVal)};
        } else if (objTag == value::TypeTags::Object) {
            BSONObjBuilder objBuilder;
            bson::convertToBsonObj(objBuilder, value::getObjectView(objVal));
            return objBuilder.obj();
        } else {
            MONGO_UNREACHABLE_TASSERT(5037004);
        }
    }();

    return {true,
            value::TypeTags::ksValue,
            value::bitcastFrom<key_string::Value*>(
                new key_string::Value(sortSpec->generateSortKey(bsonObj, collator)))};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinSortKeyComponentVectorGetElement(
    ArityType arity) {
    invariant(arity == 2);

    auto [sortVecOwned, sortVecTag, sortVecVal] = getFromStack(0);
    auto [idxOwned, idxTag, idxVal] = getFromStack(1);
    if (sortVecTag != value::TypeTags::sortKeyComponentVector ||
        idxTag != value::TypeTags::NumberInt32) {
        return {false, value::TypeTags::Nothing, 0};
    }

    auto* sortObj = value::getSortKeyComponentVectorView(sortVecVal);
    const auto idxInt32 = value::bitcastTo<int32_t>(idxVal);

    invariant(idxInt32 >= 0 && static_cast<size_t>(idxInt32) < sortObj->elts.size());
    auto [outTag, outVal] = sortObj->elts[idxInt32];
    return {false, outTag, outVal};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinSortKeyComponentVectorToArray(
    ArityType arity) {
    invariant(arity == 1);

    auto [sortVecOwned, sortVecTag, sortVecVal] = getFromStack(0);
    if (sortVecTag != value::TypeTags::sortKeyComponentVector) {
        return {false, value::TypeTags::Nothing, 0};
    }
    auto* sortVec = value::getSortKeyComponentVectorView(sortVecVal);

    if (sortVec->elts.size() == 1) {
        auto [tag, val] = sortVec->elts[0];
        auto [copyTag, copyVal] = value::copyValue(tag, val);
        return {true, copyTag, copyVal};
    } else {
        auto [arrayTag, arrayVal] = value::makeNewArray();
        value::ValueGuard arrayGuard{arrayTag, arrayVal};
        auto array = value::getArrayView(arrayVal);
        array->reserve(sortVec->elts.size());
        for (size_t i = 0; i < sortVec->elts.size(); ++i) {
            auto [tag, val] = sortVec->elts[i];
            auto [copyTag, copyVal] = value::copyValue(tag, val);
            array->push_back(copyTag, copyVal);
        }
        arrayGuard.reset();
        return {true, arrayTag, arrayVal};
    }
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinMakeBsonObj(
    ArityType arity, const CodeFragment* code) {
    tassert(6897002,
            str::stream() << "Unsupported number of arguments passed to makeBsonObj(): " << arity,
            arity >= 3);

    auto [specOwned, specTag, specVal] = getFromStack(0);
    auto [objOwned, objTag, objVal] = getFromStack(1);
    auto [hasInputFieldsOwned, hasInputFieldsTag, hasInputFieldsVal] = getFromStack(2);

    if (specTag != value::TypeTags::makeObjSpec) {
        return {false, value::TypeTags::Nothing, 0};
    }
    if (hasInputFieldsTag != value::TypeTags::Boolean) {
        return {false, value::TypeTags::Nothing, 0};
    }

    auto spec = value::getMakeObjSpecView(specVal);
    bool hasInputFields = value::bitcastTo<bool>(hasInputFieldsVal);

    if (hasInputFields && objTag != value::TypeTags::Null && !value::isObject(objTag)) {
        return {false, value::TypeTags::Nothing, 0};
    }

    if (!hasInputFields) {
        if (spec->nonObjInputBehavior != MakeObjSpec::NonObjInputBehavior::kNewObj &&
            !value::isObject(objTag)) {
            if (spec->nonObjInputBehavior == MakeObjSpec::NonObjInputBehavior::kReturnNothing) {
                // If the input is Nothing or not an Object and if 'nonObjInputBehavior' equals
                // 'kReturnNothing', then return Nothing.
                return {false, value::TypeTags::Nothing, 0};
            } else if (spec->nonObjInputBehavior ==
                       MakeObjSpec::NonObjInputBehavior::kReturnInput) {
                // If the input is Nothing or not an Object and if 'nonObjInputBehavior' equals
                // 'kReturnInput', then return the input.
                topStack(false, value::TypeTags::Nothing, 0);
                return {objOwned, objTag, objVal};
            }
        }
    }

    int numInputFields = hasInputFields && spec->numInputFields ? *spec->numInputFields : 0;
    const int fieldsStackOff = 3;
    const int argsStackOff = fieldsStackOff + numInputFields;
    const auto stackOffsets = MakeObjStackOffsets{fieldsStackOff, argsStackOff};

    UniqueBSONObjBuilder bob;

    if (!hasInputFields) {
        produceBsonObject(spec, stackOffsets, code, bob, objTag, objVal);
    } else {
        produceBsonObjectWithInputFields(spec, stackOffsets, code, bob, objTag, objVal);
    }

    bob.doneFast();
    char* data = bob.bb().release().release();
    return {true, value::TypeTags::bsonObject, value::bitcastFrom<char*>(data)};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinReverseArray(ArityType arity) {
    invariant(arity == 1);
    auto [inputOwned, inputType, inputVal] = getFromStack(0);

    if (!value::isArray(inputType)) {
        return {false, value::TypeTags::Nothing, 0};
    }

    auto [resultTag, resultVal] = value::makeNewArray();
    auto resultView = value::getArrayView(resultVal);
    value::ValueGuard resultGuard{resultTag, resultVal};

    if (inputType == value::TypeTags::Array) {
        auto inputView = value::getArrayView(inputVal);
        size_t inputSize = inputView->size();
        if (inputSize) {
            resultView->reserve(inputSize);
            for (size_t i = 0; i < inputSize; i++) {
                auto [origTag, origVal] = inputView->getAt(inputSize - 1 - i);
                auto [copyTag, copyVal] = copyValue(origTag, origVal);
                resultView->push_back(copyTag, copyVal);
            }
        }

        resultGuard.reset();
        return {true, resultTag, resultVal};
    } else if (inputType == value::TypeTags::bsonArray || inputType == value::TypeTags::ArraySet) {
        // Using intermediate vector since bsonArray and ArraySet don't
        // support reverse iteration.
        std::vector<std::pair<value::TypeTags, value::Value>> inputContents;

        if (inputType == value::TypeTags::ArraySet) {
            // Reserve space to avoid resizing on push_back calls.
            auto arraySetView = value::getArraySetView(inputVal);
            inputContents.reserve(arraySetView->size());
        }

        value::arrayForEach(inputType, inputVal, [&](value::TypeTags elTag, value::Value elVal) {
            inputContents.push_back({elTag, elVal});
        });

        if (inputContents.size()) {
            resultView->reserve(inputContents.size());

            // Run through the array backwards and copy into the result array.
            for (auto it = inputContents.rbegin(); it != inputContents.rend(); ++it) {
                auto [copyTag, copyVal] = copyValue(it->first, it->second);
                resultView->push_back(copyTag, copyVal);
            }
        }

        resultGuard.reset();
        return {true, resultTag, resultVal};
    } else {
        // Earlier in this function we bailed out if the 'inputType' wasn't
        // Array, ArraySet or bsonArray, so it should be impossible to reach
        // this point.
        MONGO_UNREACHABLE;
    }
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinSortArray(ArityType arity) {
    invariant(arity == 2 || arity == 3);
    auto [inputOwned, inputType, inputVal] = getFromStack(0);

    if (!value::isArray(inputType)) {
        return {false, value::TypeTags::Nothing, 0};
    }

    auto [specOwned, specTag, specVal] = getFromStack(1);

    if (!value::isObject(specTag)) {
        return {false, value::TypeTags::Nothing, 0};
    }

    CollatorInterface* collator = nullptr;
    if (arity == 3) {
        auto [collatorOwned, collatorType, collatorVal] = getFromStack(2);

        if (collatorType == value::TypeTags::collator) {
            collator = value::getCollatorView(collatorVal);
        } else {
            // If a third parameter was supplied but it is not a Collator, return Nothing.
            return {false, value::TypeTags::Nothing, 0};
        }
    }

    auto cmp = SbePatternValueCmp(specTag, specVal, collator);

    auto [resultTag, resultVal] = value::makeNewArray();
    auto resultView = value::getArrayView(resultVal);
    value::ValueGuard resultGuard{resultTag, resultVal};

    if (inputType == value::TypeTags::Array) {
        auto inputView = value::getArrayView(inputVal);
        size_t inputSize = inputView->size();
        if (inputSize) {
            resultView->reserve(inputSize);
            std::vector<std::pair<value::TypeTags, value::Value>> sortVector;
            for (size_t i = 0; i < inputSize; i++) {
                sortVector.push_back(inputView->getAt(i));
            }
            std::sort(sortVector.begin(), sortVector.end(), cmp);

            for (size_t i = 0; i < inputSize; i++) {
                auto [tag, val] = sortVector[i];
                auto [copyTag, copyVal] = copyValue(tag, val);
                resultView->push_back(copyTag, copyVal);
            }
        }

        resultGuard.reset();
        return {true, resultTag, resultVal};
    } else if (inputType == value::TypeTags::bsonArray || inputType == value::TypeTags::ArraySet) {
        value::ArrayEnumerator enumerator{inputType, inputVal};

        // Using intermediate vector since bsonArray and ArraySet don't
        // support reverse iteration.
        std::vector<std::pair<value::TypeTags, value::Value>> inputContents;

        if (inputType == value::TypeTags::ArraySet) {
            // Reserve space to avoid resizing on push_back calls.
            auto arraySetView = value::getArraySetView(inputVal);
            inputContents.reserve(arraySetView->size());
        }

        while (!enumerator.atEnd()) {
            inputContents.push_back(enumerator.getViewOfValue());
            enumerator.advance();
        }

        std::sort(inputContents.begin(), inputContents.end(), cmp);

        if (inputContents.size()) {
            resultView->reserve(inputContents.size());

            for (auto it = inputContents.begin(); it != inputContents.end(); ++it) {
                auto [copyTag, copyVal] = copyValue(it->first, it->second);
                resultView->push_back(copyTag, copyVal);
            }
        }

        resultGuard.reset();
        return {true, resultTag, resultVal};
    } else {
        // Earlier in this function we bailed out if the 'inputType' wasn't
        // Array, ArraySet or bsonArray, so it should be impossible to reach
        // this point.
        MONGO_UNREACHABLE;
    }
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinDateAdd(ArityType arity) {
    invariant(arity == 5);

    auto [timezoneDBOwn, timezoneDBTag, timezoneDBVal] = getFromStack(0);
    if (timezoneDBTag != value::TypeTags::timeZoneDB) {
        return {false, value::TypeTags::Nothing, 0};
    }
    auto timezoneDB = value::getTimeZoneDBView(timezoneDBVal);

    auto [startDateOwn, startDateTag, startDateVal] = getFromStack(1);
    if (!coercibleToDate(startDateTag)) {
        return {false, value::TypeTags::Nothing, 0};
    }
    auto startDate = getDate(startDateTag, startDateVal);

    auto [unitOwn, unitTag, unitVal] = getFromStack(2);
    if (!value::isString(unitTag)) {
        return {false, value::TypeTags::Nothing, 0};
    }
    std::string unitStr{value::getStringView(unitTag, unitVal)};
    if (!isValidTimeUnit(unitStr)) {
        return {false, value::TypeTags::Nothing, 0};
    }
    auto unit = parseTimeUnit(unitStr);

    auto [amountOwn, amountTag, amountVal] = getFromStack(3);
    if (amountTag != value::TypeTags::NumberInt64) {
        return {false, value::TypeTags::Nothing, 0};
    }
    auto amount = value::bitcastTo<int64_t>(amountVal);

    auto [timezoneOwn, timezoneTag, timezoneVal] = getFromStack(4);
    if (!value::isString(timezoneTag) || !isValidTimezone(timezoneTag, timezoneVal, timezoneDB)) {
        return {false, value::TypeTags::Nothing, 0};
    }
    auto timezone = getTimezone(timezoneTag, timezoneVal, timezoneDB);

    auto resDate = dateAdd(startDate, unit, amount, timezone);
    return {
        false, value::TypeTags::Date, value::bitcastFrom<int64_t>(resDate.toMillisSinceEpoch())};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinFtsMatch(ArityType arity) {
    invariant(arity == 2);

    auto [matcherOwn, matcherTag, matcherVal] = getFromStack(0);
    auto [inputOwn, inputTag, inputVal] = getFromStack(1);

    if (matcherTag != value::TypeTags::ftsMatcher || !value::isObject(inputTag)) {
        return {false, value::TypeTags::Nothing, 0};
    }

    auto obj = [inputTag = inputTag, inputVal = inputVal]() {
        if (inputTag == value::TypeTags::bsonObject) {
            return BSONObj{value::bitcastTo<const char*>(inputVal)};
        }

        invariant(inputTag == value::TypeTags::Object);
        BSONObjBuilder builder;
        bson::convertToBsonObj(builder, value::getObjectView(inputVal));
        return builder.obj();
    }();

    const bool matches = value::getFtsMatcherView(matcherVal)->matches(obj);
    return {false, value::TypeTags::Boolean, value::bitcastFrom<bool>(matches)};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinTsSecond(ArityType arity) {
    invariant(arity == 1);

    auto [inputValueOwn, inputTypeTag, inputValue] = getFromStack(0);

    if (inputTypeTag != value::TypeTags::Timestamp) {
        return {false, value::TypeTags::Nothing, 0};
    }

    auto timestamp = Timestamp(value::bitcastTo<uint64_t>(inputValue));
    return {false, value::TypeTags::NumberInt64, value::bitcastFrom<uint64_t>(timestamp.getSecs())};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinTsIncrement(ArityType arity) {
    invariant(arity == 1);

    auto [inputValueOwn, inputTypeTag, inputValue] = getFromStack(0);

    if (inputTypeTag != value::TypeTags::Timestamp) {
        return {false, value::TypeTags::Nothing, 0};
    }

    auto timestamp = Timestamp(value::bitcastTo<uint64_t>(inputValue));
    return {false, value::TypeTags::NumberInt64, value::bitcastFrom<uint64_t>(timestamp.getInc())};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinHash(ArityType arity) {
    auto hashVal = value::hashInit();
    for (ArityType idx = 0; idx < arity; ++idx) {
        auto [owned, tag, val] = getFromStack(idx);
        hashVal = value::hashCombine(hashVal, value::hashValue(tag, val));
    }

    return {false, value::TypeTags::NumberInt64, value::bitcastFrom<decltype(hashVal)>(hashVal)};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinTypeMatch(ArityType arity) {
    invariant(arity == 2);

    auto [inputOwn, inputTag, inputVal] = getFromStack(0);
    auto [typeMaskOwn, typeMaskTag, typeMaskVal] = getFromStack(1);

    if (inputTag != value::TypeTags::Nothing && typeMaskTag == value::TypeTags::NumberInt32) {
        auto typeMask = static_cast<uint32_t>(value::bitcastTo<int32_t>(typeMaskVal));
        bool matches = static_cast<bool>(getBSONTypeMask(inputTag) & typeMask);

        return {false, value::TypeTags::Boolean, value::bitcastFrom<bool>(matches)};
    }

    return {false, value::TypeTags::Nothing, 0};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinMinMaxFromArray(ArityType arity,
                                                                                Builtin f) {
    invariant(arity == 1 || arity == 2);

    CollatorInterface* collator = nullptr;
    if (arity == 2) {
        auto [collOwned, collTag, collVal] = getFromStack(1);
        if (collTag == value::TypeTags::collator) {
            collator = value::getCollatorView(collVal);
        }
    }

    auto [fieldOwned, fieldTag, fieldVal] = getFromStack(0);

    // If the argument is an array, find out the min/max value and place it in the
    // stack. If it is Nothing or another simple type, treat it as the return value.
    if (!value::isArray(fieldTag)) {
        return moveFromStack(0);
    }

    value::ArrayEnumerator arrayEnum(fieldTag, fieldVal);
    if (arrayEnum.atEnd()) {
        // The array is empty, return Nothing.
        return {false, sbe::value::TypeTags::Nothing, 0};
    }
    auto [accTag, accVal] = arrayEnum.getViewOfValue();
    arrayEnum.advance();
    int sign_adjust = f == Builtin::internalLeast ? -1 : +1;
    while (!arrayEnum.atEnd()) {
        auto [itemTag, itemVal] = arrayEnum.getViewOfValue();
        auto [tag, val] = value::compare3way(itemTag, itemVal, accTag, accVal, collator);
        if (tag == value::TypeTags::Nothing) {
            // The comparison returns Nothing if one of the arguments is Nothing or if a sort order
            // cannot be determined: bail out immediately and return Nothing.
            return {false, sbe::value::TypeTags::Nothing, 0};
        } else if (tag == value::TypeTags::NumberInt32 &&
                   (sign_adjust * value::bitcastTo<int>(val)) > 0) {
            accTag = itemTag;
            accVal = itemVal;
        }
        arrayEnum.advance();
    }
    // If the array is owned by the stack, make a copy of the item, or it will become invalid after
    // the caller clears the array from it.
    if (fieldOwned) {
        std::tie(accTag, accVal) = value::copyValue(accTag, accVal);
    }
    return {fieldOwned, accTag, accVal};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinObjectToArray(ArityType arity) {
    invariant(arity == 1);

    auto [objOwned, objTag, objVal] = getFromStack(0);

    if (!value::isObject(objTag)) {
        return {false, value::TypeTags::Nothing, 0};
    }

    auto [arrTag, arrVal] = value::makeNewArray();
    value::ValueGuard arrGuard{arrTag, arrVal};
    auto array = value::getArrayView(arrVal);

    value::ObjectEnumerator objectEnumerator(objTag, objVal);
    while (!objectEnumerator.atEnd()) {
        // get key
        auto fieldName = objectEnumerator.getFieldName();
        auto [keyTag, keyVal] = value::makeNewString(fieldName);
        value::ValueGuard keyGuard{keyTag, keyVal};

        // get value
        auto [valueTag, valueVal] = objectEnumerator.getViewOfValue();
        auto [valueCopyTag, valueCopyVal] = value::copyValue(valueTag, valueVal);

        // create a new obejct
        auto [elemTag, elemVal] = value::makeNewObject();
        value::ValueGuard elemGuard{elemTag, elemVal};
        auto elemObj = value::getObjectView(elemVal);

        // insert key and value to the object
        elemObj->push_back("k"_sd, keyTag, keyVal);
        keyGuard.reset();
        elemObj->push_back("v"_sd, valueCopyTag, valueCopyVal);

        // insert the object to array
        array->push_back(elemTag, elemVal);
        elemGuard.reset();

        objectEnumerator.advance();
    }
    arrGuard.reset();
    return {true, arrTag, arrVal};
}


FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinArrayToObject(ArityType arity) {
    invariant(arity == 1);

    auto [arrOwned, arrTag, arrVal] = getFromStack(0);

    if (!value::isArray(arrTag)) {
        return {false, value::TypeTags::Nothing, 0};
    }

    auto [objTag, objVal] = value::makeNewObject();
    value::ValueGuard objGuard{objTag, objVal};
    auto object = value::getObjectView(objVal);

    value::ArrayEnumerator arrayEnumerator(arrTag, arrVal);

    // return empty object for empty array
    if (arrayEnumerator.atEnd()) {
        objGuard.reset();
        return {true, objTag, objVal};
    }

    // There are two accepted input formats in an array: [ [key, val] ] or [ {k:key, v:val} ]. The
    // first array element determines the format for the rest of the array. Mixing input formats is
    // not allowed.
    bool inputArrayFormat;
    auto [firstElemTag, firstElemVal] = arrayEnumerator.getViewOfValue();
    if (value::isArray(firstElemTag)) {
        inputArrayFormat = true;
    } else if (value::isObject(firstElemTag)) {
        inputArrayFormat = false;
    } else {
        uasserted(5153201, "Input to $arrayToObject should be either an array or object");
    }

    // Use a StringMap to store the indices in object for added fieldNames
    // Only the last value should be added for duplicate fieldNames.
    StringMap<int> keyMap{};

    while (!arrayEnumerator.atEnd()) {
        auto [elemTag, elemVal] = arrayEnumerator.getViewOfValue();
        if (inputArrayFormat) {
            uassert(5153202,
                    "$arrayToObject requires a consistent input format. Expected an array",
                    value::isArray(elemTag));

            value::ArrayEnumerator innerArrayEnum(elemTag, elemVal);
            uassert(5153203,
                    "$arrayToObject requires an array of size 2 arrays",
                    !innerArrayEnum.atEnd());

            auto [keyTag, keyVal] = innerArrayEnum.getViewOfValue();
            uassert(5153204,
                    "$arrayToObject requires an array of key-value pairs, where the key must be of "
                    "type string",
                    value::isString(keyTag));

            innerArrayEnum.advance();
            uassert(5153205,
                    "$arrayToObject requires an array of size 2 arrays",
                    !innerArrayEnum.atEnd());

            auto [valueTag, valueVal] = innerArrayEnum.getViewOfValue();

            innerArrayEnum.advance();
            uassert(5153206,
                    "$arrayToObject requires an array of size 2 arrays",
                    innerArrayEnum.atEnd());

            auto keyStringData = value::getStringView(keyTag, keyVal);
            uassert(5153207,
                    "Key field cannot contain an embedded null byte",
                    keyStringData.find('\0') == std::string::npos);

            auto [valueCopyTag, valueCopyVal] = value::copyValue(valueTag, valueVal);
            if (keyMap.contains(keyStringData)) {
                auto idx = keyMap[keyStringData];
                object->setAt(idx, valueCopyTag, valueCopyVal);
            } else {
                keyMap[keyStringData] = object->size();
                object->push_back(keyStringData, valueCopyTag, valueCopyVal);
            }
        } else {
            uassert(5153208,
                    "$arrayToObject requires a consistent input format. Expected an object",
                    value::isObject(elemTag));

            value::ObjectEnumerator innerObjEnum(elemTag, elemVal);
            uassert(5153209,
                    "$arrayToObject requires an object keys of 'k' and 'v'. "
                    "Found incorrect number of keys",
                    !innerObjEnum.atEnd());

            auto keyName = innerObjEnum.getFieldName();
            auto [keyTag, keyVal] = innerObjEnum.getViewOfValue();

            innerObjEnum.advance();
            uassert(5153210,
                    "$arrayToObject requires an object keys of 'k' and 'v'. "
                    "Found incorrect number of keys",
                    !innerObjEnum.atEnd());

            auto valueName = innerObjEnum.getFieldName();
            auto [valueTag, valueVal] = innerObjEnum.getViewOfValue();

            innerObjEnum.advance();
            uassert(5153211,
                    "$arrayToObject requires an object keys of 'k' and 'v'. "
                    "Found incorrect number of keys",
                    innerObjEnum.atEnd());

            uassert(5153212,
                    "$arrayToObject requires an object with keys 'k' and 'v'.",
                    ((keyName == "k" && valueName == "v") || (keyName == "k" && valueName == "v")));
            if (keyName == "v" && valueName == "k") {
                std::swap(keyTag, valueTag);
                std::swap(keyVal, valueVal);
            }

            uassert(5153213,
                    "$arrayToObject requires an object with keys 'k' and 'v', where "
                    "the value of 'k' must be of type string",
                    value::isString(keyTag));

            auto keyStringData = value::getStringView(keyTag, keyVal);
            uassert(5153214,
                    "Key field cannot contain an embedded null byte",
                    keyStringData.find('\0') == std::string::npos);

            auto [valueCopyTag, valueCopyVal] = value::copyValue(valueTag, valueVal);
            if (keyMap.contains(keyStringData)) {
                auto idx = keyMap[keyStringData];
                object->setAt(idx, valueCopyTag, valueCopyVal);
            } else {
                keyMap[keyStringData] = object->size();
                object->push_back(keyStringData, valueCopyTag, valueCopyVal);
            }
        }
        arrayEnumerator.advance();
    }
    objGuard.reset();
    return {true, objTag, objVal};
}

std::tuple<value::Array*, value::Array*, size_t, size_t, int32_t, int32_t, bool> multiAccState(
    value::TypeTags stateTag, value::Value stateVal) {
    uassert(
        7548600, "The accumulator state should be an array", stateTag == value::TypeTags::Array);
    auto state = value::getArrayView(stateVal);

    uassert(7548601,
            "The accumulator state should have correct number of elements",
            state->size() == static_cast<size_t>(AggMultiElems::kSizeOfArray));

    auto [arrayTag, arrayVal] = state->getAt(static_cast<size_t>(AggMultiElems::kInternalArr));
    uassert(7548602,
            "Internal array component is not of correct type",
            arrayTag == value::TypeTags::Array);
    auto array = value::getArrayView(arrayVal);

    auto [startIndexTag, startIndexVal] =
        state->getAt(static_cast<size_t>(AggMultiElems::kStartIdx));
    uassert(7548700,
            "Index component be a 64-bit integer",
            startIndexTag == value::TypeTags::NumberInt64);

    auto [maxSizeTag, maxSize] = state->getAt(static_cast<size_t>(AggMultiElems::kMaxSize));
    uassert(7548603,
            "MaxSize component should be a 64-bit integer",
            maxSizeTag == value::TypeTags::NumberInt64);

    auto [memUsageTag, memUsage] = state->getAt(static_cast<size_t>(AggMultiElems::kMemUsage));
    uassert(7548612,
            "MemUsage component should be a 32-bit integer",
            memUsageTag == value::TypeTags::NumberInt32);

    auto [memLimitTag, memLimit] = state->getAt(static_cast<size_t>(AggMultiElems::kMemLimit));
    uassert(7548613,
            "MemLimit component should be a 32-bit integer",
            memLimitTag == value::TypeTags::NumberInt32);

    auto [isGroupAccumTag, isGroupAccumVal] =
        state->getAt(static_cast<size_t>(AggMultiElems::kIsGroupAccum));
    uassert(8070611,
            "IsGroupAccum component should be a boolean",
            isGroupAccumTag == value::TypeTags::Boolean);
    auto isGroupAccum = value::bitcastTo<bool>(isGroupAccumVal);

    return {state, array, startIndexVal, maxSize, memUsage, memLimit, isGroupAccum};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggFirstNNeedsMoreInput(
    ArityType arity) {
    auto [stateOwned, stateTag, stateVal] = getFromStack(0);
    uassert(7695200, "Unexpected accumulator state ownership", !stateOwned);

    auto state = value::getArrayView(stateVal);
    uassert(
        7695201, "The accumulator state should be an array", stateTag == value::TypeTags::Array);

    auto [arrayTag, arrayVal] = state->getAt(static_cast<size_t>(AggMultiElems::kInternalArr));
    uassert(7695202,
            "Internal array component is not of correct type",
            arrayTag == value::TypeTags::Array);
    auto array = value::getArrayView(arrayVal);

    auto [maxSizeTag, maxSize] = state->getAt(static_cast<size_t>(AggMultiElems::kMaxSize));
    uassert(7695203,
            "MaxSize component should be a 64-bit integer",
            maxSizeTag == value::TypeTags::NumberInt64);

    bool needMoreInput = (array->size() < maxSize);
    return {false, value::TypeTags::Boolean, value::bitcastFrom<bool>(needMoreInput)};
}

int32_t updateAndCheckMemUsage(value::Array* state,
                               int32_t memUsage,
                               int32_t memAdded,
                               int32_t memLimit,
                               size_t idx = static_cast<size_t>(AggMultiElems::kMemUsage)) {
    memUsage += memAdded;
    uassert(ErrorCodes::ExceededMemoryLimit,
            str::stream()
                << "Accumulator used too much memory and spilling to disk cannot reduce memory "
                   "consumption any further. Memory limit: "
                << memLimit << " bytes",
            memUsage < memLimit);
    state->setAt(idx, value::TypeTags::NumberInt32, memUsage);
    return memUsage;
}

size_t updateStartIdx(value::Array* state, size_t startIdx, size_t arrSize) {
    startIdx = (startIdx + 1) % arrSize;
    state->setAt(
        static_cast<size_t>(AggMultiElems::kStartIdx), value::TypeTags::NumberInt64, startIdx);
    return startIdx;
}

int32_t aggFirstN(value::Array* state,
                  value::Array* array,
                  size_t maxSize,
                  int32_t memUsage,
                  int32_t memLimit,
                  value::TypeTags fieldTag,
                  value::Value fieldVal) {
    value::ValueGuard fieldGuard{fieldTag, fieldVal};
    if (array->size() < maxSize) {
        memUsage = updateAndCheckMemUsage(
            state, memUsage, value::getApproximateSize(fieldTag, fieldVal), memLimit);

        // add to array
        fieldGuard.reset();
        array->push_back(fieldTag, fieldVal);
    }
    return memUsage;
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggFirstN(ArityType arity) {
    auto [stateTag, stateVal] = moveOwnedFromStack(0);
    value::ValueGuard stateGuard{stateTag, stateVal};

    auto [state, array, startIdx, maxSize, memUsage, memLimit, isGroupAccum] =
        multiAccState(stateTag, stateVal);

    auto [fieldTag, fieldVal] = moveOwnedFromStack(1);
    aggFirstN(state, array, maxSize, memUsage, memLimit, fieldTag, fieldVal);

    stateGuard.reset();
    return {true, stateTag, stateVal};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggFirstNMerge(ArityType arity) {
    auto [mergeStateTag, mergeStateVal] = moveOwnedFromStack(0);
    value::ValueGuard mergeStateGuard{mergeStateTag, mergeStateVal};

    auto [stateTag, stateVal] = moveOwnedFromStack(1);
    value::ValueGuard stateGuard{stateTag, stateVal};

    auto [mergeState,
          mergeArray,
          mergeStartIdx,
          mergeMaxSize,
          mergeMemUsage,
          mergeMemLimit,
          mergeIsGroupAccum] = multiAccState(mergeStateTag, mergeStateVal);
    auto [state, array, accStartIdx, accMaxSize, accMemUsage, accMemLimit, accIsGroupAccum] =
        multiAccState(stateTag, stateVal);
    uassert(7548604,
            "Two arrays to merge should have the same MaxSize component",
            accMaxSize == mergeMaxSize);

    for (size_t i = 0; i < array->size(); ++i) {
        if (mergeArray->size() == mergeMaxSize) {
            break;
        }

        auto [tag, val] = array->swapAt(i, value::TypeTags::Null, 0);
        mergeMemUsage =
            aggFirstN(mergeState, mergeArray, mergeMaxSize, mergeMemUsage, mergeMemLimit, tag, val);
    }

    mergeStateGuard.reset();
    return {true, mergeStateTag, mergeStateVal};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggFirstNFinalize(ArityType arity) {
    auto [stateTag, stateVal] = moveOwnedFromStack(0);
    value::ValueGuard guard{stateTag, stateVal};

    uassert(7548605, "expected an array", stateTag == value::TypeTags::Array);
    auto state = value::getArrayView(stateVal);

    auto [isGroupAccTag, isGroupAccVal] =
        state->getAt(static_cast<size_t>(AggMultiElems::kIsGroupAccum));
    auto isGroupAcc = value::bitcastTo<bool>(isGroupAccVal);

    if (isGroupAcc) {
        auto [outputTag, outputVal] = state->swapAt(
            static_cast<size_t>(AggMultiElems::kInternalArr), value::TypeTags::Null, 0);
        return {true, outputTag, outputVal};
    } else {
        auto [arrTag, arrVal] = state->getAt(static_cast<size_t>(AggMultiElems::kInternalArr));
        auto [outputTag, outputVal] = value::copyValue(arrTag, arrVal);
        return {true, outputTag, outputVal};
    }
}

std::pair<size_t, int32_t> aggLastN(value::Array* state,
                                    value::Array* array,
                                    size_t startIdx,
                                    size_t maxSize,
                                    int32_t memUsage,
                                    int32_t memLimit,
                                    value::TypeTags fieldTag,
                                    value::Value fieldVal) {
    value::ValueGuard guard{fieldTag, fieldVal};
    if (array->size() < maxSize) {
        invariant(startIdx == 0);
        guard.reset();
        array->push_back(fieldTag, fieldVal);
    } else {
        invariant(array->size() == maxSize);
        guard.reset();
        auto [oldFieldTag, oldFieldVal] = array->swapAt(startIdx, fieldTag, fieldVal);
        memUsage -= value::getApproximateSize(oldFieldTag, oldFieldVal);
        value::releaseValue(oldFieldTag, oldFieldVal);
        startIdx = updateStartIdx(state, startIdx, maxSize);
    }
    memUsage = updateAndCheckMemUsage(
        state, memUsage, value::getApproximateSize(fieldTag, fieldVal), memLimit);
    return {startIdx, memUsage};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggLastN(ArityType arity) {
    auto [stateTag, stateVal] = moveOwnedFromStack(0);
    value::ValueGuard stateGuard{stateTag, stateVal};

    auto [state, array, startIdx, maxSize, memUsage, memLimit, isGroupAccum] =
        multiAccState(stateTag, stateVal);

    auto [fieldTag, fieldVal] = moveOwnedFromStack(1);
    aggLastN(state, array, startIdx, maxSize, memUsage, memLimit, fieldTag, fieldVal);

    stateGuard.reset();
    return {true, stateTag, stateVal};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggLastNMerge(ArityType arity) {
    auto [mergeStateTag, mergeStateVal] = moveOwnedFromStack(0);
    value::ValueGuard mergeStateGuard{mergeStateTag, mergeStateVal};

    auto [stateTag, stateVal] = moveOwnedFromStack(1);
    value::ValueGuard stateGuard{stateTag, stateVal};

    auto [mergeState,
          mergeArray,
          mergeStartIdx,
          mergeMaxSize,
          mergeMemUsage,
          mergeMemLimit,
          mergeIsGroupAccum] = multiAccState(mergeStateTag, mergeStateVal);
    auto [state, array, startIdx, maxSize, memUsage, memLimit, isGroupAccum] =
        multiAccState(stateTag, stateVal);
    uassert(7548703,
            "Two arrays to merge should have the same MaxSize component",
            maxSize == mergeMaxSize);

    if (array->size() < maxSize) {
        // add values from accArr to mergeArray
        for (size_t i = 0; i < array->size(); ++i) {
            auto [tag, val] = array->swapAt(i, value::TypeTags::Null, 0);
            std::tie(mergeStartIdx, mergeMemUsage) = aggLastN(mergeState,
                                                              mergeArray,
                                                              mergeStartIdx,
                                                              mergeMaxSize,
                                                              mergeMemUsage,
                                                              mergeMemLimit,
                                                              tag,
                                                              val);
        }
        mergeStateGuard.reset();
        return {true, mergeStateTag, mergeStateVal};
    } else {
        // return accArray since it contains last n values
        invariant(array->size() == maxSize);
        stateGuard.reset();
        return {true, stateTag, stateVal};
    }
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggLastNFinalize(ArityType arity) {
    auto [stateTag, stateVal] = moveOwnedFromStack(0);
    value::ValueGuard guard{stateTag, stateVal};

    auto [state, arr, startIdx, maxSize, memUsage, memLimit, isGroupAccum] =
        multiAccState(stateTag, stateVal);
    if (startIdx == 0) {
        if (isGroupAccum) {
            auto [outTag, outVal] = state->swapAt(0, value::TypeTags::Null, 0);
            return {true, outTag, outVal};
        } else {
            auto [arrTag, arrVal] = state->getAt(0);
            auto [outTag, outVal] = value::copyValue(arrTag, arrVal);
            return {true, outTag, outVal};
        }
    }

    invariant(arr->size() == maxSize);
    auto [outArrayTag, outArrayVal] = value::makeNewArray();
    auto outArray = value::getArrayView(outArrayVal);
    outArray->reserve(maxSize);

    if (isGroupAccum) {
        for (size_t i = 0; i < maxSize; ++i) {
            auto srcIdx = (i + startIdx) % maxSize;
            auto [elemTag, elemVal] = arr->swapAt(srcIdx, value::TypeTags::Null, 0);
            outArray->push_back(elemTag, elemVal);
        }
    } else {
        for (size_t i = 0; i < maxSize; ++i) {
            auto srcIdx = (i + startIdx) % maxSize;
            auto [elemTag, elemVal] = arr->getAt(srcIdx);
            auto [copyTag, copyVal] = value::copyValue(elemTag, elemVal);
            outArray->push_back(copyTag, copyVal);
        }
    }
    return {true, outArrayTag, outArrayVal};
}

template <typename Less>
int32_t aggTopBottomNAdd(value::Array* state,
                         value::Array* array,
                         size_t maxSize,
                         int32_t memUsage,
                         int32_t memLimit,
                         const SortSpec* sortSpec,
                         std::pair<value::TypeTags, value::Value> key,
                         std::pair<value::TypeTags, value::Value> output) {
    auto memAdded = [](std::pair<value::TypeTags, value::Value> key,
                       std::pair<value::TypeTags, value::Value> output) {
        return value::getApproximateSize(key.first, key.second) +
            value::getApproximateSize(output.first, output.second);
    };

    value::ValueGuard keyGuard{key.first, key.second};
    value::ValueGuard outputGuard{output.first, output.second};
    auto less = Less(sortSpec);
    auto keyLess = PairKeyComp(less);
    auto& heap = array->values();

    if (array->size() < maxSize) {
        auto [pairTag, pairVal] = value::makeNewArray();
        value::ValueGuard pairGuard{pairTag, pairVal};
        auto pair = value::getArrayView(pairVal);
        pair->reserve(2);
        keyGuard.reset();
        pair->push_back(key.first, key.second);
        outputGuard.reset();
        pair->push_back(output.first, output.second);

        memUsage = updateAndCheckMemUsage(state, memUsage, memAdded(key, output), memLimit);

        pairGuard.reset();
        array->push_back(pairTag, pairVal);
        std::push_heap(heap.begin(), heap.end(), keyLess);
    } else {
        tassert(5807005,
                "Heap should contain same number of elements as MaxSize",
                array->size() == maxSize);

        auto [worstTag, worstVal] = heap.front();
        auto worst = value::getArrayView(worstVal);
        auto worstKey = worst->getAt(0);
        if (less(key, worstKey)) {
            memUsage = updateAndCheckMemUsage(state,
                                              memUsage,
                                              -memAdded(worst->getAt(0), worst->getAt(1)) +
                                                  memAdded(key, output),
                                              memLimit);

            std::pop_heap(heap.begin(), heap.end(), keyLess);
            keyGuard.reset();
            worst->setAt(0, key.first, key.second);
            outputGuard.reset();
            worst->setAt(1, output.first, output.second);
            std::push_heap(heap.begin(), heap.end(), keyLess);
        }
    }

    return memUsage;
}

template <typename Less>
FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggTopBottomN(ArityType arity) {
    auto [sortSpecOwned, sortSpecTag, sortSpecVal] = getFromStack(3);
    tassert(5807024, "Argument must be of sortSpec type", sortSpecTag == value::TypeTags::sortSpec);
    auto sortSpec = value::getSortSpecView(sortSpecVal);

    auto [stateTag, stateVal] = moveOwnedFromStack(0);
    value::ValueGuard stateGuard{stateTag, stateVal};
    auto [state, array, startIdx, maxSize, memUsage, memLimit, isGroupAccum] =
        multiAccState(stateTag, stateVal);
    auto key = moveOwnedFromStack(1);
    auto output = moveOwnedFromStack(2);

    aggTopBottomNAdd<Less>(state, array, maxSize, memUsage, memLimit, sortSpec, key, output);

    stateGuard.reset();
    return {true, stateTag, stateVal};
}

template <typename Less>
FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggTopBottomNMerge(
    ArityType arity) {
    auto [sortSpecOwned, sortSpecTag, sortSpecVal] = getFromStack(2);
    tassert(5807025, "Argument must be of sortSpec type", sortSpecTag == value::TypeTags::sortSpec);
    auto sortSpec = value::getSortSpecView(sortSpecVal);

    auto [stateTag, stateVal] = moveOwnedFromStack(1);
    value::ValueGuard stateGuard{stateTag, stateVal};
    auto [mergeStateTag, mergeStateVal] = moveOwnedFromStack(0);
    value::ValueGuard mergeStateGuard{mergeStateTag, mergeStateVal};
    auto [mergeState,
          mergeArray,
          mergeStartIx,
          mergeMaxSize,
          mergeMemUsage,
          mergeMemLimit,
          mergeIsGroupAccum] = multiAccState(mergeStateTag, mergeStateVal);
    auto [state, array, startIdx, maxSize, memUsage, memLimit, isGroupAccum] =
        multiAccState(stateTag, stateVal);
    tassert(5807008,
            "Two arrays to merge should have the same MaxSize component",
            maxSize == mergeMaxSize);

    for (auto [pairTag, pairVal] : array->values()) {
        auto pair = value::getArrayView(pairVal);
        auto key = pair->swapAt(0, value::TypeTags::Null, 0);
        auto output = pair->swapAt(1, value::TypeTags::Null, 0);
        mergeMemUsage = aggTopBottomNAdd<Less>(mergeState,
                                               mergeArray,
                                               mergeMaxSize,
                                               mergeMemUsage,
                                               mergeMemLimit,
                                               sortSpec,
                                               key,
                                               output);
    }

    mergeStateGuard.reset();
    return {true, mergeStateTag, mergeStateVal};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggTopBottomNFinalize(
    ArityType arity) {
    auto [sortSpecOwned, sortSpecTag, sortSpecVal] = getFromStack(1);
    tassert(5807026, "Argument must be of sortSpec type", sortSpecTag == value::TypeTags::sortSpec);
    auto sortSpec = value::getSortSpecView(sortSpecVal);

    auto [stateTag, stateVal] = moveOwnedFromStack(0);
    value::ValueGuard stateGuard{stateTag, stateVal};
    auto [state, array, startIdx, maxSize, memUsage, memLimit, isGroupAccum] =
        multiAccState(stateTag, stateVal);

    auto [outputArrayTag, outputArrayVal] = value::makeNewArray();
    value::ValueGuard outputArrayGuard{outputArrayTag, outputArrayVal};
    auto outputArray = value::getArrayView(outputArrayVal);
    outputArray->reserve(array->size());

    // We always output result in the order of sort pattern in according to MQL semantics.
    auto less = SortPatternLess(sortSpec);
    auto keyLess = PairKeyComp(less);
    std::sort(array->values().begin(), array->values().end(), keyLess);
    for (size_t i = 0; i < array->size(); ++i) {
        auto pair = value::getArrayView(array->getAt(i).second);
        if (isGroupAccum) {
            auto [outTag, outVal] = pair->swapAt(1, value::TypeTags::Null, 0);
            outputArray->push_back(outTag, outVal);
        } else {
            auto [outTag, outVal] = pair->getAt(1);
            auto [copyTag, copyVal] = value::copyValue(outTag, outVal);
            outputArray->push_back(copyTag, copyVal);
        }
    }

    outputArrayGuard.reset();
    return {true, outputArrayTag, outputArrayVal};
}

template <AccumulatorMinMaxN::MinMaxSense S>
int32_t aggMinMaxN(value::Array* state,
                   value::Array* array,
                   size_t maxSize,
                   int32_t memUsage,
                   int32_t memLimit,
                   const CollatorInterface* collator,
                   value::TypeTags fieldTag,
                   value::Value fieldVal) {
    value::ValueGuard guard{fieldTag, fieldVal};
    auto& heap = array->values();

    constexpr auto less = []() -> bool {
        if constexpr (S == AccumulatorMinMaxN::MinMaxSense::kMax) {
            return false;
        }
        return true;
    }();
    value::ValueCompare<less> comp{collator};

    if (array->size() < maxSize) {
        memUsage = updateAndCheckMemUsage(
            state, memUsage, value::getApproximateSize(fieldTag, fieldVal), memLimit);
        guard.reset();

        array->push_back(fieldTag, fieldVal);
        std::push_heap(heap.begin(), heap.end(), comp);
    } else {
        uassert(7548800,
                "Heap should contain same number of elements as MaxSize",
                array->size() == maxSize);

        auto heapRoot = heap.front();
        if (comp({fieldTag, fieldVal}, heapRoot)) {
            memUsage =
                updateAndCheckMemUsage(state,
                                       memUsage,
                                       -value::getApproximateSize(heapRoot.first, heapRoot.second) +
                                           value::getApproximateSize(fieldTag, fieldVal),
                                       memLimit);
            std::pop_heap(heap.begin(), heap.end(), comp);
            guard.reset();
            array->setAt(maxSize - 1, fieldTag, fieldVal);
            std::push_heap(heap.begin(), heap.end(), comp);
        }
    }

    return memUsage;
}

template <AccumulatorMinMaxN::MinMaxSense S>
FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggMinMaxN(ArityType arity) {
    invariant(arity == 2 || arity == 3);

    auto [stateTag, stateVal] = moveOwnedFromStack(0);
    value::ValueGuard stateGuard{stateTag, stateVal};

    auto [fieldTag, fieldVal] = moveOwnedFromStack(1);
    value::ValueGuard fieldGuard{fieldTag, fieldVal};
    if (value::isNullish(fieldTag)) {
        stateGuard.reset();
        return {true, stateTag, stateVal};
    }

    auto [state, array, startIdx, maxSize, memUsage, memLimit, isGroupAccum] =
        multiAccState(stateTag, stateVal);

    CollatorInterface* collator = nullptr;
    if (arity == 3) {
        auto [collOwned, collTag, collVal] = getFromStack(2);
        uassert(7548802, "expected a collator argument", collTag == value::TypeTags::collator);
        collator = value::getCollatorView(collVal);
    }
    fieldGuard.reset();
    aggMinMaxN<S>(state, array, maxSize, memUsage, memLimit, collator, fieldTag, fieldVal);

    stateGuard.reset();
    return {true, stateTag, stateVal};
}

template <AccumulatorMinMaxN::MinMaxSense S>
FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggMinMaxNMerge(ArityType arity) {
    invariant(arity == 2 || arity == 3);

    auto [mergeStateTag, mergeStateVal] = moveOwnedFromStack(0);
    value::ValueGuard mergeStateGuard{mergeStateTag, mergeStateVal};

    auto [stateTag, stateVal] = moveOwnedFromStack(1);
    value::ValueGuard stateGuard{stateTag, stateVal};

    auto [mergeState,
          mergeArray,
          mergeStartIdx,
          mergeMaxSize,
          mergeMemUsage,
          mergeMemLimit,
          mergeIsGroupAccum] = multiAccState(mergeStateTag, mergeStateVal);
    auto [state, array, startIdx, maxSize, memUsage, memLimit, isGroupAccum] =
        multiAccState(stateTag, stateVal);
    uassert(7548801,
            "Two arrays to merge should have the same MaxSize component",
            maxSize == mergeMaxSize);

    CollatorInterface* collator = nullptr;
    if (arity == 3) {
        auto [collOwned, collTag, collVal] = getFromStack(2);
        uassert(7548803, "expected a collator argument", collTag == value::TypeTags::collator);
        collator = value::getCollatorView(collVal);
    }

    for (size_t i = 0; i < array->size(); ++i) {
        auto [tag, val] = array->swapAt(i, value::TypeTags::Null, 0);
        mergeMemUsage = aggMinMaxN<S>(
            mergeState, mergeArray, mergeMaxSize, mergeMemUsage, mergeMemLimit, collator, tag, val);
    }

    mergeStateGuard.reset();
    return {true, mergeStateTag, mergeStateVal};
}

template <AccumulatorMinMaxN::MinMaxSense S>
FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggMinMaxNFinalize(
    ArityType arity) {
    invariant(arity == 2 || arity == 1);
    auto [stateTag, stateVal] = moveOwnedFromStack(0);
    value::ValueGuard stateGuard{stateTag, stateVal};

    auto [state, array, startIdx, maxSize, memUsage, memLimit, isGroupAccum] =
        multiAccState(stateTag, stateVal);

    CollatorInterface* collator = nullptr;
    if (arity == 2) {
        auto [collOwned, collTag, collVal] = getFromStack(1);
        uassert(7548804, "expected a collator argument", collTag == value::TypeTags::collator);
        collator = value::getCollatorView(collVal);
    }

    constexpr auto less = []() -> bool {
        if constexpr (S == AccumulatorMinMaxN::MinMaxSense::kMax) {
            return false;
        }
        return true;
    }();
    value::ValueCompare<less> comp{collator};
    std::sort(array->values().begin(), array->values().end(), comp);
    if (isGroupAccum) {
        auto [arrayTag, arrayVal] = state->swapAt(
            static_cast<size_t>(AggMultiElems::kInternalArr), value::TypeTags::Null, 0);
        return {true, arrayTag, arrayVal};
    } else {
        auto [arrTag, arrVal] = state->getAt(0);
        auto [outTag, outVal] = value::copyValue(arrTag, arrVal);
        return {true, outTag, outVal};
    }
}

std::tuple<value::Array*,
           std::pair<value::TypeTags, value::Value>,
           bool,
           int64_t,
           int64_t,
           SortSpec*>
rankState(value::TypeTags stateTag, value::Value stateVal) {
    uassert(
        7795500, "The accumulator state should be an array", stateTag == value::TypeTags::Array);
    auto state = value::getArrayView(stateVal);

    uassert(7795501,
            "The accumulator state should have correct number of elements",
            state->size() == AggRankElems::kRankArraySize);

    auto lastValue = state->getAt(AggRankElems::kLastValue);
    auto [lastValueIsNothingTag, lastValueIsNothingVal] =
        state->getAt(AggRankElems::kLastValueIsNothing);
    auto [lastRankTag, lastRankVal] = state->getAt(AggRankElems::kLastRank);
    auto [sameRankCountTag, sameRankCountVal] = state->getAt(AggRankElems::kSameRankCount);
    auto [sortSpecTag, sortSpecVal] = state->getAt(AggRankElems::kSortSpec);

    uassert(8188900,
            "Last rank is nothing component should be a boolean",
            lastValueIsNothingTag == value::TypeTags::Boolean);
    auto lastValueIsNothing = value::bitcastTo<bool>(lastValueIsNothingVal);

    uassert(7795502,
            "Last rank component should be a 64-bit integer",
            lastRankTag == value::TypeTags::NumberInt64);
    auto lastRank = value::bitcastTo<int64_t>(lastRankVal);

    uassert(7795503,
            "Same rank component should be a 64-bit integer",
            sameRankCountTag == value::TypeTags::NumberInt64);
    auto sameRankCount = value::bitcastTo<int64_t>(sameRankCountVal);

    uassert(8216800,
            "Sort spec component should be a sort spec object",
            sortSpecTag == value::TypeTags::sortSpec);
    auto sortSpec = value::getSortSpecView(sortSpecVal);

    return {state, lastValue, lastValueIsNothing, lastRank, sameRankCount, sortSpec};
}

FastTuple<bool, value::TypeTags, value::Value> builtinAggRankImpl(
    value::TypeTags stateTag,
    value::Value stateVal,
    bool valueOwned,
    value::TypeTags valueTag,
    value::Value valueVal,
    bool isAscending,
    bool dense,
    CollatorInterface* collator = nullptr) {

    const char* kTempSortKeyField = "sortKey";
    // Initialize the accumulator.
    if (stateTag == value::TypeTags::Nothing) {
        auto [newStateTag, newStateVal] = value::makeNewArray();
        value::ValueGuard newStateGuard{newStateTag, newStateVal};
        auto newState = value::getArrayView(newStateVal);
        newState->reserve(AggRankElems::kRankArraySize);
        if (!valueOwned) {
            std::tie(valueTag, valueVal) = value::copyValue(valueTag, valueVal);
        }
        if (valueTag == value::TypeTags::Nothing) {
            newState->push_back(value::TypeTags::Null, 0);  // kLastValue
            newState->push_back(value::TypeTags::Boolean,
                                value::bitcastFrom<bool>(true));  // kLastValueIsNothing
        } else {
            newState->push_back(valueTag, valueVal);  // kLastValue
            newState->push_back(value::TypeTags::Boolean,
                                value::bitcastFrom<bool>(false));  // kLastValueIsNothing
        }
        newState->push_back(value::TypeTags::NumberInt64, 1);  // kLastRank
        newState->push_back(value::TypeTags::NumberInt64, 1);  // kSameRankCount

        auto sortSpec =
            std::make_unique<SortSpec>(BSON(kTempSortKeyField << (isAscending ? 1 : -1)));
        newState->push_back(value::TypeTags::sortSpec,
                            value::bitcastFrom<SortSpec*>(sortSpec.release()));  // kSortSpec
        newStateGuard.reset();
        return {true, newStateTag, newStateVal};
    }

    value::ValueGuard stateGuard{stateTag, stateVal};
    auto [state, lastValue, lastValueIsNothing, lastRank, sameRankCount, sortSpec] =
        rankState(stateTag, stateVal);
    // Update the last value to Nothing before comparison if the flag is set.
    if (lastValueIsNothing) {
        lastValue.first = value::TypeTags::Nothing;
        lastValue.second = 0;
    }

    // Define sort-order compliant comparison function which uses fast pass logic for null and
    // missing and full sort key logic for arrays.
    auto isSameValue = [&](SortSpec* keyGen,
                           std::pair<value::TypeTags, value::Value> currValue,
                           std::pair<value::TypeTags, value::Value> lastValue) {
        if (value::isNullish(currValue.first) && value::isNullish(lastValue.first)) {
            return true;
        }
        if (value::isArray(currValue.first) || value::isArray(lastValue.first)) {
            auto getSortKey = [&](value::TypeTags tag, value::Value val) {
                BSONObjBuilder builder;
                bson::appendValueToBsonObj(builder, kTempSortKeyField, tag, val);
                return keyGen->generateSortKey(builder.obj(), collator);
            };
            auto currKey = getSortKey(currValue.first, currValue.second);
            auto lastKey = getSortKey(lastValue.first, lastValue.second);
            return currKey.compare(lastKey) == 0;
        }
        auto [compareTag, compareVal] = value::compareValue(
            currValue.first, currValue.second, lastValue.first, lastValue.second, collator);
        return compareTag == value::TypeTags::NumberInt32 && compareVal == 0;
    };

    if (isSameValue(sortSpec, std::make_pair(valueTag, valueVal), lastValue)) {
        state->setAt(AggRankElems::kSameRankCount, value::TypeTags::NumberInt64, sameRankCount + 1);
    } else {
        if (!valueOwned) {
            std::tie(valueTag, valueVal) = value::copyValue(valueTag, valueVal);
        }
        if (valueTag == value::TypeTags::Nothing) {
            state->setAt(AggRankElems::kLastValue, value::TypeTags::Null, 0);
            state->setAt(AggRankElems::kLastValueIsNothing,
                         value::TypeTags::Boolean,
                         value::bitcastFrom<bool>(true));
        } else {
            state->setAt(AggRankElems::kLastValue, valueTag, valueVal);
            state->setAt(AggRankElems::kLastValueIsNothing,
                         value::TypeTags::Boolean,
                         value::bitcastFrom<bool>(false));
        }
        state->setAt(AggRankElems::kLastRank,
                     value::TypeTags::NumberInt64,
                     dense ? lastRank + 1 : lastRank + sameRankCount);
        state->setAt(AggRankElems::kSameRankCount, value::TypeTags::NumberInt64, 1);
    }
    stateGuard.reset();
    return {true, stateTag, stateVal};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggRank(ArityType arity) {
    invariant(arity == 3);
    auto [isAscendingOwned, isAscendingTag, isAscendingVal] = getFromStack(2);
    auto [valueOwned, valueTag, valueVal] = getFromStack(1);
    auto [stateTag, stateVal] = moveOwnedFromStack(0);

    tassert(8216803,
            "Incorrect value type passed to aggRank for 'isAscending' parameter.",
            isAscendingTag == value::TypeTags::Boolean);
    auto isAscending = value::bitcastTo<bool>(isAscendingVal);

    return builtinAggRankImpl(
        stateTag, stateVal, valueOwned, valueTag, valueVal, isAscending, false /* dense */);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggRankColl(ArityType arity) {
    invariant(arity == 4);
    auto [collatorOwned, collatorTag, collatorVal] = getFromStack(3);
    auto [isAscendingOwned, isAscendingTag, isAscendingVal] = getFromStack(2);
    auto [valueOwned, valueTag, valueVal] = getFromStack(1);
    auto [stateTag, stateVal] = moveOwnedFromStack(0);

    tassert(8216804,
            "Incorrect value type passed to aggRankColl for 'isAscending' parameter.",
            isAscendingTag == value::TypeTags::Boolean);
    auto isAscending = value::bitcastTo<bool>(isAscendingVal);

    tassert(7795504,
            "Incorrect value type passed to aggRankColl for collator.",
            collatorTag == value::TypeTags::collator);
    auto collator = value::getCollatorView(collatorVal);

    return builtinAggRankImpl(stateTag,
                              stateVal,
                              valueOwned,
                              valueTag,
                              valueVal,
                              isAscending,
                              false /* dense */,
                              collator);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggDenseRank(ArityType arity) {
    invariant(arity == 3);
    auto [isAscendingOwned, isAscendingTag, isAscendingVal] = getFromStack(2);
    auto [valueOwned, valueTag, valueVal] = getFromStack(1);
    auto [stateTag, stateVal] = moveOwnedFromStack(0);

    tassert(8216805,
            "Incorrect value type passed to aggDenseRank for 'isAscending' parameter.",
            isAscendingTag == value::TypeTags::Boolean);
    auto isAscending = value::bitcastTo<bool>(isAscendingVal);

    return builtinAggRankImpl(
        stateTag, stateVal, valueOwned, valueTag, valueVal, isAscending, true /* dense */);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggDenseRankColl(ArityType arity) {
    invariant(arity == 4);
    auto [collatorOwned, collatorTag, collatorVal] = getFromStack(3);
    auto [isAscendingOwned, isAscendingTag, isAscendingVal] = getFromStack(2);
    auto [valueOwned, valueTag, valueVal] = getFromStack(1);
    auto [stateTag, stateVal] = moveOwnedFromStack(0);

    tassert(8216806,
            "Incorrect value type passed to aggDenseRankColl for 'isAscending' parameter.",
            isAscendingTag == value::TypeTags::Boolean);
    auto isAscending = value::bitcastTo<bool>(isAscendingVal);

    tassert(7795505,
            "Incorrect value type passed to aggDenseRankColl for collator.",
            collatorTag == value::TypeTags::collator);
    auto collator = value::getCollatorView(collatorVal);

    return builtinAggRankImpl(stateTag,
                              stateVal,
                              valueOwned,
                              valueTag,
                              valueVal,
                              isAscending,
                              true /* dense */,
                              collator);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggRankFinalize(ArityType arity) {
    invariant(arity == 1);
    auto [stateOwned, stateTag, stateVal] = getFromStack(0);
    auto [state, lastValue, lastValueIsNothing, lastRank, sameRankCount, sortSpec] =
        rankState(stateTag, stateVal);
    return {true, value::TypeTags::NumberInt64, value::bitcastFrom<int64_t>(lastRank)};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggExpMovingAvg(ArityType arity) {
    auto [stateTag, stateVal] = moveOwnedFromStack(0);
    value::ValueGuard stateGuard{stateTag, stateVal};

    auto [fieldOwned, fieldTag, fieldVal] = getFromStack(1);
    if (!value::isNumber(fieldTag)) {
        stateGuard.reset();
        return {true, stateTag, stateVal};
    }

    uassert(7821200, "State should be of array type", stateTag == value::TypeTags::Array);
    auto state = value::getArrayView(stateVal);
    uassert(7821201,
            "Unexpected state array size",
            state->size() == static_cast<size_t>(AggExpMovingAvgElems::kSizeOfArray));

    auto [alphaTag, alphaVal] = state->getAt(static_cast<size_t>(AggExpMovingAvgElems::kAlpha));
    uassert(7821202, "alpha is not of decimal type", alphaTag == value::TypeTags::NumberDecimal);
    auto alpha = value::bitcastTo<Decimal128>(alphaVal);

    value::TypeTags currentResultTag;
    value::Value currentResultVal;
    std::tie(currentResultTag, currentResultVal) =
        state->getAt(static_cast<size_t>(AggExpMovingAvgElems::kResult));

    auto decimalVal = value::numericCast<Decimal128>(fieldTag, fieldVal);
    auto result = [&]() {
        if (currentResultTag == value::TypeTags::Null) {
            // Accumulator result has not been yet initialised. We will now
            // set it to decimalVal
            return decimalVal;
        } else {
            uassert(7821203,
                    "currentResultTag is not of decimal type",
                    currentResultTag == value::TypeTags::NumberDecimal);
            auto currentResult = value::bitcastTo<Decimal128>(currentResultVal);
            currentResult = decimalVal.multiply(alpha).add(
                currentResult.multiply(Decimal128(1).subtract(alpha)));
            return currentResult;
        }
    }();

    auto [resultTag, resultVal] = value::makeCopyDecimal(result);

    state->setAt(static_cast<size_t>(AggExpMovingAvgElems::kResult), resultTag, resultVal);
    if (fieldTag == value::TypeTags::NumberDecimal) {
        state->setAt(static_cast<size_t>(AggExpMovingAvgElems::kIsDecimal),
                     value::TypeTags::Boolean,
                     value::bitcastFrom<bool>(true));
    }

    stateGuard.reset();
    return {true, stateTag, stateVal};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggExpMovingAvgFinalize(
    ArityType arity) {
    auto [stateOwned, stateTag, stateVal] = getFromStack(0);

    uassert(7821204, "State should be of array type", stateTag == value::TypeTags::Array);
    auto state = value::getArrayView(stateVal);

    auto [resultTag, resultVal] = state->getAt(static_cast<size_t>(AggExpMovingAvgElems::kResult));
    if (resultTag == value::TypeTags::Null) {
        return {false, value::TypeTags::Null, 0};
    }
    uassert(7821205, "Unexpected result type", resultTag == value::TypeTags::NumberDecimal);

    auto [isDecimalTag, isDecimalVal] =
        state->getAt(static_cast<size_t>(AggExpMovingAvgElems::kIsDecimal));
    uassert(7821206, "Unexpected isDecimal type", isDecimalTag == value::TypeTags::Boolean);

    if (value::bitcastTo<bool>(isDecimalVal)) {
        std::tie(resultTag, resultVal) = value::copyValue(resultTag, resultVal);
        return {true, resultTag, resultVal};
    } else {
        auto result = value::bitcastTo<Decimal128>(resultVal).toDouble();
        return {false, value::TypeTags::NumberDouble, value::bitcastFrom<double>(result)};
    }
}

std::tuple<value::Array*, int64_t, int64_t, int64_t, int64_t, int64_t> removableSumState(
    value::Array* state) {
    uassert(7795101,
            "incorrect size of state array",
            state->size() == static_cast<size_t>(AggRemovableSumElems::kSizeOfArray));

    auto [sumAccTag, sumAccVal] = state->getAt(static_cast<size_t>(AggRemovableSumElems::kSumAcc));
    uassert(7795102,
            "sum accumulator elem should be of array type",
            sumAccTag == value::TypeTags::Array);
    auto sumAcc = value::getArrayView(sumAccVal);

    auto [nanCountTag, nanCountVal] =
        state->getAt(static_cast<size_t>(AggRemovableSumElems::kNanCount));
    uassert(7795103,
            "nanCount elem should be of int64 type",
            nanCountTag == value::TypeTags::NumberInt64);
    auto nanCount = value::bitcastTo<int64_t>(nanCountVal);

    auto [posInfinityCountTag, posInfinityCountVal] =
        state->getAt(static_cast<size_t>(AggRemovableSumElems::kPosInfinityCount));
    uassert(7795104,
            "posInfinityCount elem should be of int64 type",
            posInfinityCountTag == value::TypeTags::NumberInt64);
    auto posInfinityCount = value::bitcastTo<int64_t>(posInfinityCountVal);

    auto [negInfinityCountTag, negInfinityCountVal] =
        state->getAt(static_cast<size_t>(AggRemovableSumElems::kNegInfinityCount));
    uassert(7795105,
            "negInfinityCount elem should be of int64 type",
            negInfinityCountTag == value::TypeTags::NumberInt64);
    auto negInfinityCount = value::bitcastTo<int64_t>(negInfinityCountVal);

    auto [doubleCountTag, doubleCountVal] =
        state->getAt(static_cast<size_t>(AggRemovableSumElems::kDoubleCount));
    uassert(7795106,
            "doubleCount elem should be of int64 type",
            doubleCountTag == value::TypeTags::NumberInt64);
    auto doubleCount = value::bitcastTo<int64_t>(doubleCountVal);

    auto [decimalCountTag, decimalCountVal] =
        state->getAt(static_cast<size_t>(AggRemovableSumElems::kDecimalCount));
    uassert(7795107,
            "decimalCount elem should be of int64 type",
            decimalCountTag == value::TypeTags::NumberInt64);
    auto decimalCount = value::bitcastTo<int64_t>(decimalCountVal);

    return {sumAcc, nanCount, posInfinityCount, negInfinityCount, doubleCount, decimalCount};
}

void updateRemovableSumState(value::Array* state,
                             int64_t nanCount,
                             int64_t posInfinityCount,
                             int64_t negInfinityCount,
                             int64_t doubleCount,
                             int64_t decimalCount) {
    state->setAt(static_cast<size_t>(AggRemovableSumElems::kNanCount),
                 value::TypeTags::NumberInt64,
                 value::bitcastFrom<int64_t>(nanCount));
    state->setAt(static_cast<size_t>(AggRemovableSumElems::kPosInfinityCount),
                 value::TypeTags::NumberInt64,
                 value::bitcastFrom<int64_t>(posInfinityCount));
    state->setAt(static_cast<size_t>(AggRemovableSumElems::kNegInfinityCount),
                 value::TypeTags::NumberInt64,
                 value::bitcastFrom<int64_t>(negInfinityCount));
    state->setAt(static_cast<size_t>(AggRemovableSumElems::kDoubleCount),
                 value::TypeTags::NumberInt64,
                 value::bitcastFrom<int64_t>(doubleCount));
    state->setAt(static_cast<size_t>(AggRemovableSumElems::kDecimalCount),
                 value::TypeTags::NumberInt64,
                 value::bitcastFrom<int64_t>(decimalCount));
}

template <class T, int sign>
void ByteCode::updateRemovableSumAccForIntegerType(value::Array* sumAcc,
                                                   value::TypeTags rhsTag,
                                                   value::Value rhsVal) {
    auto value = value::bitcastTo<T>(rhsVal);
    if (value == std::numeric_limits<T>::min() && sign == -1) {
        // Avoid overflow by processing in two parts.
        aggDoubleDoubleSumImpl(sumAcc, rhsTag, std::numeric_limits<T>::max());
        aggDoubleDoubleSumImpl(sumAcc, rhsTag, value::bitcastFrom<T>(1));
    } else {
        aggDoubleDoubleSumImpl(sumAcc, rhsTag, value::bitcastFrom<T>(value * sign));
    }
}

void aggRemovableSumReset(value::Array* state) {
    auto [sumAccTag, sumAccVal] = state->getAt(static_cast<size_t>(AggRemovableSumElems::kSumAcc));
    tassert(7820807,
            "sum accumulator elem should be of array type",
            sumAccTag == value::TypeTags::Array);
    auto sumAcc = value::getArrayView(sumAccVal);
    resetDoubleDoubleSumState(sumAcc);
    updateRemovableSumState(state, 0, 0, 0, 0, 0);
}

template <int sign>
void ByteCode::aggRemovableSumImpl(value::Array* state,
                                   value::TypeTags rhsTag,
                                   value::Value rhsVal) {
    static_assert(sign == 1 || sign == -1);
    if (!value::isNumber(rhsTag)) {
        return;
    }

    auto [sumAcc, nanCount, posInfinityCount, negInfinityCount, doubleCount, decimalCount] =
        removableSumState(state);

    if (rhsTag == value::TypeTags::NumberInt32) {
        updateRemovableSumAccForIntegerType<int32_t, sign>(sumAcc, rhsTag, rhsVal);
    } else if (rhsTag == value::TypeTags::NumberInt64) {
        updateRemovableSumAccForIntegerType<int64_t, sign>(sumAcc, rhsTag, rhsVal);
    } else if (rhsTag == value::TypeTags::NumberDouble) {
        doubleCount += sign;
        auto value = value::bitcastTo<double>(rhsVal);
        if (std::isnan(value)) {
            nanCount += sign;
        } else if (value == std::numeric_limits<double>::infinity()) {
            posInfinityCount += sign;
        } else if (value == -std::numeric_limits<double>::infinity()) {
            negInfinityCount += sign;
        } else {
            if constexpr (sign == -1) {
                value *= -1;
            }
            aggDoubleDoubleSumImpl(
                sumAcc, value::TypeTags::NumberDouble, value::bitcastFrom<double>(value));
        }
        updateRemovableSumState(
            state, nanCount, posInfinityCount, negInfinityCount, doubleCount, decimalCount);
    } else if (rhsTag == value::TypeTags::NumberDecimal) {
        decimalCount += sign;
        auto value = value::bitcastTo<Decimal128>(rhsVal);
        if (value.isNaN()) {
            nanCount += sign;
        } else if (value.isInfinite() && !value.isNegative()) {
            posInfinityCount += sign;
        } else if (value.isInfinite() && value.isNegative()) {
            negInfinityCount += sign;
        } else {
            if constexpr (sign == -1) {
                auto [negDecTag, negDecVal] = value::makeCopyDecimal(value.negate());
                aggDoubleDoubleSumImpl(sumAcc, negDecTag, negDecVal);
                value::releaseValue(negDecTag, negDecVal);
            } else {
                aggDoubleDoubleSumImpl(sumAcc, rhsTag, rhsVal);
            }
        }
        updateRemovableSumState(
            state, nanCount, posInfinityCount, negInfinityCount, doubleCount, decimalCount);
    } else {
        MONGO_UNREACHABLE;
    }
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::aggRemovableSumFinalizeImpl(
    value::Array* state) {
    auto [sumAcc, nanCount, posInfinityCount, negInfinityCount, doubleCount, decimalCount] =
        removableSumState(state);

    if (nanCount > 0) {
        if (decimalCount > 0) {
            return {true,
                    value::TypeTags::NumberDecimal,
                    value::makeCopyDecimal(Decimal128::kPositiveNaN).second};
        } else {
            return {false,
                    value::TypeTags::NumberDouble,
                    value::bitcastFrom<double>(std::numeric_limits<double>::quiet_NaN())};
        }
    }
    if (posInfinityCount > 0 && negInfinityCount > 0) {
        if (decimalCount > 0) {
            return {true,
                    value::TypeTags::NumberDecimal,
                    value::makeCopyDecimal(Decimal128::kPositiveNaN).second};
        } else {
            return {false,
                    value::TypeTags::NumberDouble,
                    value::bitcastFrom<double>(std::numeric_limits<double>::quiet_NaN())};
        }
    }
    if (posInfinityCount > 0) {
        if (decimalCount > 0) {
            return {true,
                    value::TypeTags::NumberDecimal,
                    value::makeCopyDecimal(Decimal128::kPositiveInfinity).second};
        } else {
            return {false,
                    value::TypeTags::NumberDouble,
                    value::bitcastFrom<double>(std::numeric_limits<double>::infinity())};
        }
    }
    if (negInfinityCount > 0) {
        if (decimalCount > 0) {
            return {true,
                    value::TypeTags::NumberDecimal,
                    value::makeCopyDecimal(Decimal128::kNegativeInfinity).second};
        } else {
            return {false,
                    value::TypeTags::NumberDouble,
                    value::bitcastFrom<double>(-std::numeric_limits<double>::infinity())};
        }
    }

    auto [sumOwned, sumTag, sumVal] = aggDoubleDoubleSumFinalizeImpl(sumAcc);
    value::ValueGuard sumGuard{sumOwned, sumTag, sumVal};

    if (sumTag == value::TypeTags::NumberDecimal && decimalCount == 0) {
        auto decimalVal = value::bitcastTo<Decimal128>(sumVal);
        if (doubleCount > 0) {  // Narrow Decimal128 to double.
            return {false,
                    value::TypeTags::NumberDouble,
                    value::bitcastFrom<double>(decimalVal.toDouble())};
        }
        std::uint32_t signalingFlags = Decimal128::SignalingFlag::kNoFlag;
        auto longVal = decimalVal.toLong(&signalingFlags);  // Narrow Decimal128 to integral.
        if (signalingFlags == Decimal128::SignalingFlag::kNoFlag) {
            auto [numTag, numVal] = value::makeIntOrLong(longVal);
            return {false, numTag, numVal};
        }
        // Narrow Decimal128 to double if overflows long.
        return {false,
                value::TypeTags::NumberDouble,
                value::bitcastFrom<double>(decimalVal.toDouble())};
    }
    if (sumTag == value::TypeTags::NumberDouble && doubleCount == 0 &&
        value::bitcastTo<double>(sumVal) >= std::numeric_limits<long long>::min() &&
        value::bitcastTo<double>(sumVal) <
            static_cast<double>(std::numeric_limits<long long>::max())) {
        // Narrow double to integral.
        auto longVal = llround(value::bitcastTo<double>(sumVal));
        auto [numTag, numVal] = value::makeIntOrLong(longVal);
        return {false, numTag, numVal};
    }
    if (sumTag == value::TypeTags::NumberInt64) {  // Narrow long to int
        auto longVal = value::bitcastTo<long long>(sumVal);
        auto [numTag, numVal] = value::makeIntOrLong(longVal);
        return {false, numTag, numVal};
    }
    sumGuard.reset();
    return {sumOwned, sumTag, sumVal};
}

std::pair<value::TypeTags, value::Value> initializeRemovableSumState() {
    auto [stateTag, stateVal] = value::makeNewArray();
    value::ValueGuard newStateGuard{stateTag, stateVal};
    auto state = value::getArrayView(stateVal);
    state->reserve(static_cast<size_t>(AggRemovableSumElems::kSizeOfArray));

    auto [sumAccTag, sumAccVal] = initializeDoubleDoubleSumState();
    state->push_back(sumAccTag, sumAccVal);  // kSumAcc
    state->push_back(value::TypeTags::NumberInt64,
                     value::bitcastFrom<int64_t>(0));  // kNanCount
    state->push_back(value::TypeTags::NumberInt64,
                     value::bitcastFrom<int64_t>(0));  // kPosInfinityCount
    state->push_back(value::TypeTags::NumberInt64,
                     value::bitcastFrom<int64_t>(0));  // kNegInfinityCount
    state->push_back(value::TypeTags::NumberInt64,
                     value::bitcastFrom<int64_t>(0));  // kDoubleCount
    state->push_back(value::TypeTags::NumberInt64,
                     value::bitcastFrom<int64_t>(0));  // kDecimalCount
    newStateGuard.reset();
    return {stateTag, stateVal};
}

template <int sign>
FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggRemovableSum(ArityType arity) {
    auto [stateTag, stateVal] = moveOwnedFromStack(0);
    auto [_, fieldTag, fieldVal] = getFromStack(1);

    // Initialize the accumulator.
    if (stateTag == value::TypeTags::Nothing) {
        std::tie(stateTag, stateVal) = initializeRemovableSumState();
    }

    value::ValueGuard stateGuard{stateTag, stateVal};
    uassert(7795108, "state should be of array type", stateTag == value::TypeTags::Array);
    auto state = value::getArrayView(stateVal);

    aggRemovableSumImpl<sign>(state, fieldTag, fieldVal);

    stateGuard.reset();
    return {true, stateTag, stateVal};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggRemovableSumFinalize(
    ArityType arity) {
    auto [_, stateTag, stateVal] = getFromStack(0);

    uassert(7795109, "state should be of array type", stateTag == value::TypeTags::Array);
    auto state = value::getArrayView(stateVal);
    return aggRemovableSumFinalizeImpl(state);
}

/**
 * Functions that operate on `ArrayQueue`
 */
// Get the underlying array, and start index and end index that demarcates the queue
std::tuple<value::Array*, size_t, size_t> getArrayQueueState(value::Array* arrayQueue) {
    auto [arrayTag, arrayVal] = arrayQueue->getAt(static_cast<size_t>(ArrayQueueElems::kArray));
    uassert(7821100, "Expected an array", arrayTag == value::TypeTags::Array);
    auto array = value::getArrayView(arrayVal);
    auto size = array->size();
    uassert(7821116, "Expected non-empty array", size > 0);

    auto [startIdxTag, startIdxVal] =
        arrayQueue->getAt(static_cast<size_t>(ArrayQueueElems::kStartIdx));
    uassert(7821101, "Expected NumberInt64 type", startIdxTag == value::TypeTags::NumberInt64);
    auto startIdx = value::bitcastTo<size_t>(startIdxVal);
    uassert(7821114,
            str::stream() << "Invalid startIdx " << startIdx << " with array size " << size,
            startIdx < size);

    auto [queueSizeTag, queueSizeVal] =
        arrayQueue->getAt(static_cast<size_t>(ArrayQueueElems::kQueueSize));
    uassert(7821102, "Expected NumberInt64 type", queueSizeTag == value::TypeTags::NumberInt64);
    auto queueSize = value::bitcastTo<size_t>(queueSizeVal);
    uassert(7821115,
            str::stream() << "Invalid queueSize " << queueSize << " with array size " << size,
            queueSize <= size);

    return {array, startIdx, queueSize};
}

// Update the startIdex and index of the `ArrayQueue`
void updateArrayQueueState(value::Array* arrayQueue, size_t startIdx, size_t queueSize) {
    arrayQueue->setAt(static_cast<size_t>(ArrayQueueElems::kStartIdx),
                      value::TypeTags::NumberInt64,
                      value::bitcastFrom<size_t>(startIdx));
    arrayQueue->setAt(static_cast<size_t>(ArrayQueueElems::kQueueSize),
                      value::TypeTags::NumberInt64,
                      value::bitcastFrom<size_t>(queueSize));
}

// Return the size of the queue
size_t arrayQueueSize(value::Array* arrayQueue) {
    auto [array, startIdx, queueSize] = getArrayQueueState(arrayQueue);
    return queueSize;
}

// Initialize an array queue
std::tuple<value::TypeTags, value::Value> arrayQueueInit() {
    auto [arrayQueueTag, arrayQueueVal] = value::makeNewArray();
    value::ValueGuard arrayQueueGuard{arrayQueueTag, arrayQueueVal};
    auto arrayQueue = value::getArrayView(arrayQueueVal);
    arrayQueue->reserve(static_cast<size_t>(ArrayQueueElems::kSizeOfArray));

    auto [bufferTag, bufferVal] = value::makeNewArray();
    value::ValueGuard bufferGuard{bufferTag, bufferVal};

    // Make the buffer has at least 1 capacity so that the start index will always be valid.
    auto buffer = value::getArrayView(bufferVal);
    buffer->push_back(value::TypeTags::Null, 0);

    bufferGuard.reset();
    arrayQueue->push_back(bufferTag, bufferVal);
    arrayQueue->push_back(value::TypeTags::NumberInt64, 0);  // kStartIdx
    arrayQueue->push_back(value::TypeTags::NumberInt64, 0);  // kQueueSize
    arrayQueueGuard.reset();
    return {arrayQueueTag, arrayQueueVal};
}

// Push an element {tag, value} into the queue
void arrayQueuePush(value::Array* arrayQueue, value::TypeTags tag, value::Value val) {
    /* The underlying array acts as a circular buffer for the queue with `startIdx` and `queueSize`
     * demarcating the filled region (with remaining region containing nulls). When pushing an
     * element to the queue, we set at the corresponding index [= (startIdx + queueSize) %
     * arraySize] the element to be added. If the underlying array is filled, we double the size of
     * the array (by adding nulls); the existing elements in the queue may need to be rearranged
     * when that happens.
     *
     * Eg, Push {v} :
     * => Initial State: (x = filled; _ = empty)
     *       [x x x x]
     *            |
     *         startIdx (queueSize = 4, arraySize = 4)
     *
     * => Double array size:
     *       [x x x x _ _ _ _]
     *            |
     *          startIdx (queueSize = 4, arraySize = 8)
     *
     * => Rearrange elements:
     *       [x x _ _ _ _ x x]
     *                    |
     *                    startIdx (queueSize = 4, arraySize = 8)
     *
     * => Add element:
     *       [x x v _ _ _ x x]
     *                    |
     *                   startIdx (queueSize = 5, arraySize = 8)
     */
    value::ValueGuard guard{tag, val};
    auto [array, startIdx, queueSize] = getArrayQueueState(arrayQueue);
    auto cap = array->size();

    if (queueSize == cap) {
        // reallocate with twice size
        auto newCap = cap * 2;
        array->reserve(newCap);
        auto extend = newCap - cap;

        for (size_t i = 0; i < extend; ++i) {
            array->push_back(value::TypeTags::Null, 0);
        }

        if (startIdx > 0) {
            // existing values wrap over the array
            // need to rearrange the values from [startIdx, cap-1]
            for (size_t from = cap - 1, to = newCap - 1; from >= startIdx; --from, --to) {
                auto [movTag, movVal] = array->swapAt(from, value::TypeTags::Null, 0);
                array->setAt(to, movTag, movVal);
            }
            startIdx += extend;
        }
        cap = newCap;
    }

    auto endIdx = (startIdx + queueSize) % cap;
    guard.reset();
    array->setAt(endIdx, tag, val);
    updateArrayQueueState(arrayQueue, startIdx, queueSize + 1);
}

/* Pops an element {tag, value} from the queue and returns it */
std::pair<value::TypeTags, value::Value> arrayQueuePop(value::Array* arrayQueue) {
    auto [array, startIdx, queueSize] = getArrayQueueState(arrayQueue);
    if (queueSize == 0) {
        return {value::TypeTags::Nothing, 0};
    }
    auto cap = array->size();
    auto pair = array->swapAt(startIdx, value::TypeTags::Null, 0);

    startIdx = (startIdx + 1) % cap;
    updateArrayQueueState(arrayQueue, startIdx, queueSize - 1);
    return pair;
}

std::pair<value::TypeTags, value::Value> arrayQueueFront(value::Array* arrayQueue) {
    auto [array, startIdx, queueSize] = getArrayQueueState(arrayQueue);
    if (queueSize == 0) {
        return {value::TypeTags::Nothing, 0};
    }
    return array->getAt(startIdx);
}

std::pair<value::TypeTags, value::Value> arrayQueueBack(value::Array* arrayQueue) {
    auto [array, startIdx, queueSize] = getArrayQueueState(arrayQueue);
    if (queueSize == 0) {
        return {value::TypeTags::Nothing, 0};
    }
    auto cap = array->size();
    auto endIdx = (startIdx + queueSize - 1) % cap;
    return array->getAt(endIdx);
}

// Returns a value::Array containing N elements at the front of the queue.
// If the queue contains less than N elements, returns all the elements
std::pair<value::TypeTags, value::Value> arrayQueueFrontN(value::Array* arrayQueue, size_t n) {
    auto [array, startIdx, queueSize] = getArrayQueueState(arrayQueue);

    auto [resultArrayTag, resultArrayVal] = value::makeNewArray();
    value::ValueGuard guard{resultArrayTag, resultArrayVal};
    auto resultArray = value::getArrayView(resultArrayVal);
    auto countElem = std::min(n, queueSize);
    resultArray->reserve(countElem);

    auto cap = array->size();
    for (size_t i = 0; i < countElem; ++i) {
        auto idx = (startIdx + i) % cap;

        auto [tag, val] = array->getAt(idx);
        auto [copyTag, copyVal] = value::copyValue(tag, val);
        resultArray->push_back(copyTag, copyVal);
    }

    guard.reset();
    return {resultArrayTag, resultArrayVal};
}

// Returns a value::Array containing N elements at the back of the queue.
// If the queue contains less than N elements, returns all the elements
std::pair<value::TypeTags, value::Value> arrayQueueBackN(value::Array* arrayQueue, size_t n) {
    auto [array, startIdx, queueSize] = getArrayQueueState(arrayQueue);

    auto [arrTag, arrVal] = value::makeNewArray();
    value::ValueGuard guard{arrTag, arrVal};
    auto arr = value::getArrayView(arrVal);
    arr->reserve(std::min(n, queueSize));

    auto cap = array->size();
    auto skip = queueSize > n ? queueSize - n : 0;
    auto elemCount = queueSize > n ? n : queueSize;
    startIdx = (startIdx + skip) % cap;

    for (size_t i = 0; i < elemCount; ++i) {
        auto idx = (startIdx + i) % cap;

        auto [tag, val] = array->getAt(idx);
        auto [copyTag, copyVal] = value::copyValue(tag, val);
        arr->push_back(copyTag, copyVal);
    }

    guard.reset();
    return {arrTag, arrVal};
}

/**
 * Helper functions for integralAdd/Remove/Finalize
 */
std::tuple<value::Array*,
           value::Array*,
           value::Array*,
           value::Array*,
           int64_t,
           boost::optional<int64_t>,
           bool>
getIntegralState(value::TypeTags stateTag, value::Value stateVal) {
    uassert(
        7821103, "The accumulator state should be an array", stateTag == value::TypeTags::Array);
    auto state = value::getArrayView(stateVal);

    auto maxSize = static_cast<size_t>(AggIntegralElems::kMaxSizeOfArray);
    uassert(7821104,
            "The accumulator state should have correct number of elements",
            state->size() == maxSize);

    auto [inputQueueTag, inputQueueVal] =
        state->getAt(static_cast<size_t>(AggIntegralElems::kInputQueue));
    uassert(7821105, "InputQueue should be of array type", inputQueueTag == value::TypeTags::Array);
    auto inputQueue = value::getArrayView(inputQueueVal);

    auto [sortByQueueTag, sortByQueueVal] =
        state->getAt(static_cast<size_t>(AggIntegralElems::kSortByQueue));
    uassert(
        7821121, "SortByQueue should be of array type", sortByQueueTag == value::TypeTags::Array);
    auto sortByQueue = value::getArrayView(sortByQueueVal);

    auto [integralTag, integralVal] =
        state->getAt(static_cast<size_t>(AggIntegralElems::kIntegral));
    uassert(7821106, "Integral should be of array type", integralTag == value::TypeTags::Array);
    auto integral = value::getArrayView(integralVal);

    auto [nanCountTag, nanCountVal] =
        state->getAt(static_cast<size_t>(AggIntegralElems::kNanCount));
    uassert(7821107,
            "nanCount should be of NumberInt64 type",
            nanCountTag == value::TypeTags::NumberInt64);
    auto nanCount = value::bitcastTo<int64_t>(nanCountVal);

    boost::optional<int64_t> unitMillis;
    auto [unitMillisTag, unitMillisVal] =
        state->getAt(static_cast<size_t>(AggIntegralElems::kUnitMillis));
    if (unitMillisTag != value::TypeTags::Null) {
        uassert(7821108,
                "unitMillis should be of type NumberInt64",
                unitMillisTag == value::TypeTags::NumberInt64);
        unitMillis = value::bitcastTo<int64_t>(unitMillisVal);
    }

    auto [isNonRemovableTag, isNonRemovableVal] =
        state->getAt(static_cast<size_t>(AggIntegralElems::kIsNonRemovable));
    uassert(7996800,
            "isNonRemovable should be of boolean type",
            isNonRemovableTag == value::TypeTags::Boolean);
    auto isNonRemovable = value::bitcastTo<bool>(isNonRemovableVal);

    return {state, inputQueue, sortByQueue, integral, nanCount, unitMillis, isNonRemovable};
}

void updateNaNCount(value::Array* state, int64_t nanCount) {
    state->setAt(static_cast<size_t>(AggIntegralElems::kNanCount),
                 value::TypeTags::NumberInt64,
                 value::bitcastFrom<int64_t>(nanCount));
}

void assertTypesForIntegeral(value::TypeTags inputTag,
                             value::TypeTags sortByTag,
                             boost::optional<int64_t> unitMillis) {
    uassert(7821109, "input value should be of numberic type", value::isNumber(inputTag));
    if (unitMillis) {
        uassert(7821110,
                "Sort-by value should be of date type when unitMillis is provided",
                sortByTag == value::TypeTags::Date);
    } else {
        uassert(7821111, "Sort-by value should be of numeric type", value::isNumber(sortByTag));
    }
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::integralOfTwoPointsByTrapezoidalRule(
    std::pair<value::TypeTags, value::Value> prevInput,
    std::pair<value::TypeTags, value::Value> prevSortByVal,
    std::pair<value::TypeTags, value::Value> newInput,
    std::pair<value::TypeTags, value::Value> newSortByVal) {
    if (value::isNaN(prevInput.first, prevInput.second) ||
        value::isNaN(prevSortByVal.first, prevSortByVal.second) ||
        value::isNaN(newInput.first, newInput.second) ||
        value::isNaN(newSortByVal.first, newSortByVal.second)) {
        return {false, value::TypeTags::NumberInt64, 0};
    }

    if ((prevSortByVal.first == value::TypeTags::Date &&
         newSortByVal.first == value::TypeTags::Date) ||
        (value::isNumber(prevSortByVal.first) && value::isNumber(newSortByVal.first))) {
        auto [deltaOwned, deltaTag, deltaVal] = genericSub(
            newSortByVal.first, newSortByVal.second, prevSortByVal.first, prevSortByVal.second);
        value::ValueGuard deltaGuard{deltaOwned, deltaTag, deltaVal};

        auto [sumYOwned, sumYTag, sumYVal] =
            genericAdd(newInput.first, newInput.second, prevInput.first, prevInput.second);
        value::ValueGuard sumYGuard{sumYOwned, sumYTag, sumYVal};

        auto [integralOwned, integralTag, integralVal] =
            genericMul(sumYTag, sumYVal, deltaTag, deltaVal);
        value::ValueGuard integralGuard{integralOwned, integralTag, integralVal};

        auto result = genericDiv(
            integralTag, integralVal, value::TypeTags::NumberInt64, value::bitcastFrom<int32_t>(2));
        return result;
    } else {
        return {false, value::TypeTags::NumberInt64, 0};
    }
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggIntegralInit(ArityType arity) {
    auto [unitOwned, unitTag, unitVal] = getFromStack(0);
    auto [isNonRemovableOwned, isNonRemovableTag, isNonRemovableVal] = getFromStack(1);

    tassert(7996820,
            "Invalid unit type",
            unitTag == value::TypeTags::Null || unitTag == value::TypeTags::NumberInt64);
    tassert(7996821, "Invalid isNonRemovable type", isNonRemovableTag == value::TypeTags::Boolean);

    auto [stateTag, stateVal] = value::makeNewArray();
    value::ValueGuard stateGuard{stateTag, stateVal};

    auto state = value::getArrayView(stateVal);
    state->reserve(static_cast<size_t>(AggIntegralElems::kMaxSizeOfArray));

    // AggIntegralElems::kInputQueue
    auto [inputQueueTag, inputQueueVal] = arrayQueueInit();
    state->push_back(inputQueueTag, inputQueueVal);

    // AggIntegralElems::kSortByQueue
    auto [sortByQueueTag, sortByQueueVal] = arrayQueueInit();
    state->push_back(sortByQueueTag, sortByQueueVal);

    // AggIntegralElems::kIntegral
    auto [integralTag, integralVal] = initializeRemovableSumState();
    state->push_back(integralTag, integralVal);

    // AggIntegralElems::kNanCount
    state->push_back(value::TypeTags::NumberInt64, 0);

    // AggIntegralElems::kUnitMillis
    state->push_back(unitTag, unitVal);

    // AggIntegralElems::kIsNonRemovable
    state->push_back(isNonRemovableTag, isNonRemovableVal);

    stateGuard.reset();
    return {true, stateTag, stateVal};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggIntegralAdd(ArityType arity) {
    auto [stateTag, stateVal] = moveOwnedFromStack(0);
    auto [inputTag, inputVal] = moveOwnedFromStack(1);
    auto [sortByTag, sortByVal] = moveOwnedFromStack(2);

    value::ValueGuard stateGuard{stateTag, stateVal};
    value::ValueGuard inputGuard{inputTag, inputVal};
    value::ValueGuard sortByGuard{sortByTag, sortByVal};

    auto [state, inputQueue, sortByQueue, integral, nanCount, unitMillis, isNonRemovable] =
        getIntegralState(stateTag, stateVal);

    assertTypesForIntegeral(inputTag, sortByTag, unitMillis);

    if (value::isNaN(inputTag, inputVal) || value::isNaN(sortByTag, sortByVal)) {
        nanCount++;
        updateNaNCount(state, nanCount);
    }

    auto queueSize = arrayQueueSize(inputQueue);
    uassert(7821119, "Queue sizes should match", queueSize == arrayQueueSize(sortByQueue));
    if (queueSize > 0) {
        auto inputBack = arrayQueueBack(inputQueue);
        auto sortByBack = arrayQueueBack(sortByQueue);

        auto [integralDeltaOwned, integralDeltaTag, integralDeltaVal] =
            integralOfTwoPointsByTrapezoidalRule(
                inputBack, sortByBack, {inputTag, inputVal}, {sortByTag, sortByVal});
        value::ValueGuard integralDeltaGuard{
            integralDeltaOwned, integralDeltaTag, integralDeltaVal};
        aggRemovableSumImpl<1>(integral, integralDeltaTag, integralDeltaVal);
    }

    if (isNonRemovable) {
        auto [tag, val] = arrayQueuePop(inputQueue);
        value::releaseValue(tag, val);
        std::tie(tag, val) = arrayQueuePop(sortByQueue);
        value::releaseValue(tag, val);
    }

    inputGuard.reset();
    arrayQueuePush(inputQueue, inputTag, inputVal);

    sortByGuard.reset();
    arrayQueuePush(sortByQueue, sortByTag, sortByVal);

    stateGuard.reset();
    return {true, stateTag, stateVal};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggIntegralRemove(ArityType arity) {
    auto [stateTag, stateVal] = moveOwnedFromStack(0);
    auto [inputOwned, inputTag, inputVal] = getFromStack(1);
    auto [sortByOwned, sortByTag, sortByVal] = getFromStack(2);

    value::ValueGuard stateGuard{stateTag, stateVal};

    auto [state, inputQueue, sortByQueue, integral, nanCount, unitMillis, isNonRemovable] =
        getIntegralState(stateTag, stateVal);
    uassert(7996801, "Expected integral window to be removable", !isNonRemovable);

    assertTypesForIntegeral(inputTag, sortByTag, unitMillis);

    // verify that the input and sortby value to be removed are the first elements of the queues
    auto [frontInputTag, frontInputVal] = arrayQueuePop(inputQueue);
    value::ValueGuard frontInputGuard{frontInputTag, frontInputVal};
    auto [cmpTag, cmpVal] = value::compareValue(frontInputTag, frontInputVal, inputTag, inputVal);
    uassert(7821113,
            "Attempted to remove unexpected input value",
            cmpTag == value::TypeTags::NumberInt32 && value::bitcastTo<int32_t>(cmpVal) == 0);

    auto [frontSortByTag, frontSortByVal] = arrayQueuePop(sortByQueue);
    value::ValueGuard frontSortByGuard{frontSortByTag, frontSortByVal};
    std::tie(cmpTag, cmpVal) =
        value::compareValue(frontSortByTag, frontSortByVal, sortByTag, sortByVal);
    uassert(7821117,
            "Attempted to remove unexpected sortby value",
            cmpTag == value::TypeTags::NumberInt32 && value::bitcastTo<int32_t>(cmpVal) == 0);

    if (value::isNaN(inputTag, inputVal) || value::isNaN(sortByTag, sortByVal)) {
        nanCount--;
        updateNaNCount(state, nanCount);
    }

    auto queueSize = arrayQueueSize(inputQueue);
    uassert(7821120, "Queue sizes should match", queueSize == arrayQueueSize(sortByQueue));
    if (queueSize > 0) {
        auto inputPair = arrayQueueFront(inputQueue);
        auto sortByPair = arrayQueueFront(sortByQueue);

        auto [integralDeltaOwned, integralDeltaTag, integralDeltaVal] =
            integralOfTwoPointsByTrapezoidalRule(
                {inputTag, inputVal}, {sortByTag, sortByVal}, inputPair, sortByPair);
        value::ValueGuard integralDeltaGuard{
            integralDeltaOwned, integralDeltaTag, integralDeltaVal};
        aggRemovableSumImpl<-1>(integral, integralDeltaTag, integralDeltaVal);
    }

    stateGuard.reset();
    return {true, stateTag, stateVal};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggIntegralFinalize(
    ArityType arity) {
    auto [stateOwned, stateTag, stateVal] = getFromStack(0);

    auto [state, inputQueue, sortByQueue, integral, nanCount, unitMillis, isNonRemovable] =
        getIntegralState(stateTag, stateVal);

    auto queueSize = arrayQueueSize(inputQueue);
    uassert(7821118, "Queue sizes should match", queueSize == arrayQueueSize(sortByQueue));
    if (queueSize == 0) {
        return {false, value::TypeTags::Null, 0};
    }

    if (nanCount > 0) {
        return {false,
                value::TypeTags::NumberDouble,
                value::bitcastFrom<double>(std::numeric_limits<double>::quiet_NaN())};
    }

    auto [resultOwned, resultTag, resultVal] = aggRemovableSumFinalizeImpl(integral);
    value::ValueGuard resultGuard{resultOwned, resultTag, resultVal};
    if (unitMillis) {
        auto [divResultOwned, divResultTag, divResultVal] =
            genericDiv(resultTag,
                       resultVal,
                       value::TypeTags::NumberInt64,
                       value::bitcastFrom<int64_t>(*unitMillis));
        return {divResultOwned, divResultTag, divResultVal};
    } else {
        resultGuard.reset();
        return {resultOwned, resultTag, resultVal};
    }
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggDerivativeFinalize(
    ArityType arity) {
    auto [unitMillisOwned, unitMillisTag, unitMillisVal] = getFromStack(0);
    auto [inputFirstOwned, inputFirstTag, inputFirstVal] = getFromStack(1);
    auto [sortByFirstOwned, sortByFirstTag, sortByFirstVal] = getFromStack(2);
    auto [inputLastOwned, inputLastTag, inputLastVal] = getFromStack(3);
    auto [sortByLastOwned, sortByLastTag, sortByLastVal] = getFromStack(4);

    if (sortByFirstTag == value::TypeTags::Nothing || sortByLastTag == value::TypeTags::Nothing) {
        return {false, value::TypeTags::Null, 0};
    }

    boost::optional<int64_t> unitMillis;
    if (unitMillisTag != value::TypeTags::Null) {
        uassert(7993408,
                "unitMillis should be of type NumberInt64",
                unitMillisTag == value::TypeTags::NumberInt64);
        unitMillis = value::bitcastTo<int64_t>(unitMillisVal);
    }

    if (unitMillis) {
        uassert(7993409,
                "Unexpected type for sortBy value",
                sortByFirstTag == value::TypeTags::Date && sortByLastTag == value::TypeTags::Date);
    } else {
        uassert(7993410,
                "Unexpected type for sortBy value",
                value::isNumber(sortByFirstTag) && value::isNumber(sortByLastTag));
    }

    auto [runOwned, runTag, runVal] =
        genericSub(sortByLastTag, sortByLastVal, sortByFirstTag, sortByFirstVal);
    value::ValueGuard runGuard{runOwned, runTag, runVal};

    auto [riseOwned, riseTag, riseVal] =
        genericSub(inputLastTag, inputLastVal, inputFirstTag, inputFirstVal);
    value::ValueGuard riseGuard{riseOwned, riseTag, riseVal};

    uassert(7821012, "Input delta should be numeric", value::isNumber(riseTag));

    // Return null if the sortBy delta is zero
    if (runTag == value::TypeTags::NumberDecimal) {
        if (numericCast<Decimal128>(runTag, runVal).isZero()) {
            return {false, value::TypeTags::Null, 0};
        }
    } else {
        if (numericCast<double>(runTag, runVal) == 0) {
            return {false, value::TypeTags::Null, 0};
        }
    }

    auto [divOwned, divTag, divVal] = genericDiv(riseTag, riseVal, runTag, runVal);
    value::ValueGuard divGuard{divOwned, divTag, divVal};

    if (unitMillis) {
        auto [mulOwned, mulTag, mulVal] = genericMul(
            divTag, divVal, value::TypeTags::NumberInt64, value::bitcastFrom<int64_t>(*unitMillis));
        return {mulOwned, mulTag, mulVal};
    } else {
        divGuard.reset();
        return {divOwned, divTag, divVal};
    }
}

std::tuple<value::Array*, value::Array*, value::Array*, value::Array*, int64_t> covarianceState(
    value::TypeTags stateTag, value::Value stateVal) {
    tassert(
        7820800, "The accumulator state should be an array", stateTag == value::TypeTags::Array);
    auto state = value::getArrayView(stateVal);

    tassert(7820801,
            "The accumulator state should have correct number of elements",
            state->size() == static_cast<size_t>(AggCovarianceElems::kSizeOfArray));

    auto [sumXTag, sumXVal] = state->getAt(static_cast<size_t>(AggCovarianceElems::kSumX));
    tassert(7820802, "SumX component should be an array", sumXTag == value::TypeTags::Array);
    auto sumX = value::getArrayView(sumXVal);

    auto [sumYTag, sumYVal] = state->getAt(static_cast<size_t>(AggCovarianceElems::kSumY));
    tassert(7820803, "SumY component should be an array", sumYTag == value::TypeTags::Array);
    auto sumY = value::getArrayView(sumYVal);

    auto [cXYTag, cXYVal] = state->getAt(static_cast<size_t>(AggCovarianceElems::kCXY));
    tassert(7820804, "CXY component should be an array", cXYTag == value::TypeTags::Array);
    auto cXY = value::getArrayView(cXYVal);

    auto [countTag, countVal] = state->getAt(static_cast<size_t>(AggCovarianceElems::kCount));
    tassert(7820805,
            "Count component should be a 64-bit integer",
            countTag == value::TypeTags::NumberInt64);
    auto count = value::bitcastTo<int64_t>(countVal);

    return {state, sumX, sumY, cXY, count};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::aggRemovableAvgFinalizeImpl(
    value::Array* sumState, int64_t count) {
    if (count == 0) {
        return {false, sbe::value::TypeTags::Null, 0};
    }
    auto [sumOwned, sumTag, sumVal] = aggRemovableSumFinalizeImpl(sumState);

    if (sumTag == value::TypeTags::NumberInt32) {
        auto sum = static_cast<double>(value::bitcastTo<int>(sumVal));
        auto avg = sum / static_cast<double>(count);
        return {false, value::TypeTags::NumberDouble, value::bitcastFrom<double>(avg)};
    } else if (sumTag == value::TypeTags::NumberInt64) {
        auto sum = static_cast<double>(value::bitcastTo<long long>(sumVal));
        auto avg = sum / static_cast<double>(count);
        return {false, value::TypeTags::NumberDouble, value::bitcastFrom<double>(avg)};
    } else if (sumTag == value::TypeTags::NumberDouble) {
        auto sum = value::bitcastTo<double>(sumVal);
        if (std::isnan(sum) || std::isinf(sum)) {
            return {false, sumTag, sumVal};
        }
        auto avg = sum / static_cast<double>(count);
        return {false, value::TypeTags::NumberDouble, value::bitcastFrom<double>(avg)};
    } else if (sumTag == value::TypeTags::NumberDecimal) {
        value::ValueGuard sumGuard{sumOwned, sumTag, sumVal};
        auto sum = value::bitcastTo<Decimal128>(sumVal);
        if (sum.isNaN() || sum.isInfinite()) {
            sumGuard.reset();
            return {sumOwned, sumTag, sumVal};
        }
        auto avg = sum.divide(Decimal128(count));
        auto [avgTag, avgVal] = value::makeCopyDecimal(avg);
        return {true, avgTag, avgVal};
    } else {
        MONGO_UNREACHABLE;
    }
}

FastTuple<bool, value::TypeTags, value::Value> covarianceCheckNonFinite(value::TypeTags xTag,
                                                                        value::Value xVal,
                                                                        value::TypeTags yTag,
                                                                        value::Value yVal) {
    int nanCnt = 0;
    int posCnt = 0;
    int negCnt = 0;
    bool isDecimal = false;
    auto checkValue = [&](value::TypeTags tag, value::Value val) {
        if (value::isNaN(tag, val)) {
            nanCnt++;
        } else if (tag == value::TypeTags::NumberDecimal) {
            if (value::isInfinity(tag, val)) {
                if (value::bitcastTo<Decimal128>(val).isNegative()) {
                    negCnt++;
                } else {
                    posCnt++;
                }
            }
            isDecimal = true;
        } else {
            auto [doubleOwned, doubleTag, doubleVal] =
                genericNumConvert(tag, val, value::TypeTags::NumberDouble);
            auto value = value::bitcastTo<double>(doubleVal);
            if (value == std::numeric_limits<double>::infinity()) {
                posCnt++;
            } else if (value == -std::numeric_limits<double>::infinity()) {
                negCnt++;
            }
        }
    };
    checkValue(xTag, xVal);
    checkValue(yTag, yVal);

    if (nanCnt == 0 && posCnt == 0 && negCnt == 0) {
        return {false, value::TypeTags::Nothing, 0};
    }
    if (nanCnt > 0 || posCnt * negCnt > 0) {
        if (isDecimal) {
            auto [decimalTag, decimalVal] = value::makeCopyDecimal(Decimal128::kPositiveNaN);
            return {true, decimalTag, decimalVal};
        } else {
            return {false,
                    value::TypeTags::NumberDouble,
                    value::bitcastFrom<double>(std::numeric_limits<double>::quiet_NaN())};
        }
    }
    if (isDecimal) {
        if (posCnt > 0) {
            auto [decimalTag, decimalVal] = value::makeCopyDecimal(Decimal128::kPositiveInfinity);
            return {true, decimalTag, decimalVal};
        } else {
            auto [decimalTag, decimalVal] = value::makeCopyDecimal(Decimal128::kNegativeInfinity);
            return {true, decimalTag, decimalVal};
        }
    } else {
        if (posCnt > 0) {
            return {false,
                    value::TypeTags::NumberDouble,
                    value::bitcastFrom<double>(std::numeric_limits<double>::infinity())};
        } else {
            return {false,
                    value::TypeTags::NumberDouble,
                    value::bitcastFrom<double>(-std::numeric_limits<double>::infinity())};
        }
    }
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggCovarianceAdd(ArityType arity) {
    auto [stateTag, stateVal] = moveOwnedFromStack(0);
    auto [xOwned, xTag, xVal] = getFromStack(1);
    auto [yOwned, yTag, yVal] = getFromStack(2);

    // Initialize the accumulator.
    if (stateTag == value::TypeTags::Nothing) {
        std::tie(stateTag, stateVal) = value::makeNewArray();
        value::ValueGuard newStateGuard{stateTag, stateVal};
        auto state = value::getArrayView(stateVal);
        state->reserve(static_cast<size_t>(AggCovarianceElems::kSizeOfArray));

        auto [sumXStateTag, sumXStateVal] = initializeRemovableSumState();
        state->push_back(sumXStateTag, sumXStateVal);  // kSumX
        auto [sumYStateTag, sumYStateVal] = initializeRemovableSumState();
        state->push_back(sumYStateTag, sumYStateVal);  // kSumY
        auto [cXYStateTag, cXYStateVal] = initializeRemovableSumState();
        state->push_back(cXYStateTag, cXYStateVal);                                      // kCXY
        state->push_back(value::TypeTags::NumberInt64, value::bitcastFrom<int64_t>(0));  // kCount
        newStateGuard.reset();
    }
    value::ValueGuard stateGuard{stateTag, stateVal};

    if (!value::isNumber(xTag) || !value::isNumber(yTag)) {
        stateGuard.reset();
        return {true, stateTag, stateVal};
    }

    auto [state, sumXState, sumYState, cXYState, count] = covarianceState(stateTag, stateVal);

    auto [nonFiniteOwned, nonFiniteTag, nonFiniteVal] =
        covarianceCheckNonFinite(xTag, xVal, yTag, yVal);
    if (nonFiniteTag != value::TypeTags::Nothing) {
        value::ValueGuard nonFiniteGuard{nonFiniteOwned, nonFiniteTag, nonFiniteVal};
        aggRemovableSumImpl<1>(cXYState, nonFiniteTag, nonFiniteVal);
        stateGuard.reset();
        return {true, stateTag, stateVal};
    }

    auto [meanXOwned, meanXTag, meanXVal] = aggRemovableAvgFinalizeImpl(sumXState, count);
    value::ValueGuard meanXGuard{meanXOwned, meanXTag, meanXVal};
    auto [deltaXOwned, deltaXTag, deltaXVal] = genericSub(xTag, xVal, meanXTag, meanXVal);
    value::ValueGuard deltaXGuard{deltaXOwned, deltaXTag, deltaXVal};
    aggRemovableSumImpl<1>(sumXState, xTag, xVal);

    aggRemovableSumImpl<1>(sumYState, yTag, yVal);
    auto [meanYOwned, meanYTag, meanYVal] = aggRemovableAvgFinalizeImpl(sumYState, count + 1);
    value::ValueGuard meanYGuard{meanYOwned, meanYTag, meanYVal};
    auto [deltaYOwned, deltaYTag, deltaYVal] = genericSub(yTag, yVal, meanYTag, meanYVal);
    value::ValueGuard deltaYGuard{deltaYOwned, deltaYTag, deltaYVal};

    auto [deltaCXYOwned, deltaCXYTag, deltaCXYVal] =
        genericMul(deltaXTag, deltaXVal, deltaYTag, deltaYVal);
    value::ValueGuard deltaCXYGuard{deltaCXYOwned, deltaCXYTag, deltaCXYVal};
    aggRemovableSumImpl<1>(cXYState, deltaCXYTag, deltaCXYVal);

    state->setAt(static_cast<size_t>(AggCovarianceElems::kCount),
                 value::TypeTags::NumberInt64,
                 value::bitcastFrom<int64_t>(count + 1));

    stateGuard.reset();
    return {true, stateTag, stateVal};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggCovarianceRemove(
    ArityType arity) {
    auto [stateTag, stateVal] = moveOwnedFromStack(0);
    auto [xOwned, xTag, xVal] = getFromStack(1);
    auto [yOwned, yTag, yVal] = getFromStack(2);
    value::ValueGuard stateGuard{stateTag, stateVal};

    if (!value::isNumber(xTag) || !value::isNumber(yTag)) {
        stateGuard.reset();
        return {true, stateTag, stateVal};
    }

    auto [state, sumXState, sumYState, cXYState, count] = covarianceState(stateTag, stateVal);

    auto [nonFiniteOwned, nonFiniteTag, nonFiniteVal] =
        covarianceCheckNonFinite(xTag, xVal, yTag, yVal);
    if (nonFiniteTag != value::TypeTags::Nothing) {
        value::ValueGuard nonFiniteGuard{nonFiniteOwned, nonFiniteTag, nonFiniteVal};
        aggRemovableSumImpl<-1>(cXYState, nonFiniteTag, nonFiniteVal);
        stateGuard.reset();
        return {true, stateTag, stateVal};
    }

    tassert(7820806, "Can't remove from an empty covariance window", count > 0);
    if (count == 1) {
        state->setAt(
            static_cast<size_t>(AggCovarianceElems::kCount), value::TypeTags::NumberInt64, 0);
        aggRemovableSumReset(sumXState);
        aggRemovableSumReset(sumYState);
        aggRemovableSumReset(cXYState);
        stateGuard.reset();
        return {true, stateTag, stateVal};
    }

    aggRemovableSumImpl<-1>(sumXState, xTag, xVal);
    auto [meanXOwned, meanXTag, meanXVal] = aggRemovableAvgFinalizeImpl(sumXState, count - 1);
    value::ValueGuard meanXGuard{meanXOwned, meanXTag, meanXVal};
    auto [deltaXOwned, deltaXTag, deltaXVal] = genericSub(xTag, xVal, meanXTag, meanXVal);
    value::ValueGuard deltaXGuard{deltaXOwned, deltaXTag, deltaXVal};

    auto [meanYOwned, meanYTag, meanYVal] = aggRemovableAvgFinalizeImpl(sumYState, count);
    value::ValueGuard meanYGuard{meanYOwned, meanYTag, meanYVal};
    auto [deltaYOwned, deltaYTag, deltaYVal] = genericSub(yTag, yVal, meanYTag, meanYVal);
    value::ValueGuard deltaYGuard{deltaYOwned, deltaYTag, deltaYVal};
    aggRemovableSumImpl<-1>(sumYState, yTag, yVal);

    auto [deltaCXYOwned, deltaCXYTag, deltaCXYVal] =
        genericMul(deltaXTag, deltaXVal, deltaYTag, deltaYVal);
    value::ValueGuard deltaCXYGuard{deltaCXYOwned, deltaCXYTag, deltaCXYVal};
    aggRemovableSumImpl<-1>(cXYState, deltaCXYTag, deltaCXYVal);

    state->setAt(static_cast<size_t>(AggCovarianceElems::kCount),
                 value::TypeTags::NumberInt64,
                 value::bitcastFrom<int64_t>(count - 1));

    stateGuard.reset();
    return {true, stateTag, stateVal};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggCovarianceFinalize(
    ArityType arity, bool isSamp) {
    auto [stateOwned, stateTag, stateVal] = getFromStack(0);
    auto [state, sumXState, sumYState, cXYState, count] = covarianceState(stateTag, stateVal);

    if (count == 1 && !isSamp) {
        return {false, value::TypeTags::NumberDouble, value::bitcastFrom<double>(0.0)};
    }

    double adjustedCount = (isSamp ? count - 1 : count);
    if (adjustedCount <= 0) {
        return {false, value::TypeTags::Null, 0};
    }

    auto [cXYOwned, cXYTag, cXYVal] = aggRemovableSumFinalizeImpl(cXYState);
    value::ValueGuard cXYGuard{cXYOwned, cXYTag, cXYVal};
    return genericDiv(
        cXYTag, cXYVal, value::TypeTags::NumberDouble, value::bitcastFrom<double>(adjustedCount));
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggCovarianceSampFinalize(
    ArityType arity) {
    return builtinAggCovarianceFinalize(arity, true /* isSamp */);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggCovariancePopFinalize(
    ArityType arity) {
    return builtinAggCovarianceFinalize(arity, false /* isSamp */);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggRemovablePushAdd(
    ArityType arity) {
    auto [stateTag, stateVal] = moveOwnedFromStack(0);
    if (stateTag == value::TypeTags::Nothing) {
        std::tie(stateTag, stateVal) = arrayQueueInit();
    }
    value::ValueGuard stateGuard{stateTag, stateVal};
    auto [inputTag, inputVal] = moveOwnedFromStack(1);
    if (inputTag == value::TypeTags::Nothing) {
        stateGuard.reset();
        return {true, stateTag, stateVal};
    }
    value::ValueGuard inputGuard{inputTag, inputVal};

    uassert(7993100, "State should be of array type", stateTag == value::TypeTags::Array);
    auto state = value::getArrayView(stateVal);
    inputGuard.reset();
    arrayQueuePush(state, inputTag, inputVal);
    stateGuard.reset();
    return {true, stateTag, stateVal};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggRemovablePushRemove(
    ArityType arity) {
    auto [stateTag, stateVal] = moveOwnedFromStack(0);
    value::ValueGuard stateGuard{stateTag, stateVal};
    auto [inputTag, inputVal] = moveOwnedFromStack(1);
    if (inputTag == value::TypeTags::Nothing) {
        stateGuard.reset();
        return {true, stateTag, stateVal};
    }
    value::ValueGuard inputGuard{inputTag, inputVal};

    uassert(7993101, "State should be of array type", stateTag == value::TypeTags::Array);
    auto state = value::getArrayView(stateVal);
    auto [tag, val] = arrayQueuePop(state);
    value::releaseValue(tag, val);
    stateGuard.reset();
    return {true, stateTag, stateVal};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggRemovablePushFinalize(
    ArityType arity) {
    auto [stateOwned, stateTag, stateVal] = getFromStack(0);
    uassert(7993102, "State should be of array type", stateTag == value::TypeTags::Array);
    auto state = value::getArrayView(stateVal);
    auto [queueBuffer, startIdx, queueSize] = getArrayQueueState(state);

    auto [resultTag, resultVal] = value::makeNewArray();
    auto result = value::getArrayView(resultVal);
    result->reserve(queueSize);

    for (size_t i = 0; i < queueSize; ++i) {
        auto idx = startIdx + i;
        if (idx >= queueBuffer->size()) {
            idx -= queueBuffer->size();
        }
        auto [tag, val] = queueBuffer->getAt(idx);
        std::tie(tag, val) = value::copyValue(tag, val);
        result->push_back(tag, val);
    }
    return {true, resultTag, resultVal};
}

std::tuple<value::Array*, value::Array*, value::Array*, int64_t, int64_t> removableStdDevState(
    value::TypeTags stateTag, value::Value stateVal) {
    uassert(8019600, "state should be of array type", stateTag == value::TypeTags::Array);
    auto state = value::getArrayView(stateVal);

    uassert(8019601,
            "incorrect size of state array",
            state->size() == static_cast<size_t>(AggRemovableStdDevElems::kSizeOfArray));

    auto [sumTag, sumVal] = state->getAt(static_cast<size_t>(AggRemovableStdDevElems::kSum));
    uassert(8019602, "sum elem should be of array type", sumTag == value::TypeTags::Array);
    auto sum = value::getArrayView(sumVal);

    auto [m2Tag, m2Val] = state->getAt(static_cast<size_t>(AggRemovableStdDevElems::kM2));
    uassert(8019603, "m2 elem should be of array type", m2Tag == value::TypeTags::Array);
    auto m2 = value::getArrayView(m2Val);

    auto [countTag, countVal] = state->getAt(static_cast<size_t>(AggRemovableStdDevElems::kCount));
    uassert(
        8019604, "count elem should be of int64 type", countTag == value::TypeTags::NumberInt64);
    auto count = value::bitcastTo<int64_t>(countVal);

    auto [nonFiniteCountTag, nonFiniteCountVal] =
        state->getAt(static_cast<size_t>(AggRemovableStdDevElems::kNonFiniteCount));
    uassert(8019605,
            "non finite count elem should be of int64 type",
            nonFiniteCountTag == value::TypeTags::NumberInt64);
    auto nonFiniteCount = value::bitcastTo<int64_t>(nonFiniteCountVal);

    return {state, sum, m2, count, nonFiniteCount};
}

void updateRemovableStdDevState(value::Array* state, int64_t count, int64_t nonFiniteCount) {
    state->setAt(static_cast<size_t>(AggRemovableStdDevElems::kCount),
                 value::TypeTags::NumberInt64,
                 value::bitcastFrom<int64_t>(count));
    state->setAt(static_cast<size_t>(AggRemovableStdDevElems::kNonFiniteCount),
                 value::TypeTags::NumberInt64,
                 value::bitcastFrom<int64_t>(nonFiniteCount));
}

template <int quantity>
void ByteCode::aggRemovableStdDevImpl(value::TypeTags stateTag,
                                      value::Value stateVal,
                                      value::TypeTags inputTag,
                                      value::Value inputVal) {
    static_assert(quantity == 1 || quantity == -1);
    auto [state, sumState, m2State, count, nonFiniteCount] =
        removableStdDevState(stateTag, stateVal);
    if (!value::isNumber(inputTag)) {
        return;
    }
    if ((inputTag == value::TypeTags::NumberDouble &&
         !std::isfinite(value::bitcastTo<double>(inputVal))) ||
        (inputTag == value::TypeTags::NumberDecimal &&
         !value::bitcastTo<Decimal128>(inputVal).isFinite())) {
        count += quantity;
        nonFiniteCount += quantity;
        updateRemovableStdDevState(state, count, nonFiniteCount);
        return;
    }

    if (count == 0) {
        // Assuming we are adding value if count == 0.
        aggDoubleDoubleSumImpl(sumState, inputTag, inputVal);
        updateRemovableStdDevState(state, ++count, nonFiniteCount);
        return;
    } else if (count + quantity == 0) {
        resetDoubleDoubleSumState(sumState);
        resetDoubleDoubleSumState(m2State);
        updateRemovableStdDevState(state, 0, 0);
        return;
    }

    auto inputDouble = value::bitcastTo<double>(value::coerceToDouble(inputTag, inputVal).second);
    auto [sumOwned, sumTag, sumVal] = aggDoubleDoubleSumFinalizeImpl(sumState);
    value::ValueGuard sumGuard{sumOwned, sumTag, sumVal};
    double x = count * inputDouble -
        value::bitcastTo<double>(value::coerceToDouble(sumTag, sumVal).second);
    count += quantity;
    aggDoubleDoubleSumImpl(sumState,
                           value::TypeTags::NumberDouble,
                           value::bitcastFrom<double>(inputDouble * quantity));
    aggDoubleDoubleSumImpl(
        m2State,
        value::TypeTags::NumberDouble,
        value::bitcastFrom<double>(x * x * quantity / (count * (count - quantity))));
    updateRemovableStdDevState(state, count, nonFiniteCount);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggRemovableStdDevAdd(
    ArityType arity) {
    auto [stateTag, stateVal] = moveOwnedFromStack(0);
    auto [inputOwned, inputTag, inputVal] = getFromStack(1);
    // Initialize the accumulator.
    if (stateTag == value::TypeTags::Nothing) {
        std::tie(stateTag, stateVal) = value::makeNewArray();
        value::ValueGuard newStateGuard{stateTag, stateVal};
        auto state = value::getArrayView(stateVal);
        state->reserve(static_cast<size_t>(AggRemovableStdDevElems::kSizeOfArray));

        auto [sumStateTag, sumStateVal] = initializeDoubleDoubleSumState();
        state->push_back(sumStateTag, sumStateVal);  // kSum
        auto [m2StateTag, m2StateVal] = initializeDoubleDoubleSumState();
        state->push_back(m2StateTag, m2StateVal);                                        // kM2
        state->push_back(value::TypeTags::NumberInt64, value::bitcastFrom<int64_t>(0));  // kCount
        state->push_back(value::TypeTags::NumberInt64,
                         value::bitcastFrom<int64_t>(0));  // kNonFiniteCount
        newStateGuard.reset();
    }
    value::ValueGuard stateGuard{stateTag, stateVal};

    aggRemovableStdDevImpl<1>(stateTag, stateVal, inputTag, inputVal);

    stateGuard.reset();
    return {true, stateTag, stateVal};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggRemovableStdDevRemove(
    ArityType arity) {
    auto [stateTag, stateVal] = moveOwnedFromStack(0);
    auto [inputOwned, inputTag, inputVal] = getFromStack(1);
    value::ValueGuard stateGuard{stateTag, stateVal};

    aggRemovableStdDevImpl<-1>(stateTag, stateVal, inputTag, inputVal);

    stateGuard.reset();
    return {true, stateTag, stateVal};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggRemovableStdDevFinalize(
    ArityType arity, bool isSamp) {
    auto [stateOwned, stateTag, stateVal] = getFromStack(0);
    auto [state, sumState, m2State, count, nonFiniteCount] =
        removableStdDevState(stateTag, stateVal);
    if (nonFiniteCount > 0) {
        return {false, value::TypeTags::Null, 0};
    }
    const long long adjustedCount = isSamp ? count - 1 : count;
    if (adjustedCount <= 0) {
        return {false, value::TypeTags::Null, 0};
    }
    auto [m2Owned, m2Tag, m2Val] = aggDoubleDoubleSumFinalizeImpl(m2State);
    value::ValueGuard m2Guard{m2Owned, m2Tag, m2Val};
    auto squaredDifferences = value::bitcastTo<double>(value::coerceToDouble(m2Tag, m2Val).second);
    if (squaredDifferences < 0 || (!isSamp && count == 1)) {
        // m2 is the sum of squared differences from the mean, so it should always be
        // nonnegative. It may take on a small negative value due to floating point error, which
        // breaks the sqrt calculation. In this case, the closest valid value for _m2 is 0, so
        // we reset _m2 and return 0 for the standard deviation.
        // If we're doing a population std dev of one element, it is also correct to return 0.
        resetDoubleDoubleSumState(m2State);
        return {false, value::TypeTags::NumberInt32, 0};
    }
    return {false,
            value::TypeTags::NumberDouble,
            value::bitcastFrom<double>(sqrt(squaredDifferences / adjustedCount))};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggRemovableStdDevSampFinalize(
    ArityType arity) {
    return builtinAggRemovableStdDevFinalize(arity, true /* isSamp */);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggRemovableStdDevPopFinalize(
    ArityType arity) {
    return builtinAggRemovableStdDevFinalize(arity, false /* isSamp */);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggRemovableAvgFinalize(
    ArityType arity) {
    auto [stateOwned, stateTag, stateVal] = getFromStack(0);
    auto [countOwned, countTag, countVal] = getFromStack(1);

    tassert(7965901,
            "The avg accumulator state should be an array",
            stateTag == value::TypeTags::Array);

    return aggRemovableAvgFinalizeImpl(value::getArrayView(stateVal), countVal);
}

/**
 * $linearFill implementation
 */

std::tuple<value::Array*,
           std::pair<value::TypeTags, value::Value>,
           std::pair<value::TypeTags, value::Value>,
           std::pair<value::TypeTags, value::Value>,
           std::pair<value::TypeTags, value::Value>,
           std::pair<value::TypeTags, value::Value>,
           int64_t>
linearFillState(value::TypeTags stateTag, value::Value stateVal) {
    tassert(
        7971200, "The accumulator state should be an array", stateTag == value::TypeTags::Array);
    auto state = value::getArrayView(stateVal);

    tassert(7971201,
            "The accumulator state should have correct number of elements",
            state->size() == static_cast<size_t>(AggLinearFillElems::kSizeOfArray));

    auto x1 = state->getAt(static_cast<size_t>(AggLinearFillElems::kX1));
    auto y1 = state->getAt(static_cast<size_t>(AggLinearFillElems::kY1));
    auto x2 = state->getAt(static_cast<size_t>(AggLinearFillElems::kX2));
    auto y2 = state->getAt(static_cast<size_t>(AggLinearFillElems::kY2));
    auto prevX = state->getAt(static_cast<size_t>(AggLinearFillElems::kPrevX));
    auto [countTag, countVal] = state->getAt(static_cast<size_t>(AggLinearFillElems::kCount));
    tassert(7971202,
            "Expected count element to be of int64 type",
            countTag == value::TypeTags::NumberInt64);
    auto count = value::bitcastTo<int64_t>(countVal);

    return {state, x1, y1, x2, y2, prevX, count};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggLinearFillCanAdd(
    ArityType arity) {
    auto [stateOwned, stateTag, stateVal] = getFromStack(0);
    auto [state, x1, y1, x2, y2, prevX, count] = linearFillState(stateTag, stateVal);

    // if y2 is non-null it means we have found a valid upper window bound. in that case if count is
    // positive it means there are still more finalize calls to be made. when count == 0 we have
    // exhausted this window.
    if (y2.first != value::TypeTags::Null) {
        return {false, value::TypeTags::Boolean, value::bitcastFrom<bool>(count == 0)};
    }

    // if y2 is null it means we have not yet found the upper window bound so keep on adding input
    // values
    return {false, value::TypeTags::Boolean, value::bitcastFrom<bool>(true)};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggLinearFillAdd(ArityType arity) {
    auto [stateTag, stateVal] = moveOwnedFromStack(0);
    value::ValueGuard stateGuard{stateTag, stateVal};

    auto [inputTag, inputVal] = moveOwnedFromStack(1);
    value::ValueGuard inputGuard{inputTag, inputVal};

    auto [sortByTag, sortByVal] = moveOwnedFromStack(2);
    value::ValueGuard sortByGuard{sortByTag, sortByVal};

    // Validate the types of the values
    uassert(7971203,
            "Expected input value type to be numeric or null",
            value::isNumber(inputTag) || inputTag == value::TypeTags::Null);
    uassert(7971204,
            "Expected sortBy value type to be numeric or date",
            value::isNumber(sortByTag) || coercibleToDate(sortByTag));

    auto [state, x1, y1, x2, y2, prevX, count] = linearFillState(stateTag, stateVal);

    // Valdiate the current sortBy value with the previous one and update prevX
    auto [cmpTag, cmpVal] = value::compareValue(sortByTag, sortByVal, prevX.first, prevX.second);
    uassert(7971205,
            "There can be no repeated values in the sort field",
            cmpTag == value::TypeTags::NumberInt32 && cmpVal != 0);

    if (prevX.first != value::TypeTags::Null) {
        uassert(7971206,
                "Conflicting sort value types, previous and current types don't match",
                (coercibleToDate(sortByTag) && coercibleToDate(prevX.first)) ||
                    (value::isNumber(sortByTag) && value::isNumber(prevX.first)));
    }

    auto [copyXTag, copyXVal] = value::copyValue(sortByTag, sortByVal);
    state->setAt(static_cast<size_t>(AggLinearFillElems::kPrevX), copyXTag, copyXVal);

    // Update x2/y2 to the current sortby/input values
    sortByGuard.reset();
    auto [oldX2Tag, oldX2Val] =
        state->swapAt(static_cast<size_t>(AggLinearFillElems::kX2), sortByTag, sortByVal);
    value::ValueGuard oldX2Guard{oldX2Tag, oldX2Val};

    inputGuard.reset();
    auto [oldY2Tag, oldY2Val] =
        state->swapAt(static_cast<size_t>(AggLinearFillElems::kY2), inputTag, inputVal);
    value::ValueGuard oldY2Guard{oldY2Tag, oldY2Val};

    // If (old) y2 is non-null, it means we need to look for new end-points (x1, y1), (x2, y2)
    // and the segment spanned be previous endpoints is exhausted. Count should be zero at
    // this point. Update (x1, y1) to the previous (x2, y2)
    if (oldY2Tag != value::TypeTags::Null) {
        tassert(7971207, "count value should be zero", count == 0);
        oldX2Guard.reset();
        state->setAt(static_cast<size_t>(AggLinearFillElems::kX1), oldX2Tag, oldX2Val);
        oldY2Guard.reset();
        state->setAt(static_cast<size_t>(AggLinearFillElems::kY1), oldY2Tag, oldY2Val);
    }

    state->setAt(
        static_cast<size_t>(AggLinearFillElems::kCount), value::TypeTags::NumberInt64, ++count);

    stateGuard.reset();
    return {true, stateTag, stateVal};
}

// Given two known points (x1, y1) and (x2, y2) and a value x that lies between those two
// points, we solve (or fill) for y with the following formula: y = y1 + (x - x1) * ((y2 -
// y1)/(x2 - x1))
FastTuple<bool, value::TypeTags, value::Value> ByteCode::linearFillInterpolate(
    std::pair<value::TypeTags, value::Value> x1,
    std::pair<value::TypeTags, value::Value> y1,
    std::pair<value::TypeTags, value::Value> x2,
    std::pair<value::TypeTags, value::Value> y2,
    std::pair<value::TypeTags, value::Value> x) {
    // (y2 - y1)
    auto [delYOwned, delYTag, delYVal] = genericSub(y2.first, y2.second, y1.first, y1.second);
    value::ValueGuard delYGuard{delYOwned, delYTag, delYVal};

    // (x2 - x1)
    auto [delXOwned, delXTag, delXVal] = genericSub(x2.first, x2.second, x1.first, x1.second);
    value::ValueGuard delXGuard{delXOwned, delXTag, delXVal};

    // (y2 - y1) / (x2 - x1)
    auto [divOwned, divTag, divVal] = genericDiv(delYTag, delYVal, delXTag, delXVal);
    value::ValueGuard divGuard{divOwned, divTag, divVal};

    // (x - x1)
    auto [subOwned, subTag, subVal] = genericSub(x.first, x.second, x1.first, x1.second);
    value::ValueGuard subGuard{subOwned, subTag, subVal};

    // (x - x1) * ((y2 - y1) / (x2 - x1))
    auto [mulOwned, mulTag, mulVal] = genericMul(subTag, subVal, divTag, divVal);
    value::ValueGuard mulGuard{mulOwned, mulTag, mulVal};

    // y1 + (x - x1) * ((y2 - y1) / (x2 - x1))
    return genericAdd(y1.first, y1.second, mulTag, mulVal);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggLinearFillFinalize(
    ArityType arity) {
    auto [stateOwned, stateTag, stateVal] = getFromStack(0);
    auto [xOwned, sortByTag, sortByVal] = getFromStack(1);
    auto [state, x1, y1, x2, y2, prevX, count] = linearFillState(stateTag, stateVal);

    tassert(7971208, "count should be positive", count > 0);
    state->setAt(
        static_cast<size_t>(AggLinearFillElems::kCount), value::TypeTags::NumberInt64, --count);

    // if y2 is null it means the current window is the last window frame in the partition
    if (y2.first == value::TypeTags::Null) {
        return {false, value::TypeTags::Null, 0};
    }

    // If count == 0, we are currently handling the last docoument in the window frame (x2/y2)
    // so we can return y2 directly. Note that the document represented by y1 was returned as
    // part of previous window (when it was y2)
    if (count == 0) {
        auto [y2Tag, y2Val] = value::copyValue(y2.first, y2.second);
        return {true, y2Tag, y2Val};
    }

    // If y1 is null it means the current window is the first window frame in the partition
    if (y1.first == value::TypeTags::Null) {
        return {false, value::TypeTags::Null, 0};
    }
    return linearFillInterpolate(x1, y1, x2, y2, {sortByTag, sortByVal});
}

/**
 * Implementation for $firstN/$lastN removable window function
 */

std::tuple<value::Array*, size_t> firstLastNState(value::TypeTags stateTag, value::Value stateVal) {
    uassert(8070600, "state should be of array type", stateTag == value::TypeTags::Array);
    auto state = value::getArrayView(stateVal);

    uassert(8070601,
            "incorrect size of state array",
            state->size() == static_cast<size_t>(AggFirstLastNElems::kSizeOfArray));

    auto [queueTag, queueVal] = state->getAt(static_cast<size_t>(AggFirstLastNElems::kQueue));
    uassert(8070602, "Queue should be of array type", queueTag == value::TypeTags::Array);
    auto queue = value::getArrayView(queueVal);

    auto [nTag, nVal] = state->getAt(static_cast<size_t>(AggFirstLastNElems::kN));
    uassert(8070603, "'n' elem should be of int64 type", nTag == value::TypeTags::NumberInt64);
    auto n = value::bitcastTo<int64_t>(nVal);

    return {queue, static_cast<size_t>(n)};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggFirstLastNInit(ArityType arity) {
    auto [fieldOwned, fieldTag, fieldVal] = getFromStack(0);

    auto [nOwned, nTag, nVal] = genericNumConvert(fieldTag, fieldVal, value::TypeTags::NumberInt64);
    uassert(8070607, "Failed to convert to 64-bit integer", nTag == value::TypeTags::NumberInt64);

    auto n = value::bitcastTo<int64_t>(nVal);
    uassert(8070608, "Expected 'n' to be positive", n > 0);

    auto [queueTag, queueVal] = arrayQueueInit();

    auto [stateTag, stateVal] = value::makeNewArray();
    auto state = value::getArrayView(stateVal);
    state->push_back(queueTag, queueVal);
    state->push_back(nTag, nVal);
    return {true, stateTag, stateVal};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggFirstLastNAdd(ArityType arity) {
    auto [stateTag, stateVal] = moveOwnedFromStack(0);
    value::ValueGuard stateGuard{stateTag, stateVal};

    auto [fieldTag, fieldVal] = moveOwnedFromStack(1);
    value::ValueGuard fieldGuard{fieldTag, fieldVal};

    auto [queue, n] = firstLastNState(stateTag, stateVal);

    fieldGuard.reset();
    arrayQueuePush(queue, fieldTag, fieldVal);

    stateGuard.reset();
    return {true, stateTag, stateVal};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggFirstLastNRemove(
    ArityType arity) {
    auto [stateTag, stateVal] = moveOwnedFromStack(0);
    value::ValueGuard stateGuard{stateTag, stateVal};

    auto [fieldTag, fieldVal] = moveOwnedFromStack(1);
    value::ValueGuard fieldGuard{fieldTag, fieldVal};

    auto [queue, n] = firstLastNState(stateTag, stateVal);

    auto [popTag, popVal] = arrayQueuePop(queue);
    value::ValueGuard popValueGuard{popTag, popVal};

    auto [cmpTag, cmpVal] = value::compareValue(popTag, popVal, fieldTag, fieldVal);
    tassert(8070604,
            "Encountered unexpected value",
            cmpTag == value::TypeTags::NumberInt32 && cmpVal == 0);

    stateGuard.reset();
    return {true, stateTag, stateVal};
}

template <AccumulatorFirstLastN::Sense S>
FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggFirstLastNFinalize(
    ArityType arity) {
    auto [stateOwned, stateTag, stateVal] = getFromStack(0);
    auto [queue, n] = firstLastNState(stateTag, stateVal);

    if constexpr (S == AccumulatorFirstLastN::Sense::kFirst) {
        auto [arrTag, arrVal] = arrayQueueFrontN(queue, n);
        return {true, arrTag, arrVal};
    } else {
        auto [arrTag, arrVal] = arrayQueueBackN(queue, n);
        return {true, arrTag, arrVal};
    }
}

std::tuple<value::Array*, value::ArrayMultiSet*, int32_t> addToSetState(value::TypeTags stateTag,
                                                                        value::Value stateVal) {
    tassert(8124900, "state should be of type Array", stateTag == value::TypeTags::Array);
    auto stateArr = value::getArrayView(stateVal);
    tassert(8124901,
            str::stream() << "state array should have "
                          << static_cast<size_t>(AggArrayWithSize::kLast) << " elements",
            stateArr->size() == static_cast<size_t>(AggArrayWithSize::kLast));

    // Read the accumulator from the state.
    auto [accMultiSetTag, accMultiSetVal] =
        stateArr->getAt(static_cast<size_t>(AggArrayWithSize::kValues));
    tassert(8124902,
            "accumulator should be of type MultiSet",
            accMultiSetTag == value::TypeTags::ArrayMultiSet);
    auto accMultiSet = value::getArrayMultiSetView(accMultiSetVal);

    auto [accMultiSetSizeTag, accMultiSetSizeVal] =
        stateArr->getAt(static_cast<size_t>(AggArrayWithSize::kSizeOfValues));
    tassert(8124903,
            "accumulator size be of type NumberInt32",
            accMultiSetSizeTag == value::TypeTags::NumberInt32);

    return {stateArr, accMultiSet, value::bitcastTo<int32_t>(accMultiSetSizeVal)};
}

FastTuple<bool, value::TypeTags, value::Value> aggRemovableAddToSetInitImpl(
    CollatorInterface* collator) {
    auto [stateTag, stateVal] = value::makeNewArray();
    value::ValueGuard stateGuard{stateTag, stateVal};
    auto stateArr = value::getArrayView(stateVal);

    auto [mSetTag, mSetVal] = value::makeNewArrayMultiSet(collator);

    // the order is important!!!
    stateArr->push_back(mSetTag, mSetVal);  // the multiset with the values
    stateArr->push_back(value::TypeTags::NumberInt32,
                        value::bitcastFrom<int32_t>(0));  // the size in bytes of the multiset
    stateGuard.reset();
    return {true, stateTag, stateVal};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggRemovableAddToSetInit(
    ArityType arity) {
    return aggRemovableAddToSetInitImpl(nullptr /* collator */);
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggRemovableAddToSetCollInit(
    ArityType arity) {
    auto [collatorOwned, collatorTag, collatorVal] = getFromStack(0);
    tassert(8124904, "expected value of type 'collator'", collatorTag == value::TypeTags::collator);

    return aggRemovableAddToSetInitImpl(value::getCollatorView(collatorVal));
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggRemovableAddToSetAdd(
    ArityType arity) {
    auto [stateTag, stateVal] = moveOwnedFromStack(0);
    value::ValueGuard stateGuard{stateTag, stateVal};
    auto [newElTag, newElVal] = moveOwnedFromStack(1);
    value::ValueGuard newElGuard{newElTag, newElVal};
    auto [sizeCapOwned, sizeCapTag, sizeCapVal] = getFromStack(2);
    tassert(8124905,
            "The size cap must be of type NumberInt32",
            sizeCapTag == value::TypeTags::NumberInt32);
    auto capSize = value::bitcastTo<int32_t>(sizeCapVal);

    auto [stateArr, accMultiSet, accMultiSetSize] = addToSetState(stateTag, stateVal);

    // Check the size of the accumulator will not exceed the cap.
    int32_t newElSize = value::getApproximateSize(newElTag, newElVal);
    if (accMultiSetSize + newElSize >= capSize) {
        auto elsNum = accMultiSet->size();
        auto setTotalSize = accMultiSetSize;
        uasserted(ErrorCodes::ExceededMemoryLimit,
                  str::stream() << "Used too much memory for a single set. Memory limit: "
                                << capSize << " bytes. The set contains " << elsNum
                                << " elements and is of size " << setTotalSize
                                << " bytes. The element being added has size " << newElSize
                                << " bytes.");
    }

    // Update the state.
    stateArr->setAt(static_cast<size_t>(AggArrayWithSize::kSizeOfValues),
                    value::TypeTags::NumberInt32,
                    value::bitcastFrom<int32_t>(accMultiSetSize + newElSize));
    accMultiSet->push_back(newElTag, newElVal);
    newElGuard.reset();
    stateGuard.reset();
    return {true, stateTag, stateVal};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggRemovableAddToSetRemove(
    ArityType arity) {
    auto [stateTag, stateVal] = moveOwnedFromStack(0);
    value::ValueGuard stateGuard{stateTag, stateVal};
    auto [elTag, elVal] = moveOwnedFromStack(1);
    value::ValueGuard elGuard{elTag, elVal};

    auto [stateArr, accMultiSet, accMultiSetSize] = addToSetState(stateTag, stateVal);

    int32_t elSize = value::getApproximateSize(elTag, elVal);
    invariant(elSize <= accMultiSetSize);
    stateArr->setAt(static_cast<size_t>(AggArrayWithSize::kSizeOfValues),
                    value::TypeTags::NumberInt32,
                    value::bitcastFrom<int32_t>(accMultiSetSize - elSize));

    accMultiSet->remove(elTag, elVal);
    stateGuard.reset();
    return {true, stateTag, stateVal};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggRemovableAddToSetFinalize(
    ArityType arity) {
    auto [stateOwned, stateTag, stateVal] = getFromStack(0);

    auto [stateArr, accMultiSet, _] = addToSetState(stateTag, stateVal);

    // Convert the multiSet to Set.
    auto [accSetTag, accSetVal] = value::makeNewArraySet(accMultiSet->getCollator());
    value::ValueGuard resGuard{accSetTag, accSetVal};
    auto accSet = value::getArraySetView(accSetVal);
    for (const auto& p : accMultiSet->values()) {
        auto [cTag, cVal] = copyValue(p.first, p.second);
        accSet->push_back(cTag, cVal);
    }
    resGuard.reset();
    return {true, accSetTag, accSetVal};
}

static std::tuple<value::Array*, value::TypeTags, value::Value, size_t, int32_t, int32_t>
accumulatorNState(value::TypeTags stateTag, value::Value stateVal) {
    tassert(
        8178100, "The accumulator state should be an array", stateTag == value::TypeTags::Array);
    auto stateArr = value::getArrayView(stateVal);

    tassert(8178101,
            str::stream() << "state array should have "
                          << static_cast<size_t>(AggAccumulatorNElems::kSizeOfArray)
                          << " elements but found " << stateArr->size(),
            stateArr->size() == static_cast<size_t>(AggAccumulatorNElems::kSizeOfArray));

    // Read the accumulator from the state.
    auto [accumulatorTag, accumulatorVal] =
        stateArr->getAt(static_cast<size_t>(AggAccumulatorNElems::kValues));

    // Read N from the state
    auto [nTag, nVal] = stateArr->getAt(static_cast<size_t>(AggAccumulatorNElems::kN));
    tassert(8178103, "N should be of type NumberInt64", nTag == value::TypeTags::NumberInt64);

    // Read memory usage information from state
    auto [memUsageTag, memUsage] =
        stateArr->getAt(static_cast<size_t>(AggAccumulatorNElems::kMemUsage));
    tassert(8178104,
            "MemUsage component should be of type NumberInt32",
            memUsageTag == value::TypeTags::NumberInt32);

    auto [memLimitTag, memLimit] =
        stateArr->getAt(static_cast<size_t>(AggAccumulatorNElems::kMemLimit));
    tassert(8178105,
            "MemLimit component should be of type NumberInt32",
            memLimitTag == value::TypeTags::NumberInt32);

    return {stateArr,
            accumulatorTag,
            accumulatorVal,
            value::bitcastTo<size_t>(nVal),
            value::bitcastTo<int32_t>(memUsage),
            value::bitcastTo<int32_t>(memLimit)};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::aggRemovableMinMaxNInitImpl(
    CollatorInterface* collator) {
    auto [sizeOwned, sizeTag, sizeVal] = getFromStack(0);

    auto [nOwned, nTag, nVal] = genericNumConvert(sizeTag, sizeVal, value::TypeTags::NumberInt64);
    uassert(8178107, "Failed to convert to 64-bit integer", nTag == value::TypeTags::NumberInt64);

    auto n = value::bitcastTo<int64_t>(nVal);
    uassert(8178108, "Expected 'n' to be positive", n > 0);

    auto [sizeCapOwned, sizeCapTag, sizeCapVal] = getFromStack(1);
    uassert(8178109,
            "The size cap must be of type NumberInt32",
            sizeCapTag == value::TypeTags::NumberInt32);

    // Initialize the state
    auto [stateTag, stateVal] = value::makeNewArray();
    value::ValueGuard stateGuard{stateTag, stateVal};

    auto stateArr = value::getArrayView(stateVal);

    // the order is important!!!
    auto [mSetTag, mSetVal] = value::makeNewArrayMultiSet(collator);
    stateArr->push_back(mSetTag, mSetVal);  // The multiset with the values.
    stateArr->push_back(nTag, nVal);        // The maximum number of elements in the multiset.
    stateArr->push_back(value::TypeTags::NumberInt32,
                        value::bitcastFrom<int32_t>(0));  // The size of the multiset in bytes.
    stateArr->push_back(sizeCapTag,
                        sizeCapVal);  // The maximum possible size of the multiset in bytes.

    stateGuard.reset();
    return {true, stateTag, stateVal};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggRemovableMinMaxNCollInit(
    ArityType arity) {
    auto [collatorOwned, collatorTag, collatorVal] = getFromStack(2);
    tassert(8178111, "expected value of type 'collator'", collatorTag == value::TypeTags::collator);
    return aggRemovableMinMaxNInitImpl(value::getCollatorView(collatorVal));
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggRemovableMinMaxNInit(
    ArityType arity) {
    return aggRemovableMinMaxNInitImpl(nullptr);
}


FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggRemovableMinMaxNAdd(
    ArityType arity) {
    auto [stateTag, stateVal] = moveOwnedFromStack(0);
    value::ValueGuard stateGuard{stateTag, stateVal};
    auto [newElTag, newElVal] = moveOwnedFromStack(1);
    value::ValueGuard newElGuard{newElTag, newElVal};

    if (value::isNullish(newElTag)) {
        stateGuard.reset();
        return {true, stateTag, stateVal};
    }

    auto [stateArr, accMultiSetTag, accMultiSetVal, n, memUsage, memLimit] =
        accumulatorNState(stateTag, stateVal);
    tassert(8178102,
            "accumulator should be of type MultiSet",
            accMultiSetTag == value::TypeTags::ArrayMultiSet);
    auto accMultiSet = value::getArrayMultiSetView(accMultiSetVal);

    int32_t newElSize = value::getApproximateSize(newElTag, newElVal);

    updateAndCheckMemUsage(stateArr,
                           memUsage,
                           newElSize,
                           memLimit,
                           static_cast<size_t>(AggAccumulatorNElems::kMemUsage));

    newElGuard.reset();
    accMultiSet->push_back(newElTag, newElVal);

    stateGuard.reset();
    return {true, stateTag, stateVal};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggRemovableMinMaxNRemove(
    ArityType arity) {
    auto [stateTag, stateVal] = moveOwnedFromStack(0);
    value::ValueGuard stateGuard{stateTag, stateVal};
    auto [elTag, elVal] = moveOwnedFromStack(1);
    value::ValueGuard elGuard{elTag, elVal};

    if (value::isNullish(elTag)) {
        stateGuard.reset();
        return {true, stateTag, stateVal};
    }

    auto [stateArr, accMultiSetTag, accMultiSetVal, n, memUsage, memLimit] =
        accumulatorNState(stateTag, stateVal);
    tassert(8155723,
            "accumulator should be of type MultiSet",
            accMultiSetTag == value::TypeTags::ArrayMultiSet);
    auto accMultiSet = value::getArrayMultiSetView(accMultiSetVal);

    int32_t elSize = value::getApproximateSize(elTag, elVal);
    invariant(elSize <= memUsage);

    // remove element
    stateArr->setAt(static_cast<size_t>(AggAccumulatorNElems::kMemUsage),
                    value::TypeTags::NumberInt32,
                    value::bitcastFrom<int32_t>(memUsage - elSize));
    elGuard.reset();
    tassert(8178116, "Element was not removed", accMultiSet->remove(elTag, elVal));

    stateGuard.reset();
    return {true, stateTag, stateVal};
}

template <AccumulatorMinMaxN::MinMaxSense S>
FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggRemovableMinMaxNFinalize(
    ArityType arity) {
    auto [stateOwned, stateTag, stateVal] = getFromStack(0);

    auto [stateArr, accMultiSetTag, accMultiSetVal, n, memUsage, memLimit] =
        accumulatorNState(stateTag, stateVal);
    tassert(8155724,
            "accumulator should be of type MultiSet",
            accMultiSetTag == value::TypeTags::ArrayMultiSet);
    auto accMultiSet = value::getArrayMultiSetView(accMultiSetVal);

    // Create an empty array to fill with the results
    auto [resultArrayTag, resultArrayVal] = value::makeNewArray();
    value::ValueGuard resultGuard{resultArrayTag, resultArrayVal};
    auto resultArray = value::getArrayView(resultArrayVal);
    resultArray->reserve(n);

    if constexpr (S == AccumulatorMinMaxN::MinMaxSense::kMin) {
        for (auto it = accMultiSet->values().cbegin();
             it != accMultiSet->values().cend() && resultArray->size() < n;
             ++it) {
            resultArray->push_back(value::copyValue(it->first, it->second));
        }
    } else {
        for (auto it = accMultiSet->values().crbegin();
             it != accMultiSet->values().crend() && resultArray->size() < n;
             ++it) {
            resultArray->push_back(value::copyValue(it->first, it->second));
        }
    }

    resultGuard.reset();
    return {true, resultArrayTag, resultArrayVal};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggRemovableTopBottomNInit(
    ArityType arity) {
    auto [maxSizeOwned, maxSizeTag, maxSizeVal] = getFromStack(0);
    auto [memLimitOwned, memLimitTag, memLimitVal] = getFromStack(1);

    auto [nOwned, nTag, nVal] =
        genericNumConvert(maxSizeTag, maxSizeVal, value::TypeTags::NumberInt64);
    uassert(8155711, "Failed to convert to 64-bit integer", nTag == value::TypeTags::NumberInt64);

    auto n = value::bitcastTo<int64_t>(nVal);
    uassert(8155708, "Expected 'n' to be positive", n > 0);

    tassert(8155709,
            "memLimit should be of type NumberInt32",
            memLimitTag == value::TypeTags::NumberInt32);

    auto [stateTag, stateVal] = value::makeNewArray();
    value::ValueGuard stateGuard{stateTag, stateVal};
    auto stateArr = value::getArrayView(stateVal);

    auto [multiMapTag, multiMapVal] = value::makeNewMultiMap();
    stateArr->push_back(multiMapTag, multiMapVal);

    stateArr->push_back(nTag, nVal);
    stateArr->push_back(value::TypeTags::NumberInt32, 0);
    stateArr->push_back(memLimitTag, memLimitVal);

    stateGuard.reset();
    return {true, stateTag, stateVal};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggRemovableTopBottomNAdd(
    ArityType arity) {
    auto [stateTag, stateVal] = moveOwnedFromStack(0);
    value::ValueGuard stateGuard{stateTag, stateVal};

    auto [state, multiMapTag, multiMapVal, n, memSize, memLimit] =
        accumulatorNState(stateTag, stateVal);
    tassert(8155702, "value should be of type MultiMap", multiMapTag == value::TypeTags::MultiMap);
    auto multiMap = value::getMultiMapView(multiMapVal);

    auto key = moveOwnedFromStack(1);
    auto value = moveOwnedFromStack(2);

    multiMap->insert(key, value);

    auto kvSize = value::getApproximateSize(key.first, key.second) +
        value::getApproximateSize(value.first, value.second);
    updateAndCheckMemUsage(
        state, memSize, kvSize, memLimit, static_cast<size_t>(AggAccumulatorNElems::kMemUsage));

    stateGuard.reset();
    return {true, stateTag, stateVal};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggRemovableTopBottomNRemove(
    ArityType arity) {
    auto [stateTag, stateVal] = moveOwnedFromStack(0);
    value::ValueGuard stateGuard{stateTag, stateVal};

    auto [state, multiMapTag, multiMapVal, n, memSize, memLimit] =
        accumulatorNState(stateTag, stateVal);
    tassert(8155726, "value should be of type MultiMap", multiMapTag == value::TypeTags::MultiMap);
    auto multiMap = value::getMultiMapView(multiMapVal);

    auto [keyOwned, keyTag, keyVal] = getFromStack(1);
    auto [outputOwned, outputTag, outputVal] = getFromStack(2);

    auto removed = multiMap->remove({keyTag, keyVal});
    tassert(8155707, "Failed to remove element from map", removed);

    auto elemSize =
        value::getApproximateSize(keyTag, keyVal) + value::getApproximateSize(outputTag, outputVal);
    memSize -= elemSize;
    state->setAt(static_cast<size_t>(AggAccumulatorNElems::kMemUsage),
                 value::TypeTags::NumberInt32,
                 value::bitcastFrom<int32_t>(memSize));

    stateGuard.reset();
    return {true, stateTag, stateVal};
}

template <TopBottomSense sense>
FastTuple<bool, value::TypeTags, value::Value> ByteCode::builtinAggRemovableTopBottomNFinalize(
    ArityType arity) {
    auto [stateOwned, stateTag, stateVal] = getFromStack(0);

    auto [state, multiMapTag, multiMapVal, n, memSize, memLimit] =
        accumulatorNState(stateTag, stateVal);
    tassert(8155727, "value should be of type MultiMap", multiMapTag == value::TypeTags::MultiMap);
    auto multiMap = value::getMultiMapView(multiMapVal);

    auto& values = multiMap->values();
    auto begin = values.begin();
    auto end = values.end();

    if constexpr (sense == TopBottomSense::kBottom) {
        // If this accumulator is removable there may be more than n elements in the map, so we must
        // skip elements that shouldn't be in the result.
        if (static_cast<size_t>(values.size()) > n) {
            std::advance(begin, values.size() - n);
        }
    }

    auto [resTag, resVal] = value::makeNewArray();
    value::ValueGuard resGuard{resTag, resVal};
    auto resArr = value::getArrayView(resVal);

    auto it = begin;
    for (size_t inserted = 0; inserted < n && it != end; ++inserted, ++it) {
        const auto& keyOutPair = *it;
        auto output = keyOutPair.second;
        auto [copyTag, copyVal] = value::copyValue(output.first, output.second);
        resArr->push_back(copyTag, copyVal);
    };

    resGuard.reset();
    return {true, resTag, resVal};
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::dispatchBuiltin(Builtin f,
                                                                         ArityType arity,
                                                                         const CodeFragment* code) {
    switch (f) {
        case Builtin::dateDiff:
            return builtinDateDiff(arity);
        case Builtin::dateParts:
            return builtinDate(arity);
        case Builtin::datePartsWeekYear:
            return builtinDateWeekYear(arity);
        case Builtin::dateToParts:
            return builtinDateToParts(arity);
        case Builtin::isoDateToParts:
            return builtinIsoDateToParts(arity);
        case Builtin::dayOfYear:
            return builtinDayOfYear(arity);
        case Builtin::dayOfMonth:
            return builtinDayOfMonth(arity);
        case Builtin::dayOfWeek:
            return builtinDayOfWeek(arity);
        case Builtin::dateToString:
            return builtinDateToString(arity);
        case Builtin::dateFromString:
            return builtinDateFromString(arity);
        case Builtin::dateFromStringNoThrow:
            return builtinDateFromStringNoThrow(arity);
        case Builtin::split:
            return builtinSplit(arity);
        case Builtin::regexMatch:
            return builtinRegexMatch(arity);
        case Builtin::replaceOne:
            return builtinReplaceOne(arity);
        case Builtin::dropFields:
            return builtinDropFields(arity);
        case Builtin::newArray:
            return builtinNewArray(arity);
        case Builtin::keepFields:
            return builtinKeepFields(arity);
        case Builtin::newArrayFromRange:
            return builtinNewArrayFromRange(arity);
        case Builtin::newObj:
            return builtinNewObj(arity);
        case Builtin::newBsonObj:
            return builtinNewBsonObj(arity);
        case Builtin::ksToString:
            return builtinKeyStringToString(arity);
        case Builtin::newKs:
            return builtinNewKeyString(arity);
        case Builtin::collNewKs:
            return builtinCollNewKeyString(arity);
        case Builtin::abs:
            return builtinAbs(arity);
        case Builtin::ceil:
            return builtinCeil(arity);
        case Builtin::floor:
            return builtinFloor(arity);
        case Builtin::trunc:
            return builtinTrunc(arity);
        case Builtin::exp:
            return builtinExp(arity);
        case Builtin::ln:
            return builtinLn(arity);
        case Builtin::log10:
            return builtinLog10(arity);
        case Builtin::sqrt:
            return builtinSqrt(arity);
        case Builtin::pow:
            return builtinPow(arity);
        case Builtin::addToArray:
            return builtinAddToArray(arity);
        case Builtin::addToArrayCapped:
            return builtinAddToArrayCapped(arity);
        case Builtin::mergeObjects:
            return builtinMergeObjects(arity);
        case Builtin::addToSet:
            return builtinAddToSet(arity);
        case Builtin::addToSetCapped:
            return builtinAddToSetCapped(arity);
        case Builtin::collAddToSet:
            return builtinCollAddToSet(arity);
        case Builtin::collAddToSetCapped:
            return builtinCollAddToSetCapped(arity);
        case Builtin::doubleDoubleSum:
            return builtinDoubleDoubleSum(arity);
        case Builtin::aggDoubleDoubleSum:
            return builtinAggDoubleDoubleSum<false /*merging*/>(arity);
        case Builtin::doubleDoubleSumFinalize:
            return builtinDoubleDoubleSumFinalize(arity);
        case Builtin::doubleDoublePartialSumFinalize:
            return builtinDoubleDoublePartialSumFinalize(arity);
        case Builtin::aggMergeDoubleDoubleSums:
            return builtinAggDoubleDoubleSum<true /*merging*/>(arity);
        case Builtin::aggStdDev:
            return builtinAggStdDev<false /*merging*/>(arity);
        case Builtin::aggMergeStdDevs:
            return builtinAggStdDev<true /*merging*/>(arity);
        case Builtin::stdDevPopFinalize:
            return builtinStdDevPopFinalize(arity);
        case Builtin::stdDevSampFinalize:
            return builtinStdDevSampFinalize(arity);
        case Builtin::bitTestZero:
            return builtinBitTestZero(arity);
        case Builtin::bitTestMask:
            return builtinBitTestMask(arity);
        case Builtin::bitTestPosition:
            return builtinBitTestPosition(arity);
        case Builtin::bsonSize:
            return builtinBsonSize(arity);
        case Builtin::strLenBytes:
            return builtinStrLenBytes(arity);
        case Builtin::toUpper:
            return builtinToUpper(arity);
        case Builtin::toLower:
            return builtinToLower(arity);
        case Builtin::trim:
            return builtinTrim(arity, true, true);
        case Builtin::ltrim:
            return builtinTrim(arity, true, false);
        case Builtin::rtrim:
            return builtinTrim(arity, false, true);
        case Builtin::coerceToBool:
            return builtinCoerceToBool(arity);
        case Builtin::coerceToString:
            return builtinCoerceToString(arity);
        case Builtin::acos:
            return builtinAcos(arity);
        case Builtin::acosh:
            return builtinAcosh(arity);
        case Builtin::asin:
            return builtinAsin(arity);
        case Builtin::asinh:
            return builtinAsinh(arity);
        case Builtin::atan:
            return builtinAtan(arity);
        case Builtin::atanh:
            return builtinAtanh(arity);
        case Builtin::atan2:
            return builtinAtan2(arity);
        case Builtin::cos:
            return builtinCos(arity);
        case Builtin::cosh:
            return builtinCosh(arity);
        case Builtin::degreesToRadians:
            return builtinDegreesToRadians(arity);
        case Builtin::radiansToDegrees:
            return builtinRadiansToDegrees(arity);
        case Builtin::sin:
            return builtinSin(arity);
        case Builtin::sinh:
            return builtinSinh(arity);
        case Builtin::tan:
            return builtinTan(arity);
        case Builtin::tanh:
            return builtinTanh(arity);
        case Builtin::round:
            return builtinRound(arity);
        case Builtin::concat:
            return builtinConcat(arity);
        case Builtin::concatArrays:
            return builtinConcatArrays(arity);
        case Builtin::aggConcatArraysCapped:
            return builtinAggConcatArraysCapped(arity);
        case Builtin::aggSetUnion:
            return builtinAggSetUnion(arity);
        case Builtin::aggCollSetUnion:
            return builtinAggCollSetUnion(arity);
        case Builtin::aggSetUnionCapped:
            return builtinAggSetUnionCapped(arity);
        case Builtin::aggCollSetUnionCapped:
            return builtinAggCollSetUnionCapped(arity);
        case Builtin::isMember:
            return builtinIsMember(arity);
        case Builtin::indexOfBytes:
            return builtinIndexOfBytes(arity);
        case Builtin::indexOfCP:
            return builtinIndexOfCP(arity);
        case Builtin::isDayOfWeek:
            return builtinIsDayOfWeek(arity);
        case Builtin::isTimeUnit:
            return builtinIsTimeUnit(arity);
        case Builtin::isTimezone:
            return builtinIsTimezone(arity);
        case Builtin::isValidToStringFormat:
            return builtinIsValidToStringFormat(arity);
        case Builtin::validateFromStringFormat:
            return builtinValidateFromStringFormat(arity);
        case Builtin::setUnion:
            return builtinSetUnion(arity);
        case Builtin::setIntersection:
            return builtinSetIntersection(arity);
        case Builtin::setDifference:
            return builtinSetDifference(arity);
        case Builtin::setEquals:
            return builtinSetEquals(arity);
        case Builtin::setIsSubset:
            return builtinSetIsSubset(arity);
        case Builtin::collSetUnion:
            return builtinCollSetUnion(arity);
        case Builtin::collSetIntersection:
            return builtinCollSetIntersection(arity);
        case Builtin::collSetDifference:
            return builtinCollSetDifference(arity);
        case Builtin::collSetEquals:
            return builtinCollSetEquals(arity);
        case Builtin::collSetIsSubset:
            return builtinCollSetIsSubset(arity);
        case Builtin::runJsPredicate:
            return builtinRunJsPredicate(arity);
        case Builtin::regexCompile:
            return builtinRegexCompile(arity);
        case Builtin::regexFind:
            return builtinRegexFind(arity);
        case Builtin::regexFindAll:
            return builtinRegexFindAll(arity);
        case Builtin::shardFilter:
            return builtinShardFilter(arity);
        case Builtin::shardHash:
            return builtinShardHash(arity);
        case Builtin::extractSubArray:
            return builtinExtractSubArray(arity);
        case Builtin::isArrayEmpty:
            return builtinIsArrayEmpty(arity);
        case Builtin::reverseArray:
            return builtinReverseArray(arity);
        case Builtin::sortArray:
            return builtinSortArray(arity);
        case Builtin::dateAdd:
            return builtinDateAdd(arity);
        case Builtin::hasNullBytes:
            return builtinHasNullBytes(arity);
        case Builtin::getRegexPattern:
            return builtinGetRegexPattern(arity);
        case Builtin::getRegexFlags:
            return builtinGetRegexFlags(arity);
        case Builtin::hash:
            return builtinHash(arity);
        case Builtin::ftsMatch:
            return builtinFtsMatch(arity);
        case Builtin::generateSortKey:
            return builtinGenerateSortKey(arity);
        case Builtin::generateCheapSortKey:
            return builtinGenerateCheapSortKey(arity);
        case Builtin::sortKeyComponentVectorGetElement:
            return builtinSortKeyComponentVectorGetElement(arity);
        case Builtin::sortKeyComponentVectorToArray:
            return builtinSortKeyComponentVectorToArray(arity);
        case Builtin::makeBsonObj:
            return builtinMakeBsonObj(arity, code);
        case Builtin::tsSecond:
            return builtinTsSecond(arity);
        case Builtin::tsIncrement:
            return builtinTsIncrement(arity);
        case Builtin::typeMatch:
            return builtinTypeMatch(arity);
        case Builtin::dateTrunc:
            return builtinDateTrunc(arity);
        case Builtin::internalLeast:
        case Builtin::internalGreatest:
            return builtinMinMaxFromArray(arity, f);
        case Builtin::year:
            return builtinYear(arity);
        case Builtin::month:
            return builtinMonth(arity);
        case Builtin::hour:
            return builtinHour(arity);
        case Builtin::minute:
            return builtinMinute(arity);
        case Builtin::second:
            return builtinSecond(arity);
        case Builtin::millisecond:
            return builtinMillisecond(arity);
        case Builtin::week:
            return builtinWeek(arity);
        case Builtin::isoWeekYear:
            return builtinISOWeekYear(arity);
        case Builtin::isoDayOfWeek:
            return builtinISODayOfWeek(arity);
        case Builtin::isoWeek:
            return builtinISOWeek(arity);
        case Builtin::objectToArray:
            return builtinObjectToArray(arity);
        case Builtin::arrayToObject:
            return builtinArrayToObject(arity);
        case Builtin::setToArray:
            return builtinSetToArray(arity);
        case Builtin::aggFirstNNeedsMoreInput:
            return builtinAggFirstNNeedsMoreInput(arity);
        case Builtin::aggFirstN:
            return builtinAggFirstN(arity);
        case Builtin::aggFirstNMerge:
            return builtinAggFirstNMerge(arity);
        case Builtin::aggFirstNFinalize:
            return builtinAggFirstNFinalize(arity);
        case Builtin::aggLastN:
            return builtinAggLastN(arity);
        case Builtin::aggLastNMerge:
            return builtinAggLastNMerge(arity);
        case Builtin::aggLastNFinalize:
            return builtinAggLastNFinalize(arity);
        case Builtin::aggTopN:
            return builtinAggTopBottomN<SortPatternLess>(arity);
        case Builtin::aggTopNMerge:
            return builtinAggTopBottomNMerge<SortPatternLess>(arity);
        case Builtin::aggTopNFinalize:
            return builtinAggTopBottomNFinalize(arity);
        case Builtin::aggBottomN:
            return builtinAggTopBottomN<SortPatternGreater>(arity);
        case Builtin::aggBottomNMerge:
            return builtinAggTopBottomNMerge<SortPatternGreater>(arity);
        case Builtin::aggBottomNFinalize:
            return builtinAggTopBottomNFinalize(arity);
        case Builtin::aggMaxN:
            return builtinAggMinMaxN<AccumulatorMinMaxN::MinMaxSense::kMax>(arity);
        case Builtin::aggMaxNMerge:
            return builtinAggMinMaxNMerge<AccumulatorMinMaxN::MinMaxSense::kMax>(arity);
        case Builtin::aggMaxNFinalize:
            return builtinAggMinMaxNFinalize<AccumulatorMinMaxN::MinMaxSense::kMax>(arity);
        case Builtin::aggMinN:
            return builtinAggMinMaxN<AccumulatorMinMaxN::MinMaxSense::kMin>(arity);
        case Builtin::aggMinNMerge:
            return builtinAggMinMaxNMerge<AccumulatorMinMaxN::MinMaxSense::kMin>(arity);
        case Builtin::aggMinNFinalize:
            return builtinAggMinMaxNFinalize<AccumulatorMinMaxN::MinMaxSense::kMin>(arity);
        case Builtin::aggRank:
            return builtinAggRank(arity);
        case Builtin::aggRankColl:
            return builtinAggRankColl(arity);
        case Builtin::aggDenseRank:
            return builtinAggDenseRank(arity);
        case Builtin::aggDenseRankColl:
            return builtinAggDenseRankColl(arity);
        case Builtin::aggRankFinalize:
            return builtinAggRankFinalize(arity);
        case Builtin::aggExpMovingAvg:
            return builtinAggExpMovingAvg(arity);
        case Builtin::aggExpMovingAvgFinalize:
            return builtinAggExpMovingAvgFinalize(arity);
        case Builtin::aggRemovableSumAdd:
            return builtinAggRemovableSum<1 /*sign*/>(arity);
        case Builtin::aggRemovableSumRemove:
            return builtinAggRemovableSum<-1 /*sign*/>(arity);
        case Builtin::aggRemovableSumFinalize:
            return builtinAggRemovableSumFinalize(arity);
        case Builtin::aggIntegralInit:
            return builtinAggIntegralInit(arity);
        case Builtin::aggIntegralAdd:
            return builtinAggIntegralAdd(arity);
        case Builtin::aggIntegralRemove:
            return builtinAggIntegralRemove(arity);
        case Builtin::aggIntegralFinalize:
            return builtinAggIntegralFinalize(arity);
        case Builtin::aggDerivativeFinalize:
            return builtinAggDerivativeFinalize(arity);
        case Builtin::aggCovarianceAdd:
            return builtinAggCovarianceAdd(arity);
        case Builtin::aggCovarianceRemove:
            return builtinAggCovarianceRemove(arity);
        case Builtin::aggCovarianceSampFinalize:
            return builtinAggCovarianceSampFinalize(arity);
        case Builtin::aggCovariancePopFinalize:
            return builtinAggCovariancePopFinalize(arity);
        case Builtin::aggRemovablePushAdd:
            return builtinAggRemovablePushAdd(arity);
        case Builtin::aggRemovablePushRemove:
            return builtinAggRemovablePushRemove(arity);
        case Builtin::aggRemovablePushFinalize:
            return builtinAggRemovablePushFinalize(arity);
        case Builtin::aggRemovableStdDevAdd:
            return builtinAggRemovableStdDevAdd(arity);
        case Builtin::aggRemovableStdDevRemove:
            return builtinAggRemovableStdDevRemove(arity);
        case Builtin::aggRemovableStdDevSampFinalize:
            return builtinAggRemovableStdDevSampFinalize(arity);
        case Builtin::aggRemovableStdDevPopFinalize:
            return builtinAggRemovableStdDevPopFinalize(arity);
        case Builtin::aggRemovableAvgFinalize:
            return builtinAggRemovableAvgFinalize(arity);
        case Builtin::aggRemovableFirstNInit:
            return builtinAggFirstLastNInit(arity);
        case Builtin::aggRemovableFirstNAdd:
            return builtinAggFirstLastNAdd(arity);
        case Builtin::aggRemovableFirstNRemove:
            return builtinAggFirstLastNRemove(arity);
        case Builtin::aggRemovableFirstNFinalize:
            return builtinAggFirstLastNFinalize<AccumulatorFirstLastN::Sense::kFirst>(arity);
        case Builtin::aggRemovableLastNInit:
            return builtinAggFirstLastNInit(arity);
        case Builtin::aggRemovableLastNAdd:
            return builtinAggFirstLastNAdd(arity);
        case Builtin::aggRemovableLastNRemove:
            return builtinAggFirstLastNRemove(arity);
        case Builtin::aggRemovableLastNFinalize:
            return builtinAggFirstLastNFinalize<AccumulatorFirstLastN::Sense::kLast>(arity);
        case Builtin::aggRemovableAddToSetInit:
            return builtinAggRemovableAddToSetInit(arity);
        case Builtin::aggRemovableAddToSetCollInit:
            return builtinAggRemovableAddToSetCollInit(arity);
        case Builtin::aggRemovableAddToSetAdd:
            return builtinAggRemovableAddToSetAdd(arity);
        case Builtin::aggRemovableAddToSetRemove:
            return builtinAggRemovableAddToSetRemove(arity);
        case Builtin::aggRemovableAddToSetFinalize:
            return builtinAggRemovableAddToSetFinalize(arity);
        case Builtin::aggRemovableMinMaxNCollInit:
            return builtinAggRemovableMinMaxNCollInit(arity);
        case Builtin::aggRemovableMinMaxNInit:
            return builtinAggRemovableMinMaxNInit(arity);
        case Builtin::aggRemovableMinMaxNAdd:
            return builtinAggRemovableMinMaxNAdd(arity);
        case Builtin::aggRemovableMinMaxNRemove:
            return builtinAggRemovableMinMaxNRemove(arity);
        case Builtin::aggRemovableMinNFinalize:
            return builtinAggRemovableMinMaxNFinalize<AccumulatorMinMaxN::MinMaxSense::kMin>(arity);
        case Builtin::aggRemovableMaxNFinalize:
            return builtinAggRemovableMinMaxNFinalize<AccumulatorMinMaxN::MinMaxSense::kMax>(arity);
        case Builtin::aggRemovableTopNInit:
        case Builtin::aggRemovableBottomNInit:
            return builtinAggRemovableTopBottomNInit(arity);
        case Builtin::aggRemovableTopNAdd:
        case Builtin::aggRemovableBottomNAdd:
            return builtinAggRemovableTopBottomNAdd(arity);
        case Builtin::aggRemovableTopNRemove:
        case Builtin::aggRemovableBottomNRemove:
            return builtinAggRemovableTopBottomNRemove(arity);
        case Builtin::aggRemovableTopNFinalize:
            return builtinAggRemovableTopBottomNFinalize<TopBottomSense::kTop>(arity);
        case Builtin::aggRemovableBottomNFinalize:
            return builtinAggRemovableTopBottomNFinalize<TopBottomSense::kBottom>(arity);
        case Builtin::aggLinearFillCanAdd:
            return builtinAggLinearFillCanAdd(arity);
        case Builtin::aggLinearFillAdd:
            return builtinAggLinearFillAdd(arity);
        case Builtin::aggLinearFillFinalize:
            return builtinAggLinearFillFinalize(arity);
        case Builtin::valueBlockExists:
            return builtinValueBlockExists(arity);
        case Builtin::valueBlockFillEmpty:
            return builtinValueBlockFillEmpty(arity);
        case Builtin::valueBlockFillEmptyBlock:
            return builtinValueBlockFillEmptyBlock(arity);
        case Builtin::valueBlockMin:
            return builtinValueBlockMin(arity);
        case Builtin::valueBlockMax:
            return builtinValueBlockMax(arity);
        case Builtin::valueBlockCount:
            return builtinValueBlockCount(arity);
        case Builtin::valueBlockDateDiff:
            return builtinValueBlockDateDiff(arity);
        case Builtin::valueBlockDateTrunc:
            return builtinValueBlockDateTrunc(arity);
        case Builtin::valueBlockTrunc:
            return builtinValueBlockTrunc(arity);
        case Builtin::valueBlockRound:
            return builtinValueBlockRound(arity);
        case Builtin::valueBlockSum:
            return builtinValueBlockSum(arity);
        case Builtin::valueBlockAdd:
            return builtinValueBlockAdd(arity);
        case Builtin::valueBlockSub:
            return builtinValueBlockSub(arity);
        case Builtin::valueBlockMult:
            return builtinValueBlockMult(arity);
        case Builtin::valueBlockDiv:
            return builtinValueBlockDiv(arity);
        case Builtin::valueBlockGtScalar:
            return builtinValueBlockGtScalar(arity);
        case Builtin::valueBlockGteScalar:
            return builtinValueBlockGteScalar(arity);
        case Builtin::valueBlockEqScalar:
            return builtinValueBlockEqScalar(arity);
        case Builtin::valueBlockNeqScalar:
            return builtinValueBlockNeqScalar(arity);
        case Builtin::valueBlockLtScalar:
            return builtinValueBlockLtScalar(arity);
        case Builtin::valueBlockLteScalar:
            return builtinValueBlockLteScalar(arity);
        case Builtin::valueBlockCmp3wScalar:
            return builtinValueBlockCmp3wScalar(arity);
        case Builtin::valueBlockCombine:
            return builtinValueBlockCombine(arity);
        case Builtin::valueBlockLogicalAnd:
            return builtinValueBlockLogicalAnd(arity);
        case Builtin::valueBlockLogicalOr:
            return builtinValueBlockLogicalOr(arity);
        case Builtin::valueBlockLogicalNot:
            return builtinValueBlockLogicalNot(arity);
        case Builtin::valueBlockNewFill:
            return builtinValueBlockNewFill(arity);
        case Builtin::valueBlockSize:
            return builtinValueBlockSize(arity);
        case Builtin::valueBlockNone:
            return builtinValueBlockNone(arity);
        case Builtin::valueBlockIsMember:
            return builtinValueBlockIsMember(arity);
        case Builtin::valueBlockCoerceToBool:
            return builtinValueBlockCoerceToBool(arity);
        case Builtin::cellFoldValues_F:
            return builtinCellFoldValues_F(arity);
        case Builtin::cellFoldValues_P:
            return builtinCellFoldValues_P(arity);
        case Builtin::cellBlockGetFlatValuesBlock:
            return builtinCellBlockGetFlatValuesBlock(arity);
    }

    MONGO_UNREACHABLE;
}


std::string builtinToString(Builtin b) {
    switch (b) {
        case Builtin::split:
            return "split";
        case Builtin::regexMatch:
            return "regexMatch";
        case Builtin::replaceOne:
            return "replaceOne";
        case Builtin::dateDiff:
            return "dateDiff";
        case Builtin::dateParts:
            return "dateParts";
        case Builtin::dateToParts:
            return "dateToParts";
        case Builtin::isoDateToParts:
            return "isoDateToParts";
        case Builtin::dayOfYear:
            return "dayOfYear";
        case Builtin::dayOfMonth:
            return "dayOfMonth";
        case Builtin::dayOfWeek:
            return "dayOfWeek";
        case Builtin::datePartsWeekYear:
            return "datePartsWeekYear";
        case Builtin::dateToString:
            return "dateToString";
        case Builtin::dateFromString:
            return "dateFromString";
        case Builtin::dateFromStringNoThrow:
            return "dateFromStringNoThrow";
        case Builtin::dropFields:
            return "dropFields";
        case Builtin::newArray:
            return "newArray";
        case Builtin::keepFields:
            return "keepFields";
        case Builtin::newArrayFromRange:
            return "newArrayFromRange";
        case Builtin::newObj:
            return "newObj";
        case Builtin::newBsonObj:
            return "newBsonObj";
        case Builtin::ksToString:
            return "ksToString";
        case Builtin::newKs:
            return "newKs";
        case Builtin::collNewKs:
            return "collNewKs";
        case Builtin::abs:
            return "abs";
        case Builtin::ceil:
            return "ceil";
        case Builtin::floor:
            return "floor";
        case Builtin::trunc:
            return "trunc";
        case Builtin::exp:
            return "exp";
        case Builtin::ln:
            return "ln";
        case Builtin::log10:
            return "log10";
        case Builtin::sqrt:
            return "sqrt";
        case Builtin::pow:
            return "pow";
        case Builtin::addToArray:
            return "addToArray";
        case Builtin::addToArrayCapped:
            return "addToArrayCapped";
        case Builtin::mergeObjects:
            return "mergeObjects";
        case Builtin::addToSet:
            return "addToSet";
        case Builtin::addToSetCapped:
            return "addToSetCapped";
        case Builtin::collAddToSet:
            return "collAddToSet";
        case Builtin::collAddToSetCapped:
            return "collAddToSetCapped";
        case Builtin::doubleDoubleSum:
            return "doubleDoubleSum";
        case Builtin::aggDoubleDoubleSum:
            return "aggDoubleDoubleSum";
        case Builtin::doubleDoubleSumFinalize:
            return "doubleDoubleSumFinalize";
        case Builtin::doubleDoublePartialSumFinalize:
            return "doubleDoublePartialSumFinalize";
        case Builtin::aggMergeDoubleDoubleSums:
            return "aggMergeDoubleDoubleSums";
        case Builtin::aggStdDev:
            return "aggStdDev";
        case Builtin::aggMergeStdDevs:
            return "aggMergeStdDevs";
        case Builtin::stdDevPopFinalize:
            return "stdDevPopFinalize";
        case Builtin::stdDevSampFinalize:
            return "stdDevSampFinalize";
        case Builtin::bitTestZero:
            return "bitTestZero";
        case Builtin::bitTestMask:
            return "bitTestMask";
        case Builtin::bitTestPosition:
            return "bitTestPosition";
        case Builtin::bsonSize:
            return "bsonSize";
        case Builtin::strLenBytes:
            return "strLenBytes";
        case Builtin::toUpper:
            return "toUpper";
        case Builtin::toLower:
            return "toLower";
        case Builtin::trim:
            return "trim";
        case Builtin::ltrim:
            return "ltrim";
        case Builtin::rtrim:
            return "rtrim";
        case Builtin::coerceToBool:
            return "coerceToBool";
        case Builtin::coerceToString:
            return "coerceToString";
        case Builtin::concat:
            return "concat";
        case Builtin::concatArrays:
            return "concatArrays";
        case Builtin::aggConcatArraysCapped:
            return "aggConcatArraysCapped";
        case Builtin::aggSetUnion:
            return "aggSetUnion";
        case Builtin::aggCollSetUnion:
            return "aggCollSetUnion";
        case Builtin::aggSetUnionCapped:
            return "aggSetUnionCapped";
        case Builtin::aggCollSetUnionCapped:
            return "aggCollSetUnionCapped";
        case Builtin::acos:
            return "acos";
        case Builtin::acosh:
            return "acosh";
        case Builtin::asin:
            return "asin";
        case Builtin::asinh:
            return "asinh";
        case Builtin::atan:
            return "atan";
        case Builtin::atanh:
            return "atanh";
        case Builtin::atan2:
            return "atan2";
        case Builtin::cos:
            return "cos";
        case Builtin::cosh:
            return "cosh";
        case Builtin::degreesToRadians:
            return "degreesToRadians";
        case Builtin::radiansToDegrees:
            return "radiansToDegrees";
        case Builtin::sin:
            return "sin";
        case Builtin::sinh:
            return "sinh";
        case Builtin::tan:
            return "tan";
        case Builtin::tanh:
            return "tanh";
        case Builtin::round:
            return "round";
        case Builtin::isMember:
            return "isMember";
        case Builtin::indexOfBytes:
            return "indexOfBytes";
        case Builtin::indexOfCP:
            return "indexOfCP";
        case Builtin::isDayOfWeek:
            return "isDayOfWeek";
        case Builtin::isTimeUnit:
            return "isTimeUnit";
        case Builtin::isTimezone:
            return "isTimezone";
        case Builtin::isValidToStringFormat:
            return "isValidToStringFormat";
        case Builtin::validateFromStringFormat:
            return "validateFromStringFormat";
        case Builtin::setUnion:
            return "setUnion";
        case Builtin::setIntersection:
            return "setIntersection";
        case Builtin::setDifference:
            return "setDifference";
        case Builtin::setEquals:
            return "setEquals";
        case Builtin::collSetUnion:
            return "collSetUnion";
        case Builtin::collSetIntersection:
            return "collSetIntersection";
        case Builtin::collSetDifference:
            return "collSetDifference";
        case Builtin::collSetEquals:
            return "collSetEquals";
        case Builtin::runJsPredicate:
            return "runJsPredicate";
        case Builtin::regexCompile:
            return "regexCompile";
        case Builtin::regexFind:
            return "regexFind";
        case Builtin::regexFindAll:
            return "regexFindAll";
        case Builtin::shardFilter:
            return "shardFilter";
        case Builtin::shardHash:
            return "shardHash";
        case Builtin::extractSubArray:
            return "extractSubArray";
        case Builtin::isArrayEmpty:
            return "isArrayEmpty";
        case Builtin::reverseArray:
            return "reverseArray";
        case Builtin::sortArray:
            return "sortArray";
        case Builtin::dateAdd:
            return "dateAdd";
        case Builtin::hasNullBytes:
            return "hasNullBytes";
        case Builtin::getRegexPattern:
            return "getRegexPattern";
        case Builtin::getRegexFlags:
            return "getRegexFlags";
        case Builtin::hash:
            return "hash";
        case Builtin::ftsMatch:
            return "ftsMatch";
        case Builtin::generateSortKey:
            return "generateSortKey";
        case Builtin::generateCheapSortKey:
            return "generateCheapSortKey";
        case Builtin::sortKeyComponentVectorGetElement:
            return "sortKeyComponentVectorGetElement";
        case Builtin::sortKeyComponentVectorToArray:
            return "sortKeyComponentVectorToArray";
        case Builtin::makeBsonObj:
            return "makeBsonObj";
        case Builtin::tsSecond:
            return "tsSecond";
        case Builtin::tsIncrement:
            return "tsIncrement";
        case Builtin::typeMatch:
            return "typeMatch";
        case Builtin::dateTrunc:
            return "dateTrunc";
        case Builtin::internalLeast:
            return "internalLeast";
        case Builtin::internalGreatest:
            return "internalGreatest";
        case Builtin::year:
            return "year";
        case Builtin::month:
            return "month";
        case Builtin::hour:
            return "hour";
        case Builtin::minute:
            return "minute";
        case Builtin::second:
            return "second";
        case Builtin::millisecond:
            return "millisecond";
        case Builtin::week:
            return "week";
        case Builtin::isoWeekYear:
            return "isoWeekYear";
        case Builtin::isoDayOfWeek:
            return "isoDayOfWeek";
        case Builtin::isoWeek:
            return "isoWeek";
        case Builtin::objectToArray:
            return "objectToArray";
        case Builtin::arrayToObject:
            return "arrayToObject";
        case Builtin::setToArray:
            return "setToArray";
        case Builtin::aggFirstNNeedsMoreInput:
            return "aggFirstNNeedsMoreInput";
        case Builtin::aggFirstN:
            return "aggFirstN";
        case Builtin::aggFirstNMerge:
            return "aggFirstNMerge";
        case Builtin::aggFirstNFinalize:
            return "aggFirstNFinalize";
        case Builtin::aggLastN:
            return "aggLastN";
        case Builtin::aggLastNMerge:
            return "aggLastNMerge";
        case Builtin::aggLastNFinalize:
            return "aggLastNFinalize";
        case Builtin::aggTopN:
            return "aggTopN";
        case Builtin::aggTopNMerge:
            return "aggTopNMerge";
        case Builtin::aggTopNFinalize:
            return "aggTopNFinalize";
        case Builtin::aggBottomN:
            return "aggBottomN";
        case Builtin::aggBottomNMerge:
            return "aggBottomNMerge";
        case Builtin::aggBottomNFinalize:
            return "aggBottomNFinalize";
        case Builtin::aggMaxN:
            return "aggMaxN";
        case Builtin::aggMaxNMerge:
            return "aggMaxNMerge";
        case Builtin::aggMaxNFinalize:
            return "aggMaxNFinalize";
        case Builtin::aggMinN:
            return "aggMinN";
        case Builtin::aggMinNMerge:
            return "aggMinNMerge";
        case Builtin::aggMinNFinalize:
            return "aggMinNFinalize";
        case Builtin::aggRank:
            return "aggRank";
        case Builtin::aggRankColl:
            return "aggRankColl";
        case Builtin::aggDenseRank:
            return "aggDenseRank";
        case Builtin::aggDenseRankColl:
            return "aggDenseRankColl";
        case Builtin::aggRankFinalize:
            return "aggRankFinalize";
        case Builtin::aggExpMovingAvg:
            return "aggExpMovingAvg";
        case Builtin::aggExpMovingAvgFinalize:
            return "aggExpMovingAvgFinalize";
        case Builtin::aggRemovableSumAdd:
            return "aggRemovableSumAdd";
        case Builtin::aggRemovableSumRemove:
            return "aggRemovableSumRemove";
        case Builtin::aggRemovableSumFinalize:
            return "aggRemovableSumFinalize";
        case Builtin::aggIntegralInit:
            return "aggIntegralInit";
        case Builtin::aggIntegralAdd:
            return "aggIntegralAdd";
        case Builtin::aggIntegralRemove:
            return "aggIntegralRemove";
        case Builtin::aggIntegralFinalize:
            return "aggIntegralFinalize";
        case Builtin::aggDerivativeFinalize:
            return "aggDerivativeFinalize";
        case Builtin::aggCovarianceAdd:
            return "aggCovarianceAdd";
        case Builtin::aggCovarianceRemove:
            return "aggCovarianceRemove";
        case Builtin::aggCovarianceSampFinalize:
            return "aggCovarianceSampFinalize";
        case Builtin::aggCovariancePopFinalize:
            return "aggCovariancePopFinalize";
        case Builtin::aggRemovablePushAdd:
            return "aggRemovablePushAdd";
        case Builtin::aggRemovablePushRemove:
            return "aggRemovablePushRemove";
        case Builtin::aggRemovablePushFinalize:
            return "aggRemovablePushFinalize";
        case Builtin::aggRemovableStdDevAdd:
            return "aggRemovableStdDevAdd";
        case Builtin::aggRemovableStdDevRemove:
            return "aggRemovableStdDevRemove";
        case Builtin::aggRemovableStdDevSampFinalize:
            return "aggRemovableStdDevSampFinalize";
        case Builtin::aggRemovableStdDevPopFinalize:
            return "aggRemovableStdDevPopFinalize";
        case Builtin::aggRemovableFirstNInit:
            return "aggRemovableFirstNInit";
        case Builtin::aggRemovableFirstNAdd:
            return "aggRemovableFirstNAdd";
        case Builtin::aggRemovableFirstNRemove:
            return "aggRemovableFirstNRemove";
        case Builtin::aggRemovableFirstNFinalize:
            return "aggRemovableFirstNFinalize";
        case Builtin::aggRemovableLastNInit:
            return "aggRemovableLastNInit";
        case Builtin::aggRemovableLastNAdd:
            return "aggRemovableLastNAdd";
        case Builtin::aggRemovableLastNRemove:
            return "aggRemovableLastNRemove";
        case Builtin::aggRemovableLastNFinalize:
            return "aggRemovableLastNFinalize";
        case Builtin::aggLinearFillCanAdd:
            return "aggLinearFillCanAdd";
        case Builtin::aggLinearFillAdd:
            return "aggLinearFillAdd";
        case Builtin::aggLinearFillFinalize:
            return "aggLinearFillFinalize";
        case Builtin::aggRemovableAddToSetInit:
            return "aggRemovableAddToSetInit";
        case Builtin::aggRemovableAddToSetCollInit:
            return "aggRemovableAddToSetCollInit";
        case Builtin::aggRemovableAddToSetAdd:
            return "aggRemovableAddToSetAdd";
        case Builtin::aggRemovableAddToSetRemove:
            return "aggRemovableAddToSetRemove";
        case Builtin::aggRemovableAddToSetFinalize:
            return "aggRemovableAddToSetFinalize";
        case Builtin::aggRemovableMinMaxNCollInit:
            return "aggRemovableMinMaxNCollInit";
        case Builtin::aggRemovableMinMaxNInit:
            return "aggRemovableMinMaxNInit";
        case Builtin::aggRemovableMinMaxNAdd:
            return "aggRemovableMinMaxNAdd";
        case Builtin::aggRemovableMinMaxNRemove:
            return "aggRemovableMinMaxNRemove";
        case Builtin::aggRemovableMinNFinalize:
            return "aggRemovableMinNFinalize";
        case Builtin::aggRemovableMaxNFinalize:
            return "aggRemovableMaxNFinalize";
        case Builtin::aggRemovableTopNInit:
            return "aggRemovableTopNInit";
        case Builtin::aggRemovableTopNAdd:
            return "aggRemovableTopNAdd";
        case Builtin::aggRemovableTopNRemove:
            return "aggRemovableTopNRemove";
        case Builtin::aggRemovableTopNFinalize:
            return "aggRemovableTopNFinalize";
        case Builtin::aggRemovableBottomNInit:
            return "aggRemovableBottomNInit";
        case Builtin::aggRemovableBottomNAdd:
            return "aggRemovableBottomNAdd";
        case Builtin::aggRemovableBottomNRemove:
            return "aggRemovableBottomNRemove";
        case Builtin::aggRemovableBottomNFinalize:
            return "aggRemovableBottomNFinalize";
        case Builtin::valueBlockExists:
            return "valueBlockExists";
        case Builtin::valueBlockFillEmpty:
            return "valueBlockFillEmpty";
        case Builtin::valueBlockFillEmptyBlock:
            return "valueBlockFillEmptyBlock";
        case Builtin::valueBlockMin:
            return "valueBlockMin";
        case Builtin::valueBlockMax:
            return "valueBlockMax";
        case Builtin::valueBlockCount:
            return "valueBlockCount";
        case Builtin::valueBlockDateDiff:
            return "valueBlockDateDiff";
        case Builtin::valueBlockDateTrunc:
            return "valueBlockDateTrunc";
        case Builtin::valueBlockTrunc:
            return "valueBlockTrunc";
        case Builtin::valueBlockRound:
            return "valueBlockRound";
        case Builtin::valueBlockSum:
            return "valueBlockSum";
        case Builtin::valueBlockAdd:
            return "valueBlockAdd";
        case Builtin::valueBlockSub:
            return "valueBlockSub";
        case Builtin::valueBlockMult:
            return "valueBlockMult";
        case Builtin::valueBlockDiv:
            return "valueBlockDiv";
        case Builtin::valueBlockGtScalar:
            return "valueBlockGtScalar";
        case Builtin::valueBlockGteScalar:
            return "valueBlockGteScalar";
        case Builtin::valueBlockEqScalar:
            return "valueBlockEqScalar";
        case Builtin::valueBlockNeqScalar:
            return "valueBlockNeqScalar";
        case Builtin::valueBlockLtScalar:
            return "valueBlockLtScalar";
        case Builtin::valueBlockLteScalar:
            return "valueBlockLteScalar";
        case Builtin::valueBlockCmp3wScalar:
            return "valueBlockCmp3wScalar";
        case Builtin::valueBlockCombine:
            return "valueBlockCombine";
        case Builtin::valueBlockLogicalAnd:
            return "valueBlockLogicalAnd";
        case Builtin::valueBlockLogicalOr:
            return "valueBlockLogicalOr";
        case Builtin::valueBlockLogicalNot:
            return "valueBlockLogicalNot";
        case Builtin::valueBlockNewFill:
            return "valueBlockNewFill";
        case Builtin::valueBlockSize:
            return "valueBlockSize";
        case Builtin::valueBlockNone:
            return "valueBlockNone";
        case Builtin::valueBlockIsMember:
            return "valueBlockIsMember";
        case Builtin::valueBlockCoerceToBool:
            return "valueBlockCoerceToBool";
        case Builtin::cellFoldValues_F:
            return "cellFoldValues_F";
        case Builtin::cellFoldValues_P:
            return "cellFoldValues_P";
        case Builtin::cellBlockGetFlatValuesBlock:
            return "cellBlockGetFlatValuesBlock";
        default:
            MONGO_UNREACHABLE;
    }
}

MONGO_COMPILER_NORETURN void reportSwapFailure();

void ByteCode::swapStack() {
    auto [rhsOwned, rhsTag, rhsValue] = getFromStack(0);
    auto [lhsOwned, lhsTag, lhsValue] = getFromStack(1);

    // Swap values only if they are not physically same. This is necessary for the
    // "swap and pop" idiom for returning a value from the top of the stack (used
    // by ELocalBind). For example, consider the case where a series of swap, pop,
    // swap, pop... instructions are executed and the value at stack[0] and
    // stack[1] are physically identical, but stack[1] is owned and stack[0] is
    // not. After swapping them, the 'pop' instruction would free the owned one and
    // leave the unowned value dangling. The only exception to this is shallow
    // values (values which fit directly inside a 64 bit Value and don't need
    // to be freed explicitly).
    if (rhsValue == lhsValue && rhsTag == lhsTag) {
        if (rhsOwned && !isShallowType(rhsTag)) {
            reportSwapFailure();
        }
    } else {
        setStack(0, lhsOwned, lhsTag, lhsValue);
        setStack(1, rhsOwned, rhsTag, rhsValue);
    }
}

MONGO_COMPILER_NORETURN void reportSwapFailure() {
    tasserted(56123, "Attempting to swap two identical values when top of stack is owned");
}

MONGO_COMPILER_NORETURN void ByteCode::runFailInstruction() {
    auto [ownedCode, tagCode, valCode] = getFromStack(1);
    invariant(tagCode == value::TypeTags::NumberInt64);

    auto [ownedMsg, tagMsg, valMsg] = getFromStack(0);
    invariant(value::isString(tagMsg));

    ErrorCodes::Error code{static_cast<ErrorCodes::Error>(value::bitcastTo<int64_t>(valCode))};
    std::string message{value::getStringView(tagMsg, valMsg)};

    uasserted(code, message);
}

template <typename T>
void ByteCode::runTagCheck(const uint8_t*& pcPointer, T&& predicate) {
    auto [popParam, moveFromParam, offsetParam] = decodeParam(pcPointer);
    auto [owned, tag, val] = getFromStack(offsetParam, popParam);

    if (tag != value::TypeTags::Nothing) {
        pushStack(false, value::TypeTags::Boolean, value::bitcastFrom<bool>(predicate(tag)));
    } else {
        pushStack(false, value::TypeTags::Nothing, 0);
    }

    if (owned && popParam) {
        value::releaseValue(tag, val);
    }
}

void ByteCode::runTagCheck(const uint8_t*& pcPointer, value::TypeTags tagRhs) {
    runTagCheck(pcPointer, [tagRhs](value::TypeTags tagLhs) { return tagLhs == tagRhs; });
}

void ByteCode::runLambdaInternal(const CodeFragment* code, int64_t position) {
    runInternal(code, position);
    swapStack();
    popAndReleaseStack();
}

void ByteCode::runInternal(const CodeFragment* code, int64_t position) {
    auto pcPointer = code->instrs().data() + position;
    auto pcEnd = pcPointer + code->instrs().size();

    while (pcPointer != pcEnd) {
        Instruction i = readFromMemory<Instruction>(pcPointer);
        pcPointer += sizeof(i);
        switch (i.tag) {
            case Instruction::pushConstVal: {
                auto tag = readFromMemory<value::TypeTags>(pcPointer);
                pcPointer += sizeof(tag);
                auto val = readFromMemory<value::Value>(pcPointer);
                pcPointer += sizeof(val);

                pushStack(false, tag, val);

                break;
            }
            case Instruction::pushAccessVal: {
                auto accessor = readFromMemory<value::SlotAccessor*>(pcPointer);
                pcPointer += sizeof(accessor);

                auto [tag, val] = accessor->getViewOfValue();
                pushStack(false, tag, val);

                break;
            }
            case Instruction::pushOwnedAccessorVal: {
                auto accessor = readFromMemory<value::OwnedValueAccessor*>(pcPointer);
                pcPointer += sizeof(accessor);

                auto [tag, val] = accessor->getViewOfValue();
                pushStack(false, tag, val);

                break;
            }
            case Instruction::pushEnvAccessorVal: {
                auto accessor = readFromMemory<RuntimeEnvironment::Accessor*>(pcPointer);
                pcPointer += sizeof(accessor);

                auto [tag, val] = accessor->getViewOfValue();
                pushStack(false, tag, val);

                break;
            }
            case Instruction::pushMoveVal: {
                auto accessor = readFromMemory<value::SlotAccessor*>(pcPointer);
                pcPointer += sizeof(accessor);

                auto [tag, val] = accessor->copyOrMoveValue();
                pushStack(true, tag, val);

                break;
            }
            case Instruction::pushLocalVal: {
                auto stackOffset = readFromMemory<int>(pcPointer);
                pcPointer += sizeof(stackOffset);

                auto [owned, tag, val] = getFromStack(stackOffset);

                pushStack(false, tag, val);

                break;
            }
            case Instruction::pushMoveLocalVal: {
                auto stackOffset = readFromMemory<int>(pcPointer);
                pcPointer += sizeof(stackOffset);

                auto [owned, tag, val] = getFromStack(stackOffset);
                setTagToNothing(stackOffset);

                pushStack(owned, tag, val);
                break;
            }
            case Instruction::pushLocalLambda: {
                auto offset = readFromMemory<int>(pcPointer);
                pcPointer += sizeof(offset);
                auto newPosition = pcPointer - code->instrs().data() + offset;

                pushStack(
                    false, value::TypeTags::LocalLambda, value::bitcastFrom<int64_t>(newPosition));
                break;
            }
            case Instruction::pop: {
                popAndReleaseStack();
                break;
            }
            case Instruction::swap: {
                swapStack();
                break;
            }
            case Instruction::add: {
                auto [popLhs, moveFromLhs, offsetLhs] = decodeParam(pcPointer);
                auto [popRhs, moveFromRhs, offsetRhs] = decodeParam(pcPointer);

                auto [rhsOwned, rhsTag, rhsVal] = getFromStack(offsetRhs, popRhs);
                value::ValueGuard rhsGuard(rhsOwned && popRhs, rhsTag, rhsVal);
                auto [lhsOwned, lhsTag, lhsVal] = getFromStack(offsetLhs, popLhs);
                value::ValueGuard lhsGuard(lhsOwned && popLhs, lhsTag, lhsVal);

                auto [owned, tag, val] = genericAdd(lhsTag, lhsVal, rhsTag, rhsVal);

                pushStack(owned, tag, val);

                break;
            }
            case Instruction::sub: {
                auto [popLhs, moveFromLhs, offsetLhs] = decodeParam(pcPointer);
                auto [popRhs, moveFromRhs, offsetRhs] = decodeParam(pcPointer);

                auto [rhsOwned, rhsTag, rhsVal] = getFromStack(offsetRhs, popRhs);
                value::ValueGuard rhsGuard(rhsOwned && popRhs, rhsTag, rhsVal);
                auto [lhsOwned, lhsTag, lhsVal] = getFromStack(offsetLhs, popLhs);
                value::ValueGuard lhsGuard(lhsOwned && popLhs, lhsTag, lhsVal);

                auto [owned, tag, val] = genericSub(lhsTag, lhsVal, rhsTag, rhsVal);

                pushStack(owned, tag, val);
                break;
            }
            case Instruction::mul: {
                auto [popLhs, moveFromLhs, offsetLhs] = decodeParam(pcPointer);
                auto [popRhs, moveFromRhs, offsetRhs] = decodeParam(pcPointer);

                auto [rhsOwned, rhsTag, rhsVal] = getFromStack(offsetRhs, popRhs);
                value::ValueGuard rhsGuard(rhsOwned && popRhs, rhsTag, rhsVal);
                auto [lhsOwned, lhsTag, lhsVal] = getFromStack(offsetLhs, popLhs);
                value::ValueGuard lhsGuard(lhsOwned && popLhs, lhsTag, lhsVal);

                auto [owned, tag, val] = genericMul(lhsTag, lhsVal, rhsTag, rhsVal);

                pushStack(owned, tag, val);
                break;
            }
            case Instruction::div: {
                auto [popLhs, moveFromLhs, offsetLhs] = decodeParam(pcPointer);
                auto [popRhs, moveFromRhs, offsetRhs] = decodeParam(pcPointer);

                auto [rhsOwned, rhsTag, rhsVal] = getFromStack(offsetRhs, popRhs);
                value::ValueGuard rhsGuard(rhsOwned && popRhs, rhsTag, rhsVal);
                auto [lhsOwned, lhsTag, lhsVal] = getFromStack(offsetLhs, popLhs);
                value::ValueGuard lhsGuard(lhsOwned && popLhs, lhsTag, lhsVal);

                auto [owned, tag, val] = genericDiv(lhsTag, lhsVal, rhsTag, rhsVal);

                pushStack(owned, tag, val);
                break;
            }
            case Instruction::idiv: {
                auto [popLhs, moveFromLhs, offsetLhs] = decodeParam(pcPointer);
                auto [popRhs, moveFromRhs, offsetRhs] = decodeParam(pcPointer);

                auto [rhsOwned, rhsTag, rhsVal] = getFromStack(offsetRhs, popRhs);
                value::ValueGuard rhsGuard(rhsOwned && popRhs, rhsTag, rhsVal);
                auto [lhsOwned, lhsTag, lhsVal] = getFromStack(offsetLhs, popLhs);
                value::ValueGuard lhsGuard(lhsOwned && popLhs, lhsTag, lhsVal);

                auto [owned, tag, val] = genericIDiv(lhsTag, lhsVal, rhsTag, rhsVal);

                pushStack(owned, tag, val);
                break;
            }
            case Instruction::mod: {
                auto [popLhs, moveFromLhs, offsetLhs] = decodeParam(pcPointer);
                auto [popRhs, moveFromRhs, offsetRhs] = decodeParam(pcPointer);

                auto [rhsOwned, rhsTag, rhsVal] = getFromStack(offsetRhs, popRhs);
                value::ValueGuard rhsGuard(rhsOwned && popRhs, rhsTag, rhsVal);
                auto [lhsOwned, lhsTag, lhsVal] = getFromStack(offsetLhs, popLhs);
                value::ValueGuard lhsGuard(lhsOwned && popLhs, lhsTag, lhsVal);

                auto [owned, tag, val] = genericMod(lhsTag, lhsVal, rhsTag, rhsVal);

                pushStack(owned, tag, val);
                break;
            }
            case Instruction::negate: {
                auto [popParam, moveFromParam, offsetParam] = decodeParam(pcPointer);
                auto [owned, tag, val] = getFromStack(offsetParam, popParam);
                value::ValueGuard paramGuard(owned && popParam, tag, val);

                auto [resultOwned, resultTag, resultVal] = genericSub(
                    value::TypeTags::NumberInt32, value::bitcastFrom<int32_t>(0), tag, val);

                pushStack(resultOwned, resultTag, resultVal);
                break;
            }
            case Instruction::numConvert: {
                auto tag = readFromMemory<value::TypeTags>(pcPointer);
                pcPointer += sizeof(tag);

                auto [owned, lhsTag, lhsVal] = getFromStack(0);

                auto [rhsOwned, rhsTag, rhsVal] = genericNumConvert(lhsTag, lhsVal, tag);

                topStack(rhsOwned, rhsTag, rhsVal);

                if (owned) {
                    value::releaseValue(lhsTag, lhsVal);
                }

                break;
            }
            case Instruction::logicNot: {
                auto [popParam, moveFromParam, offsetParam] = decodeParam(pcPointer);
                auto [owned, tag, val] = getFromStack(offsetParam, popParam);
                value::ValueGuard paramGuard(owned && popParam, tag, val);

                auto [resultTag, resultVal] = genericNot(tag, val);

                pushStack(false, resultTag, resultVal);
                break;
            }
            case Instruction::less: {
                auto [popLhs, moveFromLhs, offsetLhs] = decodeParam(pcPointer);
                auto [popRhs, moveFromRhs, offsetRhs] = decodeParam(pcPointer);

                auto [rhsOwned, rhsTag, rhsVal] = getFromStack(offsetRhs, popRhs);
                value::ValueGuard rhsGuard(rhsOwned && popRhs, rhsTag, rhsVal);
                auto [lhsOwned, lhsTag, lhsVal] = getFromStack(offsetLhs, popLhs);
                value::ValueGuard lhsGuard(lhsOwned && popLhs, lhsTag, lhsVal);

                auto [tag, val] = value::genericLt(lhsTag, lhsVal, rhsTag, rhsVal);

                pushStack(false, tag, val);
                break;
            }
            case Instruction::collLess: {
                auto [popColl, moveFromColl, offsetColl] = decodeParam(pcPointer);
                auto [popLhs, moveFromLhs, offsetLhs] = decodeParam(pcPointer);
                auto [popRhs, moveFromRhs, offsetRhs] = decodeParam(pcPointer);

                auto [rhsOwned, rhsTag, rhsVal] = getFromStack(offsetRhs, popRhs);
                value::ValueGuard rhsGuard(rhsOwned && popRhs, rhsTag, rhsVal);
                auto [lhsOwned, lhsTag, lhsVal] = getFromStack(offsetLhs, popLhs);
                value::ValueGuard lhsGuard(lhsOwned && popLhs, lhsTag, lhsVal);
                auto [collOwned, collTag, collVal] = getFromStack(offsetColl, popColl);
                value::ValueGuard collGuard(collOwned && popColl, collTag, collVal);

                if (collTag == value::TypeTags::collator) {
                    auto comp = static_cast<StringDataComparator*>(value::getCollatorView(collVal));
                    auto [tag, val] = value::genericLt(lhsTag, lhsVal, rhsTag, rhsVal, comp);
                    pushStack(false, tag, val);
                } else {
                    pushStack(false, value::TypeTags::Nothing, 0);
                }
                break;
            }
            case Instruction::lessEq: {
                auto [popLhs, moveFromLhs, offsetLhs] = decodeParam(pcPointer);
                auto [popRhs, moveFromRhs, offsetRhs] = decodeParam(pcPointer);

                auto [rhsOwned, rhsTag, rhsVal] = getFromStack(offsetRhs, popRhs);
                value::ValueGuard rhsGuard(rhsOwned && popRhs, rhsTag, rhsVal);
                auto [lhsOwned, lhsTag, lhsVal] = getFromStack(offsetLhs, popLhs);
                value::ValueGuard lhsGuard(lhsOwned && popLhs, lhsTag, lhsVal);

                auto [tag, val] = value::genericLte(lhsTag, lhsVal, rhsTag, rhsVal);

                pushStack(false, tag, val);
                break;
            }
            case Instruction::collLessEq: {
                auto [popColl, moveFromColl, offsetColl] = decodeParam(pcPointer);
                auto [popLhs, moveFromLhs, offsetLhs] = decodeParam(pcPointer);
                auto [popRhs, moveFromRhs, offsetRhs] = decodeParam(pcPointer);

                auto [rhsOwned, rhsTag, rhsVal] = getFromStack(offsetRhs, popRhs);
                value::ValueGuard rhsGuard(rhsOwned && popRhs, rhsTag, rhsVal);
                auto [lhsOwned, lhsTag, lhsVal] = getFromStack(offsetLhs, popLhs);
                value::ValueGuard lhsGuard(lhsOwned && popLhs, lhsTag, lhsVal);
                auto [collOwned, collTag, collVal] = getFromStack(offsetColl, popColl);
                value::ValueGuard collGuard(collOwned && popColl, collTag, collVal);

                if (collTag == value::TypeTags::collator) {
                    auto comp = static_cast<StringDataComparator*>(value::getCollatorView(collVal));
                    auto [tag, val] = value::genericLte(lhsTag, lhsVal, rhsTag, rhsVal, comp);
                    pushStack(false, tag, val);
                } else {
                    pushStack(false, value::TypeTags::Nothing, 0);
                }
                break;
            }
            case Instruction::greater: {
                auto [popLhs, moveFromLhs, offsetLhs] = decodeParam(pcPointer);
                auto [popRhs, moveFromRhs, offsetRhs] = decodeParam(pcPointer);

                auto [rhsOwned, rhsTag, rhsVal] = getFromStack(offsetRhs, popRhs);
                value::ValueGuard rhsGuard(rhsOwned && popRhs, rhsTag, rhsVal);
                auto [lhsOwned, lhsTag, lhsVal] = getFromStack(offsetLhs, popLhs);
                value::ValueGuard lhsGuard(lhsOwned && popLhs, lhsTag, lhsVal);

                auto [tag, val] = value::genericGt(lhsTag, lhsVal, rhsTag, rhsVal);

                pushStack(false, tag, val);

                break;
            }
            case Instruction::collGreater: {
                auto [popColl, moveFromColl, offsetColl] = decodeParam(pcPointer);
                auto [popLhs, moveFromLhs, offsetLhs] = decodeParam(pcPointer);
                auto [popRhs, moveFromRhs, offsetRhs] = decodeParam(pcPointer);

                auto [rhsOwned, rhsTag, rhsVal] = getFromStack(offsetRhs, popRhs);
                value::ValueGuard rhsGuard(rhsOwned && popRhs, rhsTag, rhsVal);
                auto [lhsOwned, lhsTag, lhsVal] = getFromStack(offsetLhs, popLhs);
                value::ValueGuard lhsGuard(lhsOwned && popLhs, lhsTag, lhsVal);
                auto [collOwned, collTag, collVal] = getFromStack(offsetColl, popColl);
                value::ValueGuard collGuard(collOwned && popColl, collTag, collVal);

                if (collTag == value::TypeTags::collator) {
                    auto comp = static_cast<StringDataComparator*>(value::getCollatorView(collVal));
                    auto [tag, val] = value::genericGt(lhsTag, lhsVal, rhsTag, rhsVal, comp);
                    pushStack(false, tag, val);
                } else {
                    pushStack(false, value::TypeTags::Nothing, 0);
                }
                break;
            }
            case Instruction::greaterEq: {
                auto [popLhs, moveFromLhs, offsetLhs] = decodeParam(pcPointer);
                auto [popRhs, moveFromRhs, offsetRhs] = decodeParam(pcPointer);

                auto [rhsOwned, rhsTag, rhsVal] = getFromStack(offsetRhs, popRhs);
                value::ValueGuard rhsGuard(rhsOwned && popRhs, rhsTag, rhsVal);
                auto [lhsOwned, lhsTag, lhsVal] = getFromStack(offsetLhs, popLhs);
                value::ValueGuard lhsGuard(lhsOwned && popLhs, lhsTag, lhsVal);

                auto [tag, val] = value::genericGte(lhsTag, lhsVal, rhsTag, rhsVal);

                pushStack(false, tag, val);
                break;
            }
            case Instruction::collGreaterEq: {
                auto [popColl, moveFromColl, offsetColl] = decodeParam(pcPointer);
                auto [popLhs, moveFromLhs, offsetLhs] = decodeParam(pcPointer);
                auto [popRhs, moveFromRhs, offsetRhs] = decodeParam(pcPointer);

                auto [rhsOwned, rhsTag, rhsVal] = getFromStack(offsetRhs, popRhs);
                value::ValueGuard rhsGuard(rhsOwned && popRhs, rhsTag, rhsVal);
                auto [lhsOwned, lhsTag, lhsVal] = getFromStack(offsetLhs, popLhs);
                value::ValueGuard lhsGuard(lhsOwned && popLhs, lhsTag, lhsVal);
                auto [collOwned, collTag, collVal] = getFromStack(offsetColl, popColl);
                value::ValueGuard collGuard(collOwned && popColl, collTag, collVal);

                if (collTag == value::TypeTags::collator) {
                    auto comp = static_cast<StringDataComparator*>(value::getCollatorView(collVal));
                    auto [tag, val] = value::genericGte(lhsTag, lhsVal, rhsTag, rhsVal, comp);
                    pushStack(false, tag, val);
                } else {
                    pushStack(false, value::TypeTags::Nothing, 0);
                }
                break;
            }
            case Instruction::eq: {
                auto [popLhs, moveFromLhs, offsetLhs] = decodeParam(pcPointer);
                auto [popRhs, moveFromRhs, offsetRhs] = decodeParam(pcPointer);

                auto [rhsOwned, rhsTag, rhsVal] = getFromStack(offsetRhs, popRhs);
                value::ValueGuard rhsGuard(rhsOwned && popRhs, rhsTag, rhsVal);
                auto [lhsOwned, lhsTag, lhsVal] = getFromStack(offsetLhs, popLhs);
                value::ValueGuard lhsGuard(lhsOwned && popLhs, lhsTag, lhsVal);

                auto [tag, val] = value::genericEq(lhsTag, lhsVal, rhsTag, rhsVal);

                pushStack(false, tag, val);
                break;
            }
            case Instruction::collEq: {
                auto [popColl, moveFromColl, offsetColl] = decodeParam(pcPointer);
                auto [popLhs, moveFromLhs, offsetLhs] = decodeParam(pcPointer);
                auto [popRhs, moveFromRhs, offsetRhs] = decodeParam(pcPointer);

                auto [rhsOwned, rhsTag, rhsVal] = getFromStack(offsetRhs, popRhs);
                value::ValueGuard rhsGuard(rhsOwned && popRhs, rhsTag, rhsVal);
                auto [lhsOwned, lhsTag, lhsVal] = getFromStack(offsetLhs, popLhs);
                value::ValueGuard lhsGuard(lhsOwned && popLhs, lhsTag, lhsVal);
                auto [collOwned, collTag, collVal] = getFromStack(offsetColl, popColl);
                value::ValueGuard collGuard(collOwned && popColl, collTag, collVal);

                if (collTag == value::TypeTags::collator) {
                    auto comp = static_cast<StringDataComparator*>(value::getCollatorView(collVal));
                    auto [tag, val] = value::genericEq(lhsTag, lhsVal, rhsTag, rhsVal, comp);
                    pushStack(false, tag, val);
                } else {
                    pushStack(false, value::TypeTags::Nothing, 0);
                }
                break;
            }
            case Instruction::neq: {
                auto [popLhs, moveFromLhs, offsetLhs] = decodeParam(pcPointer);
                auto [popRhs, moveFromRhs, offsetRhs] = decodeParam(pcPointer);

                auto [rhsOwned, rhsTag, rhsVal] = getFromStack(offsetRhs, popRhs);
                value::ValueGuard rhsGuard(rhsOwned && popRhs, rhsTag, rhsVal);
                auto [lhsOwned, lhsTag, lhsVal] = getFromStack(offsetLhs, popLhs);
                value::ValueGuard lhsGuard(lhsOwned && popLhs, lhsTag, lhsVal);

                auto [tag, val] = value::genericNeq(lhsTag, lhsVal, rhsTag, rhsVal);

                pushStack(false, tag, val);
                break;
            }
            case Instruction::collNeq: {
                auto [popColl, moveFromColl, offsetColl] = decodeParam(pcPointer);
                auto [popLhs, moveFromLhs, offsetLhs] = decodeParam(pcPointer);
                auto [popRhs, moveFromRhs, offsetRhs] = decodeParam(pcPointer);

                auto [rhsOwned, rhsTag, rhsVal] = getFromStack(offsetRhs, popRhs);
                value::ValueGuard rhsGuard(rhsOwned && popRhs, rhsTag, rhsVal);
                auto [lhsOwned, lhsTag, lhsVal] = getFromStack(offsetLhs, popLhs);
                value::ValueGuard lhsGuard(lhsOwned && popLhs, lhsTag, lhsVal);
                auto [collOwned, collTag, collVal] = getFromStack(offsetColl, popColl);
                value::ValueGuard collGuard(collOwned && popColl, collTag, collVal);

                if (collTag == value::TypeTags::collator) {
                    auto comp = static_cast<StringDataComparator*>(value::getCollatorView(collVal));
                    auto [tag, val] = value::genericNeq(lhsTag, lhsVal, rhsTag, rhsVal, comp);
                    pushStack(false, tag, val);
                } else {
                    pushStack(false, value::TypeTags::Nothing, 0);
                }
                break;
            }
            case Instruction::cmp3w: {
                auto [popLhs, moveFromLhs, offsetLhs] = decodeParam(pcPointer);
                auto [popRhs, moveFromRhs, offsetRhs] = decodeParam(pcPointer);

                auto [rhsOwned, rhsTag, rhsVal] = getFromStack(offsetRhs, popRhs);
                value::ValueGuard rhsGuard(rhsOwned && popRhs, rhsTag, rhsVal);
                auto [lhsOwned, lhsTag, lhsVal] = getFromStack(offsetLhs, popLhs);
                value::ValueGuard lhsGuard(lhsOwned && popLhs, lhsTag, lhsVal);

                auto [tag, val] = value::compare3way(lhsTag, lhsVal, rhsTag, rhsVal);

                pushStack(false, tag, val);

                break;
            }
            case Instruction::collCmp3w: {
                auto [popColl, moveFromColl, offsetColl] = decodeParam(pcPointer);
                auto [popLhs, moveFromLhs, offsetLhs] = decodeParam(pcPointer);
                auto [popRhs, moveFromRhs, offsetRhs] = decodeParam(pcPointer);

                auto [rhsOwned, rhsTag, rhsVal] = getFromStack(offsetRhs, popRhs);
                value::ValueGuard rhsGuard(rhsOwned && popRhs, rhsTag, rhsVal);
                auto [lhsOwned, lhsTag, lhsVal] = getFromStack(offsetLhs, popLhs);
                value::ValueGuard lhsGuard(lhsOwned && popLhs, lhsTag, lhsVal);
                auto [collOwned, collTag, collVal] = getFromStack(offsetColl, popColl);
                value::ValueGuard collGuard(collOwned && popColl, collTag, collVal);

                if (collTag == value::TypeTags::collator) {
                    auto comp = static_cast<StringDataComparator*>(value::getCollatorView(collVal));
                    auto [tag, val] = value::compare3way(lhsTag, lhsVal, rhsTag, rhsVal, comp);
                    pushStack(false, tag, val);
                } else {
                    pushStack(false, value::TypeTags::Nothing, 0);
                }
                break;
            }
            case Instruction::fillEmpty: {
                auto [rhsOwned, rhsTag, rhsVal] = getFromStack(0);
                popStack();
                auto [lhsOwned, lhsTag, lhsVal] = getFromStack(0);

                if (lhsTag == value::TypeTags::Nothing) {
                    topStack(rhsOwned, rhsTag, rhsVal);

                    if (lhsOwned) {
                        value::releaseValue(lhsTag, lhsVal);
                    }
                } else {
                    if (rhsOwned) {
                        value::releaseValue(rhsTag, rhsVal);
                    }
                }
                break;
            }
            case Instruction::fillEmptyImm: {
                auto k = readFromMemory<Instruction::Constants>(pcPointer);
                pcPointer += sizeof(k);

                auto [lhsOwned, lhsTag, lhsVal] = getFromStack(0);
                if (lhsTag == value::TypeTags::Nothing) {
                    switch (k) {
                        case Instruction::Nothing:
                            break;
                        case Instruction::Null:
                            topStack(false, value::TypeTags::Null, 0);
                            break;
                        case Instruction::True:
                            topStack(
                                false, value::TypeTags::Boolean, value::bitcastFrom<bool>(true));
                            break;
                        case Instruction::False:
                            topStack(
                                false, value::TypeTags::Boolean, value::bitcastFrom<bool>(false));
                            break;
                        case Instruction::Int32One:
                            topStack(false,
                                     value::TypeTags::NumberInt32,
                                     value::bitcastFrom<int32_t>(1));
                            break;
                        default:
                            MONGO_UNREACHABLE;
                    }
                }
                break;
            }
            case Instruction::getField: {
                auto [popLhs, moveFromLhs, offsetLhs] = decodeParam(pcPointer);
                auto [popRhs, moveFromRhs, offsetRhs] = decodeParam(pcPointer);

                auto [rhsOwned, rhsTag, rhsVal] = getFromStack(offsetRhs, popRhs);
                value::ValueGuard rhsGuard(rhsOwned && popRhs, rhsTag, rhsVal);

                auto [lhsOwned, lhsTag, lhsVal] = getFromStack(offsetLhs, popLhs);
                value::ValueGuard lhsGuard(lhsOwned && popLhs, lhsTag, lhsVal);

                auto [owned, tag, val] = getField(lhsTag, lhsVal, rhsTag, rhsVal);

                // Copy value only if needed
                if (lhsOwned && !owned) {
                    owned = true;
                    std::tie(tag, val) = value::copyValue(tag, val);
                }

                pushStack(owned, tag, val);
                break;
            }
            case Instruction::getFieldImm: {
                auto [popLhs, moveFromLhs, offsetLhs] = decodeParam(pcPointer);
                auto size = readFromMemory<uint8_t>(pcPointer);
                pcPointer += sizeof(size);
                StringData fieldName(reinterpret_cast<const char*>(pcPointer), size);
                pcPointer += size;

                auto [lhsOwned, lhsTag, lhsVal] = getFromStack(offsetLhs, popLhs);
                value::ValueGuard lhsGuard(lhsOwned && popLhs, lhsTag, lhsVal);

                auto [owned, tag, val] = getField(lhsTag, lhsVal, fieldName);

                // Copy value only if needed
                if (lhsOwned && !owned) {
                    owned = true;
                    std::tie(tag, val) = value::copyValue(tag, val);
                }

                pushStack(owned, tag, val);
                break;
            }
            case Instruction::getElement: {
                auto [popLhs, moveFromLhs, offsetLhs] = decodeParam(pcPointer);
                auto [popRhs, moveFromRhs, offsetRhs] = decodeParam(pcPointer);

                auto [rhsOwned, rhsTag, rhsVal] = getFromStack(offsetRhs, popRhs);
                value::ValueGuard rhsGuard(rhsOwned && popRhs, rhsTag, rhsVal);
                auto [lhsOwned, lhsTag, lhsVal] = getFromStack(offsetLhs, popLhs);
                value::ValueGuard lhsGuard(lhsOwned && popLhs, lhsTag, lhsVal);

                auto [owned, tag, val] = getElement(lhsTag, lhsVal, rhsTag, rhsVal);

                // Copy value only if needed
                if (lhsOwned && !owned) {
                    owned = true;
                    std::tie(tag, val) = value::copyValue(tag, val);
                }

                pushStack(owned, tag, val);
                break;
            }
            case Instruction::getArraySize: {
                auto [popParam, moveFromParam, offsetParam] = decodeParam(pcPointer);
                auto [owned, tag, val] = getFromStack(offsetParam, popParam);
                value::ValueGuard paramGuard(owned && popParam, tag, val);

                auto [resultOwned, resultTag, resultVal] = getArraySize(tag, val);
                pushStack(resultOwned, resultTag, resultVal);
                break;
            }
            case Instruction::collComparisonKey: {
                auto [popLhs, moveFromLhs, offsetLhs] = decodeParam(pcPointer);
                auto [popRhs, moveFromRhs, offsetRhs] = decodeParam(pcPointer);

                auto [rhsOwned, rhsTag, rhsVal] = getFromStack(offsetRhs, popRhs);
                value::ValueGuard rhsGuard(rhsOwned && popRhs, rhsTag, rhsVal);
                auto [lhsOwned, lhsTag, lhsVal] = getFromStack(offsetLhs, popLhs);
                value::ValueGuard lhsGuard(lhsOwned && popLhs, lhsTag, lhsVal);

                if (lhsTag != value::TypeTags::Nothing && rhsTag == value::TypeTags::collator) {
                    // If lhs is a collatable type, call collComparisonKey() to obtain the
                    // comparison key. If lhs is not a collatable type, we can just leave it
                    // on the stack as-is.
                    if (value::isCollatableType(lhsTag)) {
                        auto collator = value::getCollatorView(rhsVal);
                        auto [tag, val] = collComparisonKey(lhsTag, lhsVal, collator);
                        pushStack(true, tag, val);
                    } else {
                        if (popLhs) {
                            pushStack(lhsOwned, lhsTag, lhsVal);
                            lhsGuard.reset();
                        } else if (moveFromLhs) {
                            setTagToNothing(offsetLhs);
                            pushStack(lhsOwned, lhsTag, lhsVal);
                        } else {
                            pushStack(false, lhsTag, lhsVal);
                        }
                    }
                } else {
                    // If lhs was Nothing or rhs wasn't Collator, return Nothing.
                    pushStack(false, value::TypeTags::Nothing, 0);
                }
                break;
            }
            case Instruction::getFieldOrElement: {
                auto [popLhs, moveFromLhs, offsetLhs] = decodeParam(pcPointer);
                auto [popRhs, moveFromRhs, offsetRhs] = decodeParam(pcPointer);

                auto [rhsOwned, rhsTag, rhsVal] = getFromStack(offsetRhs, popRhs);
                value::ValueGuard rhsGuard(rhsOwned && popRhs, rhsTag, rhsVal);
                auto [lhsOwned, lhsTag, lhsVal] = getFromStack(offsetLhs, popLhs);
                value::ValueGuard lhsGuard(lhsOwned && popLhs, lhsTag, lhsVal);

                auto [owned, tag, val] = getFieldOrElement(lhsTag, lhsVal, rhsTag, rhsVal);

                // Copy value only if needed
                if (lhsOwned && !owned) {
                    owned = true;
                    std::tie(tag, val) = value::copyValue(tag, val);
                }

                pushStack(owned, tag, val);
                break;
            }
            case Instruction::traverseP: {
                traverseP(code);
                break;
            }
            case Instruction::traversePImm: {
                auto k = readFromMemory<Instruction::Constants>(pcPointer);
                pcPointer += sizeof(k);

                auto offset = readFromMemory<int>(pcPointer);
                pcPointer += sizeof(offset);
                auto codePosition = pcPointer - code->instrs().data() + offset;

                traverseP(code,
                          codePosition,
                          k == Instruction::Nothing ? std::numeric_limits<int64_t>::max() : 1);

                break;
            }
            case Instruction::traverseF: {
                traverseF(code);
                break;
            }
            case Instruction::traverseFImm: {
                auto k = readFromMemory<Instruction::Constants>(pcPointer);
                pcPointer += sizeof(k);

                auto offset = readFromMemory<int>(pcPointer);
                pcPointer += sizeof(offset);
                auto codePosition = pcPointer - code->instrs().data() + offset;

                traverseF(code, codePosition, k == Instruction::True ? true : false);

                break;
            }
            case Instruction::magicTraverseF: {
                magicTraverseF(code);
                break;
            }
            case Instruction::traverseCsiCellValues: {
                auto offset = readFromMemory<int>(pcPointer);
                pcPointer += sizeof(offset);
                auto codePosition = pcPointer - code->instrs().data() + offset;

                traverseCsiCellValues(code, codePosition);
                break;
            }
            case Instruction::traverseCsiCellTypes: {
                auto offset = readFromMemory<int>(pcPointer);
                pcPointer += sizeof(offset);
                auto codePosition = pcPointer - code->instrs().data() + offset;

                traverseCsiCellTypes(code, codePosition);
                break;
            }
            case Instruction::setField: {
                auto [owned, tag, val] = setField();
                popAndReleaseStack();
                popAndReleaseStack();
                popAndReleaseStack();

                pushStack(owned, tag, val);
                break;
            }
            case Instruction::aggSum: {
                auto [fieldOwned, fieldTag, fieldVal] = getFromStack(0);
                value::ValueGuard fieldGuard(fieldOwned, fieldTag, fieldVal);
                popStack();

                auto [accOwned, accTag, accVal] = getFromStack(0);

                auto [owned, tag, val] = aggSum(accTag, accVal, fieldTag, fieldVal);

                topStack(owned, tag, val);
                if (accOwned) {
                    value::releaseValue(accTag, accVal);
                }
                break;
            }
            case Instruction::aggMin: {
                auto [fieldOwned, fieldTag, fieldVal] = getFromStack(0);
                value::ValueGuard fieldGuard(fieldOwned, fieldTag, fieldVal);
                popStack();

                auto [accOwned, accTag, accVal] = getFromStack(0);

                auto [owned, tag, val] = aggMin(accTag, accVal, fieldTag, fieldVal);

                topStack(owned, tag, val);
                if (accOwned) {
                    value::releaseValue(accTag, accVal);
                }
                break;
            }
            case Instruction::aggCollMin: {
                auto [fieldOwned, fieldTag, fieldVal] = getFromStack(0);
                value::ValueGuard fieldGuard(fieldOwned, fieldTag, fieldVal);
                popStack();

                auto [collOwned, collTag, collVal] = getFromStack(0);
                value::ValueGuard collGuard(collOwned, collTag, collVal);
                popStack();

                auto [accOwned, accTag, accVal] = getFromStack(0);

                // Skip aggregation step if the collation is Nothing or an unexpected type.
                if (collTag != value::TypeTags::collator) {
                    auto [tag, val] = value::copyValue(accTag, accVal);
                    topStack(true, tag, val);
                    break;
                }
                auto collator = value::getCollatorView(collVal);

                auto [owned, tag, val] = aggMin(accTag, accVal, fieldTag, fieldVal, collator);

                topStack(owned, tag, val);
                if (accOwned) {
                    value::releaseValue(accTag, accVal);
                }
                break;
            }
            case Instruction::aggMax: {
                auto [fieldOwned, fieldTag, fieldVal] = getFromStack(0);
                value::ValueGuard fieldGuard(fieldOwned, fieldTag, fieldVal);
                popStack();

                auto [accOwned, accTag, accVal] = getFromStack(0);

                auto [owned, tag, val] = aggMax(accTag, accVal, fieldTag, fieldVal);

                topStack(owned, tag, val);
                if (accOwned) {
                    value::releaseValue(accTag, accVal);
                }
                break;
            }
            case Instruction::aggCollMax: {
                auto [fieldOwned, fieldTag, fieldVal] = getFromStack(0);
                value::ValueGuard fieldGuard(fieldOwned, fieldTag, fieldVal);
                popStack();

                auto [collOwned, collTag, collVal] = getFromStack(0);
                value::ValueGuard collGuard(collOwned, collTag, collVal);
                popStack();

                auto [accOwned, accTag, accVal] = getFromStack(0);

                // Skip aggregation step if the collation is Nothing or an unexpected type.
                if (collTag != value::TypeTags::collator) {
                    auto [tag, val] = value::copyValue(accTag, accVal);
                    topStack(true, tag, val);
                    break;
                }
                auto collator = value::getCollatorView(collVal);

                auto [owned, tag, val] = aggMax(accTag, accVal, fieldTag, fieldVal, collator);

                topStack(owned, tag, val);
                if (accOwned) {
                    value::releaseValue(accTag, accVal);
                }
                break;
            }
            case Instruction::aggFirst: {
                auto [fieldOwned, fieldTag, fieldVal] = getFromStack(0);
                value::ValueGuard fieldGuard(fieldOwned, fieldTag, fieldVal);
                popStack();

                auto [accOwned, accTag, accVal] = getFromStack(0);

                auto [owned, tag, val] = aggFirst(accTag, accVal, fieldTag, fieldVal);

                topStack(owned, tag, val);
                if (accOwned) {
                    value::releaseValue(accTag, accVal);
                }
                break;
            }
            case Instruction::aggLast: {
                auto [fieldOwned, fieldTag, fieldVal] = getFromStack(0);
                value::ValueGuard fieldGuard(fieldOwned, fieldTag, fieldVal);
                popStack();

                auto [accOwned, accTag, accVal] = getFromStack(0);

                auto [owned, tag, val] = aggLast(accTag, accVal, fieldTag, fieldVal);

                topStack(owned, tag, val);
                if (accOwned) {
                    value::releaseValue(accTag, accVal);
                }
                break;
            }
            case Instruction::exists: {
                auto [popParam, moveFromParam, offsetParam] = decodeParam(pcPointer);
                auto [owned, tag, val] = getFromStack(offsetParam, popParam);

                pushStack(false,
                          value::TypeTags::Boolean,
                          value::bitcastFrom<bool>(tag != value::TypeTags::Nothing));

                if (owned && popParam) {
                    value::releaseValue(tag, val);
                }
                break;
            }
            case Instruction::isNull: {
                runTagCheck(pcPointer, value::TypeTags::Null);
                break;
            }
            case Instruction::isObject: {
                runTagCheck(pcPointer, value::isObject);
                break;
            }
            case Instruction::isArray: {
                runTagCheck(pcPointer, value::isArray);
                break;
            }
            case Instruction::isInListData: {
                runTagCheck(pcPointer, value::isInListData);
                break;
            }
            case Instruction::isString: {
                runTagCheck(pcPointer, value::isString);
                break;
            }
            case Instruction::isNumber: {
                runTagCheck(pcPointer, value::isNumber);
                break;
            }
            case Instruction::isBinData: {
                runTagCheck(pcPointer, value::isBinData);
                break;
            }
            case Instruction::isDate: {
                runTagCheck(pcPointer, value::TypeTags::Date);
                break;
            }
            case Instruction::isNaN: {
                auto [popParam, moveFromParam, offsetParam] = decodeParam(pcPointer);
                auto [owned, tag, val] = getFromStack(offsetParam, popParam);

                if (tag != value::TypeTags::Nothing) {
                    pushStack(false,
                              value::TypeTags::Boolean,
                              value::bitcastFrom<bool>(value::isNaN(tag, val)));
                } else {
                    pushStack(false, value::TypeTags::Nothing, 0);
                }

                if (owned && popParam) {
                    value::releaseValue(tag, val);
                }
                break;
            }
            case Instruction::isInfinity: {
                auto [popParam, moveFromParam, offsetParam] = decodeParam(pcPointer);
                auto [owned, tag, val] = getFromStack(offsetParam, popParam);

                if (tag != value::TypeTags::Nothing) {
                    pushStack(false,
                              value::TypeTags::Boolean,
                              value::bitcastFrom<bool>(value::isInfinity(tag, val)));
                } else {
                    pushStack(false, value::TypeTags::Nothing, 0);
                }
                if (owned && popParam) {
                    value::releaseValue(tag, val);
                }
                break;
            }
            case Instruction::isRecordId: {
                runTagCheck(pcPointer, value::isRecordId);
                break;
            }
            case Instruction::isMinKey: {
                runTagCheck(pcPointer, value::TypeTags::MinKey);
                break;
            }
            case Instruction::isMaxKey: {
                runTagCheck(pcPointer, value::TypeTags::MaxKey);
                break;
            }
            case Instruction::isTimestamp: {
                runTagCheck(pcPointer, value::TypeTags::Timestamp);
                break;
            }
            case Instruction::typeMatchImm: {
                auto [popParam, moveFromParam, offsetParam] = decodeParam(pcPointer);
                auto mask = readFromMemory<uint32_t>(pcPointer);
                pcPointer += sizeof(mask);

                auto [owned, tag, val] = getFromStack(offsetParam, popParam);

                if (tag != value::TypeTags::Nothing) {
                    pushStack(false,
                              value::TypeTags::Boolean,
                              value::bitcastFrom<bool>(getBSONTypeMask(tag) & mask));
                } else {
                    pushStack(false, value::TypeTags::Nothing, 0);
                }
                if (owned && popParam) {
                    value::releaseValue(tag, val);
                }
                break;
            }
            case Instruction::functionSmall: {
                auto f = readFromMemory<SmallBuiltinType>(pcPointer);
                pcPointer += sizeof(f);
                SmallArityType arity{0};
                arity = readFromMemory<SmallArityType>(pcPointer);
                pcPointer += sizeof(SmallArityType);

                auto [owned, tag, val] = dispatchBuiltin(static_cast<Builtin>(f), arity, code);

                for (ArityType cnt = 0; cnt < arity; ++cnt) {
                    popAndReleaseStack();
                }

                pushStack(owned, tag, val);

                break;
            }
            case Instruction::function: {
                auto f = readFromMemory<Builtin>(pcPointer);
                pcPointer += sizeof(f);
                ArityType arity{0};
                arity = readFromMemory<ArityType>(pcPointer);
                pcPointer += sizeof(ArityType);

                auto [owned, tag, val] = dispatchBuiltin(f, arity, code);

                for (ArityType cnt = 0; cnt < arity; ++cnt) {
                    popAndReleaseStack();
                }

                pushStack(owned, tag, val);

                break;
            }
            case Instruction::jmp: {
                auto jumpOffset = readFromMemory<int>(pcPointer);
                pcPointer += sizeof(jumpOffset);

                pcPointer += jumpOffset;
                break;
            }
            case Instruction::jmpTrue: {
                auto jumpOffset = readFromMemory<int>(pcPointer);
                pcPointer += sizeof(jumpOffset);

                auto [owned, tag, val] = getFromStack(0);
                popStack();

                if (tag == value::TypeTags::Boolean && value::bitcastTo<bool>(val)) {
                    pcPointer += jumpOffset;
                }

                if (owned) {
                    value::releaseValue(tag, val);
                }
                break;
            }
            case Instruction::jmpFalse: {
                auto jumpOffset = readFromMemory<int>(pcPointer);
                pcPointer += sizeof(jumpOffset);

                auto [owned, tag, val] = getFromStack(0);
                popStack();

                if (tag == value::TypeTags::Boolean && !value::bitcastTo<bool>(val)) {
                    pcPointer += jumpOffset;
                }

                if (owned) {
                    value::releaseValue(tag, val);
                }
                break;
            }
            case Instruction::jmpNothing: {
                auto jumpOffset = readFromMemory<int>(pcPointer);
                pcPointer += sizeof(jumpOffset);

                auto [owned, tag, val] = getFromStack(0);
                if (tag == value::TypeTags::Nothing) {
                    pcPointer += jumpOffset;
                }
                break;
            }
            case Instruction::jmpNotNothing: {
                auto jumpOffset = readFromMemory<int>(pcPointer);
                pcPointer += sizeof(jumpOffset);

                auto [owned, tag, val] = getFromStack(0);
                if (tag != value::TypeTags::Nothing) {
                    pcPointer += jumpOffset;
                }
                break;
            }
            case Instruction::ret: {
                pcPointer = pcEnd;
                break;
            }
            case Instruction::allocStack: {
                auto size = readFromMemory<uint32_t>(pcPointer);
                pcPointer += sizeof(size);

                allocStack(size);
                break;
            }
            case Instruction::fail: {
                runFailInstruction();
                break;
            }
            case Instruction::dateTruncImm: {
                auto unit = readFromMemory<TimeUnit>(pcPointer);
                pcPointer += sizeof(unit);
                auto binSize = readFromMemory<int64_t>(pcPointer);
                pcPointer += sizeof(binSize);
                auto timezone = readFromMemory<TimeZone>(pcPointer);
                pcPointer += sizeof(timezone);
                auto startOfWeek = readFromMemory<DayOfWeek>(pcPointer);
                pcPointer += sizeof(startOfWeek);

                auto [dateOwned, dateTag, dateVal] = getFromStack(0);

                auto [owned, tag, val] =
                    dateTrunc(dateTag, dateVal, unit, binSize, timezone, startOfWeek);

                topStack(owned, tag, val);

                if (dateOwned) {
                    value::releaseValue(dateTag, dateVal);
                }
                break;
            }
            case Instruction::valueBlockApplyLambda: {
                valueBlockApplyLambda(code);
                break;
            }

            default:
                MONGO_UNREACHABLE;
        }
    }
}

FastTuple<bool, value::TypeTags, value::Value> ByteCode::run(const CodeFragment* code) {
    try {
        uassert(6040900,
                "The evaluation stack must be empty",
                _argStackTop + sizeOfElement == _argStack);

        allocStack(code->maxStackSize());
        runInternal(code, 0);

        uassert(4822801,
                "The evaluation stack must hold only a single value",
                _argStackTop == _argStack);

        // Transfer ownership of tag/val to the caller
        stackReset();

        return readTuple(_argStack);
    } catch (...) {
        auto sentinel = _argStack - sizeOfElement;
        while (_argStackTop != sentinel) {
            popAndReleaseStack();
        }
        throw;
    }
}

bool ByteCode::runPredicate(const CodeFragment* code) {
    auto [owned, tag, val] = run(code);

    bool pass = (tag == value::TypeTags::Boolean) && value::bitcastTo<bool>(val);

    if (owned) {
        value::releaseValue(tag, val);
    }

    return pass;
}

}  // namespace vm
}  // namespace sbe
}  // namespace mongo
