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


#include <boost/optional/optional.hpp>
#include <cstddef>
#include <optional>

#include "mongo/base/string_data.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/db/commands/server_status.h"
#include "mongo/db/operation_context.h"

#ifdef MONGO_HAVE_GOOGLE_TCMALLOC
#include <tcmalloc/malloc_extension.h>
#elif defined(MONGO_HAVE_GPERF_TCMALLOC)
#include <gperftools/malloc_extension.h>
#endif


#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kDefault

namespace mongo {
namespace {

template <typename E>
auto getUnderlyingType(E e) {
    return static_cast<std::underlying_type_t<E>>(e);
}

/**
 * For more information about tcmalloc stats, see:
 * https://github.com/google/tcmalloc/blob/master/docs/stats.md and
 * https://github.com/google/tcmalloc/blob/master/tcmalloc/malloc_extension.h
 * for the tcmalloc-google, and
 * https://github.com/gperftools/gperftools/blob/master/docs/tcmalloc.html and
 * https://github.com/gperftools/gperftools/blob/master/src/gperftools/malloc_extension.h
 * for tcmalloc-gperf
 */
class TCMallocMetrics {
public:
    virtual std::vector<StringData> getGenericStatNames() const {
        return {};
    }

    virtual std::vector<StringData> getTCMallocStatNames() const {
        return {};
    }

    virtual boost::optional<size_t> getNumericProperty(StringData propertyName) const {
        return boost::none;
    }

    virtual void appendPerCPUMetrics(BSONObjBuilder& bob) const {}

    virtual long long getReleaseRate() const {
        return 0;
    }

    virtual void appendHighVerbosityMetrics(BSONObjBuilder& bob) const {}

    virtual void appendFormattedString(BSONObjBuilder& bob) const {}

    virtual void appendCustomDerivedMetrics(BSONObjBuilder& bob) const {}
};

#ifdef MONGO_HAVE_GOOGLE_TCMALLOC
class GoogleTCMallocMetrics : public TCMallocMetrics {
public:
    std::vector<StringData> getGenericStatNames() const override {
        return {
            "bytes_in_use_by_app"_sd,
            "current_allocated_bytes"_sd,
            "heap_size"_sd,
            "peak_memory_usage"_sd,
            "physical_memory_used"_sd,
            "realized_fragmentation"_sd,
            "virtual_memory_used"_sd,
        };
    }

    std::vector<StringData> getTCMallocStatNames() const override {
        return {
            "central_cache_free"_sd,
            "cpu_free"_sd,
            "current_total_thread_cache_bytes"_sd,
            "desired_usage_limit_bytes"_sd,
            "external_fragmentation_bytes"_sd,
            "hard_usage_limit_bytes"_sd,
            "local_bytes"_sd,
            "max_total_thread_cache_bytes"_sd,
            "metadata_bytes"_sd,
            "page_algorithm"_sd,
            "pageheap_free_bytes"_sd,
            "pageheap_unmapped_bytes"_sd,
            "required_bytes"_sd,
            "sampled_internal_fragmentation"_sd,
            "sharded_transfer_cache_free"_sd,
            "thread_cache_count"_sd,
            "thread_cache_free"_sd,
            "transfer_cache_free"_sd,
        };
    }

    boost::optional<size_t> getNumericProperty(StringData propertyName) const override {
        if (auto res = tcmalloc::MallocExtension::GetNumericProperty(std::string{propertyName});
            res.has_value()) {
            return res.value();
        }

        return boost::none;
    }

    void appendPerCPUMetrics(BSONObjBuilder& bob) const override {
        _perCPUCachesActive =
            _perCPUCachesActive || tcmalloc::MallocExtension::PerCpuCachesActive();
        bob.appendBool("usingPerCPUCaches", _perCPUCachesActive);
        bob.append("maxPerCPUCacheSize", tcmalloc::MallocExtension::GetMaxPerCpuCacheSize());
    }

    long long getReleaseRate() const override {
        return getUnderlyingType(tcmalloc::MallocExtension::GetBackgroundReleaseRate());
    }

    void appendCustomDerivedMetrics(BSONObjBuilder& bob) const override {
        if (auto physicalMemory = getNumericProperty("generic.physical_memory_used");
            !!physicalMemory) {
            if (auto virtualMemory = getNumericProperty("generic.virtual_memory_used");
                !!virtualMemory) {
                long long unmappedBytes = *virtualMemory - *physicalMemory;
                bob.appendNumber("unmapped_bytes", unmappedBytes);
            }
        }
    }

private:
    // Once per-CPU caches are activated, they cannot be deactivated, and so we cache the true value
    // in order to avoid the FTDC thread loading a contested atomic from tcmalloc when it does not
    // need to.
    static inline bool _perCPUCachesActive = false;
};
#elif defined(MONGO_HAVE_GPERF_TCMALLOC)
class GperfTCMallocMetrics : public TCMallocMetrics {
public:
    std::vector<StringData> getGenericStatNames() const override {
        return {
            "current_allocated_bytes"_sd,
            "heap_size"_sd,
        };
    }

    std::vector<StringData> getTCMallocStatNames() const override {
        return {
            "pageheap_free_bytes"_sd,
            "pageheap_unmapped_bytes"_sd,
            "max_total_thread_cache_bytes"_sd,
            "current_total_thread_cache_bytes"_sd,
            "central_cache_free_bytes"_sd,
            "transfer_cache_free_bytes"_sd,
            "thread_cache_free_bytes"_sd,
            "aggressive_memory_decommit"_sd,
            "pageheap_committed_bytes"_sd,
            "pageheap_scavenge_count"_sd,
            "pageheap_commit_count"_sd,
            "pageheap_total_commit_bytes"_sd,
            "pageheap_decommit_count"_sd,
            "pageheap_total_decommit_bytes"_sd,
            "pageheap_reserve_count"_sd,
            "pageheap_total_reserve_bytes"_sd,
            "spinlock_total_delay_ns"_sd,
        };
    }

    boost::optional<size_t> getNumericProperty(StringData propertyName) const override {
        size_t value;
        if (MallocExtension::instance()->GetNumericProperty(propertyName.rawData(), &value)) {
            return {value};
        }

        return boost::none;
    }

    long long getReleaseRate() const override {
        return MallocExtension::instance()->GetMemoryReleaseRate();
    }

    void appendHighVerbosityMetrics(BSONObjBuilder& bob) const override {
#if MONGO_HAVE_GPERFTOOLS_SIZE_CLASS_STATS
        // Size class information
        std::pair<BSONArrayBuilder, BSONArrayBuilder> builders(bob.subarrayStart("size_classes"),
                                                               BSONArrayBuilder());

        // Size classes and page heap info is dumped in 1 call so that the performance
        // sensitive tcmalloc page heap lock is only taken once
        MallocExtension::instance()->SizeClasses(
            &builders, appendSizeClassInfo, appendPageHeapInfo);

        builders.first.done();
        bob.append("page_heap", builders.second.arr());
#endif  // MONGO_HAVE_GPERFTOOLS_SIZE_CLASS_STATS
    }

    void appendFormattedString(BSONObjBuilder& bob) const override {
        char buffer[4096];
        MallocExtension::instance()->GetStats(buffer, sizeof buffer);
        bob.append("formattedString", buffer);
    }

private:
#if MONGO_HAVE_GPERFTOOLS_SIZE_CLASS_STATS
    static void appendSizeClassInfo(void* bsonarr_builder, const base::MallocSizeClass* stats) {
        BSONArrayBuilder& builder =
            reinterpret_cast<std::pair<BSONArrayBuilder, BSONArrayBuilder>*>(bsonarr_builder)
                ->first;
        BSONObjBuilder doc;

        doc.appendNumber("bytes_per_object", static_cast<long long>(stats->bytes_per_obj));
        doc.appendNumber("pages_per_span", static_cast<long long>(stats->pages_per_span));
        doc.appendNumber("num_spans", static_cast<long long>(stats->num_spans));
        doc.appendNumber("num_thread_objs", static_cast<long long>(stats->num_thread_objs));
        doc.appendNumber("num_central_objs", static_cast<long long>(stats->num_central_objs));
        doc.appendNumber("num_transfer_objs", static_cast<long long>(stats->num_transfer_objs));
        doc.appendNumber("free_bytes", static_cast<long long>(stats->free_bytes));
        doc.appendNumber("allocated_bytes", static_cast<long long>(stats->alloc_bytes));

        builder.append(doc.obj());
    }

    static void appendPageHeapInfo(void* bsonarr_builder, const base::PageHeapSizeClass* stats) {
        BSONArrayBuilder& builder =
            reinterpret_cast<std::pair<BSONArrayBuilder, BSONArrayBuilder>*>(bsonarr_builder)
                ->second;
        BSONObjBuilder doc;

        doc.appendNumber("pages", static_cast<long long>(stats->pages));
        doc.appendNumber("normal_spans", static_cast<long long>(stats->normal_spans));
        doc.appendNumber("unmapped_spans", static_cast<long long>(stats->unmapped_spans));
        doc.appendNumber("normal_bytes", static_cast<long long>(stats->normal_bytes));
        doc.appendNumber("unmapped_bytes", static_cast<long long>(stats->unmapped_bytes));

        builder.append(doc.obj());
    }
#endif  // MONGO_HAVE_GPERFTOOLS_SIZE_CLASS_STATS
};
#endif  // MONGO_HAVE_GPERF_TCMALLOC

class TCMallocServerStatusSection : public ServerStatusSection {
public:
    TCMallocServerStatusSection() : ServerStatusSection("tcmalloc") {}

    bool includeByDefault() const override {
        return true;
    }

    BSONObj generateSection(OperationContext* opCtx,
                            const BSONElement& configElement) const override {
        long long verbosity = 1;
        if (configElement) {
            // Relies on the fact that safeNumberLong turns non-numbers into 0.
            long long configValue = configElement.safeNumberLong();
            if (configValue) {
                verbosity = configValue;
            }
        }

        BSONObjBuilder builder;

        auto tryAppend = [&](BSONObjBuilder& builder, StringData bsonName, StringData property) {
            if (auto value = _metrics.getNumericProperty(property); !!value) {
                builder.appendNumber(bsonName, static_cast<long long>(*value));
            }
        };

        auto tryStat = [&](BSONObjBuilder& builder, StringData topic, StringData base) {
            tryAppend(builder, base, format(FMT_STRING("{}.{}"), topic, base));
        };

        _metrics.appendPerCPUMetrics(builder);
        {
            BSONObjBuilder sub(builder.subobjStart("generic"));
            for (auto& stat : _metrics.getGenericStatNames()) {
                tryStat(sub, "generic", stat);
            }
        }

        {
            BSONObjBuilder sub(builder.subobjStart("tcmalloc"));
            for (auto& stat : _metrics.getTCMallocStatNames()) {
                tryStat(sub, "tcmalloc", stat);
            }

            sub.appendNumber("release_rate", _metrics.getReleaseRate());

            if (verbosity >= 2) {
                _metrics.appendHighVerbosityMetrics(builder);
            }

            _metrics.appendFormattedString(builder);
        }

        {
            BSONObjBuilder sub(builder.subobjStart("tcmalloc_derived"));
            _metrics.appendCustomDerivedMetrics(builder);

            static constexpr std::array totalFreeBytesParts{
                "tcmalloc.pageheap_free_bytes"_sd,
                "tcmalloc.central_cache_free"_sd,
                "tcmalloc.transfer_cache_free"_sd,
                "tcmalloc.thread_cache_free"_sd,
                "tcmalloc.cpu_free"_sd,  // Will be 0 for gperf tcmalloc
            };
            long long total = 0;
            for (auto& stat : totalFreeBytesParts) {
                if (auto value = _metrics.getNumericProperty(stat); !!value) {
                    total += *value;
                }
            }
            sub.appendNumber("total_free_bytes", total);
        }

        return builder.obj();
    }

private:
#ifdef MONGO_HAVE_GOOGLE_TCMALLOC
    using MyMetrics = GoogleTCMallocMetrics;
#elif defined(MONGO_HAVE_GPERF_TCMALLOC)
    using MyMetrics = GperfTCMallocMetrics;
#else
    using MyMetrics = TCMallocMetrics;
#endif

    MyMetrics _metrics;
};
TCMallocServerStatusSection tcmallocServerStatusSection;
}  // namespace
}  // namespace mongo
