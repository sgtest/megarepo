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

#include <array>
#include <benchmark/benchmark.h>
#include <cmath>
#include <cstdint>
#include <fmt/format.h>
#include <random>
#include <string>
#include <vector>

#include "mongo/base/string_data.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/bson/bsontypes.h"
#include "mongo/bson/oid.h"
#include "mongo/bson/timestamp.h"
#include "mongo/bson/util/bsoncolumn.h"
#include "mongo/bson/util/bsoncolumnbuilder.h"
#include "mongo/db/exec/sbe/values/bsoncolumn_materializer.h"
#include "mongo/util/base64.h"
#include "mongo/util/time_support.h"

namespace mongo {
namespace {

enum DecompressMode { kIterator, kBlockBSON, kBlockSBE };

// Simple class that defines a 'Container' for the block-based decompression API. The insert
// function is a no-op. The block-based API pushes the materialized elements into a container, and
// thus does more work than the iterator API. This container allows us to fairly compare the
// runtimes of both implementations, since inserts are no-ops.

template <typename T>
class NoOpContainerForTest {
public:
    NoOpContainerForTest() {}
    ~NoOpContainerForTest() {}

    // Increment the counter to return the number of elements processed.
    void insert(int pos, const T& element) {
        _size++;
        return;
    }

    int size() {
        return _size;
    }

    // Called by the Collector class. Means nothing in this case, since we do not insert elements
    // into this container, but we must return something.
    int end() {
        return 0;
    }

private:
    int _size = 0;
};

std::mt19937_64 seedGen(1337);

std::vector<BSONObj> generateIntegers(int num, int skipPercentage) {
    std::mt19937 gen(seedGen());
    std::normal_distribution<> d(100, 10);
    std::uniform_int_distribution skip(1, 100);

    std::vector<BSONObj> ints;

    for (int i = 0; i < num; ++i) {
        if (skip(gen) <= skipPercentage) {
            ints.push_back(BSONObj());
        } else {
            BSONObjBuilder builder;
            int32_t value = std::lround(d(gen));
            builder.append(""_sd, value);
            ints.push_back(builder.obj());
        }
    }

    return ints;
}

std::vector<BSONObj> generateDoubles(int num, int skipPercentage, int decimals) {
    std::mt19937 gen(seedGen());
    std::normal_distribution<> d(100, 10);
    std::uniform_int_distribution skip(1, 100);

    std::vector<BSONObj> doubles;

    for (int i = 0; i < num; ++i) {
        if (skip(gen) <= skipPercentage) {
            doubles.push_back(BSONObj());
        } else {
            constexpr std::array<double, 5> factors = {1.0, 10.0, 100.0, 1000.0, 10000.0};

            BSONObjBuilder builder;


            double generated = std::llround(d(gen) * factors[decimals]) / factors[decimals];
            builder.append(""_sd, generated);
            doubles.push_back(builder.obj());
        }
    }

    return doubles;
}

std::vector<BSONObj> generateTimestamps(int num, int skipPercentage, double mean, double stddev) {
    std::mt19937 gen(seedGen());
    std::normal_distribution<> d(mean, stddev);
    std::uniform_int_distribution skip(1, 100);

    std::vector<BSONObj> timestamps;

    auto now = Date_t::now().toULL();

    for (int i = 0; i < num; ++i) {
        if (skip(gen) <= skipPercentage) {
            timestamps.push_back(BSONObj());
        } else {
            BSONObjBuilder builder;
            builder.append(""_sd, Timestamp(std::llround(now + d(gen))));
            timestamps.push_back(builder.obj());
        }
    }

    return timestamps;
}

std::vector<BSONObj> generateObjectIds(int num, int skipPercentage) {
    std::mt19937 gen(seedGen());
    std::uniform_int_distribution skip(1, 100);

    std::vector<BSONObj> timestamps;

    for (int i = 0; i < num; ++i) {
        if (skip(gen) <= skipPercentage) {
            timestamps.push_back(BSONObj());
        } else {
            BSONObjBuilder builder;
            builder.append(""_sd, OID::gen());
            timestamps.push_back(builder.obj());
        }
    }

    return timestamps;
}

BSONObj buildCompressed(const std::vector<BSONObj>& elems) {
    BSONColumnBuilder col;
    for (auto&& elem : elems) {
        if (!elem.isEmpty()) {
            col.append(elem.firstElement());
        } else {
            col.skip();
        }
    }
    auto binData = col.finalize();
    BSONObjBuilder objBuilder;
    objBuilder.append(""_sd, binData);
    return objBuilder.obj();
}

BSONObj getCompressedFTDC() {
// The large literal emits this on Visual Studio: Fatal error C1091: compiler limit: string exceeds
// 65535 bytes in length
#if !defined(_MSC_VER)
    StringData compressedBase64Encoded = {};

    std::string compressed = base64::decode(compressedBase64Encoded);
    BSONObjBuilder builder;
    builder.appendBinData("data"_sd, compressed.size(), BinDataType::Column, compressed.data());
    return builder.obj();
#else
    return BSONObj();
#endif
}

void benchmarkDecompression(benchmark::State& state,
                            const BSONElement& compressedElement,
                            int skipSize) {
    uint64_t totalElements = 0;
    uint64_t totalBytes = 0;

    // Begin benchmarking loop.
    for (auto _ : state) {
        benchmark::ClobberMemory();
        BSONColumn col(compressedElement);
        auto it = col.begin();
        while (it.more()) {
            // decompress elements using iterator implementation.
            benchmark::DoNotOptimize(++it);
            ++totalElements;
        }
        totalBytes += compressedElement.size();
    }

    state.SetItemsProcessed(totalElements);
    state.SetBytesProcessed(totalBytes);
}

void benchmarkBlockBasedDecompression(benchmark::State& state,
                                      const BSONElement& compressedElement,
                                      int skipSize) {

    int size = 0;
    const char* binary = compressedElement.binData(size);
    BSONBinData bin(binary, size, Column);

    uint64_t totalElements = 0;
    uint64_t totalBytes = 0;

    auto decompress = [&](NoOpContainerForTest<BSONElement>& container) {
        bsoncolumn::BSONColumnBlockBased col(bin);
        boost::intrusive_ptr<ElementStorage> allocator = new ElementStorage();
        col.decompressIterative<bsoncolumn::BSONElementMaterializer>(container, allocator);
        return true;
    };

    // Begin benchmarking loop.
    for (auto _ : state) {
        benchmark::ClobberMemory();
        NoOpContainerForTest<BSONElement> container;
        benchmark::DoNotOptimize(decompress(container));
        totalBytes += compressedElement.size();
        totalElements += container.size();
    }

    state.SetItemsProcessed(totalElements);
    state.SetBytesProcessed(totalBytes);
}

void benchmarkBlockBasedDecompression_SBE(benchmark::State& state,
                                          const BSONElement& compressedElement,
                                          int skipSize) {

    using SBEMaterializer = sbe::bsoncolumn::SBEColumnMaterializer;
    int size = 0;
    const char* binary = compressedElement.binData(size);
    BSONBinData bin(binary, size, Column);

    uint64_t totalElements = 0;
    uint64_t totalBytes = 0;

    auto decompress = [&](NoOpContainerForTest<SBEMaterializer::Element>& container) {
        bsoncolumn::BSONColumnBlockBased col(bin);
        boost::intrusive_ptr<ElementStorage> allocator = new ElementStorage();
        col.decompressIterative<SBEMaterializer>(container, allocator);
        return true;
    };

    // Begin benchmarking loop.
    for (auto _ : state) {
        benchmark::ClobberMemory();
        NoOpContainerForTest<SBEMaterializer::Element> container;
        benchmark::DoNotOptimize(decompress(container));
        totalBytes = compressedElement.size();
        totalElements += container.size();
    }

    state.SetItemsProcessed(totalElements);
    state.SetBytesProcessed(totalBytes);
}

void benchmarkCompression(benchmark::State& state,
                          const BSONElement& compressedElement,
                          int skipSize) {
    BSONColumn col(compressedElement);
    // Iterate over BSONColumn once to fully decompress it so when we are benchmarking below we
    // don't have to pay decompression cost. Also calculate uncompressed size.
    uint64_t uncompressedSize = 0;
    for (auto&& decompressed : col) {
        if (decompressed.eoo())
            uncompressedSize += skipSize;
        else {
            uncompressedSize += decompressed.valuesize();
        }
    }

    uint64_t totalElements = 0;
    uint64_t totalBytes = 0;
    for (auto _ : state) {
        BSONColumnBuilder columnBuilder;
        for (auto&& decompressed : col) {
            columnBuilder.append(decompressed);
            totalBytes += decompressed.size();
            ++totalElements;
        }
        benchmark::DoNotOptimize(columnBuilder.finalize());
        benchmark::ClobberMemory();
    }
    state.SetItemsProcessed(totalElements);
    state.SetBytesProcessed(totalBytes);
    state.SetLabel(
        fmt::format("compress:{:.1f}%",
                    100.0 * (1 - ((double)compressedElement.valuesize() / uncompressedSize))));
}

void benchmarkReopen(benchmark::State& state, const BSONElement& compressedElement, int skipSize) {
    int size;
    const char* binary = compressedElement.binData(size);

    auto reopen = [&]() {
        BSONColumnBuilder cb(binary, size);
        return true;
    };

    for (auto _ : state) {
        benchmark::ClobberMemory();
        benchmark::DoNotOptimize(reopen());
    }
}

void benchmarkReopenNaive(benchmark::State& state,
                          const BSONElement& compressedElement,
                          int skipSize) {
    int size;
    const char* binary = compressedElement.binData(size);

    auto reopen = [&]() {
        BSONColumnBuilder cb;
        BSONColumn col(binary, size);
        for (auto&& decompressed : col) {
            cb.append(decompressed);
        }
        return true;
    };

    for (auto _ : state) {
        benchmark::ClobberMemory();
        benchmark::DoNotOptimize(reopen());
    }
}

void BM_decompressIntegers(benchmark::State& state, int skipPercentage, DecompressMode mode) {
    BSONObj compressed = buildCompressed(generateIntegers(10000, skipPercentage));
    switch (mode) {
        case kBlockBSON:
            benchmarkBlockBasedDecompression(state, compressed.firstElement(), sizeof(int32_t));
            break;
        case kBlockSBE:
            benchmarkBlockBasedDecompression_SBE(state, compressed.firstElement(), sizeof(int32_t));
            break;
        case kIterator:
        default:
            benchmarkDecompression(state, compressed.firstElement(), sizeof(int32_t));
            break;
    }
}

void BM_decompressDoubles(benchmark::State& state,
                          int decimals,
                          int skipPercentage,
                          DecompressMode mode) {
    auto doubles = generateDoubles(10000, skipPercentage, decimals);
    BSONObj compressed = buildCompressed(doubles);
    switch (mode) {
        case kBlockBSON:
            benchmarkBlockBasedDecompression(state, compressed.firstElement(), sizeof(double));
            break;
        case kBlockSBE:
            benchmarkBlockBasedDecompression_SBE(state, compressed.firstElement(), sizeof(double));
            break;
        case kIterator:
        default:
            benchmarkDecompression(state, compressed.firstElement(), sizeof(double));
            break;
    }
}

void BM_decompressTimestamps(
    benchmark::State& state, double mean, double stddev, int skipPercentage, DecompressMode mode) {
    BSONObj compressed = buildCompressed(generateTimestamps(10000, skipPercentage, mean, stddev));
    switch (mode) {
        case kBlockBSON:
            benchmarkBlockBasedDecompression(state, compressed.firstElement(), sizeof(Timestamp));
            break;
        case kBlockSBE:
            benchmarkBlockBasedDecompression_SBE(
                state, compressed.firstElement(), sizeof(Timestamp));
            break;
        case kIterator:
        default:
            benchmarkDecompression(state, compressed.firstElement(), sizeof(Timestamp));
            break;
    }
}

void BM_decompressObjectIds(benchmark::State& state, int skipPercentage, DecompressMode mode) {
    BSONObj compressed = buildCompressed(generateObjectIds(10000, skipPercentage));
    switch (mode) {
        case kBlockBSON:
            benchmarkBlockBasedDecompression(state, compressed.firstElement(), sizeof(OID));
            break;
        case kBlockSBE:
            benchmarkBlockBasedDecompression_SBE(state, compressed.firstElement(), sizeof(OID));
            break;
        case kIterator:
        default:
            benchmarkDecompression(state, compressed.firstElement(), sizeof(OID));
            break;
    }
}

void BM_decompressFTDC(benchmark::State& state, DecompressMode mode) {
    BSONObj compressed = getCompressedFTDC();
    switch (mode) {
        case kBlockBSON:
            benchmarkBlockBasedDecompression(state, compressed.firstElement(), 0);
            break;
        case kBlockSBE:
            benchmarkBlockBasedDecompression_SBE(state, compressed.firstElement(), 0);
            break;
        case kIterator:
        default:
            benchmarkDecompression(state, compressed.firstElement(), 0);
            break;
    }
}

void BM_compressIntegers(benchmark::State& state, int skipPercentage) {
    BSONObj compressed = buildCompressed(generateIntegers(10000, skipPercentage));
    benchmarkCompression(state, compressed.firstElement(), sizeof(int32_t));
}

void BM_compressDoubles(benchmark::State& state, int decimals, int skipPercentage) {
    BSONObj compressed = buildCompressed(generateDoubles(10000, skipPercentage, decimals));
    benchmarkCompression(state, compressed.firstElement(), sizeof(double));
}

void BM_compressTimestamps(benchmark::State& state,
                           double mean,
                           double stddev,
                           int skipPercentage) {
    BSONObj compressed = buildCompressed(generateTimestamps(10000, skipPercentage, mean, stddev));
    benchmarkCompression(state, compressed.firstElement(), sizeof(Timestamp));
}

void BM_compressObjectIds(benchmark::State& state, int skipPercentage) {
    BSONObj compressed = buildCompressed(generateObjectIds(10000, skipPercentage));
    benchmarkCompression(state, compressed.firstElement(), sizeof(OID));
}

void BM_reopenIntegers(benchmark::State& state, int skipPercentage, int num) {
    BSONObj compressed = buildCompressed(generateIntegers(num, skipPercentage));
    benchmarkReopen(state, compressed.firstElement(), sizeof(int32_t));
}

void BM_reopenNaiveIntegers(benchmark::State& state, int skipPercentage, int num) {
    BSONObj compressed = buildCompressed(generateIntegers(num, skipPercentage));
    benchmarkReopenNaive(state, compressed.firstElement(), sizeof(int32_t));
}

void BM_compressFTDC(benchmark::State& state) {
    BSONObj compressed = getCompressedFTDC();
    benchmarkCompression(state, compressed.firstElement(), 0);
}

// Block-based API benchmarks using the BSONElementMaterializer. We'll run a subset of the
// benchmarks on the new API.
BENCHMARK_CAPTURE(BM_decompressIntegers, Block API BSON Skip = 0 %, 0, kBlockBSON);
BENCHMARK_CAPTURE(BM_decompressIntegers, Block API BSON Skip = 50 %, 50, kBlockBSON);
BENCHMARK_CAPTURE(BM_decompressIntegers, Block API BSON Skip = 99 %, 99, kBlockBSON);

BENCHMARK_CAPTURE(BM_decompressDoubles, Block API BSON Decimals = 0 / Skip = 0 %, 0, 0, kBlockBSON);
BENCHMARK_CAPTURE(BM_decompressDoubles, Block API BSON Decimals = 1 / Skip = 0 %, 1, 0, kBlockBSON);
BENCHMARK_CAPTURE(BM_decompressDoubles, Block API BSON Decimals = 4 / Skip = 0 %, 4, 0, kBlockBSON);

BENCHMARK_CAPTURE(BM_decompressDoubles,
                  Block API BSON Decimals = 0 / Skip = 10 %
                  , 0, 10, kBlockBSON);
BENCHMARK_CAPTURE(BM_decompressDoubles,
                  Block API BSON Decimals = 1 / Skip = 10 %
                  , 1, 10, kBlockBSON);
BENCHMARK_CAPTURE(BM_decompressDoubles,
                  Block API BSON Decimals = 4 / Skip = 10 %
                  , 4, 10, kBlockBSON);

BENCHMARK_CAPTURE(BM_decompressDoubles,
                  Block API BSON Decimals = 0 / Skip = 90 %
                  , 0, 90, kBlockBSON);
BENCHMARK_CAPTURE(BM_decompressDoubles,
                  Block API BSON Decimals = 1 / Skip = 90 %
                  , 1, 90, kBlockBSON);
BENCHMARK_CAPTURE(BM_decompressDoubles,
                  Block API BSON Decimals = 4 / Skip = 90 %
                  , 4, 90, kBlockBSON);

BENCHMARK_CAPTURE(BM_decompressTimestamps,
                  Block API BSON Mean = 1 / Stddev = 0 / Skip = 0 %
                  , 0, 1, 0, kBlockBSON);
BENCHMARK_CAPTURE(BM_decompressTimestamps,
                  Block API BSON Mean = 5 / Stddev = 2 / Skip = 0 %
                  , 0, 1, 0, kBlockBSON);
BENCHMARK_CAPTURE(BM_decompressTimestamps,
                  Block API BSON Mean = 1 / Stddev = 0 / Skip = 90 %
                  , 0, 1, 90, kBlockBSON);
BENCHMARK_CAPTURE(BM_decompressTimestamps,
                  Block API BSON Mean = 5 / Stddev = 2 / Skip = 90 %
                  , 0, 1, 90, kBlockBSON);

BENCHMARK_CAPTURE(BM_decompressObjectIds, Block API BSON Skip = 0 %, 0, kBlockBSON);
BENCHMARK_CAPTURE(BM_decompressObjectIds, Block API BSON Skip = 90 %, 90, kBlockBSON);

// Block-based API benchmarks using the SBEMaterializer. We'll run a subset of the benchmarks on the
// new API.
BENCHMARK_CAPTURE(BM_decompressIntegers, Block API SBE Skip = 0 %, 0, kBlockSBE);
BENCHMARK_CAPTURE(BM_decompressIntegers, Block API SBE Skip = 50 %, 50, kBlockSBE);
BENCHMARK_CAPTURE(BM_decompressIntegers, Block API SBE Skip = 99 %, 99, kBlockSBE);

BENCHMARK_CAPTURE(BM_decompressDoubles, Block API SBE Decimals = 0 / Skip = 0 %, 0, 0, kBlockSBE);
BENCHMARK_CAPTURE(BM_decompressDoubles, Block API SBE Decimals = 1 / Skip = 0 %, 1, 0, kBlockSBE);
BENCHMARK_CAPTURE(BM_decompressDoubles, Block API SBE Decimals = 4 / Skip = 0 %, 4, 0, kBlockSBE);

BENCHMARK_CAPTURE(BM_decompressDoubles, Block API SBE Decimals = 0 / Skip = 10 %, 0, 10, kBlockSBE);
BENCHMARK_CAPTURE(BM_decompressDoubles, Block API SBE Decimals = 1 / Skip = 10 %, 1, 10, kBlockSBE);
BENCHMARK_CAPTURE(BM_decompressDoubles, Block API SBE Decimals = 4 / Skip = 10 %, 4, 10, kBlockSBE);

BENCHMARK_CAPTURE(BM_decompressDoubles, Block API SBE Decimals = 0 / Skip = 90 %, 0, 90, kBlockSBE);
BENCHMARK_CAPTURE(BM_decompressDoubles, Block API SBE Decimals = 1 / Skip = 90 %, 1, 90, kBlockSBE);
BENCHMARK_CAPTURE(BM_decompressDoubles, Block API SBE Decimals = 4 / Skip = 90 %, 4, 90, kBlockSBE);

BENCHMARK_CAPTURE(BM_decompressTimestamps,
                  Block API SBE Mean = 1 / Stddev = 0 / Skip = 0 %
                  , 0, 1, 0, kBlockSBE);
BENCHMARK_CAPTURE(BM_decompressTimestamps,
                  Block API SBE Mean = 5 / Stddev = 2 / Skip = 0 %
                  , 0, 1, 0, kBlockSBE);
BENCHMARK_CAPTURE(BM_decompressTimestamps,
                  Block API SBE Mean = 1 / Stddev = 0 / Skip = 90 %
                  , 0, 1, 90, kBlockSBE);
BENCHMARK_CAPTURE(BM_decompressTimestamps,
                  Block API SBE Mean = 5 / Stddev = 2 / Skip = 90 %
                  , 0, 1, 90, kBlockSBE);

BENCHMARK_CAPTURE(BM_decompressObjectIds, Block API SBE Skip = 0 %, 0, kBlockSBE);
BENCHMARK_CAPTURE(BM_decompressObjectIds, Block API SBE Skip = 90 %, 90, kBlockSBE);

// Iterator implementation benchmarks.
BENCHMARK_CAPTURE(BM_decompressIntegers, Iterator API Skip = 0 %, 0, kIterator);
BENCHMARK_CAPTURE(BM_decompressIntegers, Iterator API Skip = 10 %, 10, kIterator);
BENCHMARK_CAPTURE(BM_decompressIntegers, Iterator API Skip = 50 %, 50, kIterator);
BENCHMARK_CAPTURE(BM_decompressIntegers, Iterator API Skip = 90 %, 90, kIterator);
BENCHMARK_CAPTURE(BM_decompressIntegers, Iterator API Skip = 99 %, 99, kIterator);

BENCHMARK_CAPTURE(BM_decompressDoubles, Iterator API Decimals = 0 / Skip = 0 %, 0, 0, kIterator);
BENCHMARK_CAPTURE(BM_decompressDoubles, Iterator API Decimals = 1 / Skip = 0 %, 1, 0, kIterator);
BENCHMARK_CAPTURE(BM_decompressDoubles, Iterator API Decimals = 2 / Skip = 0 %, 2, 0, kIterator);
BENCHMARK_CAPTURE(BM_decompressDoubles, Iterator API Decimals = 4 / Skip = 0 %, 4, 0, kIterator);

BENCHMARK_CAPTURE(BM_decompressDoubles, Iterator API Decimals = 0 / Skip = 10 %, 0, 10, kIterator);
BENCHMARK_CAPTURE(BM_decompressDoubles, Iterator API Decimals = 1 / Skip = 10 %, 1, 10, kIterator);
BENCHMARK_CAPTURE(BM_decompressDoubles, Iterator API Decimals = 2 / Skip = 10 %, 2, 10, kIterator);
BENCHMARK_CAPTURE(BM_decompressDoubles, Iterator API Decimals = 4 / Skip = 10 %, 4, 10, kIterator);

BENCHMARK_CAPTURE(BM_decompressDoubles, Iterator API Decimals = 0 / Skip = 90 %, 0, 90, kIterator);
BENCHMARK_CAPTURE(BM_decompressDoubles, Iterator API Decimals = 1 / Skip = 90 %, 1, 90, kIterator);
BENCHMARK_CAPTURE(BM_decompressDoubles, Iterator API Decimals = 2 / Skip = 90 %, 2, 90, kIterator);
BENCHMARK_CAPTURE(BM_decompressDoubles, Iterator API Decimals = 4 / Skip = 90 %, 4, 90, kIterator);

BENCHMARK_CAPTURE(BM_decompressTimestamps,
                  Iterator API Mean = 1 / Stddev = 0 / Skip = 0 %
                  , 0, 1, 0, kIterator);
BENCHMARK_CAPTURE(BM_decompressTimestamps,
                  Iterator API Mean = 5 / Stddev = 2 / Skip = 0 %
                  , 0, 1, 0, kIterator);
BENCHMARK_CAPTURE(BM_decompressTimestamps,
                  Iterator API Mean = 1 / Stddev = 0 / Skip = 10 %
                  , 0, 1, 10, kIterator);
BENCHMARK_CAPTURE(BM_decompressTimestamps,
                  Iterator API Mean = 5 / Stddev = 2 / Skip = 10 %
                  , 0, 1, 10, kIterator);
BENCHMARK_CAPTURE(BM_decompressTimestamps,
                  Iterator API Mean = 1 / Stddev = 0 / Skip = 90 %
                  , 0, 1, 90, kIterator);
BENCHMARK_CAPTURE(BM_decompressTimestamps,
                  Iterator API Mean = 5 / Stddev = 2 / Skip = 90 %
                  , 0, 1, 90, kIterator);

BENCHMARK_CAPTURE(BM_decompressObjectIds, Iterator API Skip = 0 %, 0, kIterator);
BENCHMARK_CAPTURE(BM_decompressObjectIds, Iterator API Skip = 10 %, 10, kIterator);
BENCHMARK_CAPTURE(BM_decompressObjectIds, Iterator API Skip = 90 %, 90, kIterator);

// The large literal emits this on Visual Studio: Fatal error C1091: compiler limit: string exceeds
// 65535 bytes in length
#if !defined(_MSC_VER)
// TODO SERVER-84389 enable these benchmarks.
// BENCHMARK_CAPTURE(BM_decompressFTDC, Iterator API, kIterator);
// BENCHMARK_CAPTURE(BM_decompressFTDC, Block API BSON, kBlockBSON);
// BENCHMARK_CAPTURE(BM_decompressFTDC, Block API SBE, kBlockSBE);
#endif

BENCHMARK_CAPTURE(BM_compressIntegers, Skip = 0 %, 0);
BENCHMARK_CAPTURE(BM_compressIntegers, Skip = 10 %, 10);
BENCHMARK_CAPTURE(BM_compressIntegers, Skip = 50 %, 50);
BENCHMARK_CAPTURE(BM_compressIntegers, Skip = 90 %, 90);
BENCHMARK_CAPTURE(BM_compressIntegers, Skip = 99 %, 99);

BENCHMARK_CAPTURE(BM_compressDoubles, Scale = 1 / Skip = 0 %, 0, 0);
BENCHMARK_CAPTURE(BM_compressDoubles, Scale = 10 / Skip = 0 %, 1, 0);
BENCHMARK_CAPTURE(BM_compressDoubles, Scale = 100 / Skip = 0 %, 2, 0);
BENCHMARK_CAPTURE(BM_compressDoubles, Scale = 10000 / Skip = 0 %, 4, 0);

BENCHMARK_CAPTURE(BM_compressDoubles, Scale = 1 / Skip = 10 %, 0, 10);
BENCHMARK_CAPTURE(BM_compressDoubles, Scale = 10 / Skip = 10 %, 1, 10);
BENCHMARK_CAPTURE(BM_compressDoubles, Scale = 100 / Skip = 10 %, 2, 10);
BENCHMARK_CAPTURE(BM_compressDoubles, Scale = 10000 / Skip = 10 %, 4, 10);

BENCHMARK_CAPTURE(BM_compressDoubles, Scale = 1 / Skip = 90 %, 0, 90);
BENCHMARK_CAPTURE(BM_compressDoubles, Scale = 10 / Skip = 90 %, 1, 90);
BENCHMARK_CAPTURE(BM_compressDoubles, Scale = 100 / Skip = 90 %, 2, 90);
BENCHMARK_CAPTURE(BM_compressDoubles, Scale = 10000 / Skip = 90 %, 4, 90);

BENCHMARK_CAPTURE(BM_compressTimestamps, Mean = 1 / Stddev = 0 / Skip = 0 %, 0, 1, 0);
BENCHMARK_CAPTURE(BM_compressTimestamps, Mean = 5 / Stddev = 2 / Skip = 0 %, 0, 1, 0);
BENCHMARK_CAPTURE(BM_compressTimestamps, Mean = 1 / Stddev = 0 / Skip = 10 %, 0, 1, 10);
BENCHMARK_CAPTURE(BM_compressTimestamps, Mean = 5 / Stddev = 2 / Skip = 10 %, 0, 1, 10);
BENCHMARK_CAPTURE(BM_compressTimestamps, Mean = 1 / Stddev = 0 / Skip = 90 %, 0, 1, 90);
BENCHMARK_CAPTURE(BM_compressTimestamps, Mean = 5 / Stddev = 2 / Skip = 90 %, 0, 1, 90);

BENCHMARK_CAPTURE(BM_compressObjectIds, Skip = 0 %, 0);
BENCHMARK_CAPTURE(BM_compressObjectIds, Skip = 10 %, 10);
BENCHMARK_CAPTURE(BM_compressObjectIds, Skip = 90 %, 90);

BENCHMARK_CAPTURE(BM_reopenIntegers, Skip = 0 % / Num = 10, 0, 10);
BENCHMARK_CAPTURE(BM_reopenIntegers, Skip = 50 % / Num = 10, 50, 10);
BENCHMARK_CAPTURE(BM_reopenIntegers, Skip = 99 % / Num = 10, 99, 10);

BENCHMARK_CAPTURE(BM_reopenIntegers, Skip = 0 % / Num = 100, 0, 100);
BENCHMARK_CAPTURE(BM_reopenIntegers, Skip = 50 % / Num = 100, 50, 100);
BENCHMARK_CAPTURE(BM_reopenIntegers, Skip = 99 % / Num = 100, 99, 100);

BENCHMARK_CAPTURE(BM_reopenIntegers, Skip = 0 % / Num = 1000, 0, 1000);
BENCHMARK_CAPTURE(BM_reopenIntegers, Skip = 50 % / Num = 1000, 50, 1000);
BENCHMARK_CAPTURE(BM_reopenIntegers, Skip = 99 % / Num = 1000, 99, 1000);

BENCHMARK_CAPTURE(BM_reopenIntegers, Skip = 0 % / Num = 10000, 0, 10000);
BENCHMARK_CAPTURE(BM_reopenIntegers, Skip = 50 % / Num = 10000, 50, 10000);
BENCHMARK_CAPTURE(BM_reopenIntegers, Skip = 99 % / Num = 10000, 99, 10000);

BENCHMARK_CAPTURE(BM_reopenNaiveIntegers, Skip = 0 % / Num = 10, 0, 10);
BENCHMARK_CAPTURE(BM_reopenNaiveIntegers, Skip = 50 % / Num = 10, 50, 10);
BENCHMARK_CAPTURE(BM_reopenNaiveIntegers, Skip = 99 % / Num = 10, 99, 10);

BENCHMARK_CAPTURE(BM_reopenNaiveIntegers, Skip = 0 % / Num = 100, 0, 100);
BENCHMARK_CAPTURE(BM_reopenNaiveIntegers, Skip = 50 % / Num = 100, 50, 100);
BENCHMARK_CAPTURE(BM_reopenNaiveIntegers, Skip = 99 % / Num = 100, 99, 100);

BENCHMARK_CAPTURE(BM_reopenNaiveIntegers, Skip = 0 % / Num = 1000, 0, 1000);
BENCHMARK_CAPTURE(BM_reopenNaiveIntegers, Skip = 50 % / Num = 1000, 50, 1000);
BENCHMARK_CAPTURE(BM_reopenNaiveIntegers, Skip = 99 % / Num = 1000, 99, 1000);

BENCHMARK_CAPTURE(BM_reopenNaiveIntegers, Skip = 0 % / Num = 10000, 0, 10000);
BENCHMARK_CAPTURE(BM_reopenNaiveIntegers, Skip = 50 % / Num = 10000, 50, 10000);
BENCHMARK_CAPTURE(BM_reopenNaiveIntegers, Skip = 99 % / Num = 10000, 99, 10000);

// The large literal emits this on Visual Studio: Fatal error C1091: compiler limit: string exceeds
// 65535 bytes in length
#if !defined(_MSC_VER)
// TODO SERVER-84389 enable these benchmarks.
// BENCHMARK(BM_compressFTDC);
#endif

}  // namespace
}  // namespace mongo
