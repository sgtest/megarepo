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

#include "mongo/db/stats/counters.h"

#include <fmt/format.h>
#include <tuple>


#include "mongo/base/error_codes.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/client/authenticate.h"
#include "mongo/db/commands/server_status.h"
#include "mongo/db/operation_context.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kDefault

namespace mongo {

using namespace fmt::literals;

void OpCounters::_reset() {
    _insert->store(0);
    _query->store(0);
    _update->store(0);
    _delete->store(0);
    _getmore->store(0);
    _command->store(0);
    _nestedAggregate->store(0);

    _queryDeprecated->store(0);

    _insertOnExistingDoc->store(0);
    _updateOnMissingDoc->store(0);
    _deleteWasEmpty->store(0);
    _deleteFromMissingNamespace->store(0);
    _acceptableErrorInCommand->store(0);
}

void OpCounters::_checkWrap(CacheExclusive<AtomicWord<long long>> OpCounters::*counter, int n) {
    static constexpr auto maxCount = 1LL << 60;
    auto oldValue = (this->*counter)->fetchAndAddRelaxed(n);
    if (oldValue > maxCount) {
        _reset();
    }
}

BSONObj OpCounters::getObj() const {
    BSONObjBuilder b;
    b.append("insert", _insert->loadRelaxed());
    b.append("query", _query->loadRelaxed());
    b.append("update", _update->loadRelaxed());
    b.append("delete", _delete->loadRelaxed());
    b.append("getmore", _getmore->loadRelaxed());
    b.append("command", _command->loadRelaxed());

    auto queryDep = _queryDeprecated->loadRelaxed();
    if (queryDep > 0) {
        BSONObjBuilder d(b.subobjStart("deprecated"));
        d.append("query", queryDep);
    }

    // Append counters for constraint relaxations, only if they exist.
    auto insertOnExistingDoc = _insertOnExistingDoc->loadRelaxed();
    auto updateOnMissingDoc = _updateOnMissingDoc->loadRelaxed();
    auto deleteWasEmpty = _deleteWasEmpty->loadRelaxed();
    auto deleteFromMissingNamespace = _deleteFromMissingNamespace->loadRelaxed();
    auto acceptableErrorInCommand = _acceptableErrorInCommand->loadRelaxed();
    auto totalRelaxed = insertOnExistingDoc + updateOnMissingDoc + deleteWasEmpty +
        deleteFromMissingNamespace + acceptableErrorInCommand;

    if (totalRelaxed > 0) {
        BSONObjBuilder d(b.subobjStart("constraintsRelaxed"));
        d.append("insertOnExistingDoc", insertOnExistingDoc);
        d.append("updateOnMissingDoc", updateOnMissingDoc);
        d.append("deleteWasEmpty", deleteWasEmpty);
        d.append("deleteFromMissingNamespace", deleteFromMissingNamespace);
        d.append("acceptableErrorInCommand", acceptableErrorInCommand);
    }

    return b.obj();
}

void NetworkCounter::hitPhysicalIn(long long bytes) {
    static const int64_t MAX = 1ULL << 60;

    // don't care about the race as its just a counter
    const bool overflow = _physicalBytesIn->loadRelaxed() > MAX;

    if (overflow) {
        _physicalBytesIn->store(bytes);
    } else {
        _physicalBytesIn->fetchAndAdd(bytes);
    }
}

void NetworkCounter::hitPhysicalOut(long long bytes) {
    static const int64_t MAX = 1ULL << 60;

    // don't care about the race as its just a counter
    const bool overflow = _physicalBytesOut->loadRelaxed() > MAX;

    if (overflow) {
        _physicalBytesOut->store(bytes);
    } else {
        _physicalBytesOut->fetchAndAdd(bytes);
    }
}

void NetworkCounter::hitLogicalIn(long long bytes) {
    static const int64_t MAX = 1ULL << 60;

    // don't care about the race as its just a counter
    const bool overflow = _together->logicalBytesIn.loadRelaxed() > MAX;

    if (overflow) {
        _together->logicalBytesIn.store(bytes);
        // The requests field only gets incremented here (and not in hitPhysical) because the
        // hitLogical and hitPhysical are each called for each operation. Incrementing it in both
        // functions would double-count the number of operations.
        _together->requests.store(1);
    } else {
        _together->logicalBytesIn.fetchAndAdd(bytes);
        _together->requests.fetchAndAdd(1);
    }
}

void NetworkCounter::hitLogicalOut(long long bytes) {
    static const int64_t MAX = 1ULL << 60;

    // don't care about the race as its just a counter
    const bool overflow = _logicalBytesOut->loadRelaxed() > MAX;

    if (overflow) {
        _logicalBytesOut->store(bytes);
    } else {
        _logicalBytesOut->fetchAndAdd(bytes);
    }
}

void NetworkCounter::incrementNumSlowDNSOperations() {
    _numSlowDNSOperations->fetchAndAdd(1);
}

void NetworkCounter::incrementNumSlowSSLOperations() {
    _numSlowSSLOperations->fetchAndAdd(1);
}

void NetworkCounter::acceptedTFOIngress() {
    _tfoAccepted->fetchAndAddRelaxed(1);
}

void NetworkCounter::append(BSONObjBuilder& b) {
    b.append("bytesIn", static_cast<long long>(_together->logicalBytesIn.loadRelaxed()));
    b.append("bytesOut", static_cast<long long>(_logicalBytesOut->loadRelaxed()));
    b.append("physicalBytesIn", static_cast<long long>(_physicalBytesIn->loadRelaxed()));
    b.append("physicalBytesOut", static_cast<long long>(_physicalBytesOut->loadRelaxed()));
    b.append("numSlowDNSOperations", static_cast<long long>(_numSlowDNSOperations->loadRelaxed()));
    b.append("numSlowSSLOperations", static_cast<long long>(_numSlowSSLOperations->loadRelaxed()));
    b.append("numRequests", static_cast<long long>(_together->requests.loadRelaxed()));

    BSONObjBuilder tfo;
#ifdef __linux__
    tfo.append("kernelSetting", _tfoKernelSetting);
#endif
    tfo.append("serverSupported", _tfoKernelSupportServer);
    tfo.append("clientSupported", _tfoKernelSupportClient);
    tfo.append("accepted", _tfoAccepted->loadRelaxed());
    b.append("tcpFastOpen", tfo.obj());
}

void AuthCounter::initializeMechanismMap(const std::vector<std::string>& mechanisms) {
    invariant(_mechanisms.empty());

    const auto addMechanism = [this](const auto& mech) {
        _mechanisms.emplace(
            std::piecewise_construct, std::forward_as_tuple(mech), std::forward_as_tuple());
    };

    for (const auto& mech : mechanisms) {
        addMechanism(mech);
    }

    // When clusterAuthMode == `x509` or `sendX509`, we'll use MONGODB-X509 for intra-cluster auth
    // even if it's not explicitly enabled by authenticationMechanisms.
    // Ensure it's always included in counts.
    addMechanism(auth::kMechanismMongoX509.toString());

    // It's possible for intracluster auth to use a default fallback mechanism of SCRAM-SHA-256
    // even if it's not configured to do so.
    // Explicitly add this to the map for now so that they can be incremented if this happens.
    addMechanism(auth::kMechanismScramSha256.toString());
}

void AuthCounter::incSaslSupportedMechanismsReceived() {
    _saslSupportedMechanismsReceived.fetchAndAddRelaxed(1);
}

void AuthCounter::incAuthenticationCumulativeTime(long long micros) {
    _authenticationCumulativeMicros.fetchAndAddRelaxed(micros);
}

void AuthCounter::MechanismCounterHandle::incSpeculativeAuthenticateReceived() {
    _data->speculativeAuthenticate.received.fetchAndAddRelaxed(1);
}

void AuthCounter::MechanismCounterHandle::incSpeculativeAuthenticateSuccessful() {
    _data->speculativeAuthenticate.successful.fetchAndAddRelaxed(1);
}

void AuthCounter::MechanismCounterHandle::incAuthenticateReceived() {
    _data->authenticate.received.fetchAndAddRelaxed(1);
}

void AuthCounter::MechanismCounterHandle::incAuthenticateSuccessful() {
    _data->authenticate.successful.fetchAndAddRelaxed(1);
}

void AuthCounter::MechanismCounterHandle::incClusterAuthenticateReceived() {
    _data->clusterAuthenticate.received.fetchAndAddRelaxed(1);
}

void AuthCounter::MechanismCounterHandle::incClusterAuthenticateSuccessful() {
    _data->clusterAuthenticate.successful.fetchAndAddRelaxed(1);
}

auto AuthCounter::getMechanismCounter(StringData mechanism) -> MechanismCounterHandle {
    auto it = _mechanisms.find(mechanism.rawData());
    uassert(ErrorCodes::MechanismUnavailable,
            "Received authentication for mechanism {} which is not enabled"_format(mechanism),
            it != _mechanisms.end());

    auto& data = it->second;
    return MechanismCounterHandle(&data);
}

/**
 * authentication: {
 *   "mechanisms": {
 *     "SCRAM-SHA-256": {
 *       "speculativeAuthenticate": { received: ###, successful: ### },
 *       "authenticate": { received: ###, successful: ### },
 *     },
 *     "MONGODB-X509": {
 *       "speculativeAuthenticate": { received: ###, successful: ### },
 *       "authenticate": { received: ###, successful: ### },
 *     },
 *   },
 * }
 */
void AuthCounter::append(BSONObjBuilder* b) {
    const auto ssmReceived = _saslSupportedMechanismsReceived.load();
    b->append("saslSupportedMechsReceived", ssmReceived);

    BSONObjBuilder mechsBuilder(b->subobjStart("mechanisms"));

    for (const auto& it : _mechanisms) {
        BSONObjBuilder mechBuilder(mechsBuilder.subobjStart(it.first));

        {
            const auto received = it.second.speculativeAuthenticate.received.load();
            const auto successful = it.second.speculativeAuthenticate.successful.load();

            BSONObjBuilder specAuthBuilder(mechBuilder.subobjStart(auth::kSpeculativeAuthenticate));
            specAuthBuilder.append("received", received);
            specAuthBuilder.append("successful", successful);
            specAuthBuilder.done();
        }

        {
            const auto received = it.second.clusterAuthenticate.received.load();
            const auto successful = it.second.clusterAuthenticate.successful.load();

            BSONObjBuilder clusterAuthBuilder(mechBuilder.subobjStart(auth::kClusterAuthenticate));
            clusterAuthBuilder.append("received", received);
            clusterAuthBuilder.append("successful", successful);
            clusterAuthBuilder.done();
        }

        {
            const auto received = it.second.authenticate.received.load();
            const auto successful = it.second.authenticate.successful.load();

            BSONObjBuilder authBuilder(mechBuilder.subobjStart(auth::kAuthenticateCommand));
            authBuilder.append("received", received);
            authBuilder.append("successful", successful);
            authBuilder.done();
        }

        mechBuilder.done();
    }

    mechsBuilder.done();

    const auto totalAuthenticationTimeMicros = _authenticationCumulativeMicros.load();
    b->append("totalAuthenticationTimeMicros", totalAuthenticationTimeMicros);
}

OpCounterServerStatusSection::OpCounterServerStatusSection(const std::string& sectionName,
                                                           OpCounters* counters)
    : ServerStatusSection(sectionName), _counters(counters) {}

BSONObj OpCounterServerStatusSection::generateSection(OperationContext* opCtx,
                                                      const BSONElement& configElement) const {
    return _counters->getObj();
}

OpCounters globalOpCounters;
OpCounters replOpCounters;
NetworkCounter networkCounter;
AuthCounter authCounter;
AggStageCounters aggStageCounters{"aggStageCounters."};
DotsAndDollarsFieldsCounters dotsAndDollarsFieldsCounters;
QueryFrameworkCounters queryFrameworkCounters;
LookupPushdownCounters lookupPushdownCounters;
SortCounters sortCounters;
ValidatorCounters validatorCounters;
GroupCounters groupCounters;
PlanCacheCounters planCacheCounters;

OperatorCounters operatorCountersAggExpressions{"operatorCounters.expressions."};
OperatorCounters operatorCountersMatchExpressions{"operatorCounters.match."};
OperatorCounters operatorCountersGroupAccumulatorExpressions{"operatorCounters.groupAccumulators."};
OperatorCounters operatorCountersWindowAccumulatorExpressions{
    "operatorCounters.windowAccumulators."};

#define DEFN_QUERY_COUNTER(var)                  \
    Counter64& var = *MetricBuilder<Counter64> { \
        std::string{"query."} + #var             \
    }
DEFN_QUERY_COUNTER(updateManyCount);
DEFN_QUERY_COUNTER(deleteManyCount);
DEFN_QUERY_COUNTER(updateOneTargetedShardedCount);
DEFN_QUERY_COUNTER(deleteOneTargetedShardedCount);
DEFN_QUERY_COUNTER(findAndModifyTargetedShardedCount);
DEFN_QUERY_COUNTER(updateOneUnshardedCount);
DEFN_QUERY_COUNTER(deleteOneUnshardedCount);
DEFN_QUERY_COUNTER(findAndModifyUnshardedCount);
DEFN_QUERY_COUNTER(updateOneNonTargetedShardedCount);
DEFN_QUERY_COUNTER(deleteOneNonTargetedShardedCount);
DEFN_QUERY_COUNTER(findAndModifyNonTargetedShardedCount);
DEFN_QUERY_COUNTER(deleteOneWithoutShardKeyWithIdCount);
DEFN_QUERY_COUNTER(updateOneWithoutShardKeyWithIdRetryCount);
DEFN_QUERY_COUNTER(deleteOneWithoutShardKeyWithIdRetryCount);
#undef DEFN_QUERY_COUNTER
}  // namespace mongo
