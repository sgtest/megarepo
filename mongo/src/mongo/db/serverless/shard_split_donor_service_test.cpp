/**
 *    Copyright (C) 2022-present MongoDB, Inc.
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


#include <algorithm>
#include <boost/none.hpp>
#include <cstddef>
#include <functional>
#include <list>
#include <memory>
#include <ostream>
#include <string>
#include <type_traits>

#include <boost/move/utility_core.hpp>
#include <boost/optional/optional.hpp>

#include "mongo/base/error_codes.h"
#include "mongo/base/status_with.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/bson/bsonmisc.h"
#include "mongo/bson/bsonobjbuilder.h"
#include "mongo/bson/oid.h"
#include "mongo/bson/timestamp.h"
#include "mongo/client/connection_string.h"
#include "mongo/client/mongo_uri.h"
#include "mongo/db/catalog_raii.h"
#include "mongo/db/client.h"
#include "mongo/db/commands.h"
#include "mongo/db/concurrency/lock_manager_defs.h"
#include "mongo/db/database_name.h"
#include "mongo/db/db_raii.h"
#include "mongo/db/dbhelpers.h"
#include "mongo/db/op_observer/op_observer.h"
#include "mongo/db/op_observer/op_observer_registry.h"
#include "mongo/db/repl/primary_only_service.h"
#include "mongo/db/repl/primary_only_service_test_fixture.h"
#include "mongo/db/repl/repl_server_parameters_gen.h"
#include "mongo/db/repl/repl_set_config.h"
#include "mongo/db/repl/repl_settings.h"
#include "mongo/db/repl/replication_coordinator.h"
#include "mongo/db/repl/replication_coordinator_mock.h"
#include "mongo/db/repl/tenant_migration_access_blocker_util.h"
#include "mongo/db/serverless/serverless_operation_lock_registry.h"
#include "mongo/db/serverless/shard_split_donor_op_observer.h"
#include "mongo/db/serverless/shard_split_donor_service.h"
#include "mongo/db/serverless/shard_split_state_machine_gen.h"
#include "mongo/db/serverless/shard_split_test_utils.h"
#include "mongo/db/serverless/shard_split_utils.h"
#include "mongo/db/storage/recovery_unit.h"
#include "mongo/db/tenant_id.h"
#include "mongo/db/write_concern_options.h"
#include "mongo/dbtests/mock/mock_remote_db_server.h"
#include "mongo/dbtests/mock/mock_replica_set.h"
#include "mongo/executor/network_interface_mock.h"
#include "mongo/executor/remote_command_request.h"
#include "mongo/executor/remote_command_response.h"
#include "mongo/executor/thread_pool_mock.h"
#include "mongo/executor/thread_pool_task_executor.h"
#include "mongo/idl/idl_parser.h"
#include "mongo/idl/server_parameter_test_util.h"
#include "mongo/platform/atomic_word.h"
#include "mongo/rpc/op_msg.h"
#include "mongo/rpc/reply_builder_interface.h"
#include "mongo/rpc/reply_interface.h"
#include "mongo/rpc/unique_message.h"
#include "mongo/unittest/assert.h"
#include "mongo/unittest/bson_test_util.h"
#include "mongo/unittest/framework.h"
#include "mongo/util/assert_util.h"
#include "mongo/util/clock_source.h"
#include "mongo/util/clock_source_mock.h"
#include "mongo/util/decorable.h"
#include "mongo/util/duration.h"
#include "mongo/util/fail_point.h"
#include "mongo/util/str.h"
#include "mongo/util/time_support.h"

#define MONGO_LOGV2_DEFAULT_COMPONENT ::mongo::logv2::LogComponent::kTest


namespace mongo {

/**
 * Returns the state doc matching the document with shardSplitId from the disk if it
 * exists.
 *
 * If the stored state doc on disk contains invalid BSON, the 'InvalidBSON' error code is
 * returned.
 *
 * Returns 'NoMatchingDocument' error code if no document with 'shardSplitId' is found.
 */
namespace {

StatusWith<ShardSplitDonorDocument> getStateDocument(OperationContext* opCtx,
                                                     const UUID& shardSplitId) {
    // Use kLastApplied so that we can read the state document as a secondary.
    ReadSourceScope readSourceScope(opCtx, RecoveryUnit::ReadSource::kLastApplied);
    AutoGetCollectionForRead collection(opCtx, NamespaceString::kShardSplitDonorsNamespace);
    if (!collection) {
        return Status(ErrorCodes::NamespaceNotFound,
                      str::stream() << "Collection not found looking for state document: "
                                    << redactTenant(NamespaceString::kShardSplitDonorsNamespace));
    }

    BSONObj result;
    auto foundDoc = Helpers::findOne(opCtx,
                                     collection.getCollection(),
                                     BSON(ShardSplitDonorDocument::kIdFieldName << shardSplitId),
                                     result);

    if (!foundDoc) {
        return Status(ErrorCodes::NoMatchingDocument,
                      str::stream()
                          << "No matching state doc found with shard split id: " << shardSplitId);
    }

    try {
        return ShardSplitDonorDocument::parse(IDLParserContext("shardSplitStateDocument"), result);
    } catch (DBException& ex) {
        return ex.toStatus(str::stream()
                           << "Invalid BSON found for matching document with shard split id: "
                           << shardSplitId << " , res: " << result);
    }
}
}  // namespace

class MockReplReconfigCommandInvocation : public CommandInvocation {
public:
    MockReplReconfigCommandInvocation(const Command* command) : CommandInvocation(command) {}
    void run(OperationContext* opCtx, rpc::ReplyBuilderInterface* result) final {
        result->setCommandReply(BSON("ok" << 1));
    }

    NamespaceString ns() const final {
        return NamespaceString::kSystemReplSetNamespace;
    }

    bool supportsWriteConcern() const final {
        return true;
    }

private:
    void doCheckAuthorization(OperationContext* opCtx) const final {}
};

class MockReplReconfigCommand : public Command {
public:
    MockReplReconfigCommand() : Command("replSetReconfig") {}

    std::unique_ptr<CommandInvocation> parse(OperationContext* opCtx,
                                             const OpMsgRequest& request) final {
        stdx::lock_guard<Latch> lg(_mutex);
        _hasBeenCalled = true;
        _msg = request.body;
        return std::make_unique<MockReplReconfigCommandInvocation>(this);
    }

    AllowedOnSecondary secondaryAllowed(ServiceContext* context) const final {
        return AllowedOnSecondary::kNever;
    }

    BSONObj getLatestConfig() {
        stdx::lock_guard<Latch> lg(_mutex);
        ASSERT_TRUE(_hasBeenCalled);
        return _msg;
    }

private:
    mutable Mutex _mutex = MONGO_MAKE_LATCH("MockReplReconfigCommand::_mutex");
    bool _hasBeenCalled{false};
    BSONObj _msg;
};
MONGO_REGISTER_COMMAND(MockReplReconfigCommand).forShard();

std::ostream& operator<<(std::ostream& builder, mongo::ShardSplitDonorStateEnum state) {
    switch (state) {
        case mongo::ShardSplitDonorStateEnum::kUninitialized:
            builder << "kUninitialized";
            break;
        case mongo::ShardSplitDonorStateEnum::kAbortingIndexBuilds:
            builder << "kAbortingIndexBuilds";
            break;
        case mongo::ShardSplitDonorStateEnum::kAborted:
            builder << "kAborted";
            break;
        case mongo::ShardSplitDonorStateEnum::kBlocking:
            builder << "kBlocking";
            break;
        case mongo::ShardSplitDonorStateEnum::kRecipientCaughtUp:
            builder << "kRecipientCaughtUp";
            break;
        case mongo::ShardSplitDonorStateEnum::kCommitted:
            builder << "kCommitted";
            break;
    }

    return builder;
}

void fastForwardCommittedSnapshotOpTime(
    std::shared_ptr<ShardSplitDonorService::DonorStateMachine> instance,
    ServiceContext* serviceContext,
    OperationContext* opCtx,
    const UUID& uuid) {
    // When a state document is transitioned to kAborted, the ShardSplitDonorOpObserver will
    // transition tenant access blockers to a kAborted state if, and only if, the abort timestamp
    // is less than or equal to the currentCommittedSnapshotOpTime. Since we are using the
    // ReplicationCoordinatorMock, we must manually manage the currentCommittedSnapshotOpTime
    // using this method.
    auto replCoord = dynamic_cast<repl::ReplicationCoordinatorMock*>(
        repl::ReplicationCoordinator::get(serviceContext));

    auto foundStateDoc = uassertStatusOK(getStateDocument(opCtx, uuid));
    invariant(foundStateDoc.getCommitOrAbortOpTime());

    replCoord->setCurrentCommittedSnapshotOpTime(*foundStateDoc.getCommitOrAbortOpTime());
    serviceContext->getOpObserver()->onMajorityCommitPointUpdate(
        serviceContext, *foundStateDoc.getCommitOrAbortOpTime());
}

bool hasActiveSplitForTenants(OperationContext* opCtx, const std::vector<TenantId>& tenantIds) {
    return std::all_of(tenantIds.begin(), tenantIds.end(), [&](const auto& tenantId) {
        return tenant_migration_access_blocker::hasActiveTenantMigration(
            opCtx,
            DatabaseName::createDatabaseName_forTest(boost::none, tenantId.toString() + "_db"));
    });
}

std::pair<bool, executor::RemoteCommandRequest> checkRemoteNameEquals(
    const std::string& commandName, const executor::RemoteCommandRequest& request) {
    auto&& cmdObj = request.cmdObj;
    ASSERT_FALSE(cmdObj.isEmpty());

    if (commandName == cmdObj.firstElementFieldName()) {
        return std::make_pair(true, request);
    }

    return std::make_pair<bool, executor::RemoteCommandRequest>(false, {});
}

executor::RemoteCommandRequest assertRemoteCommandIn(
    std::vector<std::string> commandNames, const executor::RemoteCommandRequest& request) {
    for (const auto& name : commandNames) {
        if (auto res = checkRemoteNameEquals(name, request); res.first) {
            return res.second;
        }
    }

    std::stringstream ss;
    ss << "Expected one of the following commands : [\"";
    for (const auto& name : commandNames) {
        ss << name << "\",";
    }
    ss << "] in remote command request but found \"" << request.cmdObj.firstElementFieldName()
       << "\" instead: " << request.toString();

    FAIL(ss.str());

    return request;
}

executor::RemoteCommandRequest assertRemoteCommandNameEquals(
    const std::string& cmdName, const executor::RemoteCommandRequest& request) {
    auto&& cmdObj = request.cmdObj;
    ASSERT_FALSE(cmdObj.isEmpty());
    if (auto res = checkRemoteNameEquals(cmdName, request); res.first) {
        return res.second;
    } else {
        std::string msg = str::stream()
            << "Expected command name \"" << cmdName << "\" in remote command request but found \""
            << cmdObj.firstElementFieldName() << "\" instead: " << request.toString();
        FAIL(msg);
        return {};
    }
}

bool processReplSetStepUpRequest(executor::NetworkInterfaceMock* net,
                                 MockReplicaSet* replSet,
                                 Status statusToReturn) {
    const std::string commandName{"replSetStepUp"};

    ASSERT(net->hasReadyRequests());
    net->runReadyNetworkOperations();
    auto noi = net->getNextReadyRequest();
    auto request = noi->getRequest();

    // The command can also be `hello`
    assertRemoteCommandIn({"replSetStepUp", "hello"}, request);

    auto&& cmdObj = request.cmdObj;
    auto requestHost = request.target.toString();
    const auto node = replSet->getNode(requestHost);
    if (node->isRunning()) {
        if (commandName == cmdObj.firstElementFieldName() && !statusToReturn.isOK()) {
            net->scheduleErrorResponse(noi, statusToReturn);
        } else {
            const auto opmsg = static_cast<OpMsgRequest>(request);
            const auto reply = node->runCommand(request.id, opmsg)->getCommandReply();
            net->scheduleSuccessfulResponse(
                noi, executor::RemoteCommandResponse(reply, Milliseconds(0)));
        }
    } else {
        net->scheduleErrorResponse(noi, Status(ErrorCodes::HostUnreachable, "generated by test"));
    }

    return commandName == cmdObj.firstElementFieldName();
}


using IncomingRequestValidator = std::function<void(executor::RemoteCommandRequest)>;
void processIncomingRequest(executor::NetworkInterfaceMock* net,
                            MockReplicaSet* replSet,
                            const std::string& commandName,
                            IncomingRequestValidator validator = nullptr) {
    ASSERT(net->hasReadyRequests());
    net->runReadyNetworkOperations();
    auto noi = net->getNextReadyRequest();
    auto request = noi->getRequest();

    assertRemoteCommandNameEquals(commandName, request);
    if (validator) {
        validator(request);
    }

    auto requestHost = request.target.toString();
    const auto node = replSet->getNode(requestHost);
    if (!node->isRunning()) {
        net->scheduleErrorResponse(noi, Status(ErrorCodes::HostUnreachable, ""));
        return;
    }

    const auto opmsg = static_cast<OpMsgRequest>(request);
    const auto reply = node->runCommand(request.id, opmsg)->getCommandReply();
    net->scheduleSuccessfulResponse(noi, executor::RemoteCommandResponse(reply, Milliseconds(0)));
}

void waitForReadyRequest(executor::NetworkInterfaceMock* net) {
    while (!net->hasReadyRequests()) {
        net->advanceTime(net->now() + Milliseconds{1});
    }
}

class ShardSplitDonorServiceTest : public repl::PrimaryOnlyServiceMongoDTest {
public:
    void setUp() override {
        // Set a 30s timeout to prevent spurious timeouts.
        repl::shardSplitTimeoutMS.store(30 * 1000);

        repl::PrimaryOnlyServiceMongoDTest::setUp();

        // The database needs to be open before using shard split donor service.
        {
            auto opCtx = cc().makeOperationContext();
            AutoGetDb autoDb(
                opCtx.get(), NamespaceString::kShardSplitDonorsNamespace.dbName(), MODE_X);
            auto db = autoDb.ensureDbExists(opCtx.get());
            ASSERT_TRUE(db);
        }

        // Timestamps of "0 seconds" are not allowed, so we must advance our clock mock to the first
        // real second. Don't save an instance, since this just internally modified the global
        // immortal ClockSourceMockImpl.
        ClockSourceMock clockSource;
        clockSource.advance(Milliseconds(1000));

        // setup mock networking for split acceptance
        auto network = std::make_unique<executor::NetworkInterfaceMock>();
        _net = network.get();
        _executor = std::make_shared<executor::ThreadPoolTaskExecutor>(
            std::make_unique<executor::ThreadPoolMock>(
                _net, 1, executor::ThreadPoolMock::Options{}),
            std::move(network));
        _executor->startup();

        ShardSplitDonorService::DonorStateMachine::setSplitAcceptanceTaskExecutor_forTest(
            _executor);
    }

    void tearDown() override {
        _net->exitNetwork();
        _executor->shutdown();
        _executor->join();

        repl::PrimaryOnlyServiceMongoDTest::tearDown();
    }

    std::unique_ptr<repl::ReplicationCoordinator> makeReplicationCoordinator() override {
        return std::make_unique<repl::ReplicationCoordinatorMock>(getServiceContext(),
                                                                  _replSettings);
    }

protected:
    std::unique_ptr<repl::PrimaryOnlyService> makeService(ServiceContext* serviceContext) override {
        return std::make_unique<ShardSplitDonorService>(serviceContext);
    }

    void setUpOpObserverRegistry(OpObserverRegistry* opObserverRegistry) override {
        opObserverRegistry->addObserver(std::make_unique<ShardSplitDonorOpObserver>());
    }

    ShardSplitDonorDocument defaultStateDocument() const {
        auto shardSplitStateDoc = ShardSplitDonorDocument::parse(
            IDLParserContext{"donor.document"},
            BSON("_id" << _uuid << "recipientTagName" << _recipientTagName << "recipientSetName"
                       << _recipientSetName));
        shardSplitStateDoc.setTenantIds(_tenantIds);
        return shardSplitStateDoc;
    }

    /**
     * Wait for replSetStepUp command, enqueue hello response, and ignore heartbeats.
     */
    void waitForReplSetStepUp(Status statusToReturn) {
        _net->enterNetwork();
        do {
            waitForReadyRequest(_net);
        } while (!processReplSetStepUpRequest(_net, &_recipientSet, statusToReturn));
        _net->runReadyNetworkOperations();
        _net->exitNetwork();
    }

    void waitForRecipientPrimaryMajorityWrite() {
        _net->enterNetwork();
        waitForReadyRequest(_net);
        processIncomingRequest(
            _net,
            &_recipientSet,
            "appendOplogNote",
            [](const executor::RemoteCommandRequest& request) {
                ASSERT_TRUE(request.cmdObj.hasField(WriteConcernOptions::kWriteConcernField));
                ASSERT_BSONOBJ_EQ(request.cmdObj[WriteConcernOptions::kWriteConcernField].Obj(),
                                  BSON("w" << WriteConcernOptions::kMajority));
            });
        _net->runReadyNetworkOperations();
        _net->exitNetwork();
    }

    /**
     * Wait for monitors to start, and enqueue successfull hello responses
     */
    void waitForMonitorAndProcessHello() {
        _net->enterNetwork();
        waitForReadyRequest(_net);
        processIncomingRequest(_net, &_recipientSet, "hello");
        waitForReadyRequest(_net);
        processIncomingRequest(_net, &_recipientSet, "hello");
        waitForReadyRequest(_net);
        processIncomingRequest(_net, &_recipientSet, "hello");
        _net->runReadyNetworkOperations();
        _net->exitNetwork();
    }

    BSONObj getLatestConfig(OperationContext* opCtx) {
        CommandRegistry* reg = getCommandRegistry(opCtx);
        Command* baseCmd = reg->findCommand("replSetReconfig");
        invariant(baseCmd);
        auto mock = dynamic_cast<MockReplReconfigCommand*>(baseCmd);
        invariant(mock);
        return mock->getLatestConfig();
    }

    const repl::ReplSettings _replSettings = repl::createServerlessReplSettings();
    UUID _uuid = UUID::gen();
    MockReplicaSet _replSet{
        "donorSetForTest", 3, true /* hasPrimary */, false /* dollarPrefixHosts */};
    MockReplicaSet _recipientSet{
        "recipientSetForTest", 3, true /* hasPrimary */, false /* dollarPrefixHosts */};
    const NamespaceString _nss =
        NamespaceString::createNamespaceString_forTest("testDB2", "testColl2");
    std::vector<TenantId> _tenantIds = {TenantId(OID::gen()), TenantId(OID::gen())};
    std::string _recipientTagName{"$recipientNode"};
    std::string _recipientSetName{_recipientSet.getURI().getSetName()};

    std::unique_ptr<FailPointEnableBlock> _skipAcceptanceFP =
        std::make_unique<FailPointEnableBlock>("skipShardSplitWaitForSplitAcceptance");

    std::unique_ptr<FailPointEnableBlock> _skipGarbageTimeoutFP =
        std::make_unique<FailPointEnableBlock>("skipShardSplitGarbageCollectionTimeout");


    // for mocking split acceptance
    executor::NetworkInterfaceMock* _net;
    TaskExecutorPtr _executor;
};

auto makeHelloReply(const std::string& setName,
                    const repl::OpTime& lastWriteOpTime = repl::OpTime(Timestamp(100, 1), 1)) {
    BSONObjBuilder opTimeBuilder;
    lastWriteOpTime.append(&opTimeBuilder, "opTime");
    return BSON("setName" << setName << "lastWrite" << opTimeBuilder.obj());
};

void mockCommandReplies(MockReplicaSet* replSet) {
    for (const auto& hostAndPort : replSet->getHosts()) {
        auto node = replSet->getNode(hostAndPort.toString());
        node->setCommandReply("replSetStepUp", BSON("ok" << 1));
        node->setCommandReply("appendOplogNote", BSON("ok" << 1));
        node->setCommandReply("hello", makeHelloReply(replSet->getSetName()));
    }
}

TEST_F(ShardSplitDonorServiceTest, BasicShardSplitDonorServiceInstanceCreation) {
    auto opCtx = makeOperationContext();
    test::shard_split::ScopedTenantAccessBlocker scopedTenants(_uuid, opCtx.get());
    test::shard_split::reconfigToAddRecipientNodes(
        getServiceContext(), _recipientTagName, _replSet.getHosts(), _recipientSet.getHosts());

    // Shard split service will send a stepUp request to the first node in the vector.
    mockCommandReplies(&_recipientSet);

    // We reset this failpoint to test complete functionality. waitForMonitorAndProcessHello()
    // returns hello responses that makes split acceptance pass.
    _skipAcceptanceFP.reset();

    // Create and start the instance.
    auto serviceInstance = ShardSplitDonorService::DonorStateMachine::getOrCreate(
        opCtx.get(), _service, defaultStateDocument().toBSON());
    ASSERT(serviceInstance.get());
    ASSERT_EQ(_uuid, serviceInstance->getId());

    waitForMonitorAndProcessHello();
    waitForReplSetStepUp(Status(ErrorCodes::OK, ""));
    waitForRecipientPrimaryMajorityWrite();

    // Verify the serverless lock has been acquired for split.
    auto& registry = ServerlessOperationLockRegistry::get(opCtx->getServiceContext());
    ASSERT_EQ(*registry.getActiveOperationType_forTest(),
              ServerlessOperationLockRegistry::LockType::kShardSplit);

    auto result = serviceInstance->decisionFuture().get();
    ASSERT_TRUE(hasActiveSplitForTenants(opCtx.get(), _tenantIds));
    ASSERT(!result.abortReason);
    ASSERT_EQ(result.state, mongo::ShardSplitDonorStateEnum::kCommitted);

    serviceInstance->tryForget();
    auto completionFuture = serviceInstance->completionFuture();
    completionFuture.wait();

    // The lock has been released.
    ASSERT_FALSE(registry.getActiveOperationType_forTest());

    ASSERT_OK(serviceInstance->completionFuture().getNoThrow());
    ASSERT_TRUE(serviceInstance->isGarbageCollectable());
}

TEST_F(ShardSplitDonorServiceTest, ShardSplitFailsWhenLockIsHeld) {
    auto opCtx = makeOperationContext();
    test::shard_split::reconfigToAddRecipientNodes(
        getServiceContext(), _recipientTagName, _replSet.getHosts(), _recipientSet.getHosts());

    auto& registry = ServerlessOperationLockRegistry::get(opCtx->getServiceContext());
    registry.acquireLock(ServerlessOperationLockRegistry::LockType::kTenantRecipient, UUID::gen());

    // Create and start the instance.
    auto serviceInstance = ShardSplitDonorService::DonorStateMachine::getOrCreate(
        opCtx.get(), _service, defaultStateDocument().toBSON());
    ASSERT(serviceInstance.get());

    auto decisionFuture = serviceInstance->decisionFuture();

    auto result = decisionFuture.getNoThrow();
    ASSERT_EQ(result.getStatus().code(), ErrorCodes::ConflictingServerlessOperation);
}

TEST_F(ShardSplitDonorServiceTest, ReplSetStepUpRetryable) {
    auto opCtx = makeOperationContext();
    test::shard_split::ScopedTenantAccessBlocker scopedTenants(_uuid, opCtx.get());
    test::shard_split::reconfigToAddRecipientNodes(
        getServiceContext(), _recipientTagName, _replSet.getHosts(), _recipientSet.getHosts());

    // Shard split service will send a stepUp request to the first node in the vector. When it fails
    // it will send it to the next node.
    mockCommandReplies(&_recipientSet);

    // We disable this failpoint to test complete functionality. waitForMonitorAndProcessHello()
    // returns hello responses that makes split acceptance pass.
    _skipAcceptanceFP.reset();

    // Create and start the instance.
    auto serviceInstance = ShardSplitDonorService::DonorStateMachine::getOrCreate(
        opCtx.get(), _service, defaultStateDocument().toBSON());
    ASSERT(serviceInstance.get());
    ASSERT_EQ(_uuid, serviceInstance->getId());

    waitForMonitorAndProcessHello();

    // Shard split will retry the command indefinitely for timeout/retriable errors.
    waitForReplSetStepUp(Status(ErrorCodes::NetworkTimeout, "test-generated retryable error"));
    waitForReplSetStepUp(Status(ErrorCodes::SocketException, "test-generated retryable error"));
    waitForReplSetStepUp(
        Status(ErrorCodes::ConnectionPoolExpired, "test-generated retryable error"));
    waitForReplSetStepUp(Status(ErrorCodes::ExceededTimeLimit, "test-generated retryable error"));
    waitForReplSetStepUp(Status(ErrorCodes::OK, "test-generated retryable error"));
    waitForRecipientPrimaryMajorityWrite();

    auto result = serviceInstance->decisionFuture().get();

    ASSERT(!result.abortReason);
    ASSERT_EQ(result.state, mongo::ShardSplitDonorStateEnum::kCommitted);
}

TEST_F(ShardSplitDonorServiceTest, ShardSplitDonorServiceTimeout) {
    FailPointEnableBlock fp("pauseShardSplitAfterBlocking");

    auto opCtx = makeOperationContext();
    auto serviceContext = getServiceContext();
    test::shard_split::ScopedTenantAccessBlocker scopedTenants(_uuid, opCtx.get());
    test::shard_split::reconfigToAddRecipientNodes(
        serviceContext, _recipientTagName, _replSet.getHosts(), _recipientSet.getHosts());

    auto stateDocument = defaultStateDocument();

    // Set a timeout of 200 ms, and make sure we reset after this test is run
    RAIIServerParameterControllerForTest controller{"shardSplitTimeoutMS", 200};

    // Create and start the instance.
    auto serviceInstance = ShardSplitDonorService::DonorStateMachine::getOrCreate(
        opCtx.get(), _service, stateDocument.toBSON());
    ASSERT(serviceInstance.get());
    ASSERT_EQ(_uuid, serviceInstance->getId());

    auto result = serviceInstance->decisionFuture().get();

    ASSERT(result.abortReason);
    ASSERT_EQ(result.abortReason->code(), ErrorCodes::ExceededTimeLimit);

    fastForwardCommittedSnapshotOpTime(serviceInstance, serviceContext, opCtx.get(), _uuid);
    serviceInstance->tryForget();

    ASSERT_OK(serviceInstance->completionFuture().getNoThrow());
    ASSERT_TRUE(serviceInstance->isGarbageCollectable());
}

TEST_F(ShardSplitDonorServiceTest, ReconfigToRemoveSplitConfig) {
    auto opCtx = makeOperationContext();
    test::shard_split::ScopedTenantAccessBlocker scopedTenants(_uuid, opCtx.get());
    test::shard_split::reconfigToAddRecipientNodes(
        getServiceContext(), _recipientTagName, _replSet.getHosts(), _recipientSet.getHosts());

    // Shard split service will send a stepUp request to the first node in the vector.
    mockCommandReplies(&_recipientSet);

    _skipAcceptanceFP.reset();

    auto fpPtr = std::make_unique<FailPointEnableBlock>("pauseShardSplitBeforeSplitConfigRemoval");
    auto initialTimesEntered = fpPtr->initialTimesEntered();

    // Create and start the instance.
    auto serviceInstance = ShardSplitDonorService::DonorStateMachine::getOrCreate(
        opCtx.get(), _service, defaultStateDocument().toBSON());
    ASSERT(serviceInstance.get());
    ASSERT_EQ(_uuid, serviceInstance->getId());

    waitForMonitorAndProcessHello();
    waitForReplSetStepUp(Status::OK());
    waitForRecipientPrimaryMajorityWrite();

    auto result = serviceInstance->decisionFuture().get();
    ASSERT(!result.abortReason);
    ASSERT_EQ(result.state, mongo::ShardSplitDonorStateEnum::kCommitted);

    (*fpPtr)->waitForTimesEntered(initialTimesEntered + 1);

    // Validate we currently have a splitConfig and set it as the mock's return value.
    BSONObj splitConfigBson = getLatestConfig(&*opCtx);
    auto splitConfig = repl::ReplSetConfig::parse(splitConfigBson["replSetReconfig"].Obj());
    ASSERT(splitConfig.isSplitConfig());
    auto replCoord = repl::ReplicationCoordinator::get(getServiceContext());
    dynamic_cast<repl::ReplicationCoordinatorMock*>(replCoord)->setGetConfigReturnValue(
        splitConfig);

    // Validate shard split sets a new replicaSetId on the recipientConfig.
    auto recipientConfig = *splitConfig.getRecipientConfig();
    ASSERT_NE(splitConfig.getReplicaSetId(), recipientConfig.getReplicaSetId());

    // Clear the failpoint and wait for completion.
    fpPtr.reset();
    serviceInstance->tryForget();

    auto completionFuture = serviceInstance->completionFuture();
    completionFuture.wait();

    BSONObj finalConfigBson = getLatestConfig(&*opCtx);
    ASSERT_TRUE(finalConfigBson.hasField("replSetReconfig"));
    auto finalConfig = repl::ReplSetConfig::parse(finalConfigBson["replSetReconfig"].Obj());
    ASSERT(!finalConfig.isSplitConfig());
}

TEST_F(ShardSplitDonorServiceTest, SendReplSetStepUpToHighestLastApplied) {
    // Proves that the node with the highest lastAppliedOpTime is chosen as the recipient primary,
    // by replacing the default `hello` replies (set by the MockReplicaSet) with ones that report
    // `lastWrite.opTime` values in a deterministic way.
    auto opCtx = makeOperationContext();
    test::shard_split::ScopedTenantAccessBlocker scopedTenants(_uuid, opCtx.get());
    test::shard_split::reconfigToAddRecipientNodes(
        getServiceContext(), _recipientTagName, _replSet.getHosts(), _recipientSet.getHosts());

    auto newerOpTime = mongo::repl::OpTime(Timestamp(200, 1), 24);
    auto olderOpTime = mongo::repl::OpTime(Timestamp(100, 1), 24);

    mockCommandReplies(&_recipientSet);
    auto recipientPrimary = _recipientSet.getNode(_recipientSet.getHosts()[1].toString());
    recipientPrimary->setCommandReply("hello", makeHelloReply(_recipientSetName, newerOpTime));

    for (auto&& recipientNodeHost : _recipientSet.getHosts()) {
        if (recipientNodeHost == recipientPrimary->getServerHostAndPort()) {
            continue;
        }

        auto recipientNode = _recipientSet.getNode(recipientNodeHost.toString());
        recipientNode->setCommandReply("hello", makeHelloReply(_recipientSetName, olderOpTime));
    }

    _skipAcceptanceFP.reset();
    auto serviceInstance = ShardSplitDonorService::DonorStateMachine::getOrCreate(
        opCtx.get(), _service, defaultStateDocument().toBSON());
    ASSERT(serviceInstance.get());
    ASSERT_EQ(_uuid, serviceInstance->getId());
    auto splitAcceptanceFuture = serviceInstance->getSplitAcceptanceFuture_forTest();

    waitForMonitorAndProcessHello();
    waitForReplSetStepUp(Status::OK());
    waitForRecipientPrimaryMajorityWrite();

    auto result = serviceInstance->decisionFuture().get();
    ASSERT(!result.abortReason);
    ASSERT_EQ(result.state, mongo::ShardSplitDonorStateEnum::kCommitted);

    auto acceptedRecipientPrimary = splitAcceptanceFuture.get(opCtx.get());
    ASSERT_EQ(acceptedRecipientPrimary, recipientPrimary->getServerHostAndPort());
}

// Abort scenario : abortSplit called before startSplit.
TEST_F(ShardSplitDonorServiceTest, CreateInstanceInAbortedState) {
    auto opCtx = makeOperationContext();
    auto serviceContext = getServiceContext();

    test::shard_split::ScopedTenantAccessBlocker scopedTenants(_uuid, opCtx.get());
    test::shard_split::reconfigToAddRecipientNodes(
        serviceContext, _recipientTagName, _replSet.getHosts(), _recipientSet.getHosts());

    auto stateDocument = defaultStateDocument();
    stateDocument.setState(ShardSplitDonorStateEnum::kAborted);

    auto serviceInstance = ShardSplitDonorService::DonorStateMachine::getOrCreate(
        opCtx.get(), _service, stateDocument.toBSON());
    ASSERT(serviceInstance.get());

    auto result = serviceInstance->decisionFuture().get(opCtx.get());

    ASSERT(!!result.abortReason);
    ASSERT_EQ(result.abortReason->code(), ErrorCodes::TenantMigrationAborted);
    ASSERT_EQ(result.state, mongo::ShardSplitDonorStateEnum::kAborted);

    serviceInstance->tryForget();

    ASSERT_OK(serviceInstance->completionFuture().getNoThrow());
    ASSERT_TRUE(serviceInstance->isGarbageCollectable());
}

// Abort scenario : instance created through startSplit then calling abortSplit.
TEST_F(ShardSplitDonorServiceTest, CreateInstanceThenAbort) {
    auto opCtx = makeOperationContext();
    auto serviceContext = getServiceContext();

    test::shard_split::ScopedTenantAccessBlocker scopedTenants(_uuid, opCtx.get());
    test::shard_split::reconfigToAddRecipientNodes(
        serviceContext, _recipientTagName, _replSet.getHosts(), _recipientSet.getHosts());

    std::shared_ptr<ShardSplitDonorService::DonorStateMachine> serviceInstance;
    {
        FailPointEnableBlock fp("pauseShardSplitAfterBlocking");
        auto initialTimesEntered = fp.initialTimesEntered();

        serviceInstance = ShardSplitDonorService::DonorStateMachine::getOrCreate(
            opCtx.get(), _service, defaultStateDocument().toBSON());
        ASSERT(serviceInstance.get());

        fp->waitForTimesEntered(initialTimesEntered + 1);

        serviceInstance->tryAbort();
    }

    auto result = serviceInstance->decisionFuture().get(opCtx.get());

    ASSERT(!!result.abortReason);
    ASSERT_EQ(result.abortReason->code(), ErrorCodes::TenantMigrationAborted);
    ASSERT_EQ(result.state, mongo::ShardSplitDonorStateEnum::kAborted);

    fastForwardCommittedSnapshotOpTime(serviceInstance, serviceContext, opCtx.get(), _uuid);
    serviceInstance->tryForget();

    ASSERT_OK(serviceInstance->completionFuture().getNoThrow());
    ASSERT_TRUE(serviceInstance->isGarbageCollectable());
}

TEST_F(ShardSplitDonorServiceTest, StepDownTest) {
    auto opCtx = makeOperationContext();
    test::shard_split::ScopedTenantAccessBlocker scopedTenants(_uuid, opCtx.get());
    test::shard_split::reconfigToAddRecipientNodes(
        getServiceContext(), _recipientTagName, _replSet.getHosts(), _recipientSet.getHosts());

    std::shared_ptr<ShardSplitDonorService::DonorStateMachine> serviceInstance;

    {
        FailPointEnableBlock fp("pauseShardSplitAfterBlocking");
        auto initialTimesEntered = fp.initialTimesEntered();

        serviceInstance = ShardSplitDonorService::DonorStateMachine::getOrCreate(
            opCtx.get(), _service, defaultStateDocument().toBSON());
        ASSERT(serviceInstance.get());

        fp->waitForTimesEntered(initialTimesEntered + 1);

        stepDown();
    }

    auto result = serviceInstance->decisionFuture().getNoThrow();
    ASSERT_FALSE(result.isOK());
    ASSERT_EQ(ErrorCodes::CallbackCanceled, result.getStatus());

    ASSERT_EQ(serviceInstance->completionFuture().getNoThrow(), ErrorCodes::CallbackCanceled);
    ASSERT_FALSE(serviceInstance->isGarbageCollectable());
}

TEST_F(ShardSplitDonorServiceTest, DeleteStateDocMarkedGarbageCollectable) {
    // Instance building (from inserted state document) is done in a separate thread. This failpoint
    // disable it to ensure there's no race condition with the insertion of the state document.
    FailPointEnableBlock fp("PrimaryOnlyServiceSkipRebuildingInstances");

    auto opCtx = makeOperationContext();

    test::shard_split::ScopedTenantAccessBlocker scopedTenants(_uuid, opCtx.get());
    test::shard_split::reconfigToAddRecipientNodes(
        getServiceContext(), _recipientTagName, _replSet.getHosts(), _recipientSet.getHosts());

    auto stateDocument = defaultStateDocument();
    stateDocument.setState(ShardSplitDonorStateEnum::kAborted);
    stateDocument.setCommitOrAbortOpTime(repl::OpTime(Timestamp(1, 1), 1));

    Status status(ErrorCodes::CallbackCanceled, "Split has been aborted");
    BSONObjBuilder bob;
    status.serializeErrorToBSON(&bob);
    stateDocument.setAbortReason(bob.obj());

    boost::optional<mongo::Date_t> expireAt = getServiceContext()->getFastClockSource()->now() +
        Milliseconds{repl::shardSplitGarbageCollectionDelayMS.load()};
    stateDocument.setExpireAt(expireAt);

    // insert the document for the first time.
    ASSERT_OK(serverless::insertStateDoc(opCtx.get(), stateDocument));

    // deletes a document that was marked as garbage collectable and succeeds.
    StatusWith<bool> deleted = serverless::deleteStateDoc(opCtx.get(), stateDocument.getId());

    ASSERT_OK(deleted.getStatus());
    ASSERT_TRUE(deleted.getValue());

    ASSERT_EQ(getStateDocument(opCtx.get(), _uuid).getStatus().code(),
              ErrorCodes::NoMatchingDocument);
}

TEST_F(ShardSplitDonorServiceTest, AbortDueToRecipientNodesValidation) {
    auto opCtx = makeOperationContext();
    auto serviceContext = getServiceContext();
    test::shard_split::ScopedTenantAccessBlocker scopedTenants(_uuid, opCtx.get());

    // Matching recipientSetName to the replSetName to fail validation and abort shard split.
    test::shard_split::reconfigToAddRecipientNodes(
        serviceContext, _recipientTagName, _replSet.getHosts(), _recipientSet.getHosts());

    auto stateDocument = defaultStateDocument();
    stateDocument.setRecipientSetName("donor"_sd);

    // Create and start the instance.
    auto serviceInstance = ShardSplitDonorService::DonorStateMachine::getOrCreate(
        opCtx.get(), _service, stateDocument.toBSON());
    ASSERT(serviceInstance.get());
    ASSERT_EQ(_uuid, serviceInstance->getId());

    auto result = serviceInstance->decisionFuture().get();

    ASSERT_EQ(result.state, mongo::ShardSplitDonorStateEnum::kAborted);
    ASSERT(result.abortReason);
    ASSERT_EQ(result.abortReason->code(), ErrorCodes::BadValue);
    ASSERT_TRUE(serviceInstance->isGarbageCollectable());

    auto statusWithDoc = getStateDocument(opCtx.get(), stateDocument.getId());
    ASSERT_OK(statusWithDoc.getStatus());

    ASSERT_EQ(statusWithDoc.getValue().getState(), ShardSplitDonorStateEnum::kAborted);
}

TEST(RecipientAcceptSplitListenerTest, FutureReady) {
    MockReplicaSet donor{"donor", 3, true /* hasPrimary */, false /* dollarPrefixHosts */};
    auto listener =
        mongo::serverless::RecipientAcceptSplitListener(donor.getURI().connectionString());

    for (const auto& host : donor.getHosts()) {
        ASSERT_FALSE(listener.getSplitAcceptedFuture().isReady());
        listener.onServerHeartbeatSucceededEvent(host, makeHelloReply(donor.getSetName()));
    }

    ASSERT_TRUE(listener.getSplitAcceptedFuture().isReady());
}

TEST(RecipientAcceptSplitListenerTest, FutureReadyNameChange) {
    MockReplicaSet donor{"donor", 3, true /* hasPrimary */, false /* dollarPrefixHosts */};
    auto listener =
        mongo::serverless::RecipientAcceptSplitListener(donor.getURI().connectionString());

    for (const auto& host : donor.getHosts()) {
        listener.onServerHeartbeatSucceededEvent(host, makeHelloReply("invalidSetName"));
    }

    ASSERT_FALSE(listener.getSplitAcceptedFuture().isReady());

    for (const auto& host : donor.getHosts()) {
        listener.onServerHeartbeatSucceededEvent(host, makeHelloReply(donor.getSetName()));
    }

    ASSERT_TRUE(listener.getSplitAcceptedFuture().isReady());
}

TEST(RecipientAcceptSplitListenerTest, FutureNotReadyMissingNodes) {
    MockReplicaSet donor{"donor", 3, false /* hasPrimary */, false /* dollarPrefixHosts */};
    auto listener =
        mongo::serverless::RecipientAcceptSplitListener(donor.getURI().connectionString());


    for (size_t i = 0; i < donor.getHosts().size() - 1; ++i) {
        listener.onServerHeartbeatSucceededEvent(donor.getHosts()[i],
                                                 makeHelloReply(donor.getSetName()));
    }

    ASSERT_FALSE(listener.getSplitAcceptedFuture().isReady());
    listener.onServerHeartbeatSucceededEvent(donor.getHosts()[donor.getHosts().size() - 1],
                                             makeHelloReply(donor.getSetName()));

    ASSERT_TRUE(listener.getSplitAcceptedFuture().isReady());
}

TEST(RecipientAcceptSplitListenerTest, FutureNotReadyNoSetName) {
    MockReplicaSet donor{"donor", 3, true /* hasPrimary */, false /* dollarPrefixHosts */};
    auto listener =
        mongo::serverless::RecipientAcceptSplitListener(donor.getURI().connectionString());

    for (size_t i = 0; i < donor.getHosts().size() - 1; ++i) {
        listener.onServerHeartbeatSucceededEvent(donor.getHosts()[i], BSONObj());
    }

    ASSERT_FALSE(listener.getSplitAcceptedFuture().isReady());
}

TEST(RecipientAcceptSplitListenerTest, FutureNotReadyWrongSet) {
    MockReplicaSet donor{"donor", 3, true /* hasPrimary */, false /* dollarPrefixHosts */};
    auto listener =
        mongo::serverless::RecipientAcceptSplitListener(donor.getURI().connectionString());

    for (const auto& host : donor.getHosts()) {
        listener.onServerHeartbeatSucceededEvent(host, makeHelloReply("wrongSetName"));
    }

    ASSERT_FALSE(listener.getSplitAcceptedFuture().isReady());
}

TEST_F(ShardSplitDonorServiceTest, ResumeAfterStepdownTest) {
    auto opCtx = makeOperationContext();
    test::shard_split::ScopedTenantAccessBlocker scopedTenants(_uuid, opCtx.get());
    test::shard_split::reconfigToAddRecipientNodes(
        getServiceContext(), _recipientTagName, _replSet.getHosts(), _recipientSet.getHosts());

    auto firstSplitInstance = [&]() {
        FailPointEnableBlock fp("pauseShardSplitAfterBlocking");
        auto initialTimesEntered = fp.initialTimesEntered();

        std::shared_ptr<ShardSplitDonorService::DonorStateMachine> serviceInstance =
            ShardSplitDonorService::DonorStateMachine::getOrCreate(
                opCtx.get(), _service, defaultStateDocument().toBSON());
        ASSERT(serviceInstance.get());

        fp->waitForTimesEntered(initialTimesEntered + 1);
        return serviceInstance;
    }();

    stepDown();
    auto result = firstSplitInstance->completionFuture().getNoThrow();
    ASSERT_FALSE(result.isOK());
    ASSERT_EQ(ErrorCodes::CallbackCanceled, result.code());

    auto secondSplitInstance = [&]() {
        FailPointEnableBlock fp("pauseShardSplitAfterBlocking");
        stepUp(opCtx.get());
        fp->waitForTimesEntered(fp.initialTimesEntered() + 1);

        ASSERT_OK(getStateDocument(opCtx.get(), _uuid).getStatus());
        auto [serviceInstance, isPausedOrShutdown] =
            ShardSplitDonorService::DonorStateMachine::lookup(
                opCtx.get(), _service, BSON("_id" << _uuid));
        ASSERT_TRUE(serviceInstance);
        ASSERT_FALSE(isPausedOrShutdown);
        return *serviceInstance;
    }();

    ASSERT_OK(secondSplitInstance->decisionFuture().getNoThrow().getStatus());
    secondSplitInstance->tryForget();
    ASSERT_OK(secondSplitInstance->completionFuture().getNoThrow());
    ASSERT_TRUE(secondSplitInstance->isGarbageCollectable());
}

class ShardSplitPersistenceTest : public ShardSplitDonorServiceTest {
public:
    void setUpPersistence(OperationContext* opCtx) override {

        // We need to allow writes during the test's setup.
        auto replCoord = dynamic_cast<repl::ReplicationCoordinatorMock*>(
            repl::ReplicationCoordinator::get(opCtx->getServiceContext()));
        replCoord->alwaysAllowWrites(true);

        replCoord->setGetConfigReturnValue(initialDonorConfig());

        _recStateDoc = initialStateDocument();
        uassertStatusOK(serverless::insertStateDoc(opCtx, _recStateDoc));

        ServerlessOperationLockRegistry::get(getServiceContext())
            .acquireLock(ServerlessOperationLockRegistry::LockType::kShardSplit,
                         _recStateDoc.getId());

        _pauseBeforeRecipientCleanupFp =
            std::make_unique<FailPointEnableBlock>("pauseShardSplitBeforeRecipientCleanup");

        _initialTimesEntered = _pauseBeforeRecipientCleanupFp->initialTimesEntered();
    }

    virtual repl::ReplSetConfig initialDonorConfig() = 0;

    virtual ShardSplitDonorDocument initialStateDocument() = 0;

protected:
    ShardSplitDonorDocument _recStateDoc;
    std::unique_ptr<FailPointEnableBlock> _pauseBeforeRecipientCleanupFp;
    FailPoint::EntryCountT _initialTimesEntered;
};

class ShardSplitRecipientCleanupTest : public ShardSplitPersistenceTest {
public:
    repl::ReplSetConfig initialDonorConfig() override {
        BSONArrayBuilder members;
        members.append(BSON("_id" << 1 << "host"
                                  << "node1"
                                  << "tags" << BSON("recipientTagName" << UUID::gen().toString())));

        return repl::ReplSetConfig::parse(BSON("_id" << _recipientSetName << "version" << 1
                                                     << "protocolVersion" << 1 << "members"
                                                     << members.arr()));
    }

    ShardSplitDonorDocument initialStateDocument() override {

        auto stateDocument = defaultStateDocument();
        stateDocument.setBlockOpTime(repl::OpTime(Timestamp(1, 1), 1));
        stateDocument.setState(ShardSplitDonorStateEnum::kBlocking);
        stateDocument.setRecipientConnectionString(ConnectionString::forLocal());

        return stateDocument;
    }
};

TEST_F(ShardSplitRecipientCleanupTest, ShardSplitRecipientCleanup) {
    auto opCtx = makeOperationContext();
    test::shard_split::ScopedTenantAccessBlocker scopedTenants(_uuid, opCtx.get());

    ASSERT_OK(getStateDocument(opCtx.get(), _uuid).getStatus());

    ASSERT_FALSE(hasActiveSplitForTenants(opCtx.get(), _tenantIds));

    auto decisionFuture = [&]() {
        ASSERT(_pauseBeforeRecipientCleanupFp);
        (*(_pauseBeforeRecipientCleanupFp.get()))->waitForTimesEntered(_initialTimesEntered + 1);

        tenant_migration_access_blocker::recoverTenantMigrationAccessBlockers(opCtx.get());

        auto splitService = repl::PrimaryOnlyServiceRegistry::get(opCtx->getServiceContext())
                                ->lookupServiceByName(ShardSplitDonorService::kServiceName);
        auto [optionalDonor, isPausedOrShutdown] =
            ShardSplitDonorService::DonorStateMachine::lookup(
                opCtx.get(), splitService, BSON("_id" << _uuid));

        ASSERT_TRUE(optionalDonor);
        ASSERT_FALSE(isPausedOrShutdown);
        ASSERT_TRUE(hasActiveSplitForTenants(opCtx.get(), _tenantIds));

        auto serviceInstance = optionalDonor.value();
        ASSERT(serviceInstance.get());

        _pauseBeforeRecipientCleanupFp.reset();

        return serviceInstance->decisionFuture();
    }();

    auto result = decisionFuture.get();

    // We set the promise before the future chain. Cleanup will return kCommitted as a result.
    ASSERT(!result.abortReason);
    ASSERT_EQ(result.state, mongo::ShardSplitDonorStateEnum::kCommitted);

    // deleted the local state doc so this should return NoMatchingDocument
    ASSERT_EQ(getStateDocument(opCtx.get(), _uuid).getStatus().code(),
              ErrorCodes::NoMatchingDocument);
}

class ShardSplitAbortedStepUpTest : public ShardSplitPersistenceTest {
public:
    repl::ReplSetConfig initialDonorConfig() override {
        BSONArrayBuilder members;
        members.append(BSON("_id" << 1 << "host"
                                  << "node1"));

        return repl::ReplSetConfig::parse(BSON("_id"
                                               << "donorSetName"
                                               << "version" << 1 << "protocolVersion" << 1
                                               << "members" << members.arr()));
    }

    ShardSplitDonorDocument initialStateDocument() override {

        auto stateDocument = defaultStateDocument();

        stateDocument.setState(mongo::ShardSplitDonorStateEnum::kAborted);
        stateDocument.setBlockOpTime(repl::OpTime(Timestamp(1, 1), 1));
        stateDocument.setCommitOrAbortOpTime(repl::OpTime(Timestamp(1, 1), 1));

        Status status(ErrorCodes::InternalError, abortReason);
        BSONObjBuilder bob;
        status.serializeErrorToBSON(&bob);
        stateDocument.setAbortReason(bob.obj());

        return stateDocument;
    }

    std::string abortReason{"Testing simulated error"};
};

TEST_F(ShardSplitAbortedStepUpTest, ShardSplitAbortedStepUp) {
    auto opCtx = makeOperationContext();
    auto splitService = repl::PrimaryOnlyServiceRegistry::get(opCtx->getServiceContext())
                            ->lookupServiceByName(ShardSplitDonorService::kServiceName);
    auto [optionalDonor, isPausedOrShutdown] = ShardSplitDonorService::DonorStateMachine::lookup(
        opCtx.get(), splitService, BSON("_id" << _uuid));

    ASSERT_TRUE(optionalDonor);
    ASSERT_FALSE(isPausedOrShutdown);
    auto result = optionalDonor->get()->decisionFuture().get();

    ASSERT_EQ(result.state, mongo::ShardSplitDonorStateEnum::kAborted);
    ASSERT_TRUE(!!result.abortReason);
    ASSERT_EQ(result.abortReason->code(), ErrorCodes::InternalError);
    ASSERT_EQ(result.abortReason->reason(), abortReason);
}

}  // namespace mongo
