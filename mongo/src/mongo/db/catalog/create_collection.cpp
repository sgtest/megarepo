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

#include "mongo/db/catalog/create_collection.h"

#include <boost/move/utility_core.hpp>
#include <boost/none.hpp>
#include <boost/optional.hpp>
#include <boost/optional/optional.hpp>
#include <cstdint>
#include <fmt/printf.h>  // IWYU pragma: keep
#include <memory>
#include <string>
#include <utility>
#include <variant>

#include "mongo/base/error_codes.h"
#include "mongo/base/status_with.h"
#include "mongo/base/string_data.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/bson/bsonmisc.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/bson/simple_bsonobj_comparator.h"
#include "mongo/db/catalog/clustered_collection_options_gen.h"
#include "mongo/db/catalog/clustered_collection_util.h"
#include "mongo/db/catalog/collection.h"
#include "mongo/db/catalog/collection_catalog.h"
#include "mongo/db/catalog/collection_catalog_helper.h"
#include "mongo/db/catalog/collection_options.h"
#include "mongo/db/catalog/database.h"
#include "mongo/db/catalog/database_holder.h"
#include "mongo/db/catalog/index_key_validate.h"
#include "mongo/db/catalog/unique_collection_name.h"
#include "mongo/db/catalog/virtual_collection_options.h"
#include "mongo/db/catalog_raii.h"
#include "mongo/db/commands.h"
#include "mongo/db/commands/create_gen.h"
#include "mongo/db/concurrency/d_concurrency.h"
#include "mongo/db/concurrency/exception_util.h"
#include "mongo/db/concurrency/lock_manager_defs.h"
#include "mongo/db/database_name.h"
#include "mongo/db/db_raii.h"
#include "mongo/db/feature_flag.h"
#include "mongo/db/index/index_descriptor.h"
#include "mongo/db/index_builds_coordinator.h"
#include "mongo/db/namespace_string.h"
#include "mongo/db/op_observer/op_observer.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/ops/insert.h"
#include "mongo/db/pipeline/change_stream_pre_and_post_images_options_gen.h"
#include "mongo/db/query/collation/collator_factory_interface.h"
#include "mongo/db/query/query_knobs_gen.h"
#include "mongo/db/repl/replication_coordinator.h"
#include "mongo/db/s/collection_sharding_state.h"
#include "mongo/db/server_options.h"
#include "mongo/db/service_context.h"
#include "mongo/db/stats/top.h"
#include "mongo/db/storage/recovery_unit.h"
#include "mongo/db/storage/storage_parameters_gen.h"
#include "mongo/db/storage/write_unit_of_work.h"
#include "mongo/db/timeseries/timeseries_constants.h"
#include "mongo/db/timeseries/timeseries_gen.h"
#include "mongo/db/timeseries/timeseries_index_schema_conversion_functions.h"
#include "mongo/db/timeseries/timeseries_options.h"
#include "mongo/db/transaction_resources.h"
#include "mongo/idl/command_generic_argument.h"
#include "mongo/logv2/log.h"
#include "mongo/logv2/log_attr.h"
#include "mongo/logv2/log_component.h"
#include "mongo/platform/compiler.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/fail_point.h"
#include "mongo/util/namespace_string_util.h"
#include "mongo/util/str.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kCommand


namespace mongo {
namespace {

MONGO_FAIL_POINT_DEFINE(failTimeseriesViewCreation);
MONGO_FAIL_POINT_DEFINE(clusterAllCollectionsByDefault);
MONGO_FAIL_POINT_DEFINE(skipIdIndex);

using IndexVersion = IndexDescriptor::IndexVersion;

Status validateClusteredIndexSpec(OperationContext* opCtx,
                                  const NamespaceString& nss,
                                  const ClusteredIndexSpec& spec,
                                  boost::optional<int64_t> expireAfterSeconds) {
    if (!spec.getUnique()) {
        return Status(ErrorCodes::Error(5979700),
                      "The clusteredIndex option requires unique: true to be specified");
    }

    bool clusterKeyOnId =
        SimpleBSONObjComparator::kInstance.evaluate(spec.getKey() == BSON("_id" << 1));

    if (!clusterKeyOnId && !gSupportArbitraryClusterKeyIndex) {
        return Status(ErrorCodes::InvalidIndexSpecificationOption,
                      "The clusteredIndex option is only supported for key: {_id: 1}");
    }

    if (nss.isReplicated() && !clusterKeyOnId) {
        return Status(ErrorCodes::Error(5979701),
                      "The clusteredIndex option is only supported for key: {_id: 1} on replicated "
                      "collections");
    }

    if (spec.getKey().nFields() > 1) {
        return Status(ErrorCodes::Error(6053700),
                      "The clusteredIndex option does not support a compound cluster key");
    }

    const auto arbitraryClusterKeyField = clustered_util::getClusterKeyFieldName(spec);
    if (arbitraryClusterKeyField.find(".", 0) != std::string::npos) {
        return Status(
            ErrorCodes::Error(6053701),
            "The clusteredIndex option does not support a cluster key with nested fields");
    }

    const bool isForwardClusterKey = SimpleBSONObjComparator::kInstance.evaluate(
        spec.getKey() == BSON(arbitraryClusterKeyField << 1));
    if (!isForwardClusterKey) {
        return Status(ErrorCodes::Error(6053702),
                      str::stream()
                          << "The clusteredIndex option supports cluster keys like {"
                          << arbitraryClusterKeyField << ": 1}, but got " << spec.getKey());
    }

    if (expireAfterSeconds) {
        // Not included in the indexSpec itself.
        auto status = index_key_validate::validateExpireAfterSeconds(
            *expireAfterSeconds,
            index_key_validate::ValidateExpireAfterSecondsMode::kClusteredTTLIndex);
        if (!status.isOK()) {
            return status;
        }
    }

    auto versionAsInt = spec.getV();
    const IndexVersion indexVersion = static_cast<IndexVersion>(versionAsInt);
    if (indexVersion != IndexVersion::kV2) {
        return {ErrorCodes::Error(5979704),
                str::stream() << "Invalid clusteredIndex specification " << spec.toBSON()
                              << "; cannot create a clusteredIndex with v=" << versionAsInt};
    }

    return Status::OK();
}

std::tuple<Lock::CollectionLock, Lock::CollectionLock> acquireCollLocksForRename(
    OperationContext* opCtx, const NamespaceString& ns1, const NamespaceString& ns2) {
    if (ResourceId{RESOURCE_COLLECTION, ns1} < ResourceId{RESOURCE_COLLECTION, ns2}) {
        Lock::CollectionLock collLock1{opCtx, ns1, MODE_X};
        Lock::CollectionLock collLock2{opCtx, ns2, MODE_X};
        return {std::move(collLock1), std::move(collLock2)};
    } else {
        Lock::CollectionLock collLock2{opCtx, ns2, MODE_X};
        Lock::CollectionLock collLock1{opCtx, ns1, MODE_X};
        return {std::move(collLock1), std::move(collLock2)};
    }
}

void _createSystemDotViewsIfNecessary(OperationContext* opCtx, const Database* db) {
    // Create 'system.views' in a separate WUOW if it does not exist.
    if (!CollectionCatalog::get(opCtx)->lookupCollectionByNamespace(opCtx,
                                                                    db->getSystemViewsName())) {
        WriteUnitOfWork wuow(opCtx);
        invariant(db->createCollection(opCtx, db->getSystemViewsName()));
        wuow.commit();
    }
}

Status _createView(OperationContext* opCtx,
                   const NamespaceString& nss,
                   const CollectionOptions& collectionOptions) {
    // This must be checked before we take locks in order to avoid attempting to take multiple locks
    // on the <db>.system.views namespace: first a IX lock on 'ns' and then a X lock on the database
    // system.views collection.
    uassert(ErrorCodes::InvalidNamespace,
            str::stream() << "Cannot create a view called '" << nss.coll()
                          << "': this is a reserved system namespace",
            !nss.isSystemDotViews());

    return writeConflictRetry(opCtx, "create", nss, [&] {
        AutoGetDb autoDb(opCtx, nss.dbName(), MODE_IX);
        Lock::CollectionLock collLock(opCtx, nss, MODE_IX);
        // Operations all lock system.views in the end to prevent deadlock.
        Lock::CollectionLock systemViewsLock(
            opCtx, NamespaceString::makeSystemDotViewsNamespace(nss.dbName()), MODE_X);

        auto db = autoDb.ensureDbExists(opCtx);

        if (opCtx->writesAreReplicated() &&
            !repl::ReplicationCoordinator::get(opCtx)->canAcceptWritesFor(opCtx, nss)) {
            return Status(ErrorCodes::NotWritablePrimary,
                          str::stream() << "Not primary while creating collection "
                                        << nss.toStringForErrorMsg());
        }

        // This is a top-level handler for collection creation name conflicts. New commands coming
        // in, or commands that generated a WriteConflict must return a NamespaceExists error here
        // on conflict.
        Status statusNss = catalog::checkIfNamespaceExists(opCtx, nss);
        if (!statusNss.isOK()) {
            return statusNss;
        }

        CollectionShardingState::assertCollectionLockedAndAcquire(opCtx, nss)
            ->checkShardVersionOrThrow(opCtx);

        if (collectionOptions.changeStreamPreAndPostImagesOptions.getEnabled()) {
            return Status(ErrorCodes::InvalidOptions,
                          "option not supported on a view: changeStreamPreAndPostImages");
        }

        _createSystemDotViewsIfNecessary(opCtx, db);

        WriteUnitOfWork wunit(opCtx);

        AutoStatsTracker statsTracker(
            opCtx,
            nss,
            Top::LockType::NotLocked,
            AutoStatsTracker::LogMode::kUpdateTopAndCurOp,
            CollectionCatalog::get(opCtx)->getDatabaseProfileLevel(nss.dbName()));

        // If the view creation rolls back, ensure that the Top entry created for the view is
        // deleted.
        shard_role_details::getRecoveryUnit(opCtx)->onRollback(
            [nss, serviceContext = opCtx->getServiceContext()](OperationContext*) {
                Top::get(serviceContext).collectionDropped(nss);
            });

        // Even though 'collectionOptions' is passed by rvalue reference, it is not safe to move
        // because 'userCreateNS' may throw a WriteConflictException.
        Status status = db->userCreateNS(opCtx, nss, collectionOptions, /*createIdIndex=*/false);
        if (!status.isOK()) {
            return status;
        }
        wunit.commit();

        return Status::OK();
    });
}

Status _createDefaultTimeseriesIndex(OperationContext* opCtx, CollectionWriter& collection) {
    auto tsOptions = collection->getCollectionOptions().timeseries;
    if (!tsOptions->getMetaField()) {
        return Status::OK();
    }

    StatusWith<BSONObj> swBucketsSpec = timeseries::createBucketsIndexSpecFromTimeseriesIndexSpec(
        *tsOptions, BSON(*tsOptions->getMetaField() << 1 << tsOptions->getTimeField() << 1));
    if (!swBucketsSpec.isOK()) {
        return swBucketsSpec.getStatus();
    }

    const std::string indexName = str::stream()
        << *tsOptions->getMetaField() << "_1_" << tsOptions->getTimeField() << "_1";
    IndexBuildsCoordinator::get(opCtx)->createIndexesOnEmptyCollection(
        opCtx,
        collection,
        {BSON("v" << 2 << "name" << indexName << "key" << swBucketsSpec.getValue())},
        /*fromMigrate=*/false);
    return Status::OK();
}

BSONObj _generateTimeseriesValidator(int bucketVersion, StringData timeField) {
    if (bucketVersion != timeseries::kTimeseriesControlCompressedVersion &&
        bucketVersion != timeseries::kTimeseriesControlUncompressedVersion) {
        MONGO_UNREACHABLE;
    }
    // '$jsonSchema' : {
    //     bsonType: 'object',
    //     required: ['_id', 'control', 'data'],
    //     properties: {
    //         _id: {bsonType: 'objectId'},
    //         control: {
    //             bsonType: 'object',
    //             required: ['version', 'min', 'max'],
    //             properties: {
    //                 version: {bsonType: 'number'},
    //                 min: {
    //                     bsonType: 'object',
    //                     required: ['%s'],
    //                     properties: {'%s': {bsonType: 'date'}}
    //                 },
    //                 max: {
    //                     bsonType: 'object',
    //                     required: ['%s'],
    //                     properties: {'%s': {bsonType: 'date'}}
    //                 },
    //                 closed: {bsonType: 'bool'},
    //                 count: {bsonType: 'number', minimum: 1} // only if bucketVersion ==
    //                 timeseries::kTimeseriesControlCompressedVersion
    //             },
    //             additionalProperties: false // only if bucketVersion ==
    //             timeseries::kTimeseriesControlCompressedVersion
    //         },
    //         data: {bsonType: 'object'},
    //         meta: {}
    //     },
    //     additionalProperties: false
    //   }
    BSONObjBuilder validator;
    BSONObjBuilder schema(validator.subobjStart("$jsonSchema"));
    schema.append("bsonType", "object");
    schema.append("required",
                  BSON_ARRAY("_id"
                             << "control"
                             << "data"));
    {
        BSONObjBuilder properties(schema.subobjStart("properties"));
        {
            BSONObjBuilder _id(properties.subobjStart("_id"));
            _id.append("bsonType", "objectId");
            _id.done();
        }
        {
            BSONObjBuilder control(properties.subobjStart("control"));
            control.append("bsonType", "object");
            control.append("required",
                           BSON_ARRAY("version"
                                      << "min"
                                      << "max"));
            {
                BSONObjBuilder innerProperties(control.subobjStart("properties"));
                {
                    BSONObjBuilder version(innerProperties.subobjStart("version"));
                    version.append("bsonType", "number");
                    version.done();
                }
                {
                    BSONObjBuilder min(innerProperties.subobjStart("min"));
                    min.append("bsonType", "object");
                    min.append("required", BSON_ARRAY(timeField));
                    BSONObjBuilder minProperties(min.subobjStart("properties"));
                    BSONObjBuilder timeFieldObj(minProperties.subobjStart(timeField));
                    timeFieldObj.append("bsonType", "date");
                    timeFieldObj.done();
                    minProperties.done();
                    min.done();
                }

                {
                    BSONObjBuilder max(innerProperties.subobjStart("max"));
                    max.append("bsonType", "object");
                    max.append("required", BSON_ARRAY(timeField));
                    BSONObjBuilder maxProperties(max.subobjStart("properties"));
                    BSONObjBuilder timeFieldObj(maxProperties.subobjStart(timeField));
                    timeFieldObj.append("bsonType", "date");
                    timeFieldObj.done();
                    maxProperties.done();
                    max.done();
                }
                {
                    BSONObjBuilder closed(innerProperties.subobjStart("closed"));
                    closed.append("bsonType", "bool");
                    closed.done();
                }
                if (bucketVersion == timeseries::kTimeseriesControlCompressedVersion) {
                    BSONObjBuilder count(innerProperties.subobjStart("count"));
                    count.append("bsonType", "number");
                    count.append("minimum", 1);
                    count.done();
                }
                innerProperties.done();
            }
            if (bucketVersion == timeseries::kTimeseriesControlCompressedVersion) {
                control.append("additionalProperties", false);
            }
            control.done();
        }
        {
            BSONObjBuilder data(properties.subobjStart("data"));
            data.append("bsonType", "object");
            data.done();
        }
        properties.append("meta", BSONObj{});
        properties.done();
    }
    schema.append("additionalProperties", false);
    schema.done();
    return validator.obj();
}

Status _createTimeseries(OperationContext* opCtx,
                         const NamespaceString& ns,
                         const CollectionOptions& optionsArg) {
    // This path should only be taken when a user creates a new time-series collection on the
    // primary. Secondaries replicate individual oplog entries.
    invariant(!ns.isTimeseriesBucketsCollection());
    invariant(opCtx->writesAreReplicated());

    auto bucketsNs = ns.makeTimeseriesBucketsNamespace();

    CollectionOptions options = optionsArg;

    Status timeseriesOptionsValidateAndSetStatus =
        timeseries::validateAndSetBucketingParameters(options.timeseries.get());

    if (!timeseriesOptionsValidateAndSetStatus.isOK()) {
        return timeseriesOptionsValidateAndSetStatus;
    }

    // Set the validator option to a JSON schema enforcing constraints on bucket documents.
    // This validation is only structural to prevent accidental corruption by users and
    // cannot cover all constraints. Leave the validationLevel and validationAction to their
    // strict/error defaults.
    auto timeField = options.timeseries->getTimeField();
    int bucketVersion = timeseries::kTimeseriesControlLatestVersion;
    auto validatorObj = _generateTimeseriesValidator(bucketVersion, timeField);

    bool existingBucketCollectionIsCompatible = false;

    Status ret = writeConflictRetry(opCtx, "createBucketCollection", bucketsNs, [&]() -> Status {
        AutoGetDb autoDb(opCtx, bucketsNs.dbName(), MODE_IX);
        Lock::CollectionLock bucketsCollLock(opCtx, bucketsNs, MODE_X);
        auto db = autoDb.ensureDbExists(opCtx);

        // Check if there already exist a Collection on the namespace we will later create a
        // view on. We're not holding a Collection lock for this Collection so we may only check
        // if the pointer is null or not. The answer may also change at any point after this
        // call which is fine as we properly handle an orphaned bucket collection. This check is
        // just here to prevent it from being created in the common case.
        Status status = catalog::checkIfNamespaceExists(opCtx, ns);
        if (!status.isOK()) {
            return status;
        }

        if (opCtx->writesAreReplicated() &&
            !repl::ReplicationCoordinator::get(opCtx)->canAcceptWritesFor(opCtx, bucketsNs)) {
            // Report the error with the user provided namespace
            return Status(ErrorCodes::NotWritablePrimary,
                          str::stream() << "Not primary while creating collection "
                                        << ns.toStringForErrorMsg());
        }

        CollectionShardingState::assertCollectionLockedAndAcquire(opCtx, bucketsNs)
            ->checkShardVersionOrThrow(opCtx);

        WriteUnitOfWork wuow(opCtx);
        AutoStatsTracker bucketsStatsTracker(
            opCtx,
            bucketsNs,
            Top::LockType::NotLocked,
            AutoStatsTracker::LogMode::kUpdateTopAndCurOp,
            CollectionCatalog::get(opCtx)->getDatabaseProfileLevel(ns.dbName()));

        // If the buckets collection and time-series view creation roll back, ensure that their
        // Top entries are deleted.
        shard_role_details::getRecoveryUnit(opCtx)->onRollback(
            [serviceContext = opCtx->getServiceContext(), bucketsNs](OperationContext*) {
                Top::get(serviceContext).collectionDropped(bucketsNs);
            });


        // Prepare collection option and index spec using the provided options. In case the
        // collection already exist we use these to validate that they are the same as being
        // requested here.
        CollectionOptions bucketsOptions = options;
        bucketsOptions.validator = validatorObj;

        // Cluster time-series buckets collections by _id.
        auto expireAfterSeconds = options.expireAfterSeconds;
        if (expireAfterSeconds) {
            uassertStatusOK(index_key_validate::validateExpireAfterSeconds(
                *expireAfterSeconds,
                index_key_validate::ValidateExpireAfterSecondsMode::kClusteredTTLIndex));
            bucketsOptions.expireAfterSeconds = expireAfterSeconds;
        }

        bucketsOptions.clusteredIndex = clustered_util::makeCanonicalClusteredInfoForLegacyFormat();

        if (auto coll =
                CollectionCatalog::get(opCtx)->lookupCollectionByNamespace(opCtx, bucketsNs)) {
            // Compare CollectionOptions and eventual TTL index to see if this bucket collection
            // may be reused for this request.
            existingBucketCollectionIsCompatible =
                coll->getCollectionOptions().matchesStorageOptions(
                    bucketsOptions, CollatorFactoryInterface::get(opCtx->getServiceContext()));

            // We may have a bucket collection created with a previous version of mongod, this
            // is also OK as we do not convert bucket collections to latest version during
            // upgrade.
            while (!existingBucketCollectionIsCompatible &&
                   bucketVersion > timeseries::kTimeseriesControlMinVersion) {
                validatorObj = _generateTimeseriesValidator(--bucketVersion, timeField);
                bucketsOptions.validator = validatorObj;

                existingBucketCollectionIsCompatible =
                    coll->getCollectionOptions().matchesStorageOptions(
                        bucketsOptions, CollatorFactoryInterface::get(opCtx->getServiceContext()));
            }

            return Status(ErrorCodes::NamespaceExists,
                          str::stream()
                              << "Bucket Collection already exists. NS: "
                              << bucketsNs.toStringForErrorMsg() << ". UUID: " << coll->uuid());
        }

        // Create the buckets collection that will back the view.
        const bool createIdIndex = false;
        uassertStatusOK(db->userCreateNS(opCtx, bucketsNs, bucketsOptions, createIdIndex));

        CollectionWriter collectionWriter(opCtx, bucketsNs);

        uassertStatusOK(_createDefaultTimeseriesIndex(opCtx, collectionWriter));
        wuow.commit();
        return Status::OK();
    });

    // If compatible bucket collection already exists then proceed with creating view defintion.
    // If the 'temp' flag is true, we are in the $out stage, and should return without creating the
    // view defintion.
    if ((!ret.isOK() && !existingBucketCollectionIsCompatible) || options.temp)
        return ret;

    ret = writeConflictRetry(opCtx, "create", ns, [&]() -> Status {
        AutoGetCollection autoColl(
            opCtx,
            ns,
            MODE_IX,
            AutoGetCollection::Options{}.viewMode(auto_get_collection::ViewMode::kViewsPermitted));
        Lock::CollectionLock systemDotViewsLock(
            opCtx, NamespaceString::makeSystemDotViewsNamespace(ns.dbName()), MODE_X);
        auto db = autoColl.ensureDbExists(opCtx);

        // This is a top-level handler for time-series creation name conflicts. New commands coming
        // in, or commands that generated a WriteConflict must return a NamespaceExists error here
        // on conflict.
        Status status = catalog::checkIfNamespaceExists(opCtx, ns);
        if (!status.isOK()) {
            return status;
        }

        if (opCtx->writesAreReplicated() &&
            !repl::ReplicationCoordinator::get(opCtx)->canAcceptWritesFor(opCtx, ns)) {
            return {ErrorCodes::NotWritablePrimary,
                    str::stream() << "Not primary while creating collection "
                                  << ns.toStringForErrorMsg()};
        }

        CollectionShardingState::assertCollectionLockedAndAcquire(opCtx, ns)
            ->checkShardVersionOrThrow(opCtx);

        _createSystemDotViewsIfNecessary(opCtx, db);

        auto catalog = CollectionCatalog::get(opCtx);
        WriteUnitOfWork wuow(opCtx);

        AutoStatsTracker statsTracker(opCtx,
                                      ns,
                                      Top::LockType::NotLocked,
                                      AutoStatsTracker::LogMode::kUpdateTopAndCurOp,
                                      catalog->getDatabaseProfileLevel(ns.dbName()));

        // If the buckets collection and time-series view creation roll back, ensure that their
        // Top entries are deleted.
        shard_role_details::getRecoveryUnit(opCtx)->onRollback(
            [serviceContext = opCtx->getServiceContext(), ns](OperationContext*) {
                Top::get(serviceContext).collectionDropped(ns);
            });

        if (MONGO_unlikely(failTimeseriesViewCreation.shouldFail([&ns](const BSONObj& data) {
                const auto fpNss = NamespaceStringUtil::parseFailPointData(data, "ns");
                return fpNss == ns;
            }))) {
            LOGV2(5490200,
                  "failTimeseriesViewCreation fail point enabled. Failing creation of view "
                  "definition after bucket collection was created successfully.");
            return {ErrorCodes::OperationFailed,
                    str::stream() << "Timeseries view definition " << ns.toStringForErrorMsg()
                                  << " creation failed due to 'failTimeseriesViewCreation' "
                                     "fail point enabled."};
        }

        CollectionOptions viewOptions;
        viewOptions.viewOn = bucketsNs.coll().toString();
        viewOptions.collation = options.collation;
        constexpr bool asArray = true;
        viewOptions.pipeline = timeseries::generateViewPipeline(*options.timeseries, asArray);

        // Create the time-series view.
        status = db->userCreateNS(opCtx, ns, viewOptions);
        if (!status.isOK()) {
            return status.withContext(
                str::stream() << "Failed to create view on " << bucketsNs.toStringForErrorMsg()
                              << " for time-series collection " << ns.toStringForErrorMsg()
                              << " with options " << viewOptions.toBSON());
        }

        wuow.commit();
        return Status::OK();
    });

    return ret;
}

Status _createCollection(
    OperationContext* opCtx,
    const NamespaceString& nss,
    const CollectionOptions& collectionOptions,
    const boost::optional<BSONObj>& idIndex,
    const boost::optional<VirtualCollectionOptions>& virtualCollectionOptions = boost::none) {
    return writeConflictRetry(opCtx, "create", nss, [&] {
        // If a change collection is to be created, that is, the change streams are being enabled
        // for a tenant, acquire exclusive tenant lock.
        AutoGetDb autoDb(opCtx,
                         nss.dbName(),
                         MODE_IX /* database lock mode*/,
                         boost::make_optional(nss.tenantId() && nss.isChangeCollection(), MODE_X));
        Lock::CollectionLock collLock(opCtx, nss, MODE_IX);
        auto db = autoDb.ensureDbExists(opCtx);

        // This is a top-level handler for collection creation name conflicts. New commands coming
        // in, or commands that generated a WriteConflict must return a NamespaceExists error here
        // on conflict.
        Status status = catalog::checkIfNamespaceExists(opCtx, nss);
        if (!status.isOK()) {
            return status;
        }

        if (!collectionOptions.clusteredIndex && collectionOptions.expireAfterSeconds) {
            return Status(ErrorCodes::InvalidOptions,
                          "'expireAfterSeconds' requires clustering to be enabled");
        }

        if (auto clusteredIndex = collectionOptions.clusteredIndex) {
            if (clustered_util::requiresLegacyFormat(nss) != clusteredIndex->getLegacyFormat()) {
                return Status(ErrorCodes::Error(5979703),
                              "The 'clusteredIndex' legacy format {clusteredIndex: <bool>} is only "
                              "supported for specific internal collections and vice versa");
            }

            if (idIndex && !idIndex->isEmpty()) {
                return Status(
                    ErrorCodes::InvalidOptions,
                    "The 'clusteredIndex' option is not supported with the 'idIndex' option");
            }
            if (collectionOptions.autoIndexId == CollectionOptions::NO) {
                return Status(ErrorCodes::Error(6026501),
                              "The 'clusteredIndex' option does not support {autoIndexId: false}");
            }

            auto clusteredIndexStatus = validateClusteredIndexSpec(
                opCtx, nss, clusteredIndex->getIndexSpec(), collectionOptions.expireAfterSeconds);
            if (!clusteredIndexStatus.isOK()) {
                return clusteredIndexStatus;
            }
        }


        if (opCtx->writesAreReplicated() &&
            !repl::ReplicationCoordinator::get(opCtx)->canAcceptWritesFor(opCtx, nss)) {
            return Status(ErrorCodes::NotWritablePrimary,
                          str::stream() << "Not primary while creating collection "
                                        << nss.toStringForErrorMsg());
        }

        CollectionShardingState::assertCollectionLockedAndAcquire(opCtx, nss)
            ->checkShardVersionOrThrow(opCtx);

        WriteUnitOfWork wunit(opCtx);

        AutoStatsTracker statsTracker(
            opCtx,
            nss,
            Top::LockType::NotLocked,
            AutoStatsTracker::LogMode::kUpdateTopAndCurOp,
            CollectionCatalog::get(opCtx)->getDatabaseProfileLevel(nss.dbName()));

        // If the collection creation rolls back, ensure that the Top entry created for the
        // collection is deleted.
        shard_role_details::getRecoveryUnit(opCtx)->onRollback(
            [nss, serviceContext = opCtx->getServiceContext()](OperationContext*) {
                Top::get(serviceContext).collectionDropped(nss);
            });

        // Even though 'collectionOptions' is passed by rvalue reference, it is not safe to move
        // because 'userCreateNS' may throw a WriteConflictException.
        if (idIndex == boost::none || collectionOptions.clusteredIndex) {
            status = virtualCollectionOptions
                ? db->userCreateVirtualNS(opCtx, nss, collectionOptions, *virtualCollectionOptions)
                : db->userCreateNS(opCtx, nss, collectionOptions, /*createIdIndex=*/false);
        } else {
            bool createIdIndex = true;
            if (MONGO_unlikely(skipIdIndex.shouldFail())) {
                createIdIndex = false;
            }
            status = db->userCreateNS(opCtx, nss, collectionOptions, createIdIndex, *idIndex);
        }
        if (!status.isOK()) {
            return status;
        }
        wunit.commit();

        return Status::OK();
    });
}

CollectionOptions clusterByDefaultIfNecessary(const NamespaceString& nss,
                                              CollectionOptions collectionOptions,
                                              const boost::optional<BSONObj>& idIndex) {
    if (MONGO_unlikely(clusterAllCollectionsByDefault.shouldFail()) &&
        !collectionOptions.isView() && !collectionOptions.clusteredIndex.has_value() &&
        (!idIndex || idIndex->isEmpty()) && !collectionOptions.capped &&
        !clustered_util::requiresLegacyFormat(nss)) {
        // Capped, clustered collections differ in behavior significantly from normal
        // capped collections. Notably, they allow out-of-order insertion.
        //
        // Additionally, don't set the collection to be clustered in the default format if it
        // requires legacy format.
        collectionOptions.clusteredIndex = clustered_util::makeDefaultClusteredIdIndex();
    }
    return collectionOptions;
}

/**
 * Shared part of the implementation of the createCollection versions for replicated and regular
 * collection creation.
 */
Status createCollection(OperationContext* opCtx,
                        const NamespaceString& nss,
                        const BSONObj& cmdObj,
                        const boost::optional<BSONObj>& idIndex,
                        CollectionOptions::ParseKind kind) {
    BSONObjIterator it(cmdObj);

    // Skip the first cmdObj element.
    BSONElement firstElt = it.next();
    invariant(firstElt.fieldNameStringData() == "create");

    // Build options object from remaining cmdObj elements.
    BSONObjBuilder optionsBuilder;
    while (it.more()) {
        const auto elem = it.next();
        if (!isGenericArgument(elem.fieldNameStringData()))
            optionsBuilder.append(elem);
        if (elem.fieldNameStringData() == "viewOn") {
            // Views don't have UUIDs so it should always be parsed for command.
            kind = CollectionOptions::parseForCommand;
        }
    }

    BSONObj options = optionsBuilder.obj();
    uassert(14832,
            "specify size:<n> when capped is true",
            !options["capped"].trueValue() || options["size"].isNumber());

    CollectionOptions collectionOptions;
    {
        StatusWith<CollectionOptions> statusWith = CollectionOptions::parse(options, kind);
        if (!statusWith.isOK()) {
            return statusWith.getStatus();
        }
        collectionOptions = statusWith.getValue();
        bool hasExplicitlyDisabledClustering =
            options["clusteredIndex"].isBoolean() && !options["clusteredIndex"].boolean();
        if (!hasExplicitlyDisabledClustering) {
            collectionOptions =
                clusterByDefaultIfNecessary(nss, std::move(collectionOptions), idIndex);
        }
    }

    return createCollection(opCtx, nss, collectionOptions, idIndex);
}
}  // namespace

Status createTimeseries(OperationContext* opCtx,
                        const NamespaceString& ns,
                        const BSONObj& options) {
    StatusWith<CollectionOptions> statusWith =
        CollectionOptions::parse(options, CollectionOptions::parseForCommand);
    if (!statusWith.isOK()) {
        return statusWith.getStatus();
    }
    auto collectionOptions = statusWith.getValue();
    return _createTimeseries(opCtx, ns, collectionOptions);
}

Status createCollection(OperationContext* opCtx,
                        const DatabaseName& dbName,
                        const BSONObj& cmdObj,
                        const BSONObj& idIndex) {
    return createCollection(opCtx,
                            CommandHelpers::parseNsCollectionRequired(dbName, cmdObj),
                            cmdObj,
                            idIndex,
                            CollectionOptions::parseForCommand);
}

Status createCollection(OperationContext* opCtx, const CreateCommand& cmd) {
    auto options = CollectionOptions::fromCreateCommand(cmd);
    auto idIndex = std::exchange(options.idIndex, {});
    bool hasExplicitlyDisabledClustering = cmd.getClusteredIndex() &&
        holds_alternative<bool>(*cmd.getClusteredIndex()) && !get<bool>(*cmd.getClusteredIndex());
    if (!hasExplicitlyDisabledClustering) {
        options = clusterByDefaultIfNecessary(cmd.getNamespace(), std::move(options), idIndex);
    }
    return createCollection(opCtx, cmd.getNamespace(), options, idIndex);
}

Status createCollectionForApplyOps(OperationContext* opCtx,
                                   const DatabaseName& dbName,
                                   const boost::optional<UUID>& ui,
                                   const BSONObj& cmdObj,
                                   const bool allowRenameOutOfTheWay,
                                   const boost::optional<BSONObj>& idIndex) {

    invariant(shard_role_details::getLocker(opCtx)->isDbLockedForMode(dbName, MODE_IX));

    const NamespaceString newCollName(CommandHelpers::parseNsCollectionRequired(dbName, cmdObj));
    auto newCmd = cmdObj;

    auto databaseHolder = DatabaseHolder::get(opCtx);
    auto* const db = databaseHolder->getDb(opCtx, dbName);

    // If a UUID is given, see if we need to rename a collection out of the way, and whether the
    // collection already exists under a different name. If so, rename it into place. As this is
    // done during replay of the oplog, the operations do not need to be atomic, just idempotent.
    // We need to do the renaming part in a separate transaction, as we cannot transactionally
    // create a database, which could result in createCollection failing if the database
    // does not yet exist.
    if (ui) {
        auto uuid = ui.value();
        uassert(ErrorCodes::InvalidUUID,
                "Invalid UUID in applyOps create command: " + uuid.toString(),
                uuid.isRFC4122v4());

        auto catalog = CollectionCatalog::get(opCtx);
        const auto currentName = catalog->lookupNSSByUUID(opCtx, uuid);
        auto serviceContext = opCtx->getServiceContext();
        auto opObserver = serviceContext->getOpObserver();
        if (currentName && *currentName == newCollName)
            return Status::OK();

        if (currentName && currentName->isDropPendingNamespace()) {
            LOGV2(20308,
                  "CMD: create -- existing collection with conflicting UUID is in a drop-pending "
                  "state",
                  "newCollection"_attr = newCollName,
                  "conflictingUUID"_attr = uuid,
                  "existingCollection"_attr = *currentName);
            return Status(ErrorCodes::NamespaceExists,
                          str::stream()
                              << "existing collection " << currentName->toStringForErrorMsg()
                              << " with conflicting UUID " << uuid.toString()
                              << " is in a drop-pending state.");
        }

        // In the case of oplog replay, a future command may have created or renamed a
        // collection with that same name. In that case, renaming this future collection to
        // a random temporary name is correct: once all entries are replayed no temporary
        // names will remain.
        const bool stayTemp = true;
        auto futureColl = db ? catalog->lookupCollectionByNamespace(opCtx, newCollName) : nullptr;
        bool needsRenaming(futureColl);
        invariant(!needsRenaming || allowRenameOutOfTheWay,
                  str::stream() << "Name already exists. Collection name: "
                                << newCollName.toStringForErrorMsg() << ", UUID: " << uuid
                                << ", Future collection UUID: " << futureColl->uuid());

        std::string tmpNssPattern("tmp%%%%%.create");
        if (newCollName.isTimeseriesBucketsCollection()) {
            tmpNssPattern =
                NamespaceString::kTimeseriesBucketsCollectionPrefix.toString() + tmpNssPattern;
        }
        for (int tries = 0; needsRenaming && tries < 10; ++tries) {
            auto tmpNameResult = makeUniqueCollectionName(opCtx, dbName, tmpNssPattern);
            if (!tmpNameResult.isOK()) {
                return tmpNameResult.getStatus().withContext(str::stream()
                                                             << "Cannot generate temporary "
                                                                "collection namespace for applyOps "
                                                                "create command: collection: "
                                                             << newCollName.toStringForErrorMsg());
            }

            const auto& tmpName = tmpNameResult.getValue();
            auto [tmpCollLock, newCollLock] =
                acquireCollLocksForRename(opCtx, tmpName, newCollName);
            if (catalog->lookupCollectionByNamespace(opCtx, tmpName)) {
                // Conflicting on generating a unique temp collection name. Try again.
                continue;
            }

            // It is ok to log this because this doesn't happen very frequently.
            LOGV2(20309,
                  "CMD: create -- renaming existing collection with conflicting UUID to "
                  "temporary collection",
                  "newCollection"_attr = newCollName,
                  "conflictingUUID"_attr = uuid,
                  "tempName"_attr = tmpName);
            Status status =
                writeConflictRetry(opCtx, "createCollectionForApplyOps", newCollName, [&] {
                    WriteUnitOfWork wuow(opCtx);
                    Status status = db->renameCollection(opCtx, newCollName, tmpName, stayTemp);
                    if (!status.isOK())
                        return status;
                    auto futureCollUuid = futureColl->uuid();
                    opObserver->onRenameCollection(opCtx,
                                                   newCollName,
                                                   tmpName,
                                                   futureCollUuid,
                                                   /*dropTargetUUID*/ {},
                                                   /*numRecords*/ 0U,
                                                   stayTemp,
                                                   /*markFromMigrate=*/false);

                    wuow.commit();
                    // Re-fetch collection after commit to get a valid pointer
                    futureColl = CollectionCatalog::get(opCtx)->lookupCollectionByUUID(
                        opCtx, futureCollUuid);
                    return Status::OK();
                });

            if (!status.isOK()) {
                return status;
            }

            // Abort any remaining index builds on the temporary collection.
            IndexBuildsCoordinator::get(opCtx)->abortCollectionIndexBuilds(
                opCtx,
                tmpName,
                futureColl->uuid(),
                "Aborting index builds on temporary collection");

            // The existing collection has been successfully moved out of the way.
            needsRenaming = false;
        }
        if (needsRenaming) {
            return Status(ErrorCodes::NamespaceExists,
                          str::stream() << "Cannot generate temporary "
                                           "collection namespace for applyOps "
                                           "create command: collection: "
                                        << newCollName.toStringForErrorMsg());
        }

        // If the collection with the requested UUID already exists, but with a different
        // name, just rename it to 'newCollName'.
        if (catalog->lookupCollectionByUUID(opCtx, uuid)) {
            invariant(currentName);
            uassert(40655,
                    str::stream() << "Invalid name " << newCollName.toStringForErrorMsg()
                                  << " for UUID " << uuid,
                    currentName->isEqualDb(newCollName));
            return writeConflictRetry(opCtx, "createCollectionForApplyOps", newCollName, [&] {
                auto [currentCollLock, newCollLock] =
                    acquireCollLocksForRename(opCtx, *currentName, newCollName);
                WriteUnitOfWork wuow(opCtx);
                Status status = db->renameCollection(opCtx, *currentName, newCollName, stayTemp);
                if (!status.isOK())
                    return status;
                opObserver->onRenameCollection(opCtx,
                                               *currentName,
                                               newCollName,
                                               uuid,
                                               /*dropTargetUUID*/ {},
                                               /*numRecords*/ 0U,
                                               stayTemp,
                                               /*markFromMigrate=*/false);

                wuow.commit();
                return Status::OK();
            });
        }

        // A new collection with the specific UUID must be created, so add the UUID to the
        // creation options. Regular user collection creation commands cannot do this.
        auto uuidObj = uuid.toBSON();
        newCmd = cmdObj.addField(uuidObj.firstElement());
    }

    return createCollection(
        opCtx, newCollName, newCmd, idIndex, CollectionOptions::parseForStorage);
}

Status createCollection(OperationContext* opCtx,
                        const NamespaceString& ns,
                        const CollectionOptions& options,
                        const boost::optional<BSONObj>& idIndex) {
    auto status = userAllowedCreateNS(opCtx, ns);
    if (!status.isOK()) {
        return status;
    }

    if (options.isView()) {
        // system.profile will have new document inserts due to profiling. Inserts aren't supported
        // on views.
        uassert(ErrorCodes::IllegalOperation,
                "Cannot create system.profile as a view",
                !ns.isSystemDotProfile());
        uassert(ErrorCodes::OperationNotSupportedInTransaction,
                str::stream() << "Cannot create a view in a multi-document "
                                 "transaction.",
                !opCtx->inMultiDocumentTransaction());
        uassert(ErrorCodes::Error(6026500),
                "The 'clusteredIndex' option is not supported with views",
                !options.clusteredIndex);

        return _createView(opCtx, ns, options);
    } else if (options.timeseries && !ns.isTimeseriesBucketsCollection()) {
        // system.profile must be a simple collection since new document insertions directly work
        // against the usual collection API. See introspect.cpp for more details.
        uassert(ErrorCodes::IllegalOperation,
                "Cannot create system.profile as a timeseries collection",
                !ns.isSystemDotProfile());
        // This helper is designed for user-created time-series collections on primaries. If a
        // time-series buckets collection is created explicitly or during replication, treat this as
        // a normal collection creation.
        uassert(ErrorCodes::OperationNotSupportedInTransaction,
                str::stream()
                    << "Cannot create a time-series collection in a multi-document transaction.",
                !opCtx->inMultiDocumentTransaction());
        return _createTimeseries(opCtx, ns, options);
    } else {
        uassert(ErrorCodes::OperationNotSupportedInTransaction,
                str::stream() << "Cannot create system collection " << ns.toStringForErrorMsg()
                              << " within a transaction.",
                !opCtx->inMultiDocumentTransaction() || !ns.isSystem());
        return _createCollection(opCtx, ns, options, idIndex);
    }
}

Status createVirtualCollection(OperationContext* opCtx,
                               const NamespaceString& ns,
                               const VirtualCollectionOptions& vopts) {
    tassert(6968504,
            "Virtual collection is available when the compute mode is enabled",
            computeModeEnabled);
    CollectionOptions options;
    options.setNoIdIndex();
    return _createCollection(opCtx, ns, options, boost::none, vopts);
}

}  // namespace mongo
