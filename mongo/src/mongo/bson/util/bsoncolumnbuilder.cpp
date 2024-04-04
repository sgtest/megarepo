/**
 *    Copyright (C) 2021-present MongoDB, Inc.
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

#include "mongo/bson/util/bsoncolumnbuilder.h"

#include <absl/numeric/int128.h>
#include <algorithm>
#include <array>
#include <boost/cstdint.hpp>
#include <boost/none.hpp>
#include <cstdint>
#include <cstring>
#include <iterator>
#include <memory>
#include <queue>
#include <tuple>

#include <boost/move/utility_core.hpp>
#include <boost/optional/optional.hpp>

#include "mongo/base/error_codes.h"
#include "mongo/base/string_data.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/bson/oid.h"
#include "mongo/bson/util/bsoncolumn.h"
#include "mongo/bson/util/bsoncolumn_util.h"
#include "mongo/bson/util/simple8b.h"
#include "mongo/bson/util/simple8b_type_util.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/time_support.h"

namespace mongo {
using namespace bsoncolumn;

namespace {
static constexpr uint8_t kMaxCount = 16;
static constexpr uint8_t kCountMask = 0x0F;
static constexpr uint8_t kControlMask = 0xF0;
static constexpr std::ptrdiff_t kNoSimple8bControl = -1;
static constexpr int kFinalizedOffset = -1;
static constexpr size_t kDefaultBufferSize = 32;

static constexpr std::array<uint8_t, Simple8bTypeUtil::kMemoryAsInteger + 1>
    kControlByteForScaleIndex = {0x90, 0xA0, 0xB0, 0xC0, 0xD0, 0x80};

template <class BufBuilderType, class F>
ptrdiff_t incrementSimple8bCount(BufBuilderType& buffer,
                                 ptrdiff_t& controlByteOffset,
                                 uint8_t scaleIndex,
                                 F controlBlockWriter) {
    char* byte;
    uint8_t count;
    uint8_t control = kControlByteForScaleIndex[scaleIndex];

    if (controlByteOffset == kNoSimple8bControl) {
        // Allocate new control byte if we don't already have one. Record its offset so we can find
        // it even if the underlying buffer reallocates.
        byte = buffer.skip(1);
        controlByteOffset = std::distance(buffer.buf(), byte);
        count = 0;
    } else {
        // Read current count from previous control byte
        byte = buffer.buf() + controlByteOffset;

        // If previous byte was written with a different control byte then we can't re-use and need
        // to start a new one
        if ((*byte & kControlMask) != control) {
            controlBlockWriter(controlByteOffset, buffer.len() - controlByteOffset);

            controlByteOffset = kNoSimple8bControl;
            incrementSimple8bCount(buffer, controlByteOffset, scaleIndex, controlBlockWriter);
            return kNoSimple8bControl;
        }
        count = (*byte & kCountMask) + 1;
    }

    // Write back new count and clear offset if we have reached max count
    *byte = control | (count & kCountMask);
    if (count + 1 == kMaxCount) {
        auto prevControlByteOffset = controlByteOffset;
        controlByteOffset = kNoSimple8bControl;
        return prevControlByteOffset;
    }

    return kNoSimple8bControl;
}

// Encodes the double with the lowest possible scale index. In worst case we will interpret the
// memory as integer which is guaranteed to succeed.
std::pair<int64_t, uint8_t> scaleAndEncodeDouble(double value, uint8_t minScaleIndex) {
    boost::optional<int64_t> encoded;
    for (; !encoded; ++minScaleIndex) {
        encoded = Simple8bTypeUtil::encodeDouble(value, minScaleIndex);
    }

    // Subtract the last scale that was added in the loop before returning
    return {*encoded, minScaleIndex - 1};
}

// Checks if it is possible to do delta of ObjectIds
bool objectIdDeltaPossible(const OID& elem, const OID& prev) {
    return !memcmp(
        prev.getInstanceUnique().bytes, elem.getInstanceUnique().bytes, OID::kInstanceUniqueSize);
}

// Internal recursion function for traverseLockStep() when we need to traverse the reference
// object. Exits and returns 'true' when an empty sub object is encountered. Returns 'false'
// otherwise.
template <typename ElementFunc>
bool _traverseUntilEmptyObj(const BSONObj& obj, const ElementFunc& elemFunc) {
    for (const auto& elem : obj) {
        if (elem.type() == Object || elem.type() == Array) {
            if (_traverseUntilEmptyObj(elem.Obj(), elemFunc)) {
                return true;
            }
        } else {
            elemFunc(elem, BSONElement());
        }
    }

    return obj.isEmpty();
}

// Helper function for mergeObj() to detect if Object contain subfields of empty Objects
bool _hasEmptyObj(const BSONObj& obj) {
    return _traverseUntilEmptyObj(obj, [](const BSONElement&, const BSONElement&) {});
}

// Helper function to determine if provided Object contains any scalar subfields
bool _containsScalars(const BSONObj& reference) {
    for (const auto& elem : reference) {
        if (elem.type() == Object || elem.type() == Array) {
            if (_containsScalars(elem.Obj())) {
                return true;
            }
        } else {
            return true;
        }
    }
    return false;
}

// Internal recursion function for traverseLockStep(). See documentation for traverseLockStep.
template <typename ElementFunc>
std::pair<BSONObj::iterator, bool> _traverseLockStep(const BSONObj& reference,
                                                     const BSONObj& obj,
                                                     const ElementFunc& elemFunc) {
    auto it = obj.begin();
    auto end = obj.end();
    for (const auto& elem : reference) {
        if (elem.type() == Object || elem.type() == Array) {
            BSONObj refObj = elem.Obj();
            bool elemMatch = it != end && elem.fieldNameStringData() == it->fieldNameStringData();
            if (elemMatch) {
                // If 'reference' element is Object then 'obj' must also be Object.
                if (it->type() != elem.type()) {
                    return {it, false};
                }

                // Differences in empty objects are not allowed.
                if (refObj.isEmpty() != it->Obj().isEmpty()) {
                    return {it, false};
                }

                // Everything match, recurse deeper.
                auto [_, compatible] = _traverseLockStep(refObj, (it++)->Obj(), elemFunc);
                if (!compatible) {
                    return {it, false};
                }
            } else {
                // Assume field name at 'it' is coming later in 'reference'. Traverse as if it is
                // missing from 'obj'. We don't increment the iterator in this case. If it is a
                // mismatch we will detect that at end when 'it' is not at 'end'. Nothing can fail
                // below this so traverse without all the checks. Any empty object detected is an
                // error.
                if (_traverseUntilEmptyObj(refObj, elemFunc)) {
                    return {it, false};
                }
            }
        } else {
            bool sameField = it != end && elem.fieldNameStringData() == it->fieldNameStringData();

            // Going from scalar to object is not allowed, this would compress inefficiently
            if (sameField && (it->type() == Object || it->type() == Array)) {
                return {it, false};
            }

            // Non-object, call provided function with the two elements
            elemFunc(elem, sameField ? *(it++) : BSONElement());
        }
    }
    // Extra elements in 'obj' are not allowed. These needs to be merged in to 'reference' to be
    // able to compress.
    return {it, it == end};
}

// Traverses and validates BSONObj's in reference and obj in lock-step. Returns true if the object
// hierarchies are compatible for sub-object compression. To be compatible fields in 'obj' must be
// in the same order as in 'reference' and sub-objects in 'reference' must be sub-objects in 'obj'.
// The only difference between the two objects that is allowed is missing fields in 'obj' compared
// to 'reference'. 'ElementFunc' is called for every matching pair of BSONElement. Function
// signature should be void(const BSONElement&, const BSONElement&).
template <typename ElementFunc>
bool traverseLockStep(const BSONObj& reference, const BSONObj& obj, ElementFunc elemFunc) {
    auto [it, hierachyMatch] = _traverseLockStep(reference, obj, elemFunc);
    // Extra elements in 'obj' are not allowed. These needs to be merged in to 'reference' to be
    // able to compress.
    return hierachyMatch && it == obj.end();
}

// Internal recursion function for mergeObj(). See documentation for mergeObj. Returns true if merge
// was successful.
bool _mergeObj(BSONObjBuilder* builder, const BSONObj& reference, const BSONObj& obj) {
    auto refIt = reference.begin();
    auto refEnd = reference.end();
    auto it = obj.begin();
    auto end = obj.end();

    // Iterate until we reach end of any of the two objects.
    while (refIt != refEnd && it != end) {
        StringData name = refIt->fieldNameStringData();
        if (name == it->fieldNameStringData()) {
            bool refIsObjOrArray = refIt->type() == Object || refIt->type() == Array;
            bool itIsObjOrArray = it->type() == Object || it->type() == Array;

            // We can merge this sub-obj/array if both sides are Object or both are Array
            if (refIsObjOrArray && itIsObjOrArray && refIt->type() == it->type()) {
                BSONObj refObj = refIt->Obj();
                BSONObj itObj = it->Obj();
                // There may not be a mismatch in empty objects
                if (refObj.isEmpty() != itObj.isEmpty())
                    return false;

                // Recurse deeper
                BSONObjBuilder subBuilder = refIt->type() == Object ? builder->subobjStart(name)
                                                                    : builder->subarrayStart(name);
                bool res = _mergeObj(&subBuilder, refObj, itObj);
                if (!res) {
                    return false;
                }
            } else if (refIsObjOrArray || itIsObjOrArray) {
                // Both or neither elements must be Object to be mergable
                return false;
            } else {
                // If name match and neither is Object we can append from reference and increment
                // both objects.
                builder->append(*refIt);
            }

            ++refIt;
            ++it;
            continue;
        }

        // Name mismatch, first search in 'obj' if reference element exists later.
        auto n = std::next(it);
        auto namePos = std::find_if(
            n, end, [&name](const auto& elem) { return elem.fieldNameStringData() == name; });
        if (namePos == end) {
            // Reference element does not exist in 'obj' so add it and continue merging with just
            // this iterator incremented. Unless it is an empty object or contains an empty object
            // which is incompatible.
            if ((refIt->type() == Object || refIt->type() == Array) && _hasEmptyObj(refIt->Obj())) {
                return false;
            }

            if (builder->hasField(refIt->fieldNameStringData())) {
                return false;
            }

            builder->append(*(refIt++));
        } else {
            // Reference element does exist later in 'obj'. Add element in 'it' if it is the first
            // time we see it, fail otherwise (incompatible ordering). Unless 'it' is or contains an
            // empty object which is incompatible.
            if ((it->type() == Object || it->type() == Array) && _hasEmptyObj(it->Obj())) {
                return false;
            }
            if (builder->hasField(it->fieldNameStringData())) {
                return false;
            }
            builder->append(*(it++));
        }
    }

    // Add remaining reference elements when we reached end in 'obj'.
    for (; refIt != refEnd; ++refIt) {
        // We cannot allow empty object/array mismatch
        if ((refIt->type() == Object || refIt->type() == Array) && _hasEmptyObj(refIt->Obj())) {
            return false;
        }
        if (builder->hasField(refIt->fieldNameStringData())) {
            return false;
        }
        builder->append(*refIt);
    }

    // Add remaining 'obj' elements when we reached end in 'reference'.
    for (; it != end; ++it) {
        // We cannot allow empty object/array mismatch
        if ((it->type() == Object || it->type() == Array) && _hasEmptyObj(it->Obj())) {
            return false;
        }

        if (builder->hasField(it->fieldNameStringData())) {
            return false;
        }
        builder->append(*it);
    }

    return true;
}

// Tries to merge in elements from 'obj' into 'reference'. For successful merge the elements that
// already exist in 'reference' must be in 'obj' in the same order. The merged object is returned in
// case of a successful merge, empty BSONObj is returned for failure. This is quite an expensive
// operation as we are merging unsorted objects. Time complexity is O(N^2).
BSONObj mergeObj(const BSONObj& reference, const BSONObj& obj) {
    BSONObjBuilder builder;
    if (!_mergeObj(&builder, reference, obj)) {
        builder.abandon();
        return BSONObj();
    }

    return builder.obj();
}

// TODO (SERVER-87887): Remove this function.
template <class BSONObjType, class Allocator>
auto copyBufferedObjElements(
    const std::vector<BSONObjType,
                      typename std::allocator_traits<Allocator>::template rebind_alloc<
                          BSONObjType>>& bufferedObjElements,
    Allocator allocator) {
    std::vector<BSONObjType,
                typename std::allocator_traits<Allocator>::template rebind_alloc<BSONObjType>>
        copy(allocator);
    copy.reserve(bufferedObjElements.size());
    std::transform(bufferedObjElements.begin(),
                   bufferedObjElements.end(),
                   std::back_inserter(copy),
                   [allocator](const BSONObjType& obj) {
                       return BSONObjType{TrackableBSONObj{obj.get().get()}, allocator};
                   });
    return copy;
}

}  // namespace

template <class BufBuilderType, class BSONObjType, class Allocator>
class BSONColumnBuilder<BufBuilderType, BSONObjType, Allocator>::BinaryReopen {
public:
    /*
     * Traverse compressed binary and perform the following two:
     * 1. Calculate state to be able to materialize the last value. This is equivalent to
     * BSONColumn::last(). We need this to leave 'previous' in the compressor correct to be able
     * to calculate deltas for future values.
     *
     * 2. Remember the last two simple8b control blocks with their additional state from the
     * decompressor. This is as far as we need to go back to be able to undo a previous
     * 'BSONColumnBuilder::finalize()' call. The goal of this constructor is to leave this
     * BSONColumnBuilder in an identical state as-if finalize() had never been called.
     *
     * Returns 'false' if interleaved mode is encountered which is not supported in this
     * implementation. Full decompression+recompression must be done in this case.
     */
    bool scan(const char* binary, int size);

    /*
     * Initializes the provided BSONColumnBuilder from the state obtained from a previous scan.
     * Effectively undos the 'finalize()' call from the BSONColumnBuilder used to produce this
     * binary.
     */
    void reopen(BSONColumnBuilder& builder, Allocator) const;

private:
    /*
     * Performs the reopen for 64 and 128 bit types respectively.
     */
    void _reopen64BitTypes(EncodingState<BufBuilderType, Allocator>& regular,
                           Encoder64& encoder,
                           BufBuilderType& buffer,
                           int& offset,
                           uint8_t& lastControl) const;
    void _reopen128BitTypes(EncodingState<BufBuilderType, Allocator>& regular,
                            Encoder128& encoder,
                            BufBuilderType& buffer,
                            int& offset,
                            uint8_t& lastControl) const;

    /*
     * Setup RLE state for Simple8bBuilder used to detect overflow. Returns the value needed to use
     * as last for any Simple8b decoding while reopening.
     */
    template <typename T>
    static boost::optional<T> _setupRLEForOverflowDetector(Simple8bBuilder<T>& overflowDetector,
                                                           const char* s8bBlock,
                                                           int index);
    /*
     * Appends data into a Simple8bBuilder used to detect overflow. Returns the index of the
     * simple8b block that caused the overflow and sets the proper RLE state in the provided main
     * Simple8bBuilder to be the last value in the block that caused the overflow. This function
     * expects 'overflow' to be set to true when an overflow has occured.
     * The second return value is an index to an RLE block if we have not overflowed yet.
     */
    template <typename T>
    static std::pair<int, int> _appendUntilOverflow(Simple8bBuilder<T>& overflowDetector,
                                                    Simple8bBuilder<T, Allocator>& mainBuilder,
                                                    bool& overflow,
                                                    const boost::optional<T>& lastValForRLE,
                                                    const char* s8bBlock,
                                                    int index);
    /*
     * Special case of _appendUntilOverflow when we know that the last simple8b block is RLE. It is
     * trivial to calculate the overflow point as it will be inside the first discovered non-RLE
     * block and the last value for RLE will be the actual value used for RLE.
     */
    template <typename T>
    static std::pair<boost::optional<T>, int> _appendUntilOverflowForRLE(
        Simple8bBuilder<T, Allocator>& mainBuilder,
        bool& overflow,
        const char* s8bBlock,
        int index);

    struct ControlBlock {
        const char* control = nullptr;
        double lastAtEndOfBlock = 0.0;
        uint8_t scaleIndex = 5;  // reinterpret memory as integer
    };

    const char* scannedBinary;
    BSONColumn::Iterator::DecodingState state;
    BSONElement lastUncompressed;
    int64_t lastUncompressedEncoded64;
    int128_t lastUncompressedEncoded128;
    bool lastLiteralUnencodable = false;
    ControlBlock current;
    ControlBlock last;
};

template <class BufBuilderType, class BSONObjType, class Allocator>
bool BSONColumnBuilder<BufBuilderType, BSONObjType, Allocator>::BinaryReopen::scan(
    const char* binary, int size) {
    // Attempt to initialize the compressor from the provided binary, we have a fallback of full
    // decompress+recompress if anything unsupported is detected. This allows us to "support" the
    // full BSONColumn spec.
    scannedBinary = binary;
    const char* pos = binary;
    const char* end = binary + size;

    // Last encountered non-RLE block during binary scan
    uint64_t lastNonRLE = simple8b::kSingleZero;
    int128_t lastNonZeroDeltaForUnencodable{0};

    while (pos != end) {
        uint8_t control = *pos;

        // Stop at end terminal
        if (control == 0) {
            ++pos;

            // If the last literal was unencodable we need to adjust its last encoding. Unencodable
            // string literals allow non-zero deltas to follow.
            if (lastLiteralUnencodable && lastNonZeroDeltaForUnencodable != 0) {
                lastUncompressedEncoded128 = lastNonZeroDeltaForUnencodable;
            }

            return true;
        }

        // Interleaved mode is not supported, this would be super complicated to implement
        // and is honestly not worth it as the anchor point is likely to be far back in the
        // binary anyway.
        if (isInterleavedStartControlByte(control)) {
            return false;
        }

        // Remember last control byte
        last = current;

        if (isUncompressedLiteralControlByte(control)) {
            BSONElement element(pos, 1, -1);
            state.loadUncompressed(element);

            // Uncompressed literal case
            lastUncompressed = element;
            lastNonRLE = simple8b::kSingleZero;
            current.control = nullptr;
            last.control = nullptr;
            lastLiteralUnencodable = false;

            if (!uses128bit(lastUncompressed.type())) {
                auto& d64 = std::get<BSONColumn::Iterator::DecodingState::Decoder64>(state.decoder);
                lastUncompressedEncoded64 = d64.lastEncodedValue;
                if (element.type() == NumberDouble) {
                    current.lastAtEndOfBlock = lastUncompressed._numberDouble();
                }
            } else {
                auto& d128 =
                    std::get<BSONColumn::Iterator::DecodingState::Decoder128>(state.decoder);
                lastUncompressedEncoded128 = d128.lastEncodedValue;

                // Check if the string literal is encodable or not.
                if (lastUncompressed.type() == String || lastUncompressed.type() == Code) {
                    lastLiteralUnencodable =
                        !Simple8bTypeUtil::encodeString(lastUncompressed.valueStringData())
                             .has_value();
                    lastNonZeroDeltaForUnencodable = 0;
                }
            }

            pos += element.size();
            continue;
        }

        // Process this control block containing simple8b blocks. We need to calculate delta
        // to the last element.
        uint8_t blocks = numSimple8bBlocksForControlByte(control);
        int blocksSize = sizeof(uint64_t) * blocks;

        if (!uses128bit(lastUncompressed.type())) {
            auto& d64 = std::get<BSONColumn::Iterator::DecodingState::Decoder64>(state.decoder);
            d64.scaleIndex = scaleIndexForControlByte(control);
            uassert(8288100,
                    "Invalid control byte in BSON Column",
                    d64.scaleIndex == Simple8bTypeUtil::kMemoryAsInteger ||
                        (lastUncompressed.type() == NumberDouble &&
                         d64.scaleIndex != kInvalidScaleIndex));

            // For doubles we need to remember the last value from the previous block (as
            // the scaling can change between blocks).
            if (lastUncompressed.type() == NumberDouble) {
                auto encoded =
                    Simple8bTypeUtil::encodeDouble(current.lastAtEndOfBlock, d64.scaleIndex);
                uassert(8288101, "Invalid double encoding in BSON Column", encoded);
                d64.lastEncodedValue = *encoded;
            }
            if (usesDeltaOfDelta(lastUncompressed.type())) {
                d64.lastEncodedValueForDeltaOfDelta =
                    expandDelta(d64.lastEncodedValueForDeltaOfDelta,
                                simple8b::prefixSum<int64_t>(
                                    pos + 1, blocksSize, d64.lastEncodedValue, lastNonRLE));
            } else if (onlyZeroDelta(lastUncompressed.type())) {
                simple8b::visitAll<int64_t>(
                    pos + 1,
                    blocksSize,
                    lastNonRLE,
                    [](int64_t delta) {
                        uassert(8819300, "Unexpected non-zero delta in BSON Column", delta == 0);
                    },
                    []() {});
            } else {
                d64.lastEncodedValue = expandDelta(
                    d64.lastEncodedValue, simple8b::sum<int64_t>(pos + 1, blocksSize, lastNonRLE));

                if (lastUncompressed.type() == NumberDouble) {
                    current.lastAtEndOfBlock =
                        Simple8bTypeUtil::decodeDouble(d64.lastEncodedValue, d64.scaleIndex);
                }
            }

            current.scaleIndex = d64.scaleIndex;
        } else {
            uassert(8827801,
                    "Invalid control byte in BSON Column",
                    scaleIndexForControlByte(control) == Simple8bTypeUtil::kMemoryAsInteger);
            // Helper to determine if we may only encode zero deltas
            auto zeroDeltaOnly = [&]() {
                if (lastUncompressed.type() == BinData) {
                    int len;
                    lastUncompressed.binData(len);
                    if (len > 16) {
                        return true;
                    }
                }
                return false;
            };

            if (zeroDeltaOnly()) {
                simple8b::visitAll<int128_t>(
                    pos + 1,
                    blocksSize,
                    lastNonRLE,
                    [](int128_t delta) {
                        uassert(8819301, "Unexpected non-zero delta in BSON Column", delta == 0);
                    },
                    []() {});
            } else {
                auto& d128 =
                    std::get<BSONColumn::Iterator::DecodingState::Decoder128>(state.decoder);
                if (!lastLiteralUnencodable) {
                    d128.lastEncodedValue =
                        expandDelta(d128.lastEncodedValue,
                                    simple8b::sum<int128_t>(pos + 1, blocksSize, lastNonRLE));
                } else {
                    // If our literal is unencodable we need to also maintain the last non-zero
                    // value. So we cannot use the optimized sum() function and rather have to visit
                    // all values.
                    simple8b::visitAll<int128_t>(
                        pos + 1,
                        blocksSize,
                        lastNonRLE,
                        [&](int128_t delta) {
                            if (delta != 0) {
                                lastNonZeroDeltaForUnencodable = delta;
                            }
                            d128.lastEncodedValue = expandDelta(d128.lastEncodedValue, delta);
                        },
                        []() {});
                }
            }
        }

        // Remember control block and advance the position to next
        current.control = pos;
        pos += blocksSize + 1;
    }
    uasserted(8288102, "Unexpected end of BSONColumn binary");
}

template <class BufBuilderType, class BSONObjType, class Allocator>
void BSONColumnBuilder<BufBuilderType, BSONObjType, Allocator>::BinaryReopen::reopen(
    BSONColumnBuilder& builder, Allocator allocator) const {
    auto& regular = std::get<typename InternalState::Regular>(builder._is.state);
    // When the binary ends with an uncompressed element it is simple to re-initialize the
    // compressor
    if (!current.control) {
        auto& encoder = std::get<Encoder64>(regular._encoder);
        // Set last double in previous block (if any).
        encoder.lastValueInPrevBlock = last.lastAtEndOfBlock;

        // Append the last element to finish setting up the compressor
        builder.append(lastUncompressed);

        // No buffer needed to be saved
        builder._bufBuilder.reset();
        // Offset is entire binary with the last EOO removed
        builder._is.offset = lastUncompressed.rawdata() + lastUncompressed.size() - scannedBinary;
        return;
    }

    if (!uses128bit(lastUncompressed.type())) {
        auto& encoder = std::get<Encoder64>(regular._encoder);
        encoder.scaleIndex = current.scaleIndex;

        _reopen64BitTypes(
            regular, encoder, builder._bufBuilder, builder._is.offset, builder._is.lastControl);
    } else {
        auto& encoder = regular._encoder.template emplace<Encoder128>(allocator);
        _reopen128BitTypes(
            regular, encoder, builder._bufBuilder, builder._is.offset, builder._is.lastControl);
    }

    builder._is.lastBufLength = builder._bufBuilder.len();
}

template <class BufBuilderType, class BSONObjType, class Allocator>
void BSONColumnBuilder<BufBuilderType, BSONObjType, Allocator>::BinaryReopen::_reopen64BitTypes(
    EncodingState<BufBuilderType, Allocator>& regular,
    Encoder64& encoder,
    BufBuilderType& buffer,
    int& offset,
    uint8_t& lastControl) const {
    // The main difficulty with re-initializing the compressor from a compressed binary is
    // to undo the 'finalize()' call where pending values are flushed out to simple8b
    // blocks. We need to undo this operation by putting values back into the pending state.
    // The algorithm to perform this is to start from the end and add the values to a dummy
    // Simple8bBuilder and discover when this becomes full and writes out a simple8b block.
    // We will call this the 'overflow' point and all values in subsequent blocks in the
    // binary can be put back in the pending state.
    BSONType type = lastUncompressed.type();
    const char* control = current.control;
    const char* extraS8b = nullptr;
    bool overflow = false;
    Simple8bBuilder<uint64_t> s8bBuilder;

    // Calculate how many simple8b blocks this control byte contains
    auto currNumBlocks = numSimple8bBlocksForControlByte(*control);

    // First setup RLE state, the implementation for doing this differ if the last block actually
    // ends with RLE or not.
    const char* lastBlock = control + (sizeof(uint64_t) * (currNumBlocks - 1)) + 1;
    bool rle = (ConstDataView(lastBlock).read<LittleEndian<uint64_t>>() &
                simple8b_internal::kBaseSelectorMask) == simple8b_internal::kRleSelector;

    boost::optional<uint64_t> lastForS8b;
    // Current overflow point
    int currIndex;
    // Pending RLE block in current control when overflow has not happened yet.
    int pendingRle = -1;
    if (rle) {
        // If the last block ends with RLE we just need to look for the last non-RLE block to
        // discover the overflow point. The last value for RLE will be the actual last in this block
        // as we know the RLE will follow.
        std::tie(lastForS8b, currIndex) = _appendUntilOverflowForRLE(
            encoder.simple8bBuilder, overflow, control, currNumBlocks - 2);

    } else {
        // Assume that the last value in Simple8b blocks is the same as the one before the first.
        // This assumption will hold if all values are equal and RLE is eligible. If it turns out to
        // be incorrect the Simple8bBuilder will internally reset and disregard RLE.
        lastForS8b = _setupRLEForOverflowDetector(s8bBuilder, control, currNumBlocks - 1);

        // When RLE is setup we append as many values as we can to detect when we overflow
        std::tie(currIndex, pendingRle) = _appendUntilOverflow(
            s8bBuilder, encoder.simple8bBuilder, overflow, lastForS8b, control, currNumBlocks - 1);
    }

    // If we have pending RLE but no more control blocks to consider then set last for RLE to 0 as
    // the binary begins with RLE.
    if (!overflow && !last.control && pendingRle != -1) {
        lastForS8b = 0;
    }

    // If we have not yet overflowed then continue the same operation from the previous
    // simple8b block
    bool doubleRescale = false;
    if (!overflow && last.control) {
        auto blocks = numSimple8bBlocksForControlByte(*last.control);
        // Append values from control block to detect overflow. If the scale indices are
        // different we can skip this as we know we will not find a useful overflow point
        // here.
        int overflowIndex;
        // Flag to back out of processing last control if we determined that overflow happened in
        // RLE in current.
        bool resumeCurrent = false;
        if (current.scaleIndex == last.scaleIndex) {
            if (rle) {
                std::tie(lastForS8b, overflowIndex) = _appendUntilOverflowForRLE(
                    encoder.simple8bBuilder, overflow, last.control, blocks - 1);
            } else if (pendingRle != -1) {
                // Pending RLE block from current control we need to find overflow where we had our
                // overflow.
                auto [lastForRLE, rleIndexOverflow] = _appendUntilOverflowForRLE(
                    encoder.simple8bBuilder, overflow, last.control, blocks - 1);
                if (lastForRLE == lastForS8b) {
                    // Last value prior to RLE matches our RLE state after RLE. We then overflow in
                    // the block prior to RLE.
                    overflowIndex = rleIndexOverflow;
                } else {
                    // Values to not match, so the overflow happened in the pending RLE block.
                    currIndex = pendingRle;
                    resumeCurrent = true;
                }
            } else {
                std::tie(overflowIndex, pendingRle) = _appendUntilOverflow(s8bBuilder,
                                                                           encoder.simple8bBuilder,
                                                                           overflow,
                                                                           lastForS8b,
                                                                           last.control,
                                                                           blocks - 1);
            }
        } else {
            overflowIndex = blocks - 1;
            // Because we did not yet overflow we need to set last value in our simple8b
            // builder to the last value in previous block to be able to resume with RLE.
            Simple8b<uint64_t> s8b(last.control +
                                       /* offset to block at index */ overflowIndex *
                                           sizeof(uint64_t) +
                                       /* control byte*/ 1,
                                   /* one block at a time */ sizeof(uint64_t));
            boost::optional<uint64_t> lastValue;
            for (auto&& elem : s8b) {
                lastValue = elem;
            }
            encoder.simple8bBuilder.setLastForRLE(lastValue);
        }

        if (!resumeCurrent) {
            // Check if we overflowed in the first simple8b in this second control block. We can
            // then disregard this control block and proceed as-if we didn't overflow in the
            // first as there's nothing to re-write in the second control block.
            if (overflowIndex == blocks - 1) {
                // If the previous control block was not full, and we scaled then we need to
                // determine if we should consider the overflow happening in this block or not. This
                // can happen for the double type where we might not fill the control block with
                // values due to scaling. To determine if we overflowed here we will check if at
                // least one value can be re-scaled into the new scale factor as that represent a
                // "soft" boundary between the control blocks. If re-scaling was not possible there
                // is nothing from the previous control that should be kept for the following
                // values.
                if (blocks != 16 && current.scaleIndex != last.scaleIndex) {
                    // Encode last using new scale factor
                    auto encoded =
                        Simple8bTypeUtil::encodeDouble(last.lastAtEndOfBlock, current.scaleIndex);
                    Simple8b<uint64_t> rescale(
                        control + 1, currNumBlocks * sizeof(uint64_t), lastForS8b);
                    bool possible = true;
                    // See if next value can be scaled using the old scale factor
                    for (auto&& elem : rescale) {
                        if (elem) {
                            // See if this value is possible to scale using the old scale factor
                            encoded = expandDelta(*encoded, Simple8bTypeUtil::decodeInt64(*elem));
                            if (!Simple8bTypeUtil::encodeDouble(
                                    Simple8bTypeUtil::decodeDouble(*encoded, current.scaleIndex),
                                    last.scaleIndex)) {
                                possible = false;
                            }
                        }
                        break;
                    }

                    if (possible) {
                        // We could re-scale. Treat this as a special overflow where we append the
                        // necessary overflow data but mark the state as no overflow. We will then
                        // append all remaining values and the state will be setup accordingly
                        buffer.appendBuf(last.control, sizeof(uint64_t) * blocks + 1);

                        // offset will temporarily set to a negative value to compensate for the
                        // buffer we wrote above even when there's no overflow. Later on we will add
                        // a larger value which will make it positive again.
                        offset -= sizeof(uint64_t) * blocks + 1;

                        regular._controlByteOffset = 0;
                        lastControl = *last.control;

                        // Record that this code path was taken, this will prevent code later from
                        // overwriting some state we've just set.
                        doubleRescale = true;
                    }
                }

                overflow = false;
            } else {
                // If overflow happens later, we switch to this control byte as our new
                // 'current'. The previous current is remembered so we can add its values to
                // pending later.
                extraS8b = control;
                control = last.control;
                currNumBlocks = blocks;
                currIndex = overflowIndex;
            }
        }
    }

    if (!overflow) {
        // No overflow, discard entire buffer and record the offset up to this control byte. We will
        // then add everything in this control as pending which might write a control block again
        // because the values are now added in the correct order.
        offset += control - scannedBinary;
    } else {
        // Overflow, copy everything from the control byte up to the overflow point
        buffer.appendBuf(control, 1 + (currIndex + 1) * sizeof(uint64_t));

        // Set binary offset to this control byte (the binary starts with it, see the copy above)
        regular._controlByteOffset = 0;
        offset = control - scannedBinary;
        lastControl = *control;

        // Update count inside last control byte
        char* lastControlToUpdate = buffer.buf() + regular._controlByteOffset;
        *lastControlToUpdate =
            kControlByteForScaleIndex[encoder.scaleIndex] | (currIndex & kCountMask);
    }

    // Append remaining values from our current control block and add all from the next
    // block if needed
    auto appendPending = [&](const Simple8b<uint64_t>& s8b) {
        for (auto&& elem : s8b) {
            if (elem) {
                encoder.append(
                    type, *elem, buffer, regular._controlByteOffset, NoopControlBlockWriter{});
            } else {
                encoder.skip(type, buffer, regular._controlByteOffset, NoopControlBlockWriter{});
            }
        }
    };

    // Append all our pending values
    appendPending(Simple8b<uint64_t>(control + sizeof(uint64_t) * (currIndex + 1) + 1,
                                     (currNumBlocks - currIndex - 1) * sizeof(uint64_t),
                                     lastForS8b));

    if (extraS8b) {
        appendPending(
            Simple8b<uint64_t>(extraS8b + 1,
                               numSimple8bBlocksForControlByte(*extraS8b) * sizeof(uint64_t),
                               lastForS8b));
    }

    // If we did not overflow earlier we might have written a control byte when appending all
    // pending values, if this was the case make sure it is recorded. If we have rescaled, skip
    // this step as the correct control byte has already been recorded.
    if (regular._controlByteOffset != kNoSimple8bControl && !doubleRescale) {
        lastControl = *control;
    }

    // Reset last value if RLE is not possible due to the values appended above
    encoder.simple8bBuilder.resetLastForRLEIfNeeded();

    // Finally we need to set the necessary state to calculate deltas for future inserts. We
    // can take this from our decompressor state.
    auto& d64 = std::get<BSONColumn::Iterator::DecodingState::Decoder64>(state.decoder);

    // Hacky way to get an allocator to be able to materialize the last value.
    auto allocator = BSONColumn(nullptr, 1).release();
    bool deltaOfDelta = usesDeltaOfDelta(type);
    regular._storePrevious([&]() {
        if (lastUncompressed.eoo()) {
            return lastUncompressed;
        }

        // Zero delta is repeat of last uncompressed literal, no need to materialize. We can't
        // do this for doubles as the scaling may change along the way.
        if (!deltaOfDelta && d64.lastEncodedValue == lastUncompressedEncoded64 &&
            type != NumberDouble) {
            return lastUncompressed;
        }

        return d64.materialize(*allocator, lastUncompressed, ""_sd);
    }());
    // _prevEncoded64 is just set for a few types. We don't use Encoder64::initialize() as it
    // overwrites more members already set by this function.
    if (deltaOfDelta) {
        if (type == jstOID) {
            encoder.prevEncoded64 = d64.lastEncodedValueForDeltaOfDelta;
        }
        encoder.prevDelta = d64.lastEncodedValue;
    } else {
        if (type == NumberDouble) {
            encoder.prevEncoded64 = d64.lastEncodedValue;

            // Calculate last double in previous block by reversing the final pending state and
            // final delta.
            auto current = encoder.prevEncoded64;
            for (auto it = encoder.simple8bBuilder.rbegin(), end = encoder.simple8bBuilder.rend();
                 it != end;
                 ++it) {
                if (const boost::optional<uint64_t>& encoded = *it) {
                    // As we're going backwards we need to 'expandDelta' backwards which is the same
                    // as 'calcDelta'.
                    current = calcDelta(current, Simple8bTypeUtil::decodeInt64(*encoded));
                }
            }

            encoder.lastValueInPrevBlock =
                Simple8bTypeUtil::decodeDouble(current, encoder.scaleIndex);
        }
    }

    if (regular._controlByteOffset == kNoSimple8bControl) {
        // Appending pending values can flush out the control byte and leave all remaining values as
        // pending. We can discard our buffer in this case as this is equivalent to overflowing in
        // the last simple8b of the 'last' control block.
        offset += buffer.len();
        buffer.setlen(0);
        lastControl = kInvalidControlByte;
    } else if (lastControl == bsoncolumn::kInvalidControlByte) {
        // Set last control to current if still unset. This can happen if we never overflowed but
        // created a control byte when appending the pending values.
        lastControl = *control;
    }
}

template <class BufBuilderType, class BSONObjType, class Allocator>
void BSONColumnBuilder<BufBuilderType, BSONObjType, Allocator>::BinaryReopen::_reopen128BitTypes(
    EncodingState<BufBuilderType, Allocator>& regular,
    Encoder128& encoder,
    BufBuilderType& buffer,
    int& offset,
    uint8_t& lastControl) const {
    // The main difficulty with re-initializing the compressor from a compressed binary is
    // to undo the 'finalize()' call where pending values are flushed out to simple8b
    // blocks. We need to undo this operation by putting values back into the pending state.
    // The algorithm to perform this is to start from the end and add the values to a dummy
    // Simple8bBuilder and discover when this becomes full and writes out a simple8b block.
    // We will call this the 'overflow' point and all values in subsequent blocks in the
    // binary can be put back in the pending state.
    const char* control = current.control;
    const char* extraS8b = nullptr;
    bool overflow = false;
    Simple8bBuilder<uint128_t> s8bBuilder;

    // Calculate how many simple8b blocks this control byte contains
    auto currNumBlocks = numSimple8bBlocksForControlByte(*control);

    // First setup RLE state, the implementation for doing this differ if the last block actually
    // ends with RLE or not.
    const char* lastBlock = control + (sizeof(uint64_t) * (currNumBlocks - 1)) + 1;
    bool rle = (ConstDataView(lastBlock).read<LittleEndian<uint64_t>>() &
                simple8b_internal::kBaseSelectorMask) == simple8b_internal::kRleSelector;

    boost::optional<uint128_t> lastForS8b;
    int currIndex;
    int pendingRle = -1;
    if (rle) {
        // If the last block ends with RLE we just need to look for the last non-RLE block to
        // discover the overflow point. The last value for RLE will be the actual last in this block
        // as we know the RLE will follow.
        std::tie(lastForS8b, currIndex) = _appendUntilOverflowForRLE(
            encoder.simple8bBuilder, overflow, control, currNumBlocks - 2);

    } else {
        // Assume that the last value in Simple8b blocks is the same as the one before the first.
        // This assumption will hold if all values are equal and RLE is eligible. If it turns out to
        // be incorrect the Simple8bBuilder will internally reset and disregard RLE.
        lastForS8b = _setupRLEForOverflowDetector(s8bBuilder, control, currNumBlocks - 1);

        // When RLE is setup we append as many values as we can to detect when we overflow
        std::tie(currIndex, pendingRle) = _appendUntilOverflow(
            s8bBuilder, encoder.simple8bBuilder, overflow, lastForS8b, control, currNumBlocks - 1);
    }

    // If we have pending RLE but no more control blocks to consider then set last for RLE to 0 as
    // the binary begins with RLE.
    if (!overflow && !last.control && pendingRle != -1) {
        lastForS8b = 0;
    }

    // If we have not yet overflowed then continue the same operation from the previous
    // simple8b block
    if (!overflow && last.control) {

        auto blocks = numSimple8bBlocksForControlByte(*last.control);
        // Append values from control block to detect overflow.
        int overflowIndex;
        // Flag to back out of processing last control if we determined that overflow happened in
        // RLE in current.
        bool resumeCurrent = false;
        if (rle) {
            std::tie(lastForS8b, overflowIndex) = _appendUntilOverflowForRLE(
                encoder.simple8bBuilder, overflow, last.control, blocks - 1);
        } else if (pendingRle != -1) {
            // Pending RLE block from current control we need to find overflow where we had our
            // overflow.
            auto [lastForRLE, rleIndexOverflow] = _appendUntilOverflowForRLE(
                encoder.simple8bBuilder, overflow, last.control, blocks - 1);
            if (lastForRLE == lastForS8b) {
                // Last value prior to RLE matches our RLE state after RLE. We then overflow in
                // the block prior to RLE.
                overflowIndex = rleIndexOverflow;
            } else {
                // Values to not match, so the overflow happened in the pending RLE block.
                currIndex = pendingRle;
                resumeCurrent = true;
            }
        } else {
            std::tie(overflowIndex, pendingRle) = _appendUntilOverflow(s8bBuilder,
                                                                       encoder.simple8bBuilder,
                                                                       overflow,
                                                                       lastForS8b,
                                                                       last.control,
                                                                       blocks - 1);
        }

        if (!resumeCurrent) {
            // Check if we overflowed in the first simple8b in this second control block. We can
            // then disregard this control block and proceed as-if we didn't overflow in the
            // first as there's nothing to re-write in the second control block.
            if (overflowIndex == blocks - 1) {
                overflow = false;
            } else {
                // If overflow happens later, we switch to this control byte as our new
                // 'current'. The previous current is remembered so we can add its values to
                // pending later.
                extraS8b = control;
                control = last.control;
                currNumBlocks = blocks;
                currIndex = overflowIndex;
            }
        }
    }

    if (!overflow) {
        // No overflow, discard entire buffer and record the offset up to this control byte. We will
        // then add everything in this control as pending which might write a control block again
        // because the values are now added in the correct order.
        offset = control - scannedBinary;
    } else {
        // Overflow, copy everything from the control byte up to the overflow point
        buffer.appendBuf(control, 1 + (currIndex + 1) * sizeof(uint64_t));

        // Set binary offset to this control byte (the binary starts with it, see the copy above)
        regular._controlByteOffset = 0;
        offset = control - scannedBinary;

        // Update count inside last control byte
        char* lastControlToUpdate = buffer.buf() + regular._controlByteOffset;
        *lastControlToUpdate = kControlByteForScaleIndex[Simple8bTypeUtil::kMemoryAsInteger] |
            (currIndex & kCountMask);
    }

    // Append remaining values from our current control block and add all from the next
    // block if needed
    auto appendPending = [&](const Simple8b<uint128_t>& s8b) {
        for (auto&& elem : s8b) {
            if (elem) {
                encoder.append(lastUncompressed.type(),
                               *elem,
                               buffer,
                               regular._controlByteOffset,
                               NoopControlBlockWriter{});
            } else {
                encoder.skip(lastUncompressed.type(),
                             buffer,
                             regular._controlByteOffset,
                             NoopControlBlockWriter{});
            }
        }
    };

    appendPending(Simple8b<uint128_t>(control + sizeof(uint64_t) * (currIndex + 1) + 1,
                                      (currNumBlocks - currIndex - 1) * sizeof(uint64_t),
                                      lastForS8b));

    if (extraS8b) {
        appendPending(
            Simple8b<uint128_t>(extraS8b + 1,
                                numSimple8bBlocksForControlByte(*extraS8b) * sizeof(uint64_t),
                                lastForS8b));
    }

    // Reset last value if RLE is not possible due to the values appended above
    encoder.simple8bBuilder.resetLastForRLEIfNeeded();

    // Finally we need to set the necessary state to calculate deltas for future inserts. We
    // can take this from our decompressor state.
    auto& d128 = std::get<BSONColumn::Iterator::DecodingState::Decoder128>(state.decoder);

    // Hacky way to get an allocator to be able to materialize the last value.
    auto allocator = BSONColumn(nullptr, 1).release();
    regular._storePrevious([&]() {
        // Zero delta is repeat of last uncompressed literal, avoid materialization (which might
        // not be possible depending on value of last uncompressed literal). If our literal was
        // unencodable we need to force materialization as zero delta may no longer mean repeat of
        // last literal.
        if (d128.lastEncodedValue == lastUncompressedEncoded128 &&
            !(lastLiteralUnencodable && lastUncompressedEncoded128 != 0)) {
            return lastUncompressed;
        }
        return d128.materialize(*allocator, lastUncompressed, ""_sd);
    }());
    encoder.initialize(regular._previous());

    if (regular._controlByteOffset == kNoSimple8bControl) {
        // Appending pending values can flush out the control byte and leave all remaining values as
        // pending. We can discard our buffer in this case as this is equivalent to overflowing in
        // the last simple8b of the 'last' control block.
        offset += buffer.len();
        buffer.setlen(0);
    } else {
        // Set last control to current if still unset.
        lastControl = *control;
    }
}

template <class BufBuilderType, class BSONObjType, class Allocator>
template <typename T>
boost::optional<T> BSONColumnBuilder<BufBuilderType, BSONObjType, Allocator>::BinaryReopen::
    _setupRLEForOverflowDetector(Simple8bBuilder<T>& overflowDetector,
                                 const char* s8bBlock,
                                 int index) {
    // Limit the search for a non-skip value. If we go above 60 without overflow then we consider
    // skip to be the last value for RLE as it would be the only one eligible for RLE.
    constexpr int kMaxNumSkipInNonRLEBlock = 60;
    for (int numSkips = 0; index >= 0 && numSkips < kMaxNumSkipInNonRLEBlock; --index) {
        const char* block = s8bBlock + sizeof(uint64_t) * index + 1;
        bool rle = (ConstDataView(block).read<LittleEndian<uint64_t>>() &
                    simple8b_internal::kBaseSelectorMask) == simple8b_internal::kRleSelector;
        // Abort this operation when an RLE block is found, they are handled in a separate code
        // path.
        if (rle) {
            break;
        }
        Simple8b<T> s8b(block, sizeof(uint64_t));
        for (auto it = s8b.begin(), end = s8b.end();
             it != end && numSkips < kMaxNumSkipInNonRLEBlock;
             ++it) {
            const auto& elem = *it;
            if (elem) {
                // We do not need to use the actual last value for RLE when determining overflow
                // point later. We can use the first value we discover when performing this
                // iteration. For a RLE block to be undone and put back into the pending state all
                // values need to be the same. So if a value later in this Simple8b block is
                // different from this value we cannot undo all these containing a RLE. If the
                // values are not all the same we will not fit 120 zeros in pending and the RLE
                // block will be left as-is.
                overflowDetector.setLastForRLE(elem);
                return elem;
            }
            ++numSkips;
        }
    }
    // We did not find any value, so use skip as RLE. It is important that we use 'none' to
    // interpret RLE blocks going forward so we can properly undo simple8b blocks containing all
    // skip and RLE blocks.
    overflowDetector.setLastForRLE(boost::none);
    return boost::none;
}

template <class BufBuilderType, class BSONObjType, class Allocator>
template <typename T>
std::pair<int, int>
BSONColumnBuilder<BufBuilderType, BSONObjType, Allocator>::BinaryReopen::_appendUntilOverflow(
    Simple8bBuilder<T>& overflowDetector,
    Simple8bBuilder<T, Allocator>& mainBuilder,
    bool& overflow,
    const boost::optional<T>& lastValForRLE,
    const char* s8bBlock,
    int index) {
    auto writeFn = [&overflow](uint64_t block) mutable {
        overflow = true;
    };
    for (; index >= 0; --index) {
        const char* block = s8bBlock +
            /* offset to block at index */ index * sizeof(uint64_t) +
            /* control byte*/
            1;
        bool rle = (ConstDataView(block).read<LittleEndian<uint64_t>>() &
                    simple8b_internal::kBaseSelectorMask) == simple8b_internal::kRleSelector;
        if (rle) {
            // RLE detected, we need to continue to detect overflow. Depending on if the last value
            // before the RLE block matches or current last we overflowed in this RLE block or in
            // the first non-RLE block prior.
            auto [lastForRLE, rleIndexOverflow] =
                _appendUntilOverflowForRLE(mainBuilder, overflow, s8bBlock, index - 1);
            if (lastForRLE == lastValForRLE) {
                // Last value prior to RLE matches our RLE state after RLE. We then overflow in the
                // block prior to RLE.
                return std::pair(rleIndexOverflow, -1);
            } else if (rleIndexOverflow == -1) {
                // We exhausted this control block without determining where the overflow point is.
                // Return pending RLE index so we can continue this operation in the prior control
                // block.
                return std::pair(-1, index);
            }

            // Overflow inside the RLE block, we're done.
            break;
        }

        Simple8b<T> s8b(block,
                        /* one block at a time */ sizeof(uint64_t),
                        lastValForRLE);
        boost::optional<T> last;
        for (auto&& elem : s8b) {
            last = elem;
            if (elem) {
                overflowDetector.append(*last, writeFn);
            } else {
                overflowDetector.skip(writeFn);
            }
        }

        if (overflow) {
            // Overflow point detected, record the last value in last Simple8b block
            // before our pending values. This is necessary to be able to resume with
            // RLE.
            mainBuilder.setLastForRLE(last);
            break;
        }
    }
    return std::pair(index, -1);
}

template <class BufBuilderType, class BSONObjType, class Allocator>
template <typename T>
std::pair<boost::optional<T>, int>
BSONColumnBuilder<BufBuilderType, BSONObjType, Allocator>::BinaryReopen::_appendUntilOverflowForRLE(
    Simple8bBuilder<T, Allocator>& mainBuilder, bool& overflow, const char* s8bBlock, int index) {
    for (; index >= 0; --index) {
        const char* block = s8bBlock +
            /* offset to block at index */ index * sizeof(uint64_t) +
            /* control byte*/ 1;
        bool rle = (ConstDataView(block).read<LittleEndian<uint64_t>>() &
                    simple8b_internal::kBaseSelectorMask) == simple8b_internal::kRleSelector;

        if (rle) {
            continue;
        }

        Simple8b<T> s8b(block, sizeof(uint64_t), T{});

        boost::optional<T> last;
        for (auto&& elem : s8b) {
            last = elem;
        }

        // Overflow point detected, record the last value in last Simple8b block
        // before our pending values. This is necessary to be able to resume with
        // RLE.
        mainBuilder.setLastForRLE(last);
        overflow = true;
        return std::make_pair(last, index);
    }

    return std::make_pair(T{}, index);
}

template <class BufBuilderType, class BSONObjType, class Allocator>
BSONColumnBuilder<BufBuilderType, BSONObjType, Allocator>::InternalState::InternalState(Allocator a)
    : allocator(a),
      state(std::in_place_type_t<Regular>{}, allocator),
      lastControl(bsoncolumn::kInvalidControlByte) {}

template <class BufBuilderType, class BSONObjType, class Allocator>
BSONColumnBuilder<BufBuilderType, BSONObjType, Allocator>::InternalState::Interleaved::Interleaved(
    Allocator a)
    : allocator(a),
      subobjStates(allocator),
      referenceSubObj(TrackableBSONObj{BSONObj{}}, allocator),
      bufferedObjElements(allocator) {}

template <class BufBuilderType, class BSONObjType, class Allocator>
BSONColumnBuilder<BufBuilderType, BSONObjType, Allocator>::InternalState::InternalState::
    Interleaved::Interleaved(const Interleaved& other)
    : allocator(other.allocator),
      mode(other.mode),
      subobjStates(other.subobjStates),
      referenceSubObj(TrackableBSONObj{other.referenceSubObj.get().get()}, allocator),
      referenceSubObjType(other.referenceSubObjType),
      bufferedObjElements(copyBufferedObjElements(other.bufferedObjElements, allocator)) {}

template <class BufBuilderType, class BSONObjType, class Allocator>
typename BSONColumnBuilder<BufBuilderType, BSONObjType, Allocator>::InternalState::Interleaved&
BSONColumnBuilder<BufBuilderType, BSONObjType, Allocator>::InternalState::Interleaved::operator=(
    const Interleaved& other) {
    if (&other == this) {
        return *this;
    }

    allocator = other.allocator;
    mode = other.mode;
    subobjStates = other.subobjStates;
    referenceSubObj = {TrackableBSONObj{other.referenceSubObj.get().get()}, allocator};
    referenceSubObjType = other.referenceSubObjType;
    bufferedObjElements = copyBufferedObjElements(other.bufferedObjElements, allocator);

    return *this;
}

template <class BufBuilderType, class BSONObjType, class Allocator>
BSONColumnBuilder<BufBuilderType, BSONObjType, Allocator>::BSONColumnBuilder(Allocator allocator)
    : BSONColumnBuilder(BufBuilderType{allocator, kDefaultBufferSize}, allocator) {}

template <class BufBuilderType, class BSONObjType, class Allocator>
BSONColumnBuilder<BufBuilderType, BSONObjType, Allocator>::BSONColumnBuilder(BufBuilderType builder,
                                                                             Allocator allocator)
    : _is(allocator), _bufBuilder(std::move(builder)) {
    _bufBuilder.reset();
}

template <class BufBuilderType, class BSONObjType, class Allocator>
BSONColumnBuilder<BufBuilderType, BSONObjType, Allocator>::BSONColumnBuilder(const char* binary,
                                                                             int size,
                                                                             Allocator allocator)
    : BSONColumnBuilder(BufBuilderType{allocator, kDefaultBufferSize}, allocator) {
    using namespace bsoncolumn;

    // Handle empty case
    uassert(8288103, "BSONColumn binaries are at least 1 byte in size", size > 0);
    if (size == 1) {
        uassert(8288104, "Unexpected end of BSONColumn binary", *binary == '\0');
        return;
    }

    BinaryReopen helper;

    // Handle interleaved mode separately. Fully reset this BSONColumnBuilder and then
    // decompress and append all data.
    if (!helper.scan(binary, size)) {
        _bufBuilder.reset();
        _is.state.template emplace<typename InternalState::Regular>(allocator);

        BSONColumn decompressor(binary, size);
        for (auto&& elem : decompressor) {
            append(elem);
        }
        [[maybe_unused]] auto diff = intermediate();
        return;
    }

    // Perform the reopen from the scanned state
    helper.reopen(*this, _is.allocator);
}

template <class BufBuilderType, class BSONObjType, class Allocator>
BSONColumnBuilder<BufBuilderType, BSONObjType, Allocator>&
BSONColumnBuilder<BufBuilderType, BSONObjType, Allocator>::append(BSONElement elem) {
    auto type = elem.type();
    if (elem.eoo()) {
        return skip();
    }

    if ((type != Object && type != Array) || elem.Obj().isEmpty()) {
        // Flush previous sub-object compression when non-object is appended
        if (std::holds_alternative<typename InternalState::Interleaved>(_is.state)) {
            _flushSubObjMode();
        }
        std::get<typename InternalState::Regular>(_is.state).append(
            elem, _bufBuilder, NoopControlBlockWriter{}, _is.allocator);
        return *this;
    }

    return _appendObj(elem);
}

template <class BufBuilderType, class BSONObjType, class Allocator>
BSONColumnBuilder<BufBuilderType, BSONObjType, Allocator>&
BSONColumnBuilder<BufBuilderType, BSONObjType, Allocator>::append(const BSONObj& obj) {
    return _appendObj({obj, Object});
}

template <class BufBuilderType, class BSONObjType, class Allocator>
BSONColumnBuilder<BufBuilderType, BSONObjType, Allocator>&
BSONColumnBuilder<BufBuilderType, BSONObjType, Allocator>::append(const BSONArray& arr) {
    return _appendObj({arr, Array});
}

template <class BufBuilderType, class BSONObjType, class Allocator>
BSONColumnBuilder<BufBuilderType, BSONObjType, Allocator>&
BSONColumnBuilder<BufBuilderType, BSONObjType, Allocator>::_appendObj(Element elem) {
    auto type = elem.type;
    auto obj = elem.value.Obj();
    bool containsScalars = _containsScalars(obj);

    if (auto* regular = std::get_if<typename InternalState::Regular>(&_is.state)) {
        if (!containsScalars) {
            regular->append(elem, _bufBuilder, NoopControlBlockWriter{}, _is.allocator);
        } else {
            _startDetermineSubObjReference(obj, type);
        }

        return *this;
    }

    // Use a pointer here so that it can get reassigned below in case we need to restart subobj
    // compression.
    auto* interleaved = &std::get<typename InternalState::Interleaved>(_is.state);

    // Different types on root is not allowed
    if (type != interleaved->referenceSubObjType) {
        _flushSubObjMode();
        _startDetermineSubObjReference(obj, type);
        return *this;
    }

    if (interleaved->mode == InternalState::Interleaved::Mode::kDeterminingReference) {
        // We are in DeterminingReference mode, check if this current object is compatible and merge
        // in any new fields that are discovered.
        uint32_t numElementsReferenceObj = 0;
        auto perElementLockStep = [this, &numElementsReferenceObj](const BSONElement& ref,
                                                                   const BSONElement& elem) {
            ++numElementsReferenceObj;
        };
        if (!traverseLockStep(interleaved->referenceSubObj.get().get(), obj, perElementLockStep)) {
            BSONObj merged = [&] {
                return mergeObj(interleaved->referenceSubObj.get().get(), obj);
            }();
            if (merged.isEmptyPrototype()) {
                // If merge failed, flush current sub-object compression and start over.
                _flushSubObjMode();

                // If we only contain empty subobj (no value elements) then append in regular mode
                // instead of re-starting subobj compression.
                if (!containsScalars) {
                    std::get<typename InternalState::Regular>(_is.state).append(
                        elem, _bufBuilder, NoopControlBlockWriter{}, _is.allocator);
                    return *this;
                }

                interleaved =
                    &_is.state.template emplace<typename InternalState::Interleaved>(_is.allocator);
                interleaved->referenceSubObj = {TrackableBSONObj{obj.getOwned()}, _is.allocator};
                interleaved->referenceSubObjType = type;
                interleaved->bufferedObjElements.emplace_back(
                    TrackableBSONObj{interleaved->referenceSubObj.get().get()}, _is.allocator);
                return *this;
            }
            interleaved->referenceSubObj = {TrackableBSONObj{merged}, _is.allocator};
        }

        // If we've buffered twice as many objects as we have sub-elements we will achieve good
        // compression so use the currently built reference.
        if (numElementsReferenceObj * 2 >= interleaved->bufferedObjElements.size()) {
            interleaved->bufferedObjElements.emplace_back(TrackableBSONObj{obj.getOwned()},
                                                          _is.allocator);
            return *this;
        }

        _finishDetermineSubObjReference();
    }

    // Reference already determined for sub-object compression, try to add this new object.
    if (!_appendSubElements(obj)) {
        // If we were not compatible restart subobj compression unless our object contain no value
        // fields (just empty subobjects)
        if (!containsScalars) {
            std::get<typename InternalState::Regular>(_is.state).append(
                elem, _bufBuilder, NoopControlBlockWriter{}, _is.allocator);
        } else {
            _startDetermineSubObjReference(obj, type);
        }
    }
    return *this;
}

template <class BufBuilderType, class BSONObjType, class Allocator>
BSONColumnBuilder<BufBuilderType, BSONObjType, Allocator>&
BSONColumnBuilder<BufBuilderType, BSONObjType, Allocator>::skip() {
    if (auto* regular = std::get_if<typename InternalState::Regular>(&_is.state)) {
        regular->skip(_bufBuilder, NoopControlBlockWriter{});
        return *this;
    }

    auto& interleaved = std::get<typename InternalState::Interleaved>(_is.state);

    // If the reference object contain any empty subobjects we need to end interleaved mode as
    // skipping in all substreams would not be encoded as skipped root object.
    if (_hasEmptyObj(interleaved.referenceSubObj.get().get())) {
        _flushSubObjMode();
        return skip();
    }

    if (interleaved.mode == InternalState::Interleaved::Mode::kDeterminingReference) {
        interleaved.bufferedObjElements.emplace_back(TrackableBSONObj{BSONObj()}, _is.allocator);
    } else {
        for (auto&& subobj : interleaved.subobjStates) {
            subobj.state.skip(subobj.buffer, subobj.controlBlockWriter());
        }
    }

    return *this;
}

template <class BufBuilderType, class BSONObjType, class Allocator>
typename BSONColumnBuilder<BufBuilderType, BSONObjType, Allocator>::BinaryDiff
BSONColumnBuilder<BufBuilderType, BSONObjType, Allocator>::intermediate() {
    // If we are finalized it is not possible to calculate an intermediate diff
    invariant(_is.offset != kFinalizedOffset);

    // Save internal state before finalizing
    InternalState newState = _is;
    int length = _bufBuilder.len();
    // Number of identical bytes in the binary this call to intermediate produces compared to
    // previous binaries. This is to make an as small diff as possible to the user, we can calculate
    // this by simply comparing how the last control byte changes.
    int identicalBytes = 0;
    // Save some state related to last control byte so we can see how it changes after finalize() is
    // called.
    ptrdiff_t controlOffset =
        visit(OverloadedVisitor{[](const typename InternalState::Regular& regular) {
                                    return regular._controlByteOffset;
                                },
                                [](const typename InternalState::Interleaved&) {
                                    return kNoSimple8bControl;
                                }},
              _is.state);
    uint8_t lastControlByte =
        controlOffset != kNoSimple8bControl ? *(_bufBuilder.buf() + controlOffset) : 0;

    // Finalize binary
    int prevOffset = _is.offset;
    _is.offset = 0;
    finalize();

    // Copy data into new buffer that we need to keep in the builder. If we have no control byte in
    // regular mode we're currently writing on, then we can consume the entire binary. Otherwise we
    // can only consume up to this control byte as it may change in the future.
    auto buffer = [&]() {
        if (controlOffset == kNoSimple8bControl) {
            newState.offset += length;
            newState.lastControl = kInvalidControlByte;
            newState.lastBufLength = 0;
            return BufBuilderType{_is.allocator, 0};
        }

        // After calling intermediate, the control byte we're currently working on need to be the
        // first byte in the new binary going forward. This is the first byte that may change when
        // more data is appended.
        auto buffer = BufBuilderType{_is.allocator, static_cast<size_t>(length - controlOffset)};
        buffer.appendChar(lastControlByte);
        buffer.appendBuf(_bufBuilder.buf() + controlOffset + 1, length - controlOffset - 1);
        std::get<typename InternalState::Regular>(newState.state)._controlByteOffset = 0;
        newState.offset += controlOffset;
        newState.lastBufLength = length - controlOffset;

        // Compare the control byte at the beginning of the finalized binary against state of last
        // finalized binary. If they are the same we can advance the point of the first byte that
        // changed to the user. However, if this is the first time we call intermediate, make sure
        // we return the full binary.
        if (_is.lastControl != kInvalidControlByte) {
            // When lastControl has been set, the control byte we're working on is always at the
            // beginning of the binary.
            uint8_t controlByteThisBinary = *_bufBuilder.buf();
            /* TODO (SERVER-87383): Determine a safe optimization to minimize diffs
            if (prevOffset != 0 && _is.lastControl == controlByteThisBinary &&
                _is.lastBufLength > controlOffset) {
                identicalBytes = _is.lastBufLength - controlOffset;
            }*/
            newState.lastControl = controlByteThisBinary;
        } else {
            newState.lastControl = *(_bufBuilder.buf() + controlOffset);
        }

        return buffer;
    }();

    // Swap buffers so we return the finalized one and keep the data we need to keep in this
    // builder.
    using std::swap;
    swap(buffer, _bufBuilder);

    // Restore previous state.
    _is = std::move(newState);

    // Return data
    int bufSize = buffer.len();
    return {buffer.release(), bufSize, identicalBytes, prevOffset + identicalBytes};
}

template <class BufBuilderType, class BSONObjType, class Allocator>
BSONBinData BSONColumnBuilder<BufBuilderType, BSONObjType, Allocator>::finalize() {
    // We may only finalize when we have the full binary
    invariant(_is.offset == 0);

    if (auto* regular = std::get_if<typename InternalState::Regular>(&_is.state)) {
        regular->flush(_bufBuilder, NoopControlBlockWriter{});
    } else {
        _flushSubObjMode();
    }

    // Write EOO at the end
    _bufBuilder.appendChar(EOO);

    _is.offset = kFinalizedOffset;

    return {_bufBuilder.buf(), _bufBuilder.len(), BinDataType::Column};
}

template <class BufBuilderType, class BSONObjType, class Allocator>
BufBuilderType BSONColumnBuilder<BufBuilderType, BSONObjType, Allocator>::detach() {
    return std::move(_bufBuilder);
}

template <class BufBuilderType, class BSONObjType, class Allocator>
int BSONColumnBuilder<BufBuilderType, BSONObjType, Allocator>::numInterleavedStartWritten() const {
    return _numInterleavedStartWritten;
}

template <class BufBuilderType, class BSONObjType, class Allocator>
BSONElement BSONColumnBuilder<BufBuilderType, BSONObjType, Allocator>::last() const {
    return visit(OverloadedVisitor{[](const typename InternalState::Regular& regular) {
                                       return BSONElement{
                                           regular._prev.data(),
                                           /*field name size including null terminator*/ 1,
                                           /*total size*/ static_cast<int>(regular._prev.size()),
                                           BSONElement::TrustedInitTag{}};
                                   },
                                   [](const typename InternalState::Interleaved&) {
                                       return BSONElement{};
                                   }},
                 _is.state);
}

namespace bsoncolumn {
bool Element::operator==(const Element& rhs) const {
    if (type != rhs.type || size != rhs.size)
        return false;

    return memcmp(value.value(), rhs.value.value(), size) == 0;
}

template <class BufBuilderType, class Allocator>
EncodingState<BufBuilderType, Allocator>::Encoder64::Encoder64(Allocator allocator)
    : simple8bBuilder(allocator), scaleIndex(Simple8bTypeUtil::kMemoryAsInteger) {}

template <class BufBuilderType, class Allocator>
void EncodingState<BufBuilderType, Allocator>::Encoder64::initialize(Element elem) {
    switch (elem.type) {
        case NumberDouble: {
            lastValueInPrevBlock = elem.value.Double();
            std::tie(prevEncoded64, scaleIndex) = scaleAndEncodeDouble(lastValueInPrevBlock, 0);
        } break;
        case jstOID: {
            prevEncoded64 = Simple8bTypeUtil::encodeObjectId(elem.value.ObjectID());
        } break;
        default:
            break;
    }
}

template <class BufBuilderType, class Allocator>
template <class F>
bool EncodingState<BufBuilderType, Allocator>::Encoder64::appendDelta(Element elem,
                                                                      Element previous,
                                                                      BufBuilderType& buffer,
                                                                      ptrdiff_t& controlByteOffset,
                                                                      F controlBlockWriter,
                                                                      Allocator allocator) {
    // Variable to indicate that it was possible to encode this BSONElement as an integer
    // for storage inside Simple8b. If encoding is not possible the element is stored as
    // uncompressed.
    bool encodingPossible = true;
    // Value to store in Simple8b if encoding is possible.
    int64_t value = 0;
    switch (elem.type) {
        case NumberDouble:
            return _appendDouble(elem.value.Double(),
                                 previous.value.Double(),
                                 buffer,
                                 controlByteOffset,
                                 controlBlockWriter,
                                 allocator);
        case NumberInt:
            value = calcDelta(elem.value.Int32(), previous.value.Int32());
            break;
        case NumberLong:
            value = calcDelta(elem.value.Int64(), previous.value.Int64());
            break;
        case jstOID: {
            auto oid = elem.value.ObjectID();
            auto prevOid = previous.value.ObjectID();
            encodingPossible = objectIdDeltaPossible(oid, prevOid);
            if (!encodingPossible)
                break;

            int64_t curEncoded = Simple8bTypeUtil::encodeObjectId(oid);
            value = calcDelta(curEncoded, prevEncoded64);
            prevEncoded64 = curEncoded;
            break;
        }
        case bsonTimestamp: {
            value = calcDelta(elem.value.TimestampValue(), previous.value.TimestampValue());
            break;
        }
        case Date:
            value = calcDelta(elem.value.Date().toMillisSinceEpoch(),
                              previous.value.Date().toMillisSinceEpoch());
            break;
        case Bool:
            value = calcDelta(elem.value.Boolean(), previous.value.Boolean());
            break;
        case Undefined:
        case jstNULL:
            value = 0;
            break;
        case RegEx:
        case DBRef:
        case CodeWScope:
        case Symbol:
        case Object:
        case Array:
            encodingPossible = false;
            break;
        default:
            MONGO_UNREACHABLE;
    };
    if (usesDeltaOfDelta(elem.type)) {
        int64_t currentDelta = value;
        value = calcDelta(currentDelta, prevDelta);
        prevDelta = currentDelta;
    }
    if (encodingPossible) {
        return append(elem.type,
                      Simple8bTypeUtil::encodeInt64(value),
                      buffer,
                      controlByteOffset,
                      controlBlockWriter);
    }
    return false;
}

template <class BufBuilderType, class Allocator>
template <class F>
bool EncodingState<BufBuilderType, Allocator>::Encoder64::append(BSONType type,
                                                                 uint64_t value,
                                                                 BufBuilderType& buffer,
                                                                 ptrdiff_t& controlByteOffset,
                                                                 F controlBlockWriter) {
    return simple8bBuilder.append(
        value,
        Simple8bBlockWriter64<F>(*this, buffer, controlByteOffset, type, controlBlockWriter));
}

template <class BufBuilderType, class Allocator>
template <class F>
void EncodingState<BufBuilderType, Allocator>::Encoder64::skip(BSONType type,
                                                               BufBuilderType& buffer,
                                                               ptrdiff_t& controlByteOffset,
                                                               F controlBlockWriter) {
    simple8bBuilder.skip(
        Simple8bBlockWriter64<F>(*this, buffer, controlByteOffset, type, controlBlockWriter));
}

template <class BufBuilderType, class Allocator>
template <class F>
void EncodingState<BufBuilderType, Allocator>::Encoder64::flush(BSONType type,
                                                                BufBuilderType& buffer,
                                                                ptrdiff_t& controlByteOffset,
                                                                F controlBlockWriter) {
    simple8bBuilder.flush(
        Simple8bBlockWriter64<F>(*this, buffer, controlByteOffset, type, controlBlockWriter));
}

template <class BufBuilderType, class Allocator>
EncodingState<BufBuilderType, Allocator>::Encoder128::Encoder128(Allocator allocator)
    : simple8bBuilder(allocator) {}

template <class BufBuilderType, class Allocator>
void EncodingState<BufBuilderType, Allocator>::Encoder128::initialize(Element elem) {
    switch (elem.type) {
        case String:
        case Code: {
            prevEncoded128 = Simple8bTypeUtil::encodeString(elem.value.String());
        } break;
        case BinData: {
            auto binData = elem.value.BinData();
            prevEncoded128 = Simple8bTypeUtil::encodeBinary(static_cast<const char*>(binData.data),
                                                            binData.length);
        } break;
        case NumberDecimal: {
            prevEncoded128 = Simple8bTypeUtil::encodeDecimal128(elem.value.Decimal());
        } break;
        default:
            break;
    }
}

template <class BufBuilderType, class Allocator>
template <class F>
bool EncodingState<BufBuilderType, Allocator>::Encoder128::appendDelta(Element elem,
                                                                       Element previous,
                                                                       BufBuilderType& buffer,
                                                                       ptrdiff_t& controlByteOffset,
                                                                       F controlBlockWriter,
                                                                       Allocator) {
    auto appendEncoded = [&](int128_t encoded) {
        // If previous wasn't encodable we cannot store 0 in Simple8b as that would create
        // an ambiguity between 0 and repeat of previous
        if (prevEncoded128 || encoded != 0) {
            bool appended = append(
                elem.type,
                Simple8bTypeUtil::encodeInt128(calcDelta(encoded, prevEncoded128.value_or(0))),
                buffer,
                controlByteOffset,
                controlBlockWriter);
            prevEncoded128 = encoded;
            return appended;
        }
        return false;
    };

    switch (elem.type) {
        case String:
        case Code:
            if (auto encoded = Simple8bTypeUtil::encodeString(elem.value.String())) {
                return appendEncoded(*encoded);
            }
            break;
        case BinData: {
            auto binData = elem.value.BinData();
            auto prevBinData = previous.value.BinData();
            // We only do delta encoding of binary if the binary type and size are
            // exactly the same. To support size difference we'd need to add a count to
            // be able to reconstruct binaries starting with zero bytes. We don't want
            // to waste bits for this.
            if (binData.length != prevBinData.length || binData.type != prevBinData.type)
                return false;

            if (auto encoded = Simple8bTypeUtil::encodeBinary(
                    static_cast<const char*>(binData.data), binData.length)) {
                return appendEncoded(*encoded);
            }
        } break;
        case NumberDecimal:
            return appendEncoded(Simple8bTypeUtil::encodeDecimal128(elem.value.Decimal()));
            break;
        default:
            MONGO_UNREACHABLE;
    };
    return false;
}

template <class BufBuilderType, class Allocator>
template <class F>
bool EncodingState<BufBuilderType, Allocator>::Encoder128::append(BSONType type,
                                                                  uint128_t value,
                                                                  BufBuilderType& buffer,
                                                                  ptrdiff_t& controlByteOffset,
                                                                  F controlBlockWriter) {
    return simple8bBuilder.append(
        value, Simple8bBlockWriter128<F>(buffer, controlByteOffset, controlBlockWriter));
}

template <class BufBuilderType, class Allocator>
template <class F>
void EncodingState<BufBuilderType, Allocator>::Encoder128::skip(BSONType type,
                                                                BufBuilderType& buffer,
                                                                ptrdiff_t& controlByteOffset,
                                                                F controlBlockWriter) {
    simple8bBuilder.skip(Simple8bBlockWriter128<F>(buffer, controlByteOffset, controlBlockWriter));
}

template <class BufBuilderType, class Allocator>
template <class F>
void EncodingState<BufBuilderType, Allocator>::Encoder128::flush(BSONType type,
                                                                 BufBuilderType& buffer,
                                                                 ptrdiff_t& controlByteOffset,
                                                                 F controlBlockWriter) {
    simple8bBuilder.flush(Simple8bBlockWriter128<F>(buffer, controlByteOffset, controlBlockWriter));
}

template <class BufBuilderType, class Allocator>
EncodingState<BufBuilderType, Allocator>::EncodingState(Allocator allocator)
    : _encoder(std::in_place_type<Encoder64>, allocator),
      _prev(allocator),
      _controlByteOffset(kNoSimple8bControl) {
    // Store EOO type with empty field name as previous.
    _storePrevious(BSONElement());
}

template <class BufBuilderType, class Allocator>
template <class F>
void EncodingState<BufBuilderType, Allocator>::append(Element elem,
                                                      BufBuilderType& buffer,
                                                      F controlBlockWriter,
                                                      Allocator allocator) {
    auto type = elem.type;
    auto previous = _previous();

    // If we detect a type change (or this is first value). Flush all pending values in Simple-8b
    // and write uncompressed literal. Reset all default values.
    if (previous.type != elem.type) {
        _storePrevious(elem);
        visit(
            [&](auto& encoder) {
                encoder.flush(type, buffer, _controlByteOffset, controlBlockWriter);
            },
            _encoder);

        _writeLiteralFromPrevious(buffer, controlBlockWriter, allocator);
        return;
    }

    visit(
        [&](auto& encoder) {
            appendDelta(encoder, elem, previous, buffer, controlBlockWriter, allocator);
        },
        _encoder);
}

template <class BufBuilderType, class Allocator>
template <class Encoder, class F>
void EncodingState<BufBuilderType, Allocator>::appendDelta(Encoder& encoder,
                                                           Element elem,
                                                           Element previous,
                                                           BufBuilderType& buffer,
                                                           F controlBlockWriter,
                                                           Allocator allocator) {
    auto type = elem.type;
    // Store delta in Simple-8b if types match
    bool compressed = !usesDeltaOfDelta(type) && elem == previous;
    if (compressed) {
        encoder.append(type, 0, buffer, _controlByteOffset, controlBlockWriter);
    }

    if (!compressed) {
        compressed = encoder.appendDelta(
            elem, previous, buffer, _controlByteOffset, controlBlockWriter, allocator);
    }
    _storePrevious(elem);

    // Store uncompressed literal if value is outside of range of encodable values.
    if (!compressed) {
        encoder.flush(type, buffer, _controlByteOffset, controlBlockWriter);
        _writeLiteralFromPrevious(buffer, controlBlockWriter, allocator);
    }
}

template <class BufBuilderType, class Allocator>
template <class F>
void EncodingState<BufBuilderType, Allocator>::skip(BufBuilderType& buffer, F controlBlockWriter) {
    auto before = buffer.len();
    visit(
        [&](auto& encoder) {
            encoder.skip(_previous().type, buffer, _controlByteOffset, controlBlockWriter);
        },
        _encoder);

    // Rescale previous known value if this skip caused Simple-8b blocks to be written
    if (before != buffer.len() && _previous().type == NumberDouble) {
        auto& encoder = std::get<Encoder64>(_encoder);
        std::tie(encoder.prevEncoded64, encoder.scaleIndex) =
            scaleAndEncodeDouble(encoder.lastValueInPrevBlock, 0);
    }
}

template <class BufBuilderType, class Allocator>
template <class F>
void EncodingState<BufBuilderType, Allocator>::flush(BufBuilderType& buffer, F controlBlockWriter) {
    visit(
        [&](auto& encoder) {
            encoder.flush(_previous().type, buffer, _controlByteOffset, controlBlockWriter);
        },
        _encoder);

    if (_controlByteOffset != kNoSimple8bControl) {
        controlBlockWriter(_controlByteOffset, buffer.len() - _controlByteOffset);
    }
}

template <class BufBuilderType, class Allocator>
boost::optional<Simple8bBuilder<uint64_t, Allocator>>
EncodingState<BufBuilderType, Allocator>::Encoder64::_tryRescalePending(int64_t encoded,
                                                                        uint8_t newScaleIndex,
                                                                        Allocator allocator) const {
    // Encode last value in the previous block with old and new scale index. We know that scaling
    // with the old index is possible.
    int64_t prev = *Simple8bTypeUtil::encodeDouble(lastValueInPrevBlock, scaleIndex);
    boost::optional<int64_t> prevRescaled =
        Simple8bTypeUtil::encodeDouble(lastValueInPrevBlock, newScaleIndex);

    // Fail if we could not rescale
    bool possible = prevRescaled.has_value();
    if (!possible)
        return boost::none;

    // Create a new Simple8bBuilder for the rescaled values. If any Simple8b block is finalized when
    // adding the new values then rescaling is less optimal than flushing with the current scale. So
    // we just record if this happens in our write callback.
    auto writeFn = [&possible](uint64_t block) {
        possible = false;
    };
    Simple8bBuilder<uint64_t, Allocator> builder{allocator};

    // Iterate over our pending values, decode them back into double, rescale and append to our new
    // Simple8b builder
    for (const auto& pending : simple8bBuilder) {
        if (!pending) {
            builder.skip(writeFn);
            continue;
        }

        // Apply delta to previous, decode to double and rescale
        prev = expandDelta(prev, Simple8bTypeUtil::decodeInt64(*pending));
        auto rescaled = Simple8bTypeUtil::encodeDouble(
            Simple8bTypeUtil::decodeDouble(prev, scaleIndex), newScaleIndex);

        // Fail if we could not rescale
        if (!rescaled || !prevRescaled)
            return boost::none;

        // Append the scaled delta
        auto appended = builder.append(
            Simple8bTypeUtil::encodeInt64(calcDelta(*rescaled, *prevRescaled)), writeFn);

        // Fail if are out of range for Simple8b or a block was written
        if (!appended || !possible)
            return boost::none;

        // Remember previous for next value
        prevRescaled = rescaled;
    }

    // Last add our new value
    auto appended =
        builder.append(Simple8bTypeUtil::encodeInt64(calcDelta(encoded, *prevRescaled)), writeFn);
    if (!appended || !possible)
        return boost::none;

    // We managed to add all re-scaled values, this will thus compress better. Set write callback to
    // our buffer writer and return
    return builder;
}

template <class BufBuilderType, class Allocator>
template <class F>
bool EncodingState<BufBuilderType, Allocator>::Encoder64::_appendDouble(
    double value,
    double previous,
    BufBuilderType& buffer,
    ptrdiff_t& controlByteOffset,
    F controlBlockWriter,
    Allocator allocator) {
    // Scale with lowest possible scale index
    auto [encoded, scale] = scaleAndEncodeDouble(value, scaleIndex);

    if (scale != scaleIndex) {
        // New value need higher scale index. We have two choices:
        // (1) Re-scale pending values to use this larger scale factor
        // (2) Flush pending and start a new block with this higher scale factor
        // We try both options and select the one that compresses best
        auto rescaled = _tryRescalePending(encoded, scale, allocator);
        if (rescaled) {
            // Re-scale possible, use this Simple8b builder
            std::swap(simple8bBuilder, *rescaled);
            prevEncoded64 = encoded;
            scaleIndex = scale;
            return true;
        }

        // Re-scale not possible, flush and start new block with the higher scale factor
        flush(NumberDouble, buffer, controlByteOffset, controlBlockWriter);
        if (controlByteOffset != kNoSimple8bControl) {
            controlBlockWriter(controlByteOffset, buffer.len() - controlByteOffset);
        }
        controlByteOffset = kNoSimple8bControl;

        // Make sure value and previous are using the same scale factor.
        uint8_t prevScaleIndex;
        std::tie(prevEncoded64, prevScaleIndex) = scaleAndEncodeDouble(previous, scale);
        if (scale != prevScaleIndex) {
            std::tie(encoded, scale) = scaleAndEncodeDouble(value, prevScaleIndex);
            std::tie(prevEncoded64, prevScaleIndex) = scaleAndEncodeDouble(previous, scale);
        }

        // Record our new scale factor
        scaleIndex = scale;
    }

    // Append delta and check if we wrote a Simple8b block. If we did we may be able to reduce the
    // scale factor when starting a new block
    auto before = buffer.len();
    if (!append(NumberDouble,
                Simple8bTypeUtil::encodeInt64(calcDelta(encoded, prevEncoded64)),
                buffer,
                controlByteOffset,
                controlBlockWriter))
        return false;

    if (buffer.len() == before) {
        prevEncoded64 = encoded;
        return true;
    }

    // Reset the scale factor to 0 and append all pending values to a new Simple8bBuilder. In
    // the worse case we will end up with an identical scale factor.
    auto prevScale = scaleIndex;
    std::tie(prevEncoded64, scaleIndex) = scaleAndEncodeDouble(lastValueInPrevBlock, 0);

    // Create a new Simple8bBuilder.
    Simple8bBuilder<uint64_t, Allocator> builder{allocator};
    builder.initializeRLEFrom(simple8bBuilder);
    std::swap(simple8bBuilder, builder);

    // Iterate over previous pending values and re-add them recursively. That will increase the
    // scale factor as needed. No need to set '_prevEncoded64' in this code path as that will be
    // done in the recursive call to '_appendDouble' below.
    auto prev = lastValueInPrevBlock;
    auto prevEncoded = *Simple8bTypeUtil::encodeDouble(prev, prevScale);
    for (const auto& pending : builder) {
        if (pending) {
            prevEncoded = expandDelta(prevEncoded, Simple8bTypeUtil::decodeInt64(*pending));
            auto val = Simple8bTypeUtil::decodeDouble(prevEncoded, prevScale);
            _appendDouble(val, prev, buffer, controlByteOffset, controlBlockWriter, allocator);
            prev = val;
        } else {
            skip(NumberDouble, buffer, controlByteOffset, controlBlockWriter);
        }
    }
    return true;
}

template <class BufBuilderType, class Allocator>
Element EncodingState<BufBuilderType, Allocator>::_previous() const {
    // The first two bytes are type and field name null terminator
    return {static_cast<BSONType>(static_cast<signed char>(*_prev.data())),
            BSONElementValue(_prev.data() + 2),
            static_cast<int>(_prev.size() - 2)};
}

template <class BufBuilderType, class Allocator>
void EncodingState<BufBuilderType, Allocator>::_storePrevious(Element elem) {
    // Add space for type byte and field name null terminator
    auto size = elem.size + 2;
    _prev.resize(size);

    // Copy element into buffer for previous. Omit field name.
    _prev[0] = elem.type;
    // Store null terminator, this byte will never change
    _prev[1] = '\0';
    memcpy(_prev.data() + 2, elem.value.value(), elem.size);
}

template <class BufBuilderType, class Allocator>
template <class F>
void EncodingState<BufBuilderType, Allocator>::_writeLiteralFromPrevious(BufBuilderType& buffer,
                                                                         F controlBlockWriter,
                                                                         Allocator allocator) {
    // Write literal without field name and reset control byte to force new one to be written when
    // appending next value.
    if (_controlByteOffset != kNoSimple8bControl) {
        controlBlockWriter(_controlByteOffset, buffer.len() - _controlByteOffset);
    }
    buffer.appendBuf(_prev.data(), _prev.size());
    controlBlockWriter(buffer.len() - _prev.size(), _prev.size());

    // Reset state
    _controlByteOffset = kNoSimple8bControl;

    _initializeFromPrevious(allocator);
}

template <class BufBuilderType, class Allocator>
void EncodingState<BufBuilderType, Allocator>::_initializeFromPrevious(Allocator allocator) {
    // Initialize previous encoded when needed
    auto previous = _previous();
    if (uses128bit(previous.type)) {
        _encoder.template emplace<Encoder128>(allocator).initialize(previous);
    } else {
        _encoder.template emplace<Encoder64>(allocator).initialize(previous);
    }
}

template <class BufBuilderType, class Allocator>
template <class F>
ptrdiff_t EncodingState<BufBuilderType, Allocator>::_incrementSimple8bCount(BufBuilderType& buffer,
                                                                            F controlBlockWriter) {
    char* byte;
    uint8_t count;
    uint8_t scaleIndex = Simple8bTypeUtil::kMemoryAsInteger;
    if (auto encoder = std::get_if<Encoder64>(&_encoder)) {
        scaleIndex = encoder->scaleIndex;
    }
    uint8_t control = kControlByteForScaleIndex[scaleIndex];

    if (_controlByteOffset == kNoSimple8bControl) {
        // Allocate new control byte if we don't already have one. Record its offset so we can find
        // it even if the underlying buffer reallocates.
        byte = buffer.skip(1);
        _controlByteOffset = std::distance(buffer.buf(), byte);
        count = 0;
    } else {
        // Read current count from previous control byte
        byte = buffer.buf() + _controlByteOffset;

        // If previous byte was written with a different control byte then we can't re-use and need
        // to start a new one
        if ((*byte & kControlMask) != control) {
            controlBlockWriter(_controlByteOffset, buffer.len() - _controlByteOffset);

            _controlByteOffset = kNoSimple8bControl;
            _incrementSimple8bCount(buffer, controlBlockWriter);
            return kNoSimple8bControl;
        }
        count = (*byte & kCountMask) + 1;
    }

    // Write back new count and clear offset if we have reached max count
    *byte = control | (count & kCountMask);
    if (count + 1 == kMaxCount) {
        auto prevControlByteOffset = _controlByteOffset;
        _controlByteOffset = kNoSimple8bControl;
        return prevControlByteOffset;
    }

    return kNoSimple8bControl;
}

template <class BufBuilderType, class Allocator>
template <class F>
void EncodingState<BufBuilderType, Allocator>::Simple8bBlockWriter128<F>::operator()(
    uint64_t block) {
    // Write/update block count
    ptrdiff_t fullControlOffset = incrementSimple8bCount(
        _buffer, _controlByteOffset, Simple8bTypeUtil::kMemoryAsInteger, _controlBlockWriter);

    // Write Simple-8b block in little endian byte order
    _buffer.appendNum(block);

    // Write control block if this Simple-8b block made it full.
    if (fullControlOffset != kNoSimple8bControl) {
        _controlBlockWriter(fullControlOffset, _buffer.len() - fullControlOffset);
    }
}

template <class BufBuilderType, class Allocator>
template <class F>
void EncodingState<BufBuilderType, Allocator>::Simple8bBlockWriter64<F>::operator()(
    uint64_t block) {
    // Write/update block count
    ptrdiff_t fullControlOffset = incrementSimple8bCount(
        _buffer, _controlByteOffset, _encoder.scaleIndex, _controlBlockWriter);

    // Write Simple-8b block in little endian byte order
    _buffer.appendNum(block);

    // Write control block if this Simple-8b block made it full.
    if (fullControlOffset != kNoSimple8bControl) {
        _controlBlockWriter(fullControlOffset, _buffer.len() - fullControlOffset);
    }

    // If we are double we need to remember the last value written in the block. There could
    // be multiple values pending still so we need to loop backwards and re-construct the
    // value before the first value in pending.
    if (_type != NumberDouble)
        return;

    auto current = _encoder.prevEncoded64;
    for (auto it = _encoder.simple8bBuilder.rbegin(), end = _encoder.simple8bBuilder.rend();
         it != end;
         ++it) {
        if (const boost::optional<uint64_t>& encoded = *it) {
            // As we're going backwards we need to 'expandDelta' backwards which is the same
            // as 'calcDelta'.
            current = calcDelta(current, Simple8bTypeUtil::decodeInt64(*encoded));
        }
    }

    _encoder.lastValueInPrevBlock = Simple8bTypeUtil::decodeDouble(current, _encoder.scaleIndex);
}

template struct EncodingState<BufBuilder, std::allocator<void>>;
template struct EncodingState<TrackedBufBuilder, TrackingAllocator<void>>;
}  // namespace bsoncolumn

template <class BufBuilderType, class BSONObjType, class Allocator>
BSONColumnBuilder<BufBuilderType, BSONObjType, Allocator>::InternalState::SubObjState::SubObjState(
    Allocator a)
    : allocator(a),
      state(allocator),
      buffer(allocator, kDefaultBufferSize),
      controlBlocks(allocator) {}

template <class BufBuilderType, class BSONObjType, class Allocator>
BSONColumnBuilder<BufBuilderType, BSONObjType, Allocator>::InternalState::SubObjState::SubObjState(
    const SubObjState& other)
    : allocator(other.allocator),
      state(other.state),
      buffer(allocator, static_cast<size_t>(other.buffer.capacity())),
      controlBlocks(other.controlBlocks) {
    buffer.appendBuf(other.buffer.buf(), other.buffer.len());
}

template <class BufBuilderType, class BSONObjType, class Allocator>
typename BSONColumnBuilder<BufBuilderType, BSONObjType, Allocator>::InternalState::SubObjState&
BSONColumnBuilder<BufBuilderType, BSONObjType, Allocator>::InternalState::SubObjState::operator=(
    const SubObjState& rhs) {
    if (&rhs == this)
        return *this;

    allocator = rhs.allocator;
    state = rhs.state;
    controlBlocks = rhs.controlBlocks;
    buffer.reset();
    buffer.appendBuf(rhs.buffer.buf(), rhs.buffer.len());
    return *this;
}

template <class BufBuilderType, class BSONObjType, class Allocator>
BSONColumnBuilder<BufBuilderType, BSONObjType, Allocator>::InternalState::SubObjState::
    InterleavedControlBlockWriter::InterleavedControlBlockWriter(
        std::vector<ControlBlock, ControlBlockAllocator>& controlBlocks)
    : _controlBlocks(controlBlocks) {}

template <class BufBuilderType, class BSONObjType, class Allocator>
void BSONColumnBuilder<BufBuilderType, BSONObjType, Allocator>::InternalState::SubObjState::
    InterleavedControlBlockWriter::operator()(ptrdiff_t controlBlockOffset, size_t size) {
    _controlBlocks.emplace_back(controlBlockOffset, size);
}

template <class BufBuilderType, class BSONObjType, class Allocator>
typename BSONColumnBuilder<BufBuilderType, BSONObjType, Allocator>::InternalState::SubObjState::
    InterleavedControlBlockWriter
    BSONColumnBuilder<BufBuilderType, BSONObjType, Allocator>::InternalState::SubObjState::
        controlBlockWriter() {
    return InterleavedControlBlockWriter(controlBlocks);
}

template <class BufBuilderType, class BSONObjType, class Allocator>
bool BSONColumnBuilder<BufBuilderType, BSONObjType, Allocator>::_appendSubElements(
    const BSONObj& obj) {
    auto& interleaved = std::get<typename InternalState::Interleaved>(_is.state);

    // Check if added object is compatible with selected reference object. Collect a flat vector of
    // all elements while we are doing this.
    std::vector<BSONElement> flattenedAppendedObj;

    auto perElement = [&flattenedAppendedObj](const BSONElement& ref, const BSONElement& elem) {
        flattenedAppendedObj.push_back(elem);
    };
    if (!traverseLockStep(interleaved.referenceSubObj.get().get(), obj, perElement)) {
        _flushSubObjMode();
        return false;
    }

    // We should have received one callback for every sub-element in reference object. This should
    // match number of encoding states setup previously.
    invariant(flattenedAppendedObj.size() == interleaved.subobjStates.size());
    auto statesIt = interleaved.subobjStates.begin();
    auto subElemIt = flattenedAppendedObj.begin();
    auto subElemEnd = flattenedAppendedObj.end();

    // Append elements to corresponding encoding state.
    for (; subElemIt != subElemEnd; ++subElemIt, ++statesIt) {
        const auto& subelem = *subElemIt;
        auto& subobj = *statesIt;
        if (!subelem.eoo())
            subobj.state.append(subelem, subobj.buffer, subobj.controlBlockWriter(), _is.allocator);
        else
            subobj.state.skip(subobj.buffer, subobj.controlBlockWriter());
    }
    return true;
}

template <class BufBuilderType, class BSONObjType, class Allocator>
void BSONColumnBuilder<BufBuilderType, BSONObjType, Allocator>::_startDetermineSubObjReference(
    const BSONObj& obj, BSONType type) {
    // Start sub-object compression. Enter DeterminingReference mode, we use this first Object
    // as the first reference
    std::get<typename InternalState::Regular>(_is.state).flush(_bufBuilder,
                                                               NoopControlBlockWriter{});

    auto& interleaved =
        _is.state.template emplace<typename InternalState::Interleaved>(_is.allocator);
    interleaved.referenceSubObj = {TrackableBSONObj{obj.getOwned()}, _is.allocator};
    interleaved.referenceSubObjType = type;
    interleaved.bufferedObjElements.emplace_back(
        TrackableBSONObj{interleaved.referenceSubObj.get().get()}, _is.allocator);
}

template <class BufBuilderType, class BSONObjType, class Allocator>
void BSONColumnBuilder<BufBuilderType, BSONObjType, Allocator>::_finishDetermineSubObjReference() {
    auto& interleaved = std::get<typename InternalState::Interleaved>(_is.state);

    // Done determining reference sub-object. Write this control byte and object to stream.
    const char interleavedStartControlByte = [&] {
        return interleaved.referenceSubObjType == Object
            ? bsoncolumn::kInterleavedStartControlByte
            : bsoncolumn::kInterleavedStartArrayRootControlByte;
    }();
    _bufBuilder.appendChar(interleavedStartControlByte);
    _bufBuilder.appendBuf(interleaved.referenceSubObj.get().get().objdata(),
                          interleaved.referenceSubObj.get().get().objsize());
    ++_numInterleavedStartWritten;

    // Initialize all encoding states. We do this by traversing in lock-step between the reference
    // object and first buffered element. We can use the fact if sub-element exists in reference to
    // determine if we should start with a zero delta or skip.
    auto perElement = [this, &interleaved](const BSONElement& ref, const BSONElement& elem) {
        // Set a valid 'previous' into the encoding state to avoid a full
        // literal to be written when we append the first element. We want this
        // to be a zero delta as the reference object already contain this
        // literal.
        interleaved.subobjStates.emplace_back(_is.allocator);
        auto& subobj = interleaved.subobjStates.back();
        subobj.state._storePrevious(ref);
        subobj.state._initializeFromPrevious(_is.allocator);
        if (!elem.eoo()) {
            subobj.state.append(elem, subobj.buffer, subobj.controlBlockWriter(), _is.allocator);
        } else {
            subobj.state.skip(subobj.buffer, subobj.controlBlockWriter());
        }
    };

    invariant(traverseLockStep(interleaved.referenceSubObj.get().get(),
                               interleaved.bufferedObjElements.front().get().get(),
                               perElement));
    interleaved.mode = InternalState::Interleaved::Mode::kAppending;

    // Append remaining buffered objects.
    auto it = interleaved.bufferedObjElements.begin() + 1;
    auto end = interleaved.bufferedObjElements.end();
    for (; it != end; ++it) {
        // The objects we append here should always be compatible with our reference object. If they
        // are not then there is a bug somewhere.
        invariant(_appendSubElements(it->get().get()));
    }
    interleaved.bufferedObjElements.clear();
}

template <class BufBuilderType, class BSONObjType, class Allocator>
void BSONColumnBuilder<BufBuilderType, BSONObjType, Allocator>::_flushSubObjMode() {
    auto& interleaved = std::get<typename InternalState::Interleaved>(_is.state);

    if (interleaved.mode == InternalState::Interleaved::Mode::kDeterminingReference) {
        _finishDetermineSubObjReference();
    }

    // Flush all EncodingStates, this will cause them to write out all their elements that is
    // captured by the controlBlockWriter.
    for (auto&& subobj : interleaved.subobjStates) {
        subobj.state.flush(subobj.buffer, subobj.controlBlockWriter());
    }

    // We now need to write all control blocks to the binary stream in the right order. This is done
    // in the decoder's perspective where a DecodingState that exhausts its elements will read the
    // next control byte. We can use a min-heap to see which encoding states have written the fewest
    // elements so far. In case of tie we use the smallest encoder/decoder index.
    struct HeapElement {
        HeapElement(uint32_t index) : encoderIndex(index) {}

        uint32_t numElementsWritten = 0;
        uint32_t encoderIndex;
        uint32_t controlBlockIndex = 0;

        bool operator>(const HeapElement& rhs) const {
            // Implement operator using std::pair
            return std::tie(numElementsWritten, encoderIndex) >
                std::tie(rhs.numElementsWritten, rhs.encoderIndex);
        }
    };
    std::vector<HeapElement> heap;
    for (uint32_t i = 0; i < interleaved.subobjStates.size(); ++i) {
        heap.emplace_back(i);
    }

    // Initialize as min-heap
    std::make_heap(heap.begin(), heap.end(), std::greater<>{});

    // Append all control blocks
    while (!heap.empty()) {
        // Take out encoding state with fewest elements written from heap
        std::pop_heap(heap.begin(), heap.end(), std::greater<>{});
        // And we take out control blocks in FIFO order from this encoding state
        auto& top = heap.back();
        auto& slot = interleaved.subobjStates[top.encoderIndex];
        const char* controlBlock =
            slot.buffer.buf() + slot.controlBlocks.at(top.controlBlockIndex).first;
        size_t size = slot.controlBlocks.at(top.controlBlockIndex).second;

        // Write it to the buffer
        _bufBuilder.appendBuf(controlBlock, size);
        ++top.controlBlockIndex;
        if (top.controlBlockIndex == slot.controlBlocks.size()) {
            // No more control blocks for this encoding state so remove it from the heap
            heap.pop_back();
            continue;
        }

        // Calculate how many elements were in this control block
        uint32_t elems = bsoncolumn::numElemsForControlByte(controlBlock);

        // Append num elements and put this encoding state back into the heap.
        top.numElementsWritten += elems;
        std::push_heap(heap.begin(), heap.end(), std::greater<>{});
    }
    // All control blocks written, write EOO to end the interleaving and cleanup.
    _bufBuilder.appendChar(EOO);
    _is.state.template emplace<typename InternalState::Regular>(_is.allocator);
}

template <class BufBuilderType, class BSONObjType, class Allocator>
bool BSONColumnBuilder<BufBuilderType, BSONObjType, Allocator>::isInternalStateIdentical(
    const BSONColumnBuilder& other) const {
    auto areBufBuildersIdentical = [](const BufBuilderType& bufBuilder,
                                      const BufBuilderType& otherBufBuilder) {
        if (bufBuilder.len() != otherBufBuilder.len()) {
            return false;
        }

        if (bufBuilder.len() > 0 &&
            std::memcmp(bufBuilder.buf(), otherBufBuilder.buf(), bufBuilder.len()) != 0) {
            return false;
        }

        return true;
    };

    if (!areBufBuildersIdentical(_bufBuilder, other._bufBuilder)) {
        return false;
    }

    // Validate intermediate data
    if (_is.offset != other._is.offset) {
        return false;
    }
    if (_is.lastBufLength != other._is.lastBufLength) {
        return false;
    }
    if (_is.lastControl != other._is.lastControl) {
        return false;
    }

    if (_is.state.index() != other._is.state.index()) {
        return false;
    }

    auto areEncodingStatesIdentical =
        [](const bsoncolumn::EncodingState<BufBuilderType, Allocator>& encodingState,
           const bsoncolumn::EncodingState<BufBuilderType, Allocator>& otherEncodingState) {
            if (encodingState._controlByteOffset != otherEncodingState._controlByteOffset) {
                return false;
            }

            if (encodingState._prev != otherEncodingState._prev) {
                return false;
            }

            if (encodingState._encoder.index() != otherEncodingState._encoder.index()) {
                return false;
            }

            return visit(OverloadedVisitor{
                             [&](const Encoder64& encoder) {
                                 auto& otherEncoder =
                                     std::get<Encoder64>(otherEncodingState._encoder);

                                 if (encoder.scaleIndex != otherEncoder.scaleIndex) {
                                     return false;
                                 }

                                 // NaN does not compare equal to itself, so we bit cast and perform
                                 // this comparison as interger
                                 if (absl::bit_cast<uint64_t>(encoder.lastValueInPrevBlock) !=
                                     absl::bit_cast<uint64_t>(otherEncoder.lastValueInPrevBlock)) {
                                     return false;
                                 }

                                 if (encoder.prevDelta != otherEncoder.prevDelta) {
                                     return false;
                                 }

                                 if (encoder.prevEncoded64 != otherEncoder.prevEncoded64) {
                                     return false;
                                 }

                                 return encoder.simple8bBuilder.isInternalStateIdentical(
                                     otherEncoder.simple8bBuilder);
                             },
                             [&](const Encoder128& encoder) {
                                 auto& otherEncoder =
                                     std::get<Encoder128>(otherEncodingState._encoder);

                                 if (encoder.prevEncoded128 != otherEncoder.prevEncoded128) {
                                     return false;
                                 }

                                 return encoder.simple8bBuilder.isInternalStateIdentical(
                                     otherEncoder.simple8bBuilder);
                             },
                         },
                         encodingState._encoder);
        };

    return visit(
        OverloadedVisitor{
            [&](const typename InternalState::Regular& regular) {
                return areEncodingStatesIdentical(
                    regular, std::get<typename InternalState::Regular>(other._is.state));
            },
            [&](const typename InternalState::Interleaved& interleaved) {
                auto& otherInterleaved =
                    std::get<typename InternalState::Interleaved>(other._is.state);

                if (interleaved.mode != otherInterleaved.mode) {
                    return false;
                }

                if (interleaved.subobjStates.size() != otherInterleaved.subobjStates.size()) {
                    return false;
                }

                for (size_t i = 0; i < interleaved.subobjStates.size(); ++i) {
                    auto& subObjState = interleaved.subobjStates[i];
                    auto& otherSubObjState = otherInterleaved.subobjStates[i];

                    if (!areEncodingStatesIdentical(subObjState.state, otherSubObjState.state)) {
                        return false;
                    }

                    if (!areBufBuildersIdentical(subObjState.buffer, otherSubObjState.buffer)) {
                        return false;
                    }

                    if (subObjState.controlBlocks != otherSubObjState.controlBlocks) {
                        return false;
                    }
                }

                if (!interleaved.referenceSubObj.get().get().binaryEqual(
                        otherInterleaved.referenceSubObj.get().get())) {
                    return false;
                }

                if (interleaved.referenceSubObjType != otherInterleaved.referenceSubObjType) {
                    return false;
                }

                if (interleaved.bufferedObjElements.size() !=
                    otherInterleaved.bufferedObjElements.size()) {
                    return false;
                }

                for (size_t i = 0; i < interleaved.bufferedObjElements.size(); ++i) {
                    auto& bufferedObjElement = interleaved.bufferedObjElements[i];
                    auto& otherBufferedObjElement = otherInterleaved.bufferedObjElements[i];

                    if (!bufferedObjElement.get().get().binaryEqual(
                            otherBufferedObjElement.get().get())) {
                        return false;
                    }
                }

                return true;
            },
        },
        _is.state);
}

template class BSONColumnBuilder<UntrackedBufBuilder, UntrackedBSONObj, std::allocator<void>>;
template class BSONColumnBuilder<TrackedBufBuilder, TrackedBSONObj, TrackingAllocator<void>>;

}  // namespace mongo
