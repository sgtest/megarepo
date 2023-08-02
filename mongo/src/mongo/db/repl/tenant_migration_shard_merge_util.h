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

#include <boost/filesystem/operations.hpp>
#include <boost/filesystem/path.hpp>
#include <fmt/format.h>
#include <memory>
#include <string>
#include <vector>

#include "mongo/base/string_data.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/bson/oid.h"
#include "mongo/client/dbclient_connection.h"
#include "mongo/db/cursor_id.h"
#include "mongo/db/namespace_string.h"
#include "mongo/db/operation_context.h"
#include "mongo/db/repl/oplog.h"
#include "mongo/db/repl/tenant_migration_shared_data.h"
#include "mongo/db/storage/storage_options.h"
#include "mongo/db/storage/wiredtiger/wiredtiger_import.h"
#include "mongo/executor/scoped_task_executor.h"
#include "mongo/executor/task_executor.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/cancellation.h"
#include "mongo/util/concurrency/thread_pool.h"
#include "mongo/util/future.h"
#include "mongo/util/net/hostandport.h"
#include "mongo/util/uuid.h"

namespace mongo::repl::shard_merge_utils {

inline constexpr StringData kDonatedFilesPrefix = "donatedFiles."_sd;
inline constexpr StringData kImportDoneMarkerPrefix = "importDoneMarker."_sd;
inline constexpr StringData kMigrationTmpDirPrefix = "migrationTmpFiles"_sd;
inline constexpr StringData kMigrationIdFieldName = "migrationId"_sd;
inline constexpr StringData kBackupIdFieldName = "backupId"_sd;
inline constexpr StringData kDonorHostNameFieldName = "donorHostName"_sd;
inline constexpr StringData kDonorDbPathFieldName = "dbpath"_sd;

inline bool isDonatedFilesCollection(const NamespaceString& ns) {
    return ns.isConfigDB() && ns.coll().startsWith(kDonatedFilesPrefix);
}

inline NamespaceString getDonatedFilesNs(const UUID& migrationUUID) {
    return NamespaceString::makeGlobalConfigCollection(kDonatedFilesPrefix +
                                                       migrationUUID.toString());
}

inline NamespaceString getImportDoneMarkerNs(const UUID& migrationUUID) {
    return NamespaceString::makeLocalCollection(kImportDoneMarkerPrefix + migrationUUID.toString());
}

inline boost::filesystem::path fileClonerTempDir(const UUID& migrationId) {
    return boost::filesystem::path(storageGlobalParams.dbpath) /
        fmt::format("{}.{}", kMigrationTmpDirPrefix.toString(), migrationId.toString());
}

/**
 * Computes a boost::filesystem::path generic-style relative path (always uses slashes)
 * from a base path and a relative path.
 */
std::string getPathRelativeTo(const std::string& path, const std::string& basePath);

/**
 * Represents the document structure of config.donatedFiles_<MigrationUUID> collection.
 */
struct MetadataInfo {
    explicit MetadataInfo(const UUID& backupId,
                          const UUID& migrationId,
                          const std::string& donorHostAndPort,
                          const std::string& donorDbPath)
        : backupId(backupId),
          migrationId(migrationId),
          donorHostAndPort(donorHostAndPort),
          donorDbPath(donorDbPath) {}
    UUID backupId;
    UUID migrationId;
    std::string donorHostAndPort;
    std::string donorDbPath;

    static MetadataInfo constructMetadataInfo(const UUID& migrationId,
                                              const std::string& donorHostAndPort,
                                              const BSONObj& obj) {
        auto backupId = UUID(uassertStatusOK(UUID::parse(obj[kBackupIdFieldName])));
        auto donorDbPath = obj[kDonorDbPathFieldName].String();
        return MetadataInfo{backupId, migrationId, donorHostAndPort, donorDbPath};
    }

    BSONObj toBSON(const BSONObj& extraFields) const {
        BSONObjBuilder bob;

        migrationId.appendToBuilder(&bob, kMigrationIdFieldName);
        backupId.appendToBuilder(&bob, kBackupIdFieldName);
        bob.append(kDonorHostNameFieldName, donorHostAndPort);
        bob.append(kDonorDbPathFieldName, donorDbPath);
        bob.append("_id", OID::gen());
        bob.appendElements(extraFields);

        return bob.obj();
    }
};

/**
 * Helpers to create and drop the import done marker collection.
 */
void createImportDoneMarkerLocalCollection(OperationContext* opCtx, const UUID& migrationId);
void dropImportDoneMarkerLocalCollection(OperationContext* opCtx, const UUID& migrationId);

/**
 * Runs rollback to stable on the cloned files associated with the given migration id,
 * and then import the stable cloned files into the main WT instance.
 */
void runRollbackAndThenImportFiles(OperationContext* opCtx, const UUID& migrationId);

/**
 * Send a "getMore" to keep a backup cursor from timing out.
 */
SemiFuture<void> keepBackupCursorAlive(CancellationSource cancellationSource,
                                       std::shared_ptr<executor::TaskExecutor> executor,
                                       HostAndPort hostAndPort,
                                       CursorId cursorId,
                                       NamespaceString namespaceString);
}  // namespace mongo::repl::shard_merge_utils
