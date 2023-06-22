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

#pragma once

#include <boost/optional/optional.hpp>
#include <vector>

#include "mongo/db/catalog/database.h"
#include "mongo/db/catalog/database_holder.h"
#include "mongo/db/database_name.h"
#include "mongo/db/operation_context.h"
#include "mongo/stdx/unordered_map.h"
#include "mongo/util/concurrency/mutex.h"
#include "mongo/util/string_map.h"

namespace mongo {

class DatabaseHolderImpl : public DatabaseHolder {
public:
    DatabaseHolderImpl() = default;

    Database* getDb(OperationContext* opCtx, const DatabaseName& dbName) const override;

    bool dbExists(OperationContext* opCtx, const DatabaseName& dbName) const override;

    Database* openDb(OperationContext* opCtx,
                     const DatabaseName& dbName,
                     bool* justCreated = nullptr) override;

    void dropDb(OperationContext* opCtx, Database* db) override;

    void close(OperationContext* opCtx, const DatabaseName& dbName) override;

    void closeAll(OperationContext* opCtx) override;

    boost::optional<DatabaseName> getNameWithConflictingCasing(const DatabaseName& dbName) override;

    std::vector<DatabaseName> getNames() override;

private:
    boost::optional<DatabaseName> _getNameWithConflictingCasing_inlock(const DatabaseName& dbName);

    typedef stdx::unordered_map<DatabaseName, Database*> DBs;
    mutable SimpleMutex _m;
    DBs _dbs;
};

}  // namespace mongo
