/**
 *    Copyright (C) 2024-present MongoDB, Inc.
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

#include <algorithm>

#include "mongo/bson/util/bsoncolumn_helpers.h"

namespace mongo::bsoncolumn {

/**
 * We are often dealing with vectors of buffers below, but there is almost always only one buffer.
 */
template <typename T>
using BufferVector = boost::container::small_vector<T, 1>;

/**
 * Helper class that will append a sub-object to a buffer once it's complete.
 */
template <typename Buffer>
struct BlockBasedSubObjectFinisher {
    BlockBasedSubObjectFinisher(const BufferVector<Buffer*>& buffers) : _buffers(buffers) {}
    void finish(const char* elemBytes, int fieldNameSize, int totalSize);

    const BufferVector<Buffer*>& _buffers;
};

/**
 * A helper class for block-based decompression of object data.
 */
template <class CMaterializer>
class BlockBasedInterleavedDecompressor {
public:
    /**
     * One instance of this class will decompress an interleaved block that begins at "control."
     * Parameter "end" should point to the byte after the end of the BSONColumn data, used for
     * sanity checks.
     */
    BlockBasedInterleavedDecompressor(ElementStorage& allocator,
                                      const char* control,
                                      const char* end);

    /**
     * Decompresses interleaved data where data at a given path is sent to the corresonding buffer.
     * Returns a pointer to the next byte after the EOO that ends the interleaved data.
     */
    template <typename Path, typename Buffer>
    const char* decompress(std::vector<std::pair<Path, Buffer>>& paths);

private:
    struct DecodingState;

    template <typename Buffer>
    struct FastDecodingState;

    template <typename Buffer>
    const char* decompressGeneral(
        absl::flat_hash_map<const void*, BufferVector<Buffer*>>&& elemToBuffer);

    template <typename T, typename Encoding, class Buffer, typename Materialize, typename Finish>
    static void decompressAllDelta(const char* ptr,
                                   const char* end,
                                   Buffer& buffer,
                                   Encoding last,
                                   const BSONElement& reference,
                                   const Materialize& materialize,
                                   const Finish& finish);

    template <typename Buffer>
    const char* decompressFast(
        absl::flat_hash_map<const void*, BufferVector<Buffer*>>&& elemToBuffer);

    BSONElement writeToElementStorage(typename DecodingState::Elem elem, StringData fieldName);

    template <class Buffer>
    static void appendToBuffers(BufferVector<Buffer*>& buffers, typename DecodingState::Elem elem);

    template <typename Buffer, typename T>
    static void appendEncodedToBuffers(BufferVector<Buffer*>& buffers, int64_t encoded) {
        for (auto&& b : buffers) {
            b->append(static_cast<T>(encoded));
        }
    }

    ElementStorage& _allocator;
    const char* const _control;
    const char* const _end;
    const BSONType _rootType;
    const bool _traverseArrays;
};

template <typename Buffer>
void BlockBasedSubObjectFinisher<Buffer>::finish(const char* elemBytes,
                                                 int fieldNameSize,
                                                 int totalSize) {
    BSONElement elem{elemBytes, fieldNameSize, totalSize, BSONElement::TrustedInitTag{}};
    for (auto&& buffer : _buffers) {
        // use preallocated method here to indicate that the element does not need to be
        // copied to longer-lived memory.
        buffer->appendPreallocated(elem);
    }
}

template <class CMaterializer>
BlockBasedInterleavedDecompressor<CMaterializer>::BlockBasedInterleavedDecompressor(
    ElementStorage& allocator, const char* control, const char* end)
    : _allocator(allocator),
      _control(control),
      _end(end),
      _rootType(*control == bsoncolumn::kInterleavedStartArrayRootControlByte ? Array : Object),
      _traverseArrays(*control == bsoncolumn::kInterleavedStartControlByte ||
                      *control == bsoncolumn::kInterleavedStartArrayRootControlByte) {
    invariant(bsoncolumn::isInterleavedStartControlByte(*control),
              "request to do interleaved decompression on non-interleaved data");
}

/**
 * Decompresses interleaved data where data at a given path is sent to the corresonding buffer.
 * Returns a pointer to the next byte after the EOO that ends the interleaved data.
 */
template <class CMaterializer>
template <typename Path, typename Buffer>
const char* BlockBasedInterleavedDecompressor<CMaterializer>::decompress(
    std::vector<std::pair<Path, Buffer>>& paths) {

    // The reference object will appear right after the control byte that starts interleaved
    // mode.
    BSONObj refObj{_control + 1};

    // find all the scalar elements in the reference object.
    absl::flat_hash_set<const void*> scalarElems;
    {
        BSONObjTraversal findScalar{
            _traverseArrays,
            _rootType,
            [](StringData fieldName, const BSONObj& obj, BSONType type) { return true; },
            [&scalarElems](const BSONElement& elem) {
                scalarElems.insert(elem.value());
                // keep traversing to find every scalar field.
                return true;
            }};
        findScalar.traverse(refObj);
    }

    // For each path, we can use a fast implementation if it just decompresses a single
    // scalar field to a buffer.
    absl::flat_hash_map<const void*, BufferVector<Buffer*>> elemToBufferFast;
    absl::flat_hash_map<const void*, BufferVector<Buffer*>> elemToBufferGeneral;
    for (auto&& path : paths) {
        auto elems = path.first.elementsToMaterialize(refObj);
        if (elems.size() == 1 && scalarElems.contains(elems[0])) {
            elemToBufferFast[elems[0]].push_back(&path.second);
        } else {
            for (const void* valueAddr : elems) {
                elemToBufferGeneral[valueAddr].push_back(&path.second);
            }
        }
    }

    // If there were any paths that needed to use the general pass, then do that now.
    const char* newGeneralControl = nullptr;
    if (!elemToBufferGeneral.empty()) {
        newGeneralControl = decompressGeneral(std::move(elemToBufferGeneral));
    }

    // There are now a couple possibilities:
    // - There are paths that use the fast implementation. In that case, do so.
    // - All the paths produce zero elements for this reference object (i.e., paths requesting a
    //   field that does not exist). In that case call decompressFast() with the empty hash map
    //   purely to advance to the next control byte.
    const char* newFastControl = nullptr;
    if (!elemToBufferFast.empty() || newGeneralControl == nullptr) {
        newFastControl = decompressFast(std::move(elemToBufferFast));
    }

    // We need to have taken either the general or the fast path, in order to tell the caller where
    // the interleaved data ends.
    invariant(newGeneralControl != nullptr || newFastControl != nullptr,
              "either the general or fast impl must have been used");

    // Ensure that if we had paths for both the fast and general case that the location of the new
    // control byte is the same.
    invariant(newGeneralControl == nullptr || newFastControl == nullptr ||
                  newGeneralControl == newFastControl,
              "fast impl and general impl control byte location does not agree");

    // In either case, we return a pointer to the byte after the EOO that ends interleaved mode.
    return newFastControl == nullptr ? newGeneralControl : newFastControl;
}

/**
 * Decompresses interleaved data that starts at "control", with data at a given path sent to the
 * corresonding buffer. Returns a pointer to the next byte after the interleaved data.
 */
template <class CMaterializer>
template <typename Buffer>
const char* BlockBasedInterleavedDecompressor<CMaterializer>::decompressGeneral(
    absl::flat_hash_map<const void*, BufferVector<Buffer*>>&& elemToBuffer) {
    const char* control = _control;

    // The reference object will appear right after the control byte that starts interleaved
    // mode.
    BSONObj refObj{control + 1};

    // A vector that maps the ordinal position of the pre-order traversal of the reference
    // object to the buffers where that element should be materialized. The length of the vector
    // will be the same as the number of elements in the reference object, with empty vectors
    // for those elements that aren't being materialized.
    //
    // Use BufferVector, which is optimized for one element, because there will almost always be
    // just one buffer.
    std::vector<BufferVector<Buffer*>> posToBuffers;

    // Decoding states for each scalar field appearing in the refence object, in pre-order
    // traversal order.
    std::vector<DecodingState> decoderStates;

    {
        BSONObjTraversal trInit{
            _traverseArrays,
            _rootType,
            [&](StringData fieldName, const BSONObj& obj, BSONType type) {
                if (auto it = elemToBuffer.find(obj.objdata()); it != elemToBuffer.end()) {
                    posToBuffers.push_back(std::move(it->second));
                } else {
                    // An empty list to indicate that this element isn't being materialized.
                    posToBuffers.push_back({});
                }

                return true;
            },
            [&](const BSONElement& elem) {
                decoderStates.emplace_back();
                decoderStates.back().loadUncompressed(elem);
                if (auto it = elemToBuffer.find(elem.value()); it != elemToBuffer.end()) {
                    posToBuffers.push_back(std::move(it->second));
                } else {
                    // An empty list to indicate that this element isn't being materialized.
                    posToBuffers.push_back({});
                }
                return true;
            }};
        trInit.traverse(refObj);
    }

    // Advance past the reference object to the compressed data of the first field.
    control += refObj.objsize() + 1;
    uassert(8625732, "Invalid BSON Column encoding", _control < _end);

    using SOAlloc = SubObjectAllocator<BlockBasedSubObjectFinisher<Buffer>>;
    using OptionalSOAlloc = boost::optional<SOAlloc>;
    static_assert(std::is_move_constructible<OptionalSOAlloc>::value,
                  "must be able to move a sub-object allocator to ensure that RAII properties "
                  "are followed");

    /*
     * Each traversal of the reference object can potentially produce a value for each path
     * passed in by the caller. For the root object or sub-objects that are to be materialized,
     * we create an instance of SubObjectAllocator to create the object.
     */
    int scalarIdx = 0;
    int nodeIdx = 0;
    BSONObjTraversal trDecompress{
        _traverseArrays,
        _rootType,
        [&](StringData fieldName, const BSONObj& obj, BSONType type) -> OptionalSOAlloc {
            auto& buffers = posToBuffers[nodeIdx];
            ++nodeIdx;

            if (!buffers.empty() || _allocator.contiguousEnabled()) {
                // If we have already entered contiguous mode, but there are buffers
                // corresponding to this subobject, that means caller has requested nested
                // paths, e.g., "a" and "a.b".
                //
                // TODO(SERVER-86220): Nested paths dosn't seem like they would be common, but
                // we should be able to handle it.
                invariant(buffers.empty() || !_allocator.contiguousEnabled(),
                          "decompressing paths with a nested relationship is not yet supported");

                // Either caller requested that this sub-object be materialized to a
                // container, or we are already materializing this object because it is
                // contained by such a sub-object.
                return SOAlloc(
                    _allocator, fieldName, obj, type, BlockBasedSubObjectFinisher{buffers});
            }

            return boost::none;
        },
        [&](const BSONElement& referenceField) {
            auto& state = decoderStates[scalarIdx];
            ++scalarIdx;

            auto& buffers = posToBuffers[nodeIdx];
            ++nodeIdx;

            invariant((std::holds_alternative<typename DecodingState::Decoder64>(state.decoder)),
                      "only supporting 64-bit encoding for now");
            auto& d64 = std::get<typename DecodingState::Decoder64>(state.decoder);

            // Get the next element for this scalar field.
            typename DecodingState::Elem decodingStateElem;
            if (d64.pos.valid() && (++d64.pos).more()) {
                // We have an iterator into a block of deltas
                decodingStateElem = state.loadDelta(_allocator, d64);
            } else if (*control == EOO) {
                // End of interleaved mode. Stop object traversal early by returning false.
                return false;
            } else {
                // No more deltas for this scalar field. The next control byte is guaranteed
                // to belong to this scalar field, since traversal order is fixed.
                auto result = state.loadControl(_allocator, control);
                control += result.size;
                uassert(8625731, "Invalid BSON Column encoding", _control < _end);
                decodingStateElem = result.element;
            }

            // If caller has requested materialization of this field, do it.
            if (_allocator.contiguousEnabled()) {
                // TODO(SERVER-86220): Nested paths dosn't seem like they would be common, but
                // we should be able to handle it.
                invariant(buffers.empty(),
                          "decompressing paths with a nested relationship is not yet supported");

                // We must write a BSONElement to ElementStorage since this scalar is part
                // of an object being materialized.
                BSONElement elem =
                    writeToElementStorage(decodingStateElem, referenceField.fieldNameStringData());
            } else if (buffers.size() > 0) {
                appendToBuffers(buffers, decodingStateElem);
            }

            return true;
        }};

    bool more = true;
    while (more || *control != EOO) {
        scalarIdx = 0;
        nodeIdx = 0;
        more = trDecompress.traverse(refObj);
    }

    // Advance past the EOO that ends interleaved mode.
    ++control;
    return control;
}

/**
 * TODO: This code cloned and modified from bsoncolumn.inl. We should be sharing this code.
 */
template <typename CMaterializer>
template <typename T, typename Encoding, class Buffer, typename Materialize, typename Finish>
void BlockBasedInterleavedDecompressor<CMaterializer>::decompressAllDelta(
    const char* ptr,
    const char* end,
    Buffer& buffer,
    Encoding last,
    const BSONElement& reference,
    const Materialize& materialize,
    const Finish& finish) {
    size_t elemCount = 0;
    uint8_t size = numSimple8bBlocksForControlByte(*ptr) * sizeof(uint64_t);
    Simple8b<make_unsigned_t<Encoding>> s8b(ptr + 1, size);

    auto it = s8b.begin();
    // process all copies of the reference object efficiently
    // this can otherwise get more complicated on string/binary types
    for (; it != s8b.end(); ++it) {
        const auto& delta = *it;
        if (delta) {
            if (*delta == 0) {
                buffer.template append<T>(reference);
                ++elemCount;
            } else {
                break;
            }
        } else {
            buffer.appendMissing();
            ++elemCount;
        }
    }

    for (; it != s8b.end(); ++it) {
        const auto& delta = *it;
        if (delta) {
            last =
                expandDelta(last, Simple8bTypeUtil::decodeInt<make_unsigned_t<Encoding>>(*delta));
            materialize(last, reference, buffer);
            ++elemCount;
        } else {
            buffer.appendMissing();
            ++elemCount;
        }
    }

    finish(elemCount, last);
}

/**
 * A data structure that tracks the state of a stream of scalars that appears in interleaved
 * BSONColumn data. This structure is used with a min heap to understand which bits of compressed
 * data belong to which stream.
 */
template <typename CMaterializer>
template <typename Buffer>
struct BlockBasedInterleavedDecompressor<CMaterializer>::FastDecodingState {

    FastDecodingState(size_t fieldPos,
                      const BSONElement& refElem,
                      BufferVector<Buffer*>&& buffers = {})
        : _valueCount(0), _fieldPos(fieldPos), _refElem(refElem), _buffers(std::move(buffers)) {}


    // The number of values seen so far by this stream.
    size_t _valueCount;

    // The ordinal position in the reference object to which this stream corresponds.
    size_t _fieldPos;

    // The most recent uncompressed element for this stream.
    BSONElement _refElem;

    // The list of buffers to which this stream must be materialized. For streams that aren't
    // materialized, this will be empty.
    BufferVector<Buffer*> _buffers;

    // The last uncompressed value for this stream. The delta is applied against this to compute a
    // new uncompressed value.
    std::variant<int64_t, int128_t> _lastValue;

    // Given the current reference element, set _lastValue.
    void setLastValueFromBSONElem() {
        switch (_refElem.type()) {
            case Bool:
                _lastValue.emplace<int64_t>(_refElem.boolean());
                break;
            case NumberInt:
                _lastValue.emplace<int64_t>(_refElem._numberInt());
                break;
            case NumberLong:
                _lastValue.emplace<int64_t>(_refElem._numberLong());
                break;
            default:
                invariant(false, "unsupported type");
        }
    }

    bool operator>(const FastDecodingState& other) {
        return std::tie(_valueCount, _fieldPos) > std::tie(other._valueCount, other._fieldPos);
    }
};

/**
 * The fast path for those paths that are only materializing a single scalar field.
 */
template <class CMaterializer>
template <typename Buffer>
const char* BlockBasedInterleavedDecompressor<CMaterializer>::decompressFast(
    absl::flat_hash_map<const void*, BufferVector<Buffer*>>&& elemToBuffer) {
    const char* control = _control;

    // The reference object will appear right after the control byte that starts interleaved
    // mode.
    BSONObj refObj{control + 1};
    control += refObj.objsize() + 1;
    uassert(8625730, "Invalid BSON Column encoding", _control < _end);

    /**
     * The code below uses std::make_heap(), etc such that the element at the top of the heap always
     * represents the stream assigned to the next control byte. The stream that has processed the
     * fewest number of elements will be at the top, with the stream's ordinal position in the
     * reference object used to break ties.
     *
     * For streams that are being materialized to a buffer, we materialize uncompressed elements as
     * well as expanded elements produced by simple8b blocks.
     *
     * For streams that are not being materialized, when we encounter simpl8b blocks, we create a
     * simple8b iterator and advance it in a tight loop just to count the number of elements in the
     * stream.
     */

    // Initialize a vector of states.
    std::vector<FastDecodingState<Buffer>> heap;
    size_t scalarIdx = 0;
    BSONObjTraversal trInit{
        _traverseArrays,
        _rootType,
        [&](StringData fieldName, const BSONObj& obj, BSONType type) { return true; },
        [&](const BSONElement& elem) {
            if (auto it = elemToBuffer.find(elem.value()); it != elemToBuffer.end()) {
                heap.emplace_back(scalarIdx, elem, std::move(it->second));
            } else {
                heap.emplace_back(scalarIdx, elem);
            }
            heap.back().setLastValueFromBSONElem();
            ++scalarIdx;
            return true;
        },
    };
    trInit.traverse(refObj);

    // Use greater() so that we have a min heap, i.e., the streams that have processed the fewest
    // elements are at the top.
    std::make_heap(heap.begin(), heap.end(), std::greater<>());

    // Iterate over the control bytes that appear in this section of interleaved data.
    while (*control != EOO) {
        std::pop_heap(heap.begin(), heap.end(), std::greater<>());
        FastDecodingState<Buffer>& state = heap.back();
        if (isUncompressedLiteralControlByte(*control)) {
            state._refElem = BSONElement{control, 1, -1};
            for (auto&& b : state._buffers) {
                b->template append<BSONElement>(state._refElem);
            }
            state.setLastValueFromBSONElem();
            ++state._valueCount;
            control += state._refElem.size();
        } else {
            uint8_t size = numSimple8bBlocksForControlByte(*control) * sizeof(uint64_t);
            if (state._buffers.empty()) {
                // simple8b blocks for a stream that we are not materializing. Just skip over the
                // deltas, keeping track of how many elements there were.
                state._valueCount += numElemsForControlByte(control);
            } else {
                // simple8b blocks for a stream that we are materializing.
                auto finish64 = [&state](size_t valueCount, int64_t lastValue) {
                    state._valueCount += valueCount;
                    state._lastValue.template emplace<int64_t>(lastValue);
                };
                switch (state._refElem.type()) {
                    case Bool:
                        for (auto&& buffer : state._buffers) {
                            decompressAllDelta<bool, int64_t, Buffer>(
                                control,
                                control + size + 1,
                                *buffer,
                                std::get<int64_t>(state._lastValue),
                                state._refElem,
                                [](const int64_t v, const BSONElement& ref, Buffer& buffer) {
                                    buffer.append(static_cast<bool>(v));
                                },
                                finish64);
                        }
                        break;
                    case NumberInt:
                        for (auto&& buffer : state._buffers) {
                            decompressAllDelta<int32_t, int64_t, Buffer>(
                                control,
                                control + size + 1,
                                *buffer,
                                std::get<int64_t>(state._lastValue),
                                state._refElem,
                                [](const int64_t v, const BSONElement& ref, Buffer& buffer) {
                                    buffer.append(static_cast<int32_t>(v));
                                },
                                finish64);
                        }
                        break;
                    case NumberLong:
                        for (auto&& buffer : state._buffers) {
                            decompressAllDelta<int64_t, int64_t, Buffer>(
                                control,
                                control + size + 1,
                                *buffer,
                                std::get<int64_t>(state._lastValue),
                                state._refElem,
                                [](const int64_t v, const BSONElement& ref, Buffer& buffer) {
                                    buffer.append(v);
                                },
                                finish64);
                        }
                        break;
                    default:
                        invariant(false, "unsupported type");
                }
            }
            control += (1 + size);
        }
        std::push_heap(heap.begin(), heap.end(), std::greater<>());
    }

    // Advance past the EOO that ends interleaved mode.
    ++control;
    return control;
}

/**
 * Given an element that is being materialized as part of a sub-object, write it to the allocator as
 * a BSONElement with the appropriate field name.
 */
template <class CMaterializer>
BSONElement BlockBasedInterleavedDecompressor<CMaterializer>::writeToElementStorage(
    typename DecodingState::Elem elem, StringData fieldName) {
    return visit(OverloadedVisitor{
                     [&](BSONElement& bsonElem) {
                         ElementStorage::Element esElem =
                             _allocator.allocate(bsonElem.type(), fieldName, bsonElem.valuesize());
                         memcpy(esElem.value(), bsonElem.value(), bsonElem.valuesize());
                         return esElem.element();
                     },
                     [&](std::pair<BSONType, int64_t> elem) {
                         switch (elem.first) {
                             case NumberInt: {
                                 ElementStorage::Element esElem =
                                     _allocator.allocate(elem.first, fieldName, 4);
                                 DataView(esElem.value()).write<LittleEndian<int32_t>>(elem.second);
                                 return esElem.element();
                             } break;
                             case NumberLong: {
                                 ElementStorage::Element esElem =
                                     _allocator.allocate(elem.first, fieldName, 8);
                                 DataView(esElem.value()).write<LittleEndian<int64_t>>(elem.second);
                                 return esElem.element();
                             } break;
                             case Bool: {
                                 ElementStorage::Element esElem =
                                     _allocator.allocate(elem.first, fieldName, 1);
                                 DataView(esElem.value()).write<LittleEndian<bool>>(elem.second);
                                 return esElem.element();
                             } break;
                             default:
                                 invariant(false, "attempt to materialize unsupported type");
                         }
                         return BSONElement{};
                     },
                     [&](std::pair<BSONType, int128_t>) {
                         invariant(false, "tried to materialize a 128-bit type");
                         return BSONElement{};
                     },
                 },
                 elem);
}

template <class CMaterializer>
template <class Buffer>
void BlockBasedInterleavedDecompressor<CMaterializer>::appendToBuffers(
    BufferVector<Buffer*>& buffers, typename DecodingState::Elem elem) {
    visit(OverloadedVisitor{
              [&](BSONElement& bsonElem) {
                  if (bsonElem.eoo()) {
                      for (auto&& b : buffers) {
                          b->appendMissing();
                      }
                  } else {
                      for (auto&& b : buffers) {
                          b->template append<BSONElement>(bsonElem);
                      }
                  }
              },
              [&](std::pair<BSONType, int64_t>& encoded) {
                  switch (encoded.first) {
                      case NumberLong:
                          appendEncodedToBuffers<Buffer, int64_t>(buffers, encoded.second);
                          break;
                      case NumberInt:
                          appendEncodedToBuffers<Buffer, int32_t>(buffers, encoded.second);
                          break;
                      case Bool:
                          appendEncodedToBuffers<Buffer, bool>(buffers, encoded.second);
                          break;
                      default:
                          invariant(false, "unsupported encoded data type");
                  }
              },
              [&](std::pair<BSONType, int128_t>& encoded) {
                  invariant(false, "128-bit encoded types not supported yet");
              },
          },
          elem);
}

/**
 * Decoding state for a stream of values corresponding to a scalar field.
 */
template <class CMaterializer>
struct BlockBasedInterleavedDecompressor<CMaterializer>::DecodingState {

    /**
     * A tagged union type representing values decompressed from BSONColumn bytes. This can a
     * BSONElement if the value appeared uncompressed, or it can be an encoded representation
     * that was computed from a delta.
     */
    using Elem =
        std::variant<BSONElement, std::pair<BSONType, int64_t>, std::pair<BSONType, int128_t>>;

    /**
     * State when decoding deltas for 64-bit values.
     */
    struct Decoder64 {
        boost::optional<int64_t> lastEncodedValue;
        Simple8b<uint64_t>::Iterator pos;
    };

    /**
     * State when decoding deltas for 128-bit values. (TBD)
     */
    struct Decoder128 {};

    /**
     * Initializes a decoder given an uncompressed BSONElement in the BSONColumn bytes.
     */
    void loadUncompressed(const BSONElement& elem) {
        BSONType type = elem.type();
        invariant(!uses128bit(type));
        invariant(!usesDeltaOfDelta(type));
        auto& d64 = decoder.template emplace<Decoder64>();
        switch (type) {
            case Bool:
                d64.lastEncodedValue = elem.boolean();
                break;
            case NumberInt:
                d64.lastEncodedValue = elem._numberInt();
                break;
            case NumberLong:
                d64.lastEncodedValue = elem._numberLong();
                break;
            default:
                invariant(false, "unsupported type");
        }

        _lastLiteral = elem;
    }

    struct LoadControlResult {
        Elem element;
        int size;
    };

    /**
     * Assuming that buffer points at the next control byte, takes the appropriate action:
     * - If the control byte begins an uncompressed literal: initializes a decoder, and returns
     *   the literal.
     * - If the control byte precedes blocks of deltas, applies the first delta and returns the
     *   new expanded element.
     * In both cases, the "size" field will contain the number of bytes to the next control
     * byte.
     */
    LoadControlResult loadControl(ElementStorage& allocator, const char* buffer) {
        uint8_t control = *buffer;
        if (isUncompressedLiteralControlByte(control)) {
            BSONElement literalElem(buffer, 1, -1);
            return {literalElem, literalElem.size()};
        }

        uint8_t blocks = numSimple8bBlocksForControlByte(control);
        int size = sizeof(uint64_t) * blocks;

        auto& d64 = std::get<DecodingState::Decoder64>(decoder);
        // We can read the last known value from the decoder iterator even as it has
        // reached end.
        boost::optional<uint64_t> lastSimple8bValue = d64.pos.valid() ? *d64.pos : 0;
        d64.pos = Simple8b<uint64_t>(buffer + 1, size, lastSimple8bValue).begin();
        Elem deltaElem = loadDelta(allocator, d64);
        return LoadControlResult{deltaElem, size + 1};
    }

    /**
     * Apply a delta to an encoded representation to get a new element value. May also apply a 0
     * delta to an uncompressed literal, simply returning the literal.
     */
    Elem loadDelta(ElementStorage& allocator, Decoder64& d64) {
        invariant(d64.pos.valid());
        const auto& delta = *d64.pos;
        if (!delta) {
            // boost::none represents skip, just return an EOO BSONElement.
            return BSONElement{};
        }

        // Note: delta-of-delta not handled here yet.
        if (*delta == 0) {
            // If we have an encoded representation of the last value, return it.
            if (d64.lastEncodedValue) {
                return std::pair{_lastLiteral.type(), *d64.lastEncodedValue};
            }
            // Otherwise return the last uncompressed value we found.
            return _lastLiteral;
        }

        uassert(8625729,
                "attempt to expand delta for type that does not have encoded representation",
                d64.lastEncodedValue);
        d64.lastEncodedValue =
            expandDelta(*d64.lastEncodedValue, Simple8bTypeUtil::decodeInt64(*delta));

        return std::pair{_lastLiteral.type(), *d64.lastEncodedValue};
    }

    /**
     * The last uncompressed literal from the BSONColumn bytes.
     */
    BSONElement _lastLiteral;

    /**
     * 64- or 128-bit specific state.
     */
    std::variant<Decoder64, Decoder128> decoder = Decoder64{};
};
}  // namespace mongo::bsoncolumn
