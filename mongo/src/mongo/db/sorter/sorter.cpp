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

/**
 * This is the implementation for the Sorter.
 *
 * It is intended to be included in other cpp files like this:
 *
 * #include <normal/include/files.h>
 *
 * #include "mongo/db/sorter/sorter.h"
 *
 * namespace mongo {
 *     // Your code
 * }
 *
 * #include "mongo/db/sorter/sorter.cpp"
 * MONGO_CREATE_SORTER(MyKeyType, MyValueType, MyComparatorType);
 *
 * Do this once for each unique set of parameters to MONGO_CREATE_SORTER.
 */

#include "mongo/db/sorter/sorter.h"

#include <algorithm>
#include <cerrno>
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <deque>
#include <exception>
#include <functional>
#include <istream>
#include <iterator>
#include <limits>
#include <memory>
#include <queue>
#include <snappy.h>
#include <string>
#include <system_error>
#include <utility>
#include <vector>

// IWYU pragma: no_include "boost/container/detail/std_fwd.hpp"
#include <boost/filesystem/operations.hpp>
#include <boost/filesystem/path.hpp>
#include <boost/move/utility_core.hpp>
#include <boost/optional/optional.hpp>

#include "mongo/base/data_range.h"
#include "mongo/base/error_codes.h"
#include "mongo/base/status.h"
#include "mongo/bson/util/builder.h"
#include "mongo/bson/util/builder_fwd.h"
#include "mongo/config.h"  // IWYU pragma: keep
#include "mongo/db/server_feature_flags_gen.h"
#include "mongo/db/server_options.h"
#include "mongo/db/service_context.h"
#include "mongo/db/sorter/sorter_checksum_calculator.h"
#include "mongo/db/sorter/sorter_gen.h"
#include "mongo/db/sorter/sorter_stats.h"
#include "mongo/db/storage/encryption_hooks.h"
#include "mongo/db/storage/storage_parameters_gen.h"
#include "mongo/logv2/log.h"
#include "mongo/logv2/log_attr.h"
#include "mongo/logv2/log_component.h"
#include "mongo/platform/atomic_word.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/bufreader.h"
#include "mongo/util/destructor_guard.h"
#include "mongo/util/shared_buffer_fragment.h"
#include "mongo/util/str.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kDefault

namespace mongo {

namespace {

void checkNoExternalSortOnMongos(const SortOptions& opts) {
    // This should be checked by consumers, but if it isn't try to fail early.
    uassert(16947,
            "Attempting to use external sort from mongos. This is not allowed.",
            !(serverGlobalParams.clusterRole.hasExclusively(ClusterRole::RouterServer) &&
              opts.extSortAllowed));
}

/**
 * Returns the current EncryptionHooks registered with the global service context.
 * Returns nullptr if the service context is not available; or if the EncyptionHooks
 * registered is not enabled.
 */
EncryptionHooks* getEncryptionHooksIfEnabled() {
    // Some tests may not run with a global service context.
    if (!hasGlobalServiceContext()) {
        return nullptr;
    }
    auto service = getGlobalServiceContext();
    auto encryptionHooks = EncryptionHooks::get(service);
    if (!encryptionHooks->enabled()) {
        return nullptr;
    }
    return encryptionHooks;
}

constexpr std::size_t kSortedFileBufferSize = 64 * 1024;

}  // namespace

namespace sorter {

// We need to use the "real" errno everywhere, not GetLastError() on Windows
inline std::string myErrnoWithDescription() {
    int errnoCopy = errno;
    StringBuilder sb;
    sb << "errno:" << errnoCopy << ' ' << strerror(errnoCopy);
    return sb.str();
}

template <typename Key, typename Comparator>
void dassertCompIsSane(const Comparator& comp, const Key& lhs, const Key& rhs) {
#if defined(MONGO_CONFIG_DEBUG_BUILD) && !defined(_MSC_VER)
    // MSVC++ already does similar verification in debug mode in addition to using
    // algorithms that do more comparisons. Doing our own verification in addition makes
    // debug builds considerably slower without any additional safety.

    // test reversed comparisons
    const int regular = comp(lhs, rhs);
    if (regular == 0) {
        invariant(comp(rhs, lhs) == 0);
    } else if (regular < 0) {
        invariant(comp(rhs, lhs) > 0);
    } else {
        invariant(comp(rhs, lhs) < 0);
    }

    // test reflexivity
    invariant(comp(lhs, lhs) == 0);
    invariant(comp(rhs, rhs) == 0);
#endif
}

/**
 * Returns results from sorted in-memory storage.
 */
template <typename Key, typename Value>
class InMemIterator : public SortIteratorInterface<Key, Value> {
public:
    typedef std::pair<Key, Value> Data;

    /// No data to iterate
    InMemIterator() {}

    /// Only a single value
    InMemIterator(const Data& singleValue) : _data(1, singleValue) {}

    /// Any number of values
    template <typename Container>
    InMemIterator(const Container& input) : _data(input.begin(), input.end()) {}

    InMemIterator(std::deque<Data> data) : _data(std::move(data)) {}

    void openSource() {}
    void closeSource() {}

    bool more() {
        return !_data.empty();
    }
    Data next() {
        Data out = std::move(_data.front());
        _data.pop_front();
        return out;
    }

    Key nextWithDeferredValue() override {
        MONGO_UNREACHABLE;
    }

    Value getDeferredValue() override {
        MONGO_UNREACHABLE;
    }

    const Key& current() override {
        tasserted(ErrorCodes::NotImplemented, "current() not implemented for InMemIterator");
    }

private:
    std::deque<Data> _data;
};

/**
 * This class is used to return the in-memory state from the sorter in read-only mode.
 * This is used by streams checkpoint use case mainly to save in-memory state on persistent
 * storage."
 */
template <typename Key, typename Value, typename Container>
class InMemReadOnlyIterator : public SortIteratorInterface<Key, Value> {
public:
    typedef std::pair<Key, Value> Data;

    InMemReadOnlyIterator(const Container& data) : _data(data) {
        _iterator = _data.begin();
    }

    void openSource() override {}
    void closeSource() override {}

    bool more() override {
        return _iterator != _data.end();
    }

    Data next() override {
        Data out = *_iterator++;
        return out;
    }

    Key nextWithDeferredValue() override {
        MONGO_UNIMPLEMENTED_TASSERT(8248302);
    }

    Value getDeferredValue() override {
        MONGO_UNIMPLEMENTED_TASSERT(8248303);
    }

    const Key& current() override {
        return std::prev(_iterator)->first;
    }

private:
    const Container& _data;
    typename Container::const_iterator _iterator;
};

/**
 * Returns results from a sorted range within a file. Each instance is given a file name and start
 * and end offsets.
 *
 * This class is NOT responsible for file clean up / deletion. There are openSource() and
 * closeSource() functions to ensure the FileIterator is not holding the file open when the file is
 * deleted. Since it is one among many FileIterators, it cannot close a file that may still be in
 * use elsewhere.
 */
template <typename Key, typename Value>
class FileIterator : public SortIteratorInterface<Key, Value> {
public:
    typedef std::pair<typename Key::SorterDeserializeSettings,
                      typename Value::SorterDeserializeSettings>
        Settings;
    typedef std::pair<Key, Value> Data;

    FileIterator(std::shared_ptr<typename Sorter<Key, Value>::File> file,
                 std::streamoff fileStartOffset,
                 std::streamoff fileEndOffset,
                 const Settings& settings,
                 const boost::optional<DatabaseName>& dbName,
                 const size_t checksum,
                 const SorterChecksumVersion checksumVersion)
        : _settings(settings),
          _file(std::move(file)),
          _fileStartOffset(fileStartOffset),
          _fileCurrentOffset(fileStartOffset),
          _fileEndOffset(fileEndOffset),
          _dbName(dbName),
          _afterReadChecksumCalculator(checksumVersion),
          _originalChecksum(checksum) {}

    void openSource() {}

    void closeSource() {
        // If the file iterator reads through all data objects, we can ensure non-corrupt data
        // by comparing the newly calculated checksum with the original checksum from the data
        // written to disk. Some iterators do not read back all data from the file, which prohibits
        // the _afterReadChecksum from obtaining all the information needed. Thus, we only fassert
        // if all data that was written to disk is read back and the checksums are not equivalent.
        if (_done && _bufferReader->atEof() &&
            (_originalChecksum != _afterReadChecksumCalculator.checksum())) {
            fassert(31182,
                    Status(ErrorCodes::Error::ChecksumMismatch,
                           "Data read from disk does not match what was written to disk. Possible "
                           "corruption of data."));
        }
    }

    bool more() {
        invariant(!_startOfNewData);
        if (!_done)
            _fillBufferIfNeeded();  // may change _done
        return !_done;
    }

    Data next() {
        Key deserializedKey = nextWithDeferredValue();
        Value deserializedValue = getDeferredValue();
        return Data(std::move(deserializedKey), std::move(deserializedValue));
    }

    Key nextWithDeferredValue() override {
        invariant(!_done);
        invariant(!_startOfNewData);
        _fillBufferIfNeeded();

        _startOfNewData = static_cast<const char*>(_bufferReader->pos());

        // Note: calling read() on the _bufferReader buffer in the deserialize function advances the
        // buffer. Since Key comes before Value in the _bufferReader, and C++ makes no function
        // parameter evaluation order guarantees, we cannot deserialize Key and Value straight into
        // the Data constructor
        return Key::deserializeForSorter(*_bufferReader, _settings.first);
    }

    Value getDeferredValue() override {
        invariant(!_done);
        invariant(_startOfNewData);
        Value deserializedValue = Value::deserializeForSorter(*_bufferReader, _settings.second);

        // The difference of _bufferReader's position before and after reading the data
        // will provide the length of the data that was just read.
        const char* endOfNewData = static_cast<const char*>(_bufferReader->pos());

        if (_afterReadChecksumCalculator.version() == SorterChecksumVersion::v1) {
            _afterReadChecksumCalculator.addData(_startOfNewData, endOfNewData - _startOfNewData);
        }
        _startOfNewData = nullptr;
        return deserializedValue;
    }

    const Key& current() override {
        tasserted(ErrorCodes::NotImplemented, "current() not implemented for FileIterator");
    }

    SorterRange getRange() const {
        SorterRange range{
            _fileStartOffset, _fileEndOffset, static_cast<int64_t>(_originalChecksum)};
        if (_afterReadChecksumCalculator.version() != SorterChecksumVersion::v1) {
            range.setChecksumVersion(_afterReadChecksumCalculator.version());
        }
        return range;
    }

private:
    /**
     * Attempts to refill the _bufferReader if it is empty. Expects _done to be false.
     */
    void _fillBufferIfNeeded() {
        invariant(!_done);

        if (!_bufferReader || _bufferReader->atEof()) {
            _fillBufferFromDisk();
            if (_afterReadChecksumCalculator.version() > SorterChecksumVersion::v1 &&
                !_bufferReader->atEof()) {
                _afterReadChecksumCalculator.addData(static_cast<const char*>(_bufferReader->pos()),
                                                     _bufferReader->remaining());
            }
        }
    }

    /**
     * Tries to read from disk and places any results in _bufferReader. If there is no more data to
     * read, then _done is set to true and the function returns immediately.
     */
    void _fillBufferFromDisk() {
        int32_t rawSize;
        _read(&rawSize, sizeof(rawSize));
        if (_done)
            return;

        // negative size means compressed
        const bool compressed = rawSize < 0;
        int32_t blockSize = std::abs(rawSize);

        _buffer.reset(new char[blockSize]);
        _read(_buffer.get(), blockSize);
        uassert(16816, "file too short?", !_done);

        if (auto encryptionHooks = getEncryptionHooksIfEnabled()) {
            std::unique_ptr<char[]> out(new char[blockSize]);
            size_t outLen;
            Status status =
                encryptionHooks->unprotectTmpData(reinterpret_cast<const uint8_t*>(_buffer.get()),
                                                  blockSize,
                                                  reinterpret_cast<uint8_t*>(out.get()),
                                                  blockSize,
                                                  &outLen,
                                                  _dbName);
            uassert(28841,
                    str::stream() << "Failed to unprotect data: " << status.toString(),
                    status.isOK());
            blockSize = outLen;
            _buffer.swap(out);
        }

        if (!compressed) {
            _bufferReader.reset(new BufReader(_buffer.get(), blockSize));
            return;
        }

        dassert(snappy::IsValidCompressedBuffer(_buffer.get(), blockSize));

        size_t uncompressedSize;
        uassert(17061,
                "couldn't get uncompressed length",
                snappy::GetUncompressedLength(_buffer.get(), blockSize, &uncompressedSize));

        std::unique_ptr<char[]> decompressionBuffer(new char[uncompressedSize]);
        uassert(17062,
                "decompression failed",
                snappy::RawUncompress(_buffer.get(), blockSize, decompressionBuffer.get()));

        // hold on to decompressed data and throw out compressed data at block exit
        _buffer.swap(decompressionBuffer);
        _bufferReader.reset(new BufReader(_buffer.get(), uncompressedSize));
    }

    /**
     * Attempts to read data from disk. Sets _done to true when file offset reaches _fileEndOffset.
     */
    void _read(void* out, size_t size) {
        if (_fileCurrentOffset == _fileEndOffset) {
            _done = true;
            return;
        }

        invariant(_fileCurrentOffset < _fileEndOffset,
                  str::stream() << "Current file offset (" << _fileCurrentOffset
                                << ") greater than end offset (" << _fileEndOffset << ")");

        _file->read(_fileCurrentOffset, size, out);
        _fileCurrentOffset += size;
    }

    const Settings _settings;
    bool _done = false;

    std::unique_ptr<char[]> _buffer;
    std::unique_ptr<BufReader> _bufferReader;
    std::shared_ptr<typename Sorter<Key, Value>::File>
        _file;                          // File containing the sorted data range.
    std::streamoff _fileStartOffset;    // File offset at which the sorted data range starts.
    std::streamoff _fileCurrentOffset;  // File offset at which we are currently reading from.
    std::streamoff _fileEndOffset;      // File offset at which the sorted data range ends.
    boost::optional<DatabaseName> _dbName;

    // Points to the beginning of a serialized key in the key-value pair currently being read, and
    // used for computing the checksum value. This is set to nullptr after reading each key-value
    // pair.
    const char* _startOfNewData = nullptr;
    // Checksum value that is updated with each read of a data object from disk. We can compare
    // this value with _originalChecksum to check for data corruption if and only if the
    // FileIterator is exhausted.
    SorterChecksumCalculator _afterReadChecksumCalculator;

    // Checksum value retrieved from SortedFileWriter that was calculated as data was spilled
    // to disk. This is not modified, and is only used for comparison against _afterReadChecksum
    // when the FileIterator is exhausted to ensure no data corruption.
    const size_t _originalChecksum;
};

/**
 * Merge-sorts results from 0 or more FileIterators, all of which should be iterating over sorted
 * ranges within the same file. The input iterators must implement nextWithDeferredValue() and
 * getDeferredValue(). This class is given the data source file name upon construction and is
 * responsible for deleting the data source file upon destruction.
 */
template <typename Key, typename Value, typename Comparator>
class MergeIterator : public SortIteratorInterface<Key, Value> {
public:
    typedef SortIteratorInterface<Key, Value> Input;
    typedef std::pair<Key, Value> Data;

    MergeIterator(const std::vector<std::shared_ptr<Input>>& iters,
                  const SortOptions& opts,
                  const Comparator& comp)
        : _opts(opts),
          _remaining(opts.limit ? opts.limit : std::numeric_limits<unsigned long long>::max()),
          _positioned(false),
          _greater(comp) {
        for (size_t i = 0; i < iters.size(); i++) {
            iters[i]->openSource();
            if (iters[i]->more()) {
                _heap.push_back(
                    std::make_shared<Stream>(i, iters[i]->nextWithDeferredValue(), iters[i]));
                if (i > _maxFile) {
                    _maxFile = i;
                }
            } else {
                iters[i]->closeSource();
            }
        }

        if (_heap.empty()) {
            _remaining = 0;
            return;
        }

        std::make_heap(_heap.begin(), _heap.end(), _greater);
        std::pop_heap(_heap.begin(), _heap.end(), _greater);
        _current = _heap.back();
        _heap.pop_back();

        _positioned = true;
    }

    ~MergeIterator() {
        _current.reset();
        _heap.clear();
    }

    void openSource() {}
    void closeSource() {}

    void addSource(std::shared_ptr<Input> iter) {
        iter->openSource();
        if (iter->more()) {
            _heap.push_back(
                std::make_shared<Stream>(++_maxFile, iter->nextWithDeferredValue(), iter));
            std::push_heap(_heap.begin(), _heap.end(), _greater);

            if (_greater(_current, _heap.front())) {
                std::pop_heap(_heap.begin(), _heap.end(), _greater);
                std::swap(_current, _heap.back());
                std::push_heap(_heap.begin(), _heap.end(), _greater);
            }
        } else {
            iter->closeSource();
        }
    }

    bool more() {
        if (_remaining > 0 && (_positioned || !_heap.empty() || _current->more()))
            return true;

        _remaining = 0;
        return false;
    }

    const Key& current() override {
        invariant(_remaining);

        if (!_positioned) {
            advance();
            _positioned = true;
        }

        return _current->current();
    }

    Data next() {
        invariant(_remaining);

        _remaining--;

        if (_positioned) {
            _positioned = false;
        } else {
            advance();
        }
        Key key = _current->current();
        Value value = _current->getDeferredValue();
        return Data(std::move(key), std::move(value));
    }

    Key nextWithDeferredValue() override {
        MONGO_UNREACHABLE;
    }

    Value getDeferredValue() override {
        MONGO_UNREACHABLE;
    }

    void advance() {
        if (!_current->advance()) {
            invariant(!_heap.empty());
            std::pop_heap(_heap.begin(), _heap.end(), _greater);
            _current = _heap.back();
            _heap.pop_back();
        } else if (!_heap.empty() && _greater(_current, _heap.front())) {
            std::pop_heap(_heap.begin(), _heap.end(), _greater);
            std::swap(_current, _heap.back());
            std::push_heap(_heap.begin(), _heap.end(), _greater);
        }
    }

private:
    /**
     * Data iterator over an Input stream.
     *
     * This class is responsible for closing the Input source upon destruction, unfortunately,
     * because that is the path of least resistence to a design change requiring MergeIterator to
     * handle eventual deletion of said Input source.
     */
    class Stream {
    public:
        Stream(size_t fileNum, const Key& first, std::shared_ptr<Input> rest)
            : fileNum(fileNum), _current(first), _rest(rest) {}

        ~Stream() {
            _rest->closeSource();
        }

        const Key& current() const {
            return _current;
        }
        Value getDeferredValue() {
            return _rest->getDeferredValue();
        }
        bool more() {
            return _rest->more();
        }
        bool advance() {
            if (!_rest->more())
                return false;

            _current = _rest->nextWithDeferredValue();
            return true;
        }

        const size_t fileNum;

    private:
        Key _current;
        std::shared_ptr<Input> _rest;
    };

    class STLComparator {  // uses greater rather than less-than to maintain a MinHeap
    public:
        explicit STLComparator(const Comparator& comp) : _comp(comp) {}

        template <typename Ptr>
        bool operator()(const Ptr& lhs, const Ptr& rhs) const {
            // first compare data
            dassertCompIsSane(_comp, lhs->current(), rhs->current());
            int ret = _comp(lhs->current(), rhs->current());
            if (ret)
                return ret > 0;

            // then compare fileNums to ensure stability
            return lhs->fileNum > rhs->fileNum;
        }

    private:
        const Comparator _comp;
    };

    SortOptions _opts;
    unsigned long long _remaining;
    bool _positioned;
    std::shared_ptr<Stream> _current;
    std::vector<std::shared_ptr<Stream>> _heap;  // MinHeap
    STLComparator _greater;                      // named so calls make sense
    size_t _maxFile = 0;                         // The maximum file identifier used thus far
};

template <typename Key, typename Value, typename Comparator>
class MergeableSorter : public Sorter<Key, Value> {
public:
    typedef std::pair<typename Key::SorterDeserializeSettings,
                      typename Value::SorterDeserializeSettings>
        Settings;
    typedef SortIteratorInterface<Key, Value> Iterator;

    MergeableSorter(const SortOptions& opts, const Comparator& comp, const Settings& settings)
        : Sorter<Key, Value>(opts), _comp(comp), _settings(settings) {}

    MergeableSorter(const SortOptions& opts,
                    const std::string& fileName,
                    const Comparator& comp,
                    const Settings& settings)
        : Sorter<Key, Value>(opts, fileName), _comp(comp), _settings(settings) {}

protected:
    /**
     * Merge the spills in order to approximately respect memory usage. This method will calculate
     * the number of spills that can be merged simultaneously in order to respect memory limits and
     * reduce the spills to that number if necessary by merging them iteratively.
     */
    void _mergeSpillsToRespectMemoryLimits() {
        auto numTargetedSpills = std::max(this->_opts.maxMemoryUsageBytes / kSortedFileBufferSize,
                                          static_cast<std::size_t>(2));
        if (this->_iters.size() > numTargetedSpills) {
            this->_mergeSpills(numTargetedSpills);
        }
    }

    /**
     * An implementation of a k-way merge sort.
     *
     * This method will take a target number of sorted spills to merge and will proceed to merge the
     * set of them in batches of at most numTargetedSpills until it reaches the target.
     *
     * To give an example, if we have 5 spills and a target number of 2 the algorithm will do the
     * following:
     *
     * {1, 2, 3, 4, 5}
     * {12, 34, 5}
     * {1234, 5}
     */
    void _mergeSpills(std::size_t numTargetedSpills) {
        using File = typename Sorter<Key, Value>::File;

        std::shared_ptr<File> file = std::move(this->_file);
        std::vector<std::shared_ptr<Iterator>> iterators = std::move(this->_iters);

        LOGV2_INFO(6033104,
                   "Number of spills exceeds maximum spills to merge at a time, proceeding to "
                   "merge them to reduce the number",
                   "currentNumSpills"_attr = iterators.size(),
                   "maxNumSpills"_attr = numTargetedSpills);

        while (iterators.size() > numTargetedSpills) {
            std::shared_ptr<File> newSpillsFile = std::make_shared<File>(
                this->_opts.tempDir + "/" + nextFileName(), this->_opts.sorterFileStats);

            LOGV2_DEBUG(6033103,
                        1,
                        "Created new intermediate file for merged spills",
                        "path"_attr = newSpillsFile->path().string());

            std::vector<std::shared_ptr<Iterator>> mergedIterators;
            for (std::size_t i = 0; i < iterators.size(); i += numTargetedSpills) {
                std::vector<std::shared_ptr<Iterator>> spillsToMerge;
                auto endIndex = std::min(i + numTargetedSpills, iterators.size());
                std::move(iterators.begin() + i,
                          iterators.begin() + endIndex,
                          std::back_inserter(spillsToMerge));

                LOGV2_DEBUG(6033102,
                            2,
                            "Merging spills",
                            "beginIdx"_attr = i,
                            "endIdx"_attr = endIndex - 1);

                auto mergeIterator =
                    std::unique_ptr<Iterator>(Iterator::merge(spillsToMerge, this->_opts, _comp));
                mergeIterator->openSource();
                SortedFileWriter<Key, Value> writer(this->_opts, newSpillsFile, _settings);
                while (mergeIterator->more()) {
                    auto pair = mergeIterator->next();
                    writer.addAlreadySorted(pair.first, pair.second);
                }
                auto iteratorPtr = std::shared_ptr<Iterator>(writer.done());
                mergeIterator->closeSource();
                mergedIterators.push_back(std::move(iteratorPtr));
                this->_stats.incrementSpilledRanges();
            }

            LOGV2_DEBUG(6033101,
                        1,
                        "Merged spills",
                        "currentNumSpills"_attr = mergedIterators.size(),
                        "targetSpills"_attr = numTargetedSpills);

            iterators = std::move(mergedIterators);
            file = std::move(newSpillsFile);
        }
        this->_file = std::move(file);
        this->_iters = std::move(iterators);

        LOGV2_INFO(6033100, "Finished merging spills");
    }

    const Comparator _comp;
    const Settings _settings;
};

template <typename Key, typename Value, typename Comparator>
class NoLimitSorter : public MergeableSorter<Key, Value, Comparator> {
public:
    typedef std::pair<Key, Value> Data;
    typedef std::function<Value()> ValueProducer;
    using Iterator = typename MergeableSorter<Key, Value, Comparator>::Iterator;
    using Settings = typename MergeableSorter<Key, Value, Comparator>::Settings;

    NoLimitSorter(const SortOptions& opts,
                  const Comparator& comp,
                  const Settings& settings = Settings())
        : MergeableSorter<Key, Value, Comparator>(opts, comp, settings) {
        invariant(opts.limit == 0);
    }

    NoLimitSorter(const std::string& fileName,
                  const std::vector<SorterRange>& ranges,
                  const SortOptions& opts,
                  const Comparator& comp,
                  const Settings& settings = Settings())
        : MergeableSorter<Key, Value, Comparator>(opts, fileName, comp, settings) {
        invariant(opts.extSortAllowed);

        uassert(16815,
                str::stream() << "Unexpected empty file: " << this->_file->path().string(),
                ranges.empty() || boost::filesystem::file_size(this->_file->path()) != 0);

        this->_iters.reserve(ranges.size());
        std::transform(ranges.begin(),
                       ranges.end(),
                       std::back_inserter(this->_iters),
                       [this](const SorterRange& range) {
                           return std::make_shared<sorter::FileIterator<Key, Value>>(
                               this->_file,
                               range.getStartOffset(),
                               range.getEndOffset(),
                               this->_settings,
                               this->_opts.dbName,
                               range.getChecksum(),
                               range.getChecksumVersion().value_or(SorterChecksumVersion::v1));
                       });
        this->_stats.setSpilledRanges(this->_iters.size());
    }

    template <typename DataProducer>
    void addImpl(DataProducer dataProducer) {
        invariant(!_done);
        invariant(!_paused);

        auto& keyVal = _data.emplace_back(dataProducer());

        auto& memPool = this->_memPool;
        if (memPool) {
            auto memUsedInsideSorter = (sizeof(Key) + sizeof(Value)) * (_data.size() + 1);
            this->_stats.setMemUsage(memPool->memUsage() + memUsedInsideSorter);
        } else {
            auto memUsage = keyVal.first.memUsageForSorter() + keyVal.second.memUsageForSorter();
            this->_stats.incrementMemUsage(memUsage);
        }

        if (this->_stats.memUsage() > this->_opts.maxMemoryUsageBytes) {
            spill();
        }
    }

    void add(const Key& key, const Value& val) override {
        addImpl([&]() -> Data { return {key.getOwned(), val.getOwned()}; });
    }

    void emplace(Key&& key, ValueProducer valProducer) override {
        addImpl([&]() -> Data {
            key.makeOwned();
            auto val = valProducer();
            val.makeOwned();
            return {std::move(key), std::move(val)};
        });
    }

    Iterator* done() {
        invariant(!std::exchange(_done, true));

        if (this->_iters.empty()) {
            sort();
            if (this->_opts.moveSortedDataIntoIterator) {
                return new InMemIterator<Key, Value>(std::move(_data));
            }
            return new InMemIterator<Key, Value>(_data);
        }

        spill();
        this->_mergeSpillsToRespectMemoryLimits();

        return Iterator::merge(this->_iters, this->_opts, this->_comp);
    }

    Iterator* pause() override {
        invariant(!_done);
        invariant(!_paused);

        _paused = true;
        if (this->_iters.empty()) {
            return new InMemReadOnlyIterator<Key, Value, std::deque<Data>>(_data);
        }
        tassert(8248300, "Spilled sort cannot be paused", this->_iters.empty());
        return nullptr;
    }

    void resume() override {
        _paused = false;
    }

private:
    class STLComparator {
    public:
        explicit STLComparator(const Comparator& comp) : _comp(comp) {}
        bool operator()(const Data& lhs, const Data& rhs) const {
            dassertCompIsSane(_comp, lhs.first, rhs.first);
            return _comp(lhs.first, rhs.first) < 0;
        }

    private:
        const Comparator& _comp;
    };

    void sort() {
        STLComparator less(this->_comp);
        std::stable_sort(_data.begin(), _data.end(), less);
        this->_stats.incrementNumSorted(_data.size());
        auto& memPool = this->_memPool;
        if (memPool) {
            invariant(memPool->totalFragmentBytesUsed() >= this->_stats.bytesSorted());
            this->_stats.incrementBytesSorted(memPool->totalFragmentBytesUsed() -
                                              this->_stats.bytesSorted());
        } else {
            this->_stats.incrementBytesSorted(this->_stats.memUsage());
        }
    }

    void spill() {
        if (_data.empty())
            return;

        if (!this->_opts.extSortAllowed) {
            // This error message only applies to sorts from user queries made through the find or
            // aggregation commands. Other clients, such as bulk index builds, should suppress this
            // error, either by allowing external sorting or by catching and throwing a more
            // appropriate error.
            uasserted(ErrorCodes::QueryExceededMemoryLimitNoDiskUseAllowed,
                      str::stream()
                          << "Sort exceeded memory limit of " << this->_opts.maxMemoryUsageBytes
                          << " bytes, but did not opt in to external sorting.");
        }

        sort();

        SortedFileWriter<Key, Value> writer(this->_opts, this->_file, this->_settings);
        for (; !_data.empty(); _data.pop_front()) {
            writer.addAlreadySorted(_data.front().first, _data.front().second);
        }
        Iterator* iteratorPtr = writer.done();

        this->_iters.push_back(std::shared_ptr<Iterator>(iteratorPtr));

        auto& memPool = this->_memPool;
        if (memPool) {
            // We expect that all buffers are unused at this point.
            memPool->freeUnused();
            this->_stats.setMemUsage(memPool->memUsage());
        } else {
            this->_stats.resetMemUsage();
        }

        this->_stats.incrementSpilledRanges();
    }

    bool _done = false;
    std::deque<Data> _data;  // Data that has not been spilled.
    bool _paused = false;
};

template <typename Key, typename Value, typename Comparator>
class LimitOneSorter : public Sorter<Key, Value> {
    // Since this class is only used for limit==1, it omits all logic to
    // spill to disk and only tracks memory usage if explicitly requested.
public:
    typedef std::pair<Key, Value> Data;
    typedef std::function<Value()> ValueProducer;
    typedef SortIteratorInterface<Key, Value> Iterator;

    LimitOneSorter(const SortOptions& opts, const Comparator& comp)
        : Sorter<Key, Value>(opts), _comp(comp), _haveData(false) {
        invariant(opts.limit == 1);
    }

    template <typename DataProducer>
    void addImpl(const Key& key, DataProducer dataProducer) {
        this->_stats.incrementNumSorted();
        if (_haveData) {
            dassertCompIsSane(_comp, _best.first, key);
            if (_comp(_best.first, key) <= 0)
                return;  // not good enough
        } else {
            _haveData = true;
        }

        // Invoking dataProducer could invalidate key if it uses move semantics,
        // don't reference them anymore from this point on.
        _best = dataProducer();
    }

    void add(const Key& key, const Value& val) override {
        addImpl(key, [&]() -> Data { return {key.getOwned(), val.getOwned()}; });
    }

    void emplace(Key&& key, ValueProducer valProducer) override {
        addImpl(key, [&]() -> Data {
            key.makeOwned();
            auto val = valProducer();
            val.makeOwned();
            return {std::move(key), std::move(val)};
        });
    }

    Iterator* done() {
        if (_haveData) {
            if (this->_opts.moveSortedDataIntoIterator) {
                return new InMemIterator<Key, Value>(std::move(_best));
            }
            return new InMemIterator<Key, Value>(_best);
        } else {
            return new InMemIterator<Key, Value>();
        }
    }

    Iterator* pause() override {
        if (_haveData) {
            // ok to return InMemIterator as this is a single value constructed from copy
            return new InMemIterator<Key, Value>(_best);
        } else {
            return new InMemIterator<Key, Value>();
        }
    }

    void resume() override {}

private:
    void spill() {
        invariant(false, "LimitOneSorter does not spill to disk");
    }

    const Comparator _comp;
    Data _best;
    bool _haveData;  // false at start, set to true on first call to add()
};

template <typename Key, typename Value, typename Comparator>
class TopKSorter : public MergeableSorter<Key, Value, Comparator> {
public:
    typedef std::pair<Key, Value> Data;
    typedef std::function<Value()> ValueProducer;
    using Iterator = typename MergeableSorter<Key, Value, Comparator>::Iterator;
    using Settings = typename MergeableSorter<Key, Value, Comparator>::Settings;

    TopKSorter(const SortOptions& opts,
               const Comparator& comp,
               const Settings& settings = Settings())
        : MergeableSorter<Key, Value, Comparator>(opts, comp, settings),
          _haveCutoff(false),
          _worstCount(0),
          _medianCount(0) {
        // This also *works* with limit==1 but LimitOneSorter should be used instead
        invariant(opts.limit > 1);

        // Preallocate a fixed sized vector of the required size if we don't expect it to have a
        // major impact on our memory budget. This is the common case with small limits.
        if (opts.limit <
            std::min((opts.maxMemoryUsageBytes / 10) / sizeof(typename decltype(_data)::value_type),
                     _data.max_size())) {
            _data.reserve(opts.limit);
        }
    }

    template <typename DataProducer>
    void addImpl(const Key& key, DataProducer dataProducer) {
        invariant(!_done);
        invariant(!_paused);

        this->_stats.incrementNumSorted();

        STLComparator less(this->_comp);

        if (_data.size() < this->_opts.limit) {
            if (_haveCutoff && this->_comp(key, _cutoff.first) >= 0)
                return;

            // Invoking dataProducer could invalidate key if it uses move semantics,
            // don't reference them anymore from this point on.
            auto& keyVal = _data.emplace_back(dataProducer());

            auto memUsage = keyVal.first.memUsageForSorter() + keyVal.second.memUsageForSorter();
            this->_stats.incrementMemUsage(memUsage);

            if (_data.size() == this->_opts.limit)
                std::make_heap(_data.begin(), _data.end(), less);

            if (this->_stats.memUsage() > this->_opts.maxMemoryUsageBytes)
                spill();

            return;
        }

        invariant(_data.size() == this->_opts.limit);

        if (this->_comp(key, _data.front().first) >= 0)
            return;  // not good enough

        // Remove the old worst pair and insert the contender, adjusting _memUsed

        this->_stats.decrementMemUsage(_data.front().first.memUsageForSorter());
        this->_stats.decrementMemUsage(_data.front().second.memUsageForSorter());

        std::pop_heap(_data.begin(), _data.end(), less);

        // Invoking dataProducer could invalidate key if it uses move semantics,
        // don't reference them anymore from this point on.
        _data.back() = dataProducer();

        this->_stats.incrementMemUsage(_data.back().first.memUsageForSorter());
        this->_stats.incrementMemUsage(_data.back().second.memUsageForSorter());

        std::push_heap(_data.begin(), _data.end(), less);

        if (this->_stats.memUsage() > this->_opts.maxMemoryUsageBytes)
            spill();
    }

    void add(const Key& key, const Value& val) override {
        addImpl(key, [&]() -> Data { return {key.getOwned(), val.getOwned()}; });
    }

    void emplace(Key&& key, ValueProducer valProducer) override {
        addImpl(key, [&]() -> Data {
            key.makeOwned();
            auto val = valProducer();
            val.makeOwned();
            return {std::move(key), std::move(val)};
        });
    }

    Iterator* done() {
        if (this->_iters.empty()) {
            sort();
            if (this->_opts.moveSortedDataIntoIterator) {
                return new InMemIterator<Key, Value>(std::move(_data));
            }
            return new InMemIterator<Key, Value>(_data);
        }

        spill();
        this->_mergeSpillsToRespectMemoryLimits();

        Iterator* iterator = Iterator::merge(this->_iters, this->_opts, this->_comp);
        _done = true;
        return iterator;
    }

    Iterator* pause() override {
        invariant(!_done);
        invariant(!_paused);
        _paused = true;

        if (this->_iters.empty()) {
            return new InMemReadOnlyIterator<Key, Value, std::vector<Data>>(_data);
        }
        tassert(8248301, "Spilled sort cannot be paused", this->_iters.empty());
        return nullptr;
    }

    void resume() override {
        _paused = false;
    }

private:
    class STLComparator {
    public:
        explicit STLComparator(const Comparator& comp) : _comp(comp) {}
        bool operator()(const Data& lhs, const Data& rhs) const {
            dassertCompIsSane(_comp, lhs.first, rhs.first);
            return _comp(lhs.first, rhs.first) < 0;
        }

    private:
        const Comparator& _comp;
    };

    void sort() {
        STLComparator less(this->_comp);

        if (_data.size() == this->_opts.limit) {
            std::sort_heap(_data.begin(), _data.end(), less);
        } else {
            std::stable_sort(_data.begin(), _data.end(), less);
        }

        this->_stats.incrementBytesSorted(this->_stats.memUsage());
    }

    // Can only be called after _data is sorted
    void updateCutoff() {
        // Theory of operation: We want to be able to eagerly ignore values we know will not
        // be in the TopK result set by setting _cutoff to a value we know we have at least
        // K values equal to or better than. There are two values that we track to
        // potentially become the next value of _cutoff: _worstSeen and _lastMedian. When
        // one of these values becomes the new _cutoff, its associated counter is reset to 0
        // and a new value is chosen for that member the next time we spill.
        //
        // _worstSeen is the worst value we've seen so that all kept values are better than
        // (or equal to) it. This means that once _worstCount >= _opts.limit there is no
        // reason to consider values worse than _worstSeen so it can become the new _cutoff.
        // This technique is especially useful when the input is already roughly sorted (eg
        // sorting ASC on an ObjectId or Date field) since we will quickly find a cutoff
        // that will exclude most later values, making the full TopK operation including
        // the MergeIterator phase is O(K) in space and O(N + K*Log(K)) in time.
        //
        // _lastMedian was the median of the _data in the first spill() either overall or
        // following a promotion of _lastMedian to _cutoff. We count the number of kept
        // values that are better than or equal to _lastMedian in _medianCount and can
        // promote _lastMedian to _cutoff once _medianCount >=_opts.limit. Assuming
        // reasonable median selection (which should happen when the data is completely
        // unsorted), after the first K spilled values, we will keep roughly 50% of the
        // incoming values, 25% after the second K, 12.5% after the third K, etc. This means
        // that by the time we spill 3*K values, we will have seen (1*K + 2*K + 4*K) values,
        // so the expected number of kept values is O(Log(N/K) * K). The final run time if
        // using the O(K*Log(N)) merge algorithm in MergeIterator is O(N + K*Log(K) +
        // K*LogLog(N/K)) which is much closer to O(N) than O(N*Log(K)).
        //
        // This leaves a currently unoptimized worst case of data that is already roughly
        // sorted, but in the wrong direction, such that the desired results are all the
        // last ones seen. It will require O(N) space and O(N*Log(K)) time. Since this
        // should be trivially detectable, as a future optimization it might be nice to
        // detect this case and reverse the direction of input (if possible) which would
        // turn this into the best case described above.
        //
        // Pedantic notes: The time complexities above (which count number of comparisons)
        // ignore the sorting of batches prior to spilling to disk since they make it more
        // confusing without changing the results. If you want to add them back in, add an
        // extra term to each time complexity of (SPACE_COMPLEXITY * Log(BATCH_SIZE)). Also,
        // all space complexities measure disk space rather than memory since this class is
        // O(1) in memory due to the _opts.maxMemoryUsageBytes limit.

        STLComparator less(this->_comp);  // less is "better" for TopK.

        // Pick a new _worstSeen or _lastMedian if should.
        if (_worstCount == 0 || less(_worstSeen, _data.back())) {
            _worstSeen = _data.back();
        }
        if (_medianCount == 0) {
            size_t medianIndex = _data.size() / 2;  // chooses the higher if size() is even.
            _lastMedian = _data[medianIndex];
        }

        // Add the counters of kept objects better than or equal to _worstSeen/_lastMedian.
        _worstCount += _data.size();  // everything is better or equal
        typename std::vector<Data>::iterator firstWorseThanLastMedian =
            std::upper_bound(_data.begin(), _data.end(), _lastMedian, less);
        _medianCount += std::distance(_data.begin(), firstWorseThanLastMedian);


        // Promote _worstSeen or _lastMedian to _cutoff and reset counters if should.
        if (_worstCount >= this->_opts.limit) {
            if (!_haveCutoff || less(_worstSeen, _cutoff)) {
                _cutoff = _worstSeen;
                _haveCutoff = true;
            }
            _worstCount = 0;
        }
        if (_medianCount >= this->_opts.limit) {
            if (!_haveCutoff || less(_lastMedian, _cutoff)) {
                _cutoff = _lastMedian;
                _haveCutoff = true;
            }
            _medianCount = 0;
        }
    }

    void spill() {
        invariant(!_done);

        if (_data.empty())
            return;

        if (!this->_opts.extSortAllowed) {
            // This error message only applies to sorts from user queries made through the find or
            // aggregation commands. Other clients should suppress this error, either by allowing
            // external sorting or by catching and throwing a more appropriate error.
            uasserted(ErrorCodes::QueryExceededMemoryLimitNoDiskUseAllowed,
                      str::stream()
                          << "Sort exceeded memory limit of " << this->_opts.maxMemoryUsageBytes
                          << " bytes, but did not opt in to external sorting. Aborting operation."
                          << " Pass allowDiskUse:true to opt in.");
        }

        sort();
        updateCutoff();

        SortedFileWriter<Key, Value> writer(this->_opts, this->_file, this->_settings);
        for (size_t i = 0; i < _data.size(); i++) {
            writer.addAlreadySorted(_data[i].first, _data[i].second);
        }

        // clear _data and release backing array's memory
        std::vector<Data>().swap(_data);

        Iterator* iteratorPtr = writer.done();
        this->_iters.push_back(std::shared_ptr<Iterator>(iteratorPtr));

        this->_stats.resetMemUsage();
        this->_stats.incrementSpilledRanges();
    }

    bool _done = false;
    bool _paused = false;

    // Data that has not been spilled. Organized as max-heap if size == limit.
    std::vector<Data> _data;

    // See updateCutoff() for a full description of how these members are used.
    bool _haveCutoff;
    Data _cutoff;         // We can definitely ignore values worse than this.
    Data _worstSeen;      // The worst Data seen so far. Reset when _worstCount >= _opts.limit.
    size_t _worstCount;   // Number of docs better or equal to _worstSeen kept so far.
    Data _lastMedian;     // Median of a batch. Reset when _medianCount >= _opts.limit.
    size_t _medianCount;  // Number of docs better or equal to _lastMedian kept so far.
};

}  // namespace sorter

namespace {
SharedBufferFragmentBuilder makeMemPool() {
    return SharedBufferFragmentBuilder(
        gOperationMemoryPoolBlockInitialSizeKB.loadRelaxed() * static_cast<size_t>(1024),
        SharedBufferFragmentBuilder::DoubleGrowStrategy(
            gOperationMemoryPoolBlockMaxSizeKB.loadRelaxed() * static_cast<size_t>(1024)));
}
}  // namespace

template <typename Key, typename Value>
Sorter<Key, Value>::Sorter(const SortOptions& opts)
    : SorterBase(opts.sorterTracker),
      _opts(opts),
      _file(opts.extSortAllowed ? std::make_shared<Sorter<Key, Value>::File>(
                                      opts.tempDir + "/" + nextFileName(), opts.sorterFileStats)
                                : nullptr) {
    if (opts.useMemPool) {
        _memPool.emplace(makeMemPool());
    }
}

template <typename Key, typename Value>
Sorter<Key, Value>::Sorter(const SortOptions& opts, const std::string& fileName)
    : SorterBase(opts.sorterTracker),
      _opts(opts),
      _file(std::make_shared<Sorter<Key, Value>::File>(opts.tempDir + "/" + fileName,
                                                       opts.sorterFileStats)) {
    invariant(opts.extSortAllowed);
    invariant(!opts.tempDir.empty());
    invariant(!fileName.empty());
    if (opts.useMemPool) {
        _memPool.emplace(makeMemPool());
    }
}

template <typename Key, typename Value>
typename Sorter<Key, Value>::PersistedState Sorter<Key, Value>::persistDataForShutdown() {
    spill();
    this->_file->keep();

    std::vector<SorterRange> ranges;
    ranges.reserve(_iters.size());
    std::transform(_iters.begin(), _iters.end(), std::back_inserter(ranges), [](const auto it) {
        return it->getRange();
    });

    return {_file->path().filename().string(), ranges};
}

template <typename Key, typename Value>
Sorter<Key, Value>::File::File(std::string path, SorterFileStats* stats)
    : _path(std::move(path)), _stats(stats) {
    invariant(!_path.empty());
    if (_stats && boost::filesystem::exists(_path) && boost::filesystem::is_regular_file(_path)) {
        _stats->addSpilledDataSize(boost::filesystem::file_size(_path));
    }
}

template <typename Key, typename Value>
Sorter<Key, Value>::File::~File() {
    if (_stats && _file.is_open()) {
        _stats->closed.addAndFetch(1);
    }

    if (_keep) {
        return;
    }

    if (_file.is_open()) {
        DESTRUCTOR_GUARD(_file.exceptions(std::ios::failbit));
        DESTRUCTOR_GUARD(_file.close());
    }

    DESTRUCTOR_GUARD(boost::filesystem::remove(_path));
}

template <typename Key, typename Value>
void Sorter<Key, Value>::File::read(std::streamoff offset, std::streamsize size, void* out) {
    if (!_file.is_open()) {
        _open();
    }

    // If the _offset is not -1, we may have written data to it, so we must flush.
    if (_offset != -1) {
        _file.exceptions(std::ios::goodbit);
        _file.flush();
        _offset = -1;

        uassert(5479100,
                str::stream() << "Error flushing file " << _path.string() << ": "
                              << sorter::myErrnoWithDescription(),
                _file);
    }

    _file.seekg(offset);
    _file.read(reinterpret_cast<char*>(out), size);

    uassert(16817,
            str::stream() << "Error reading file " << _path.string() << ": "
                          << sorter::myErrnoWithDescription(),
            _file);

    invariant(_file.gcount() == size,
              str::stream() << "Number of bytes read (" << _file.gcount()
                            << ") not equal to expected number (" << size << ")");

    uassert(51049,
            str::stream() << "Error reading file " << _path.string() << ": "
                          << sorter::myErrnoWithDescription(),
            _file.tellg() >= 0);
}

template <typename Key, typename Value>
void Sorter<Key, Value>::File::write(const char* data, std::streamsize size) {
    _ensureOpenForWriting();

    try {
        _file.write(data, size);
        _offset += size;
        if (_stats) {
            this->_stats->addSpilledDataSize(size);
        };
    } catch (const std::system_error& ex) {
        if (ex.code() == std::errc::no_space_on_device) {
            uasserted(ErrorCodes::OutOfDiskSpace,
                      str::stream() << ex.what() << ": " << _path.string());
        }
        uasserted(5642403,
                  str::stream() << "Error writing to file " << _path.string() << ": "
                                << sorter::myErrnoWithDescription());
    } catch (const std::exception&) {
        uasserted(16821,
                  str::stream() << "Error writing to file " << _path.string() << ": "
                                << sorter::myErrnoWithDescription());
    }
}

template <typename Key, typename Value>
std::streamoff Sorter<Key, Value>::File::currentOffset() {
    _ensureOpenForWriting();
    invariant(_offset >= 0);
    return _offset;
}

template <typename Key, typename Value>
void Sorter<Key, Value>::File::_open() {
    invariant(!_file.is_open());

    boost::filesystem::create_directories(_path.parent_path());

    // We open the provided file in append mode so that SortedFileWriter instances can share
    // the same file, used serially. We want to share files in order to stay below system
    // open file limits.
    _file.open(_path.string(), std::ios::app | std::ios::binary | std::ios::in | std::ios::out);

    uassert(16818,
            str::stream() << "Error opening file " << _path.string() << ": "
                          << sorter::myErrnoWithDescription(),
            _file.good());

    if (_stats) {
        _stats->opened.addAndFetch(1);
    }
}

template <typename Key, typename Value>
void Sorter<Key, Value>::File::_ensureOpenForWriting() {
    if (!_file.is_open()) {
        _open();
    }

    // If we are opening the file for the first time, or if we previously flushed and switched to
    // read mode, we need to set the _offset to the file size.
    if (_offset == -1) {
        _file.exceptions(std::ios::failbit | std::ios::badbit);
        _offset = boost::filesystem::file_size(_path);
    }
}

//
// SortedFileWriter
//

template <typename Key, typename Value>
SortedFileWriter<Key, Value>::SortedFileWriter(
    const SortOptions& opts,
    std::shared_ptr<typename Sorter<Key, Value>::File> file,
    const Settings& settings)
    : _settings(settings),
      _file(std::move(file)),
      _checksumCalculator(_getSorterChecksumVersion()),
      _fileStartOffset(_file->currentOffset()),
      _opts(opts) {
    // This should be checked by consumers, but if we get here don't allow writes.
    uassert(16946,
            "Attempting to use external sort from mongos. This is not allowed.",
            !serverGlobalParams.clusterRole.hasExclusively(ClusterRole::RouterServer));

    uassert(17148,
            "Attempting to use external sort without setting SortOptions::tempDir",
            !opts.tempDir.empty());
}

template <typename Key, typename Value>
void SortedFileWriter<Key, Value>::addAlreadySorted(const Key& key, const Value& val) {

    // Offset that points to the place in the buffer where a new data object will be stored.
    int _nextObjPos = _buffer.len();

    // Add serialized key and value to the buffer.
    key.serializeForSorter(_buffer);
    val.serializeForSorter(_buffer);

    // Serializing the key and value grows the buffer, but _buffer.buf() still points to the
    // beginning. Use _buffer.len() to determine portion of buffer containing new datum.
    if (_checksumCalculator.version() == SorterChecksumVersion::v1) {
        _checksumCalculator.addData(_buffer.buf() + _nextObjPos, _buffer.len() - _nextObjPos);
    }

    if (_buffer.len() > static_cast<int>(kSortedFileBufferSize))
        writeChunk();
}

template <typename Key, typename Value>
void SortedFileWriter<Key, Value>::writeChunk() {
    int32_t size = _buffer.len();
    char* outBuffer = _buffer.buf();

    if (size == 0)
        return;

    if (_checksumCalculator.version() > SorterChecksumVersion::v1) {
        _checksumCalculator.addData(outBuffer, size);
    }

    if (_opts.sorterFileStats) {
        _opts.sorterFileStats->addSpilledDataSizeUncompressed(size);
    }

    std::string compressed;
    snappy::Compress(outBuffer, size, &compressed);
    invariant(compressed.size() <= size_t(std::numeric_limits<int32_t>::max()));

    const bool shouldCompress = compressed.size() < (size_t(_buffer.len()) / 10 * 9);
    if (shouldCompress) {
        size = compressed.size();
        outBuffer = const_cast<char*>(compressed.data());
    }

    std::unique_ptr<char[]> out;
    if (auto encryptionHooks = getEncryptionHooksIfEnabled()) {
        size_t protectedSizeMax = size + encryptionHooks->additionalBytesForProtectedBuffer();
        out.reset(new char[protectedSizeMax]);
        size_t resultLen;
        Status status = encryptionHooks->protectTmpData(reinterpret_cast<const uint8_t*>(outBuffer),
                                                        size,
                                                        reinterpret_cast<uint8_t*>(out.get()),
                                                        protectedSizeMax,
                                                        &resultLen,
                                                        _opts.dbName);
        uassert(28842,
                str::stream() << "Failed to compress data: " << status.toString(),
                status.isOK());
        outBuffer = out.get();
        size = resultLen;
    }

    // Negative size means compressed.
    size = shouldCompress ? -size : size;
    _file->write(reinterpret_cast<const char*>(&size), sizeof(size));
    _file->write(outBuffer, std::abs(size));

    _buffer.reset();
}

template <typename Key, typename Value>
SortIteratorInterface<Key, Value>* SortedFileWriter<Key, Value>::done() {
    writeChunk();

    return new sorter::FileIterator<Key, Value>(_file,
                                                _fileStartOffset,
                                                _file->currentOffset(),
                                                _settings,
                                                _opts.dbName,
                                                _checksumCalculator.checksum(),
                                                _checksumCalculator.version());
}

template <typename Key, typename Value>
std::shared_ptr<SortIteratorInterface<Key, Value>>
SortedFileWriter<Key, Value>::createFileIteratorForResume(
    std::shared_ptr<typename Sorter<Key, Value>::File> file,
    std::streamoff fileStartOffset,
    std::streamoff fileEndOffset,
    const Settings& settings,
    const boost::optional<DatabaseName>& dbName,
    const size_t checksum,
    const SorterChecksumVersion checksumVersion) {

    return std::shared_ptr<SortIteratorInterface<Key, Value>>(new sorter::FileIterator<Key, Value>(
        file, fileStartOffset, fileEndOffset, settings, dbName, checksum, checksumVersion));
}

template <typename Key, typename Value>
SorterChecksumVersion SortedFileWriter<Key, Value>::_getSorterChecksumVersion() const {
    // We need to use isEnabledUseLatestFCVWhenUninitialized instead of isEnabled because
    // this could run during currentOp which is allowed during initial sync while the FCV is still
    // uninitialized.
    if (gFeatureFlagUseSorterChecksumV2.isEnabledUseLatestFCVWhenUninitialized(
            serverGlobalParams.featureCompatibility.acquireFCVSnapshot())) {
        return SorterChecksumVersion::v2;
    }
    return SorterChecksumVersion::v1;
}

template <typename Key, typename Value, typename Comparator, typename BoundMaker>
BoundedSorter<Key, Value, Comparator, BoundMaker>::BoundedSorter(const SortOptions& opts,
                                                                 Comparator comp,
                                                                 BoundMaker makeBound,
                                                                 bool checkInput)
    : BoundedSorterInterface<Key, Value>(opts),
      compare(comp),
      makeBound(makeBound),
      _checkInput(checkInput),
      _opts(opts),
      _heap(Greater{&compare}),
      _file(opts.extSortAllowed ? std::make_shared<typename Sorter<Key, Value>::File>(
                                      opts.tempDir + "/" + nextFileName(), opts.sorterFileStats)
                                : nullptr) {}

template <typename Key, typename Value, typename Comparator, typename BoundMaker>
void BoundedSorter<Key, Value, Comparator, BoundMaker>::add(Key key, Value value) {
    invariant(!_done);
    // If a new value violates what we thought was our min bound, something has gone wrong.
    uassert(6369910,
            str::stream() << "BoundedSorter input is too out-of-order: with bound "
                          << _min->toString() << ", did not expect input " << key.toString(),
            !_checkInput || !_min || compare(*_min, key) <= 0);

    // Each new item can potentially give us a tighter bound (a higher min).
    Key newMin = makeBound(key, value);
    if (!_min || compare(*_min, newMin) < 0)
        _min = newMin;

    auto memUsage = key.memUsageForSorter() + value.memUsageForSorter();
    _heap.emplace(std::move(key), std::move(value));

    this->_stats.incrementMemUsage(memUsage);
    this->_stats.incrementBytesSorted(memUsage);
    if (this->_stats.memUsage() > _opts.maxMemoryUsageBytes)
        _spill();
}

template <typename Key, typename Value, typename Comparator, typename BoundMaker>
void BoundedSorter<Key, Value, Comparator, BoundMaker>::restart() {
    tassert(
        6434804, "BoundedSorter must be in state kDone to restart()", getState() == State::kDone);

    // In state kDone, the heap and spill are usually empty, because kDone means the sorter has
    // no more elements to return. However, if there is a limit then we can also reach state
    // kDone when 'this->_stats.numSorted() == _opts.limit'.
    _spillIter.reset();
    _heap = decltype(_heap){Greater{&compare}};
    this->_stats.resetMemUsage();

    _done = false;
    _min.reset();

    // There are now two possible states we could be in:
    // - Typically, we should be ready for more input (kWait).
    // - If there is a limit and we reached it, then we're done. We were done before restart()
    //   and we're still done.
    if (_opts.limit && this->_stats.numSorted() == _opts.limit) {
        tassert(6434806,
                "BoundedSorter has fulfilled _opts.limit and should still be in state kDone",
                getState() == State::kDone);
    } else {
        tassert(6434805, "BoundedSorter should now be ready for input", getState() == State::kWait);
    }
}

template <typename Key, typename Value, typename Comparator, typename BoundMaker>
typename BoundedSorterInterface<Key, Value>::State
BoundedSorter<Key, Value, Comparator, BoundMaker>::getState() const {
    if (_opts.limit > 0 && _opts.limit == this->_stats.numSorted()) {
        return State::kDone;
    }

    if (_done) {
        // No more input will arrive, so we're never in state kWait.
        return _heap.empty() && !_spillIter ? State::kDone : State::kReady;
    }

    if (_heap.empty() && !_spillIter)
        return State::kWait;

    // _heap.top() is the min of _heap, but we also need to consider whether a smaller input
    // will arrive later. So _heap.top() is safe to return only if _heap.top() < _min.
    if (!_heap.empty() && compare(_heap.top().first, *_min) < 0)
        return State::kReady;

    // Similarly, we can return the next element from the spilled iterator if it's < _min.
    if (_spillIter && compare(_spillIter->current(), *_min) < 0)
        return State::kReady;

    // A later call to add() may improve _min. Or in the worst case, after done() is called
    // we will return everything in _heap.
    return State::kWait;
}

template <typename Key, typename Value, typename Comparator, typename BoundMaker>
std::pair<Key, Value> BoundedSorter<Key, Value, Comparator, BoundMaker>::next() {
    dassert(getState() == State::kReady);
    std::pair<Key, Value> result;

    auto pullFromHeap = [this, &result]() {
        result = std::move(_heap.top());
        _heap.pop();

        auto memUsage = result.first.memUsageForSorter() + result.second.memUsageForSorter();
        if (static_cast<int64_t>(memUsage) > static_cast<int64_t>(this->_stats.memUsage())) {
            this->_stats.resetMemUsage();
        } else {
            this->_stats.decrementMemUsage(memUsage);
        }
    };

    auto pullFromSpilled = [this, &result]() {
        result = _spillIter->next();
        if (!_spillIter->more()) {
            _spillIter.reset();
        }
    };

    if (!_heap.empty() && _spillIter) {
        if (compare(_heap.top().first, _spillIter->current()) <= 0) {
            pullFromHeap();
        } else {
            pullFromSpilled();
        }
    } else if (!_heap.empty()) {
        pullFromHeap();
    } else {
        pullFromSpilled();
    }

    this->_stats.incrementNumSorted();

    return result;
}

template <typename Key, typename Value, typename Comparator, typename BoundMaker>
void BoundedSorter<Key, Value, Comparator, BoundMaker>::_spill() {
    if (_heap.empty())
        return;

    // If we have a small $limit, we can simply extract that many of the smallest elements from
    // the _heap and discard the rest, avoiding an expensive spill to disk.
    if (_opts.limit > 0 && _opts.limit < (_heap.size() / 2)) {
        this->_stats.resetMemUsage();
        decltype(_heap) retained{Greater{&compare}};
        for (size_t i = 0; i < _opts.limit; ++i) {
            this->_stats.incrementMemUsage(_heap.top().first.memUsageForSorter() +
                                           _heap.top().second.memUsageForSorter());
            retained.emplace(_heap.top());
            _heap.pop();
        }
        _heap.swap(retained);

        if (this->_stats.memUsage() < _opts.maxMemoryUsageBytes) {
            return;
        }
    }

    uassert(ErrorCodes::QueryExceededMemoryLimitNoDiskUseAllowed,
            str::stream() << "Sort exceeded memory limit of " << this->_opts.maxMemoryUsageBytes
                          << " bytes, but did not opt in to external sorting.",
            _opts.extSortAllowed);

    this->_stats.incrementSpilledRanges();

    // Write out all the values from the heap in sorted order.
    SortedFileWriter<Key, Value> writer(_opts, _file, {});
    while (!_heap.empty()) {
        writer.addAlreadySorted(_heap.top().first, _heap.top().second);
        _heap.pop();
    }
    std::shared_ptr<SpillIterator> iteratorPtr(writer.done());

    if (auto* mergeIter = static_cast<typename sorter::MergeIterator<Key, Value, Comparator>*>(
            _spillIter.get())) {
        mergeIter->addSource(std::move(iteratorPtr));
    } else {
        std::vector<std::shared_ptr<SpillIterator>> iters{std::move(iteratorPtr)};
        _spillIter.reset(SpillIterator::merge(iters, _opts, compare));
    }

    dassert(_spillIter->more());

    this->_stats.resetMemUsage();
}

//
// Factory Functions
//

template <typename Key, typename Value>
template <typename Comparator>
SortIteratorInterface<Key, Value>* SortIteratorInterface<Key, Value>::merge(
    const std::vector<std::shared_ptr<SortIteratorInterface>>& iters,
    const SortOptions& opts,
    const Comparator& comp) {
    return new sorter::MergeIterator<Key, Value, Comparator>(iters, opts, comp);
}

template <typename Key, typename Value>
template <typename Comparator>
Sorter<Key, Value>* Sorter<Key, Value>::make(const SortOptions& opts,
                                             const Comparator& comp,
                                             const Settings& settings) {
    checkNoExternalSortOnMongos(opts);

    uassert(17149,
            "Attempting to use external sort without setting SortOptions::tempDir",
            !(opts.extSortAllowed && opts.tempDir.empty()));
    switch (opts.limit) {
        case 0:
            return new sorter::NoLimitSorter<Key, Value, Comparator>(opts, comp, settings);
        case 1:
            return new sorter::LimitOneSorter<Key, Value, Comparator>(opts, comp);
        default:
            return new sorter::TopKSorter<Key, Value, Comparator>(opts, comp, settings);
    }
}

template <typename Key, typename Value>
template <typename Comparator>
Sorter<Key, Value>* Sorter<Key, Value>::makeFromExistingRanges(
    const std::string& fileName,
    const std::vector<SorterRange>& ranges,
    const SortOptions& opts,
    const Comparator& comp,
    const Settings& settings) {
    checkNoExternalSortOnMongos(opts);

    invariant(opts.limit == 0,
              str::stream() << "Creating a Sorter from existing ranges is only available with the "
                               "NoLimitSorter (limit 0), but got limit "
                            << opts.limit);

    return new sorter::NoLimitSorter<Key, Value, Comparator>(
        fileName, ranges, opts, comp, settings);
}
}  // namespace mongo
#undef MONGO_LOGV2_DEFAULT_COMPONENT
