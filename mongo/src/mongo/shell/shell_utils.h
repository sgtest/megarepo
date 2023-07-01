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

#include <boost/filesystem.hpp>
#include <boost/filesystem/path.hpp>
#include <map>
#include <set>
#include <string>

#include "mongo/base/string_data.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/client/connection_string.h"
#include "mongo/client/mongo_uri.h"
#include "mongo/db/jsobj.h"
#include "mongo/platform/mutex.h"
#include "mongo/util/concurrency/mutex.h"

namespace mongo {

class Scope;
class DBClientBase;

namespace shell_utils {

bool isBalanced(const std::string& code);

extern std::string dbConnect;
using EnterpriseShellCallback = void(Scope&);

void RecordMyLocation(const char* _argv0);
void installShellUtils(Scope& scope);

void initScope(Scope& scope);
void onConnect(DBClientBase& c, StringData uri);

boost::filesystem::path getHistoryFilePath();
void setEnterpriseShellCallback(EnterpriseShellCallback* callback);


BSONElement singleArg(const BSONObj& args);
extern const BSONObj undefinedReturn;

/** Prompt for confirmation from cin. */
class Prompter {
public:
    Prompter(const std::string& prompt);
    /** @return prompted confirmation or cached confirmation. */
    bool confirm();

private:
    const std::string _prompt;
    bool _confirmed;
};

/** Registry of server connections. */
class ConnectionRegistry {
public:
    ConnectionRegistry();
    void registerConnection(DBClientBase& client, StringData uri);
    void killOperationsOnAllConnections(bool withPrompt) const;

private:
    std::map<std::string, std::set<std::string>> _connectionUris;
    mutable Mutex _mutex = MONGO_MAKE_LATCH("ConnectionRegistry::_mutex");
};

extern ConnectionRegistry connectionRegistry;

// Helper to tell if a file exists cross platform
// TODO: Remove this when we have a cross platform file utility library
bool fileExists(const std::string& file);

// If the test began a GoldenTestContext, end it and compare actual/expected results.
void closeGoldenTestContext();

/**
 * Thrown when a GoldenTestContextShell test fails.
 */
struct GoldenTestContextShellFailure {
    std::string message;
    std::string actualOutputFile;
    std::string expectedOutputFile;

    std::string toString() const;
    void diff() const;
};

}  // namespace shell_utils
}  // namespace mongo
