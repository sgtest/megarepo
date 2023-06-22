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

#include <cmath>
#include <limits>
#include <memory>

#include "mongo/base/error_codes.h"
#include "mongo/base/status.h"
#include "mongo/base/status_with.h"
#include "mongo/base/string_data.h"
#include "mongo/bson/bsonmisc.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/db/catalog/index_key_validate.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/query/collation/collator_interface.h"
#include "mongo/db/query/collation/collator_interface_mock.h"
#include "mongo/db/query/query_test_service_context.h"
#include "mongo/unittest/assert.h"
#include "mongo/unittest/bson_test_util.h"
#include "mongo/unittest/framework.h"

namespace mongo {
namespace {

using index_key_validate::validateIdIndexSpec;
using index_key_validate::validateIndexSpec;
using index_key_validate::validateIndexSpecCollation;

constexpr OperationContext* kDefaultOpCtx = nullptr;

/**
 * Helper function used to return the fields of a BSONObj in a consistent order.
 */
BSONObj sorted(const BSONObj& obj) {
    BSONObjIteratorSorted iter(obj);
    BSONObjBuilder bob;
    while (iter.more()) {
        bob.append(iter.next());
    }
    return bob.obj();
}

TEST(IndexSpecValidateTest, ReturnsAnErrorIfKeyPatternIsNotAnObject) {
    ASSERT_EQ(ErrorCodes::TypeMismatch,
              validateIndexSpec(kDefaultOpCtx,
                                BSON("key" << 1 << "name"
                                           << "indexName")));
    ASSERT_EQ(ErrorCodes::TypeMismatch,
              validateIndexSpec(kDefaultOpCtx,
                                BSON("key"
                                     << "not an object"
                                     << "name"
                                     << "indexName")));
    ASSERT_EQ(ErrorCodes::TypeMismatch,
              validateIndexSpec(kDefaultOpCtx,
                                BSON("key" << BSONArray() << "name"
                                           << "indexName")));
}

TEST(IndexSpecValidateTest, ReturnsAnErrorIfFieldRepeatedInKeyPattern) {
    ASSERT_EQ(ErrorCodes::BadValue,
              validateIndexSpec(kDefaultOpCtx,
                                BSON("key" << BSON("field" << 1 << "field" << 1) << "name"
                                           << "indexName")));
    ASSERT_EQ(ErrorCodes::BadValue,
              validateIndexSpec(kDefaultOpCtx,
                                BSON("key" << BSON("field" << 1 << "otherField" << -1 << "field"
                                                           << "2dsphere")
                                           << "name"
                                           << "indexName")));
}

TEST(IndexSpecValidateTest, ReturnsAnErrorIfKeyPatternIsNotPresent) {
    ASSERT_EQ(ErrorCodes::FailedToParse,
              validateIndexSpec(kDefaultOpCtx,
                                BSON("name"
                                     << "indexName")));
}

TEST(IndexSpecValidateTest, ReturnsAnErrorIfNameIsNotAString) {
    ASSERT_EQ(ErrorCodes::TypeMismatch,
              validateIndexSpec(kDefaultOpCtx, BSON("key" << BSON("field" << 1) << "name" << 1)));
}

TEST(IndexSpecValidateTest, ReturnsAnErrorIfNameIsNotPresent) {
    ASSERT_EQ(ErrorCodes::FailedToParse,
              validateIndexSpec(kDefaultOpCtx, BSON("key" << BSON("field" << 1))));
}

TEST(IndexSpecValidateTest, ReturnsIndexSpecUnchangedIfVersionIsPresent) {
    auto result = validateIndexSpec(kDefaultOpCtx,
                                    BSON("key" << BSON("field" << 1) << "name"
                                               << "indexName"
                                               << "v" << 1));
    ASSERT_OK(result.getStatus());

    // We don't care about the order of the fields in the resulting index specification.
    ASSERT_BSONOBJ_EQ(sorted(BSON("key" << BSON("field" << 1) << "name"
                                        << "indexName"
                                        << "v" << 1)),
                      sorted(result.getValue()));
}

TEST(IndexSpecValidateTest, ReturnsAnErrorIfVersionIsNotANumber) {
    ASSERT_EQ(ErrorCodes::TypeMismatch,
              validateIndexSpec(kDefaultOpCtx,
                                BSON("key" << BSON("field" << 1) << "name"
                                           << "indexName"
                                           << "v"
                                           << "not a number")));
    ASSERT_EQ(ErrorCodes::TypeMismatch,
              validateIndexSpec(kDefaultOpCtx,
                                BSON("key" << BSON("field" << 1) << "name"
                                           << "indexName"
                                           << "v" << BSONObj())));
}

TEST(IndexSpecValidateTest, ReturnsAnErrorIfVersionIsNotRepresentableAsInt) {
    ASSERT_EQ(ErrorCodes::BadValue,
              validateIndexSpec(kDefaultOpCtx,
                                BSON("key" << BSON("field" << 1) << "name"
                                           << "indexName"
                                           << "v" << 2.2)));
    ASSERT_EQ(ErrorCodes::BadValue,
              validateIndexSpec(kDefaultOpCtx,
                                BSON("key" << BSON("field" << 1) << "name"
                                           << "indexName"
                                           << "v" << std::nan("1"))));
    ASSERT_EQ(ErrorCodes::BadValue,
              validateIndexSpec(kDefaultOpCtx,
                                BSON("key" << BSON("field" << 1) << "name"
                                           << "indexName"
                                           << "v" << std::numeric_limits<double>::infinity())));
    ASSERT_EQ(ErrorCodes::BadValue,
              validateIndexSpec(kDefaultOpCtx,
                                BSON("key" << BSON("field" << 1) << "name"
                                           << "indexName"
                                           << "v" << std::numeric_limits<long long>::max())));
}

TEST(IndexSpecValidateTest, ReturnsAnErrorIfVersionIsV0) {
    ASSERT_EQ(ErrorCodes::CannotCreateIndex,
              validateIndexSpec(kDefaultOpCtx,
                                BSON("key" << BSON("field" << 1) << "name"
                                           << "indexName"
                                           << "v" << 0)));
}

TEST(IndexSpecValidateTest, ReturnsAnErrorIfVersionIsUnsupported) {
    ASSERT_EQ(ErrorCodes::CannotCreateIndex,
              validateIndexSpec(kDefaultOpCtx,
                                BSON("key" << BSON("field" << 1) << "name"
                                           << "indexName"
                                           << "v" << 3 << "collation"
                                           << BSON("locale"
                                                   << "en"))));

    ASSERT_EQ(ErrorCodes::CannotCreateIndex,
              validateIndexSpec(kDefaultOpCtx,
                                BSON("key" << BSON("field" << 1) << "name"
                                           << "indexName"
                                           << "v" << -3LL)));
}

TEST(IndexSpecValidateTest, AcceptsIndexVersionsThatAreAllowedForCreation) {
    auto result = validateIndexSpec(kDefaultOpCtx,
                                    BSON("key" << BSON("field" << 1) << "name"
                                               << "indexName"
                                               << "v" << 1));
    ASSERT_OK(result.getStatus());

    // We don't care about the order of the fields in the resulting index specification.
    ASSERT_BSONOBJ_EQ(sorted(BSON("key" << BSON("field" << 1) << "name"
                                        << "indexName"
                                        << "v" << 1)),
                      sorted(result.getValue()));

    result = validateIndexSpec(kDefaultOpCtx,
                               BSON("key" << BSON("field" << 1) << "name"
                                          << "indexName"
                                          << "v" << 2LL));
    ASSERT_OK(result.getStatus());

    // We don't care about the order of the fields in the resulting index specification.
    ASSERT_BSONOBJ_EQ(sorted(BSON("key" << BSON("field" << 1) << "name"
                                        << "indexName"
                                        << "v" << 2LL)),
                      sorted(result.getValue()));
}

TEST(IndexSpecValidateTest, DefaultIndexVersionIsV2) {
    auto result = validateIndexSpec(kDefaultOpCtx,
                                    BSON("key" << BSON("field" << 1) << "name"
                                               << "indexName"));
    ASSERT_OK(result.getStatus());

    // We don't care about the order of the fields in the resulting index specification.
    ASSERT_BSONOBJ_EQ(sorted(BSON("key" << BSON("field" << 1) << "name"
                                        << "indexName"
                                        << "v" << 2)),
                      sorted(result.getValue()));

    // Verify that the index specification we returned is still considered valid.
    ASSERT_OK(validateIndexSpec(kDefaultOpCtx, result.getValue()));
}

TEST(IndexSpecValidateTest, AcceptsIndexVersionV1) {
    auto result = validateIndexSpec(kDefaultOpCtx,
                                    BSON("key" << BSON("field" << 1) << "name"
                                               << "indexName"
                                               << "v" << 1));
    ASSERT_OK(result.getStatus());

    // We don't care about the order of the fields in the resulting index specification.
    ASSERT_BSONOBJ_EQ(sorted(BSON("key" << BSON("field" << 1) << "name"
                                        << "indexName"
                                        << "v" << 1)),
                      sorted(result.getValue()));
}

TEST(IndexSpecValidateTest, ReturnsAnErrorIfCollationIsNotAnObject) {
    ASSERT_EQ(ErrorCodes::TypeMismatch,
              validateIndexSpec(kDefaultOpCtx,
                                BSON("key" << BSON("field" << 1) << "name"
                                           << "indexName"
                                           << "collation" << 1)));
    ASSERT_EQ(ErrorCodes::TypeMismatch,
              validateIndexSpec(kDefaultOpCtx,
                                BSON("key" << BSON("field" << 1) << "name"
                                           << "indexName"
                                           << "collation"
                                           << "not an object")));
    ASSERT_EQ(ErrorCodes::TypeMismatch,
              validateIndexSpec(kDefaultOpCtx,
                                BSON("key" << BSON("field" << 1) << "name"
                                           << "indexName"
                                           << "collation" << BSONArray())));
}

TEST(IndexSpecValidateTest, ReturnsAnErrorIfCollationIsEmpty) {
    ASSERT_EQ(ErrorCodes::BadValue,
              validateIndexSpec(kDefaultOpCtx,
                                BSON("key" << BSON("field" << 1) << "name"
                                           << "indexName"
                                           << "collation" << BSONObj())));
}

TEST(IndexSpecValidateTest, ReturnsAnErrorIfCollationIsPresentAndVersionIsLessThanV2) {
    ASSERT_EQ(ErrorCodes::CannotCreateIndex,
              validateIndexSpec(kDefaultOpCtx,
                                BSON("key" << BSON("field" << 1) << "name"
                                           << "indexName"
                                           << "collation"
                                           << BSON("locale"
                                                   << "simple")
                                           << "v" << 1)));
}

TEST(IndexSpecValidateTest, AcceptsAnyNonEmptyObjectValueForCollation) {
    auto result = validateIndexSpec(kDefaultOpCtx,
                                    BSON("key" << BSON("field" << 1) << "name"
                                               << "indexName"
                                               << "v" << 2 << "collation"
                                               << BSON("locale"
                                                       << "simple")));
    ASSERT_OK(result.getStatus());

    // We don't care about the order of the fields in the resulting index specification.
    ASSERT_BSONOBJ_EQ(sorted(BSON("key" << BSON("field" << 1) << "name"
                                        << "indexName"
                                        << "v" << 2 << "collation"
                                        << BSON("locale"
                                                << "simple"))),
                      sorted(result.getValue()));

    result = validateIndexSpec(kDefaultOpCtx,
                               BSON("key" << BSON("field" << 1) << "name"
                                          << "indexName"
                                          << "v" << 2 << "collation"
                                          << BSON("unknownCollationOption" << true)));
    ASSERT_OK(result.getStatus());

    // We don't care about the order of the fields in the resulting index specification.
    ASSERT_BSONOBJ_EQ(
        sorted(BSON("key" << BSON("field" << 1) << "name"
                          << "indexName"
                          << "v" << 2 << "collation" << BSON("unknownCollationOption" << true))),
        sorted(result.getValue()));
}

TEST(IndexSpecValidateTest, AcceptsIndexSpecIfCollationIsPresentAndVersionIsEqualToV2) {
    auto result = validateIndexSpec(kDefaultOpCtx,
                                    BSON("key" << BSON("field" << 1) << "name"
                                               << "indexName"
                                               << "v" << 2 << "collation"
                                               << BSON("locale"
                                                       << "en")));
    ASSERT_OK(result.getStatus());

    // We don't care about the order of the fields in the resulting index specification.
    ASSERT_BSONOBJ_EQ(sorted(BSON("key" << BSON("field" << 1) << "name"
                                        << "indexName"
                                        << "v" << 2 << "collation"
                                        << BSON("locale"
                                                << "en"))),
                      sorted(result.getValue()));
}

TEST(IndexSpecValidateTest, ReturnsAnErrorIfUnknownFieldIsPresentInSpecV2) {
    auto result = validateIndexSpec(kDefaultOpCtx,
                                    BSON("key" << BSON("field" << 1) << "name"
                                               << "indexName"
                                               << "v" << 2 << "unknownField" << 1));
    ASSERT_EQ(ErrorCodes::InvalidIndexSpecificationOption, result);
}

TEST(IndexSpecValidateTest, ReturnsAnErrorIfUnknownFieldIsPresentInSpecV1) {
    auto result = validateIndexSpec(kDefaultOpCtx,
                                    BSON("key" << BSON("field" << 1) << "name"
                                               << "indexName"
                                               << "v" << 1 << "unknownField" << 1));
    ASSERT_EQ(ErrorCodes::InvalidIndexSpecificationOption, result);
}

TEST(IndexSpecValidateTest, DisallowSpecifyingBothUniqueAndPrepareUnique) {
    auto result = validateIndexSpec(kDefaultOpCtx,
                                    BSON("key" << BSON("a" << 1) << "name"
                                               << "indexName"
                                               << "unique" << true << "prepareUnique" << true));
    ASSERT_EQ(result.getStatus().code(), ErrorCodes::CannotCreateIndex);
}

TEST(IdIndexSpecValidateTest, ReturnsAnErrorIfKeyPatternIsIncorrectForIdIndex) {
    ASSERT_EQ(ErrorCodes::BadValue,
              validateIdIndexSpec(BSON("key" << BSON("_id" << -1) << "name"
                                             << "_id_"
                                             << "v" << 2)));
    ASSERT_EQ(ErrorCodes::BadValue,
              validateIdIndexSpec(BSON("key" << BSON("a" << 1) << "name"
                                             << "_id_"
                                             << "v" << 2)));
}

TEST(IdIndexSpecValidateTest, ReturnsOKStatusIfKeyPatternCorrectForIdIndex) {
    ASSERT_OK(validateIdIndexSpec(BSON("key" << BSON("_id" << 1) << "name"
                                             << "anyname"
                                             << "v" << 2)));
}

TEST(IdIndexSpecValidateTest, ReturnsAnErrorIfFieldNotAllowedForIdIndex) {
    ASSERT_EQ(ErrorCodes::InvalidIndexSpecificationOption,
              validateIdIndexSpec(BSON("key" << BSON("_id" << 1) << "name"
                                             << "_id_"
                                             << "v" << 2 << "background" << false)));
    ASSERT_EQ(ErrorCodes::InvalidIndexSpecificationOption,
              validateIdIndexSpec(BSON("key" << BSON("_id" << 1) << "name"
                                             << "_id_"
                                             << "v" << 2 << "unique" << true)));
    ASSERT_EQ(ErrorCodes::InvalidIndexSpecificationOption,
              validateIdIndexSpec(BSON("key" << BSON("_id" << 1) << "name"
                                             << "_id_"
                                             << "v" << 2 << "partialFilterExpression"
                                             << BSON("a" << 5))));
    ASSERT_EQ(ErrorCodes::InvalidIndexSpecificationOption,
              validateIdIndexSpec(BSON("key" << BSON("_id" << 1) << "name"
                                             << "_id_"
                                             << "v" << 2 << "sparse" << false)));
    ASSERT_EQ(ErrorCodes::InvalidIndexSpecificationOption,
              validateIdIndexSpec(BSON("key" << BSON("_id" << 1) << "name"
                                             << "_id_"
                                             << "v" << 2 << "expireAfterSeconds" << 3600)));
    ASSERT_EQ(ErrorCodes::InvalidIndexSpecificationOption,
              validateIdIndexSpec(BSON("key" << BSON("_id" << 1) << "name"
                                             << "_id_"
                                             << "v" << 2 << "storageEngine" << BSONObj())));
}

TEST(IdIndexSpecValidateTest, ReturnsOKStatusIfAllFieldsAllowedForIdIndex) {
    ASSERT_OK(validateIdIndexSpec(BSON("key" << BSON("_id" << 1) << "name"
                                             << "_id_"
                                             << "v" << 2 << "collation"
                                             << BSON("locale"
                                                     << "simple"))));
}

TEST(IndexSpecCollationValidateTest, FillsInFullCollationSpec) {
    QueryTestServiceContext serviceContext;
    auto opCtx = serviceContext.makeOperationContext();

    const CollatorInterface* defaultCollator = nullptr;

    auto result = validateIndexSpecCollation(opCtx.get(),
                                             BSON("key" << BSON("field" << 1) << "name"
                                                        << "indexName"
                                                        << "v" << 2 << "collation"
                                                        << BSON("locale"
                                                                << "mock_reverse_string")),
                                             defaultCollator);
    ASSERT_OK(result.getStatus());

    // We don't care about the order of the fields in the resulting index specification.
    ASSERT_BSONOBJ_EQ(
        sorted(BSON("key" << BSON("field" << 1) << "name"
                          << "indexName"
                          << "v" << 2 << "collation"
                          << BSON("locale"
                                  << "mock_reverse_string"
                                  << "caseLevel" << false << "caseFirst"
                                  << "off"
                                  << "strength" << 3 << "numericOrdering" << false << "alternate"
                                  << "non-ignorable"
                                  << "maxVariable"
                                  << "punct"
                                  << "normalization" << false << "backwards" << false << "version"
                                  << "mock_version"))),
        sorted(result.getValue()));
}

TEST(IndexSpecCollationValidateTest, RemovesCollationFieldIfSimple) {
    QueryTestServiceContext serviceContext;
    auto opCtx = serviceContext.makeOperationContext();

    const CollatorInterface* defaultCollator = nullptr;

    auto result = validateIndexSpecCollation(opCtx.get(),
                                             BSON("key" << BSON("field" << 1) << "name"
                                                        << "indexName"
                                                        << "v" << 2 << "collation"
                                                        << BSON("locale"
                                                                << "simple")),
                                             defaultCollator);
    ASSERT_OK(result.getStatus());

    // We don't care about the order of the fields in the resulting index specification.
    ASSERT_BSONOBJ_EQ(sorted(BSON("key" << BSON("field" << 1) << "name"
                                        << "indexName"
                                        << "v" << 2)),
                      sorted(result.getValue()));
}

TEST(IndexSpecCollationValidateTest, FillsInCollationFieldWithCollectionDefaultIfNotPresent) {
    QueryTestServiceContext serviceContext;
    auto opCtx = serviceContext.makeOperationContext();

    const CollatorInterfaceMock defaultCollator(CollatorInterfaceMock::MockType::kReverseString);

    auto result = validateIndexSpecCollation(opCtx.get(),
                                             BSON("key" << BSON("field" << 1) << "name"
                                                        << "indexName"
                                                        << "v" << 2),
                                             &defaultCollator);
    ASSERT_OK(result.getStatus());

    // We don't care about the order of the fields in the resulting index specification.
    ASSERT_BSONOBJ_EQ(
        sorted(BSON("key" << BSON("field" << 1) << "name"
                          << "indexName"
                          << "v" << 2 << "collation"
                          << BSON("locale"
                                  << "mock_reverse_string"
                                  << "caseLevel" << false << "caseFirst"
                                  << "off"
                                  << "strength" << 3 << "numericOrdering" << false << "alternate"
                                  << "non-ignorable"
                                  << "maxVariable"
                                  << "punct"
                                  << "normalization" << false << "backwards" << false << "version"
                                  << "mock_version"))),
        sorted(result.getValue()));
}

TEST(IndexSpecPartialFilterTest, FailsIfPartialFilterIsNotAnObject) {
    auto result = validateIndexSpec(kDefaultOpCtx,
                                    BSON("key" << BSON("field" << 1) << "name"
                                               << "indexName"
                                               << "partialFilterExpression" << 1));
    ASSERT_EQ(result.getStatus(), ErrorCodes::TypeMismatch);
}

TEST(IndexSpecPartialFilterTest, FailsIfPartialFilterContainsBannedFeature) {
    auto result = validateIndexSpec(kDefaultOpCtx,
                                    BSON("key" << BSON("field" << 1) << "name"
                                               << "indexName"
                                               << "partialFilterExpression"
                                               << BSON("$jsonSchema" << BSONObj())));
    ASSERT_EQ(result.getStatus(), ErrorCodes::QueryFeatureNotAllowed);
}

TEST(IndexSpecPartialFilterTest, AcceptsValidPartialFilterExpression) {
    auto result = validateIndexSpec(kDefaultOpCtx,
                                    BSON("key" << BSON("field" << 1) << "name"
                                               << "indexName"
                                               << "partialFilterExpression" << BSON("a" << 1)));
    ASSERT_OK(result.getStatus());
}

TEST(IndexSpecWildcard, SucceedsWithInclusion) {
    auto result =
        validateIndexSpec(kDefaultOpCtx,
                          BSON("key" << BSON("$**" << 1) << "name"
                                     << "indexName"
                                     << "wildcardProjection" << BSON("a" << 1 << "b" << 1)));
    ASSERT_OK(result.getStatus());
}

TEST(IndexSpecWildcard, SucceedsWithExclusion) {
    auto result =
        validateIndexSpec(kDefaultOpCtx,
                          BSON("key" << BSON("$**" << 1) << "name"
                                     << "indexName"
                                     << "wildcardProjection" << BSON("a" << 0 << "b" << 0)));
    ASSERT_OK(result.getStatus());
}

TEST(IndexSpecWildcard, SucceedsWithExclusionIncludingId) {
    auto result = validateIndexSpec(kDefaultOpCtx,
                                    BSON("key" << BSON("$**" << 1) << "name"
                                               << "indexName"
                                               << "wildcardProjection"
                                               << BSON("_id" << 1 << "a" << 0 << "b" << 0)));
    ASSERT_OK(result.getStatus());
}

TEST(IndexSpecWildcard, SucceedsWithInclusionExcludingId) {
    auto result = validateIndexSpec(kDefaultOpCtx,
                                    BSON("key" << BSON("$**" << 1) << "name"
                                               << "indexName"
                                               << "wildcardProjection"
                                               << BSON("_id" << 0 << "a" << 1 << "b" << 1)));
    ASSERT_OK(result.getStatus());
}

TEST(IndexSpecWildcard, FailsWithInclusionExcludingIdSubfield) {
    auto result = validateIndexSpec(kDefaultOpCtx,
                                    BSON("key" << BSON("$**" << 1) << "name"
                                               << "indexName"
                                               << "wildcardProjection"
                                               << BSON("_id.field" << 0 << "a" << 1 << "b" << 1)));
    ASSERT_EQ(result.getStatus().code(), 31253);
}

TEST(IndexSpecWildcard, FailsWithExclusionIncludingIdSubfield) {
    auto result = validateIndexSpec(kDefaultOpCtx,
                                    BSON("key" << BSON("$**" << 1) << "name"
                                               << "indexName"
                                               << "wildcardProjection"
                                               << BSON("_id.field" << 1 << "a" << 0 << "b" << 0)));
    ASSERT_EQ(result.getStatus().code(), 31254);
}

TEST(IndexSpecWildcard, FailsWithMixedProjection) {
    auto result =
        validateIndexSpec(kDefaultOpCtx,
                          BSON("key" << BSON("$**" << 1) << "name"
                                     << "indexName"
                                     << "wildcardProjection" << BSON("a" << 1 << "b" << 0)));
    ASSERT_EQ(result.getStatus().code(), 31254);
}

TEST(IndexSpecWildcard, FailsWithComputedFieldsInProjection) {
    auto result = validateIndexSpec(kDefaultOpCtx,
                                    BSON("key" << BSON("$**" << 1) << "name"
                                               << "indexName"
                                               << "wildcardProjection"
                                               << BSON("a" << 1 << "b"
                                                           << "string")));
    ASSERT_EQ(result.getStatus().code(), 51271);
}

TEST(IndexSpecWildcard, FailsWhenProjectionPluginNotWildcard) {
    auto result = validateIndexSpec(kDefaultOpCtx,
                                    BSON("key" << BSON("a" << 1) << "name"
                                               << "indexName"
                                               << "wildcardProjection" << BSON("a" << 1)));
    ASSERT_EQ(result.getStatus().code(), ErrorCodes::BadValue);
}

TEST(IndexSpecWildcard, FailsWhenProjectionIsNotAnObject) {
    auto result = validateIndexSpec(kDefaultOpCtx,
                                    BSON("key" << BSON("$**" << 1) << "name"
                                               << "indexName"
                                               << "wildcardProjection" << 4));
    ASSERT_EQ(result.getStatus().code(), ErrorCodes::TypeMismatch);
}

TEST(IndexSpecWildcard, FailsWithEmptyProjection) {
    auto result = validateIndexSpec(kDefaultOpCtx,
                                    BSON("key" << BSON("$**" << 1) << "name"
                                               << "indexName"
                                               << "wildcardProjection" << BSONObj()));
    ASSERT_EQ(result.getStatus().code(), ErrorCodes::FailedToParse);
}

TEST(IndexSpecWildcard, FailsWhenInclusionWithSubpath) {
    auto result = validateIndexSpec(kDefaultOpCtx,
                                    BSON("key" << BSON("a.$**" << 1) << "name"
                                               << "indexName"
                                               << "wildcardProjection" << BSON("a" << 1)));
    ASSERT_EQ(result.getStatus().code(), ErrorCodes::FailedToParse);
}

TEST(IndexSpecWildcard, FailsWhenExclusionWithSubpath) {
    auto result = validateIndexSpec(kDefaultOpCtx,
                                    BSON("key" << BSON("a.$**" << 1) << "name"
                                               << "indexName"
                                               << "wildcardProjection" << BSON("b" << 0)));
    ASSERT_EQ(result.getStatus().code(), ErrorCodes::FailedToParse);
}

TEST(IndexSpecColumnStore, SucceedsWithInclusion) {
    auto result =
        validateIndexSpec(kDefaultOpCtx,
                          BSON("key" << BSON("$**"
                                             << "columnstore")
                                     << "name"
                                     << "indexName"
                                     << "columnstoreProjection" << BSON("a" << 1 << "b" << 1)));
    ASSERT_OK(result.getStatus());
}

TEST(IndexSpecColumnStore, SucceedsWithExclusion) {
    auto result =
        validateIndexSpec(kDefaultOpCtx,
                          BSON("key" << BSON("$**"
                                             << "columnstore")
                                     << "name"
                                     << "indexName"
                                     << "columnstoreProjection" << BSON("a" << 0 << "b" << 0)));
    ASSERT_OK(result.getStatus());
}

TEST(IndexSpecColumnStore, SucceedsWithExclusionIncludingId) {
    auto result = validateIndexSpec(kDefaultOpCtx,
                                    BSON("key" << BSON("$**"
                                                       << "columnstore")
                                               << "name"
                                               << "indexName"
                                               << "columnstoreProjection"
                                               << BSON("_id" << 1 << "a" << 0 << "b" << 0)));
    ASSERT_OK(result.getStatus());
}

TEST(IndexSpecColumnStore, SucceedsWithInclusionExcludingId) {
    auto result = validateIndexSpec(kDefaultOpCtx,
                                    BSON("key" << BSON("$**"
                                                       << "columnstore")
                                               << "name"
                                               << "indexName"
                                               << "columnstoreProjection"
                                               << BSON("_id" << 0 << "a" << 1 << "b" << 1)));
    ASSERT_OK(result.getStatus());
}

TEST(IndexSpecColumnStore, FailsWithInclusionExcludingIdSubfield) {
    auto result = validateIndexSpec(kDefaultOpCtx,
                                    BSON("key" << BSON("$**"
                                                       << "columnstore")
                                               << "name"
                                               << "indexName"
                                               << "columnstoreProjection"
                                               << BSON("_id.field" << 0 << "a" << 1 << "b" << 1)));
    ASSERT_EQ(result.getStatus().code(), 31253);
}

TEST(IndexSpecColumnStore, FailsWithExclusionIncludingIdSubfield) {
    auto result = validateIndexSpec(kDefaultOpCtx,
                                    BSON("key" << BSON("$**"
                                                       << "columnstore")
                                               << "name"
                                               << "indexName"
                                               << "columnstoreProjection"
                                               << BSON("_id.field" << 1 << "a" << 0 << "b" << 0)));
    ASSERT_EQ(result.getStatus().code(), 31254);
}

TEST(IndexSpecColumnStore, FailsWithMixedProjection) {
    auto result =
        validateIndexSpec(kDefaultOpCtx,
                          BSON("key" << BSON("$**"
                                             << "columnstore")
                                     << "name"
                                     << "indexName"
                                     << "columnstoreProjection" << BSON("a" << 1 << "b" << 0)));
    ASSERT_EQ(result.getStatus().code(), 31254);
}

TEST(IndexSpecColumnStore, FailsWithComputedFieldsInProjection) {
    auto result = validateIndexSpec(kDefaultOpCtx,
                                    BSON("key" << BSON("$**"
                                                       << "columnstore")
                                               << "name"
                                               << "indexName"
                                               << "columnstoreProjection"
                                               << BSON("a" << 1 << "b"
                                                           << "string")));
    ASSERT_EQ(result.getStatus().code(), 51271);
}

TEST(IndexSpecColumnStore, FailsWhenProjectionPluginNotColumnStore) {
    auto result = validateIndexSpec(kDefaultOpCtx,
                                    BSON("key" << BSON("a"
                                                       << "columnstore")
                                               << "name"
                                               << "indexName"
                                               << "columnstoreProjection" << BSON("a" << 1)));
    ASSERT_EQ(result.getStatus().code(), ErrorCodes::CannotCreateIndex);
}

TEST(IndexSpecColumnStore, FailsWhenProjectionIsNotAnObject) {
    auto result = validateIndexSpec(kDefaultOpCtx,
                                    BSON("key" << BSON("$**"
                                                       << "columnstore")
                                               << "name"
                                               << "indexName"
                                               << "columnstoreProjection" << 4));
    ASSERT_EQ(result.getStatus().code(), ErrorCodes::TypeMismatch);
}

TEST(IndexSpecColumnStore, FailsWithEmptyProjection) {
    auto result = validateIndexSpec(kDefaultOpCtx,
                                    BSON("key" << BSON("$**"
                                                       << "columnstore")
                                               << "name"
                                               << "indexName"
                                               << "columnstoreProjection" << BSONObj()));
    ASSERT_EQ(result.getStatus().code(), ErrorCodes::FailedToParse);
}

TEST(IndexSpecColumnStore, FailsWhenInclusionWithSubpath) {
    auto result = validateIndexSpec(kDefaultOpCtx,
                                    BSON("key" << BSON("a.$**"
                                                       << "columnstore")
                                               << "name"
                                               << "indexName"
                                               << "columnstoreProjection" << BSON("a" << 1)));
    ASSERT_EQ(result.getStatus().code(), ErrorCodes::FailedToParse);
}

TEST(IndexSpecColumnStore, FailsWhenExclusionWithSubpath) {
    auto result = validateIndexSpec(kDefaultOpCtx,
                                    BSON("key" << BSON("a.$**"
                                                       << "columnstore")
                                               << "name"
                                               << "indexName"
                                               << "columnstoreProjection" << BSON("b" << 0)));
    ASSERT_EQ(result.getStatus().code(), ErrorCodes::FailedToParse);
}

TEST(IndexSpecColumnStore, SucceedsWithCompressor) {
    ASSERT_OK(validateIndexSpec(kDefaultOpCtx,
                                BSON("key" << BSON("$**"
                                                   << "columnstore")
                                           << "name"
                                           << "indexName"
                                           << "columnstoreCompressor"
                                           << "none")));

    ASSERT_OK(validateIndexSpec(kDefaultOpCtx,
                                BSON("key" << BSON("$**"
                                                   << "columnstore")
                                           << "name"
                                           << "indexName"
                                           << "columnstoreCompressor"
                                           << "zstd")));
}

TEST(IndexSpecColumnStore, FailsWhenCompressorIsANumber) {
    auto result = validateIndexSpec(kDefaultOpCtx,
                                    BSON("key" << BSON("$**"
                                                       << "columnstore")
                                               << "name"
                                               << "indexName"
                                               << "columnstoreCompressor" << 1.23));
    ASSERT_EQ(result.getStatus().code(), ErrorCodes::TypeMismatch);
}

TEST(IndexSpecColumnStore, FailsWhenCompressorIsAnObject) {
    auto result = validateIndexSpec(kDefaultOpCtx,
                                    BSON("key" << BSON("$**"
                                                       << "columnstore")
                                               << "name"
                                               << "indexName"
                                               << "columnstoreCompressor"
                                               << BSON("compressor"
                                                       << "zstd")));
    ASSERT_EQ(result.getStatus().code(), ErrorCodes::TypeMismatch);
}

TEST(IndexSpecColumnStore, FailsWhenCompressorIsFictional) {
    auto result = validateIndexSpec(kDefaultOpCtx,
                                    BSON("key" << BSON("$**"
                                                       << "columnstore")
                                               << "name"
                                               << "indexName"
                                               << "columnstoreCompressor"
                                               << "middleout"));
    ASSERT_EQ(result.getStatus().code(), ErrorCodes::InvalidIndexSpecificationOption);
}

}  // namespace
}  // namespace mongo
