/**
 *    Copyright (C) 2020-present MongoDB, Inc.
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

#include <cstddef>
#include <memory>
#include <string>
#include <vector>

#include "mongo/base/status.h"
#include "mongo/base/string_data.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/bson/timestamp.h"
#include "mongo/client/dbclient_connection.h"
#include "mongo/db/repl/base_cloner.h"
#include "mongo/db/repl/storage_interface.h"
#include "mongo/db/repl/tenant_base_cloner.h"
#include "mongo/db/repl/tenant_database_cloner.h"
#include "mongo/db/repl/tenant_migration_shared_data.h"
#include "mongo/util/concurrency/thread_pool.h"
#include "mongo/util/net/hostandport.h"
#include "mongo/util/time_support.h"

namespace mongo {
namespace repl {

class TenantAllDatabaseCloner final : public TenantBaseCloner {
public:
    struct Stats {
        size_t databasesToClone{0};
        size_t databasesCloned{0};
        size_t databasesClonedBeforeFailover{0};
        std::vector<TenantDatabaseCloner::Stats> databaseStats;
        Date_t start;

        long long approxTotalDataSize{0};
        long long approxTotalBytesCopied{0};

        std::string toString() const;
        BSONObj toBSON() const;
        void append(BSONObjBuilder* builder) const;
    };

    TenantAllDatabaseCloner(TenantMigrationSharedData* sharedData,
                            const HostAndPort& source,
                            DBClientConnection* client,
                            StorageInterface* storageInterface,
                            ThreadPool* dbPool,
                            StringData tenantId);

    virtual ~TenantAllDatabaseCloner() = default;

    Stats getStats() const;

    std::string toString() const;

    Timestamp getOperationTime_forTest();

protected:
    ClonerStages getStages() final;

private:
    friend class TenantAllDatabaseClonerTest;

    class TenantAllDatabaseClonerStage : public ClonerStage<TenantAllDatabaseCloner> {
    public:
        TenantAllDatabaseClonerStage(std::string name,
                                     TenantAllDatabaseCloner* cloner,
                                     ClonerRunFn stageFunc)
            : ClonerStage<TenantAllDatabaseCloner>(name, cloner, stageFunc) {}

        bool isTransientError(const Status& status) override {
            // Always abort on error.
            return false;
        }
    };

    /**
     * Stage function that retrieves database information from the donor.
     */
    AfterStageBehavior listDatabasesStage();

    /**
     * Stage function that retrieves information locally on the recipient for databases that are
     * already cloned.
     */
    AfterStageBehavior listExistingDatabasesStage();

    /**
     * Stage function that initializes several stats before carrying on to the 'postStage'.
     */
    AfterStageBehavior initializeStatsStage();

    /**
     * The preStage sets the start time in _stats.
     */
    void preStage() final;

    /**
     *
     * The postStage creates and runs the individual TenantDatabaseCloners on each database found on
     * the sync source.
     */
    void postStage() final;

    // All member variables are labeled with one of the following codes indicating the
    // synchronization rules for accessing them.
    //
    // (R)  Read-only in concurrent operation; no synchronization required.
    // (S)  Self-synchronizing; access according to classes own rules.
    // (M)  Reads and writes guarded by _mutex (defined in base class).
    // (X)  Access only allowed from the main flow of control called from run() or constructor.
    // (MX) Write access with mutex from main flow of control, read access with mutex from other
    //      threads, read access allowed from main flow without mutex.
    std::vector<std::string> _databases;                           // (X)
    std::unique_ptr<TenantDatabaseCloner> _currentDatabaseCloner;  // (MX)

    // The database name prefix of the tenant associated with this migration.
    std::string _tenantId;  // (R)

    TenantAllDatabaseClonerStage _listDatabasesStage;          // (R)
    TenantAllDatabaseClonerStage _listExistingDatabasesStage;  // (R)
    TenantAllDatabaseClonerStage _initializeStatsStage;        // (R)

    // The operationTime returned with the listDatabases result.
    Timestamp _operationTime;  // (X)

    Stats _stats;  // (MX)
};

}  // namespace repl
}  // namespace mongo
