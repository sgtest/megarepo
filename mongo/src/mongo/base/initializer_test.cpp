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
 * Unit tests of the Initializer type.
 */

#include <cstddef>
#include <fmt/format.h>

#include "mongo/base/error_codes.h"
#include "mongo/base/init.h"  // IWYU pragma: keep
#include "mongo/base/initializer.h"
#include "mongo/base/string_data.h"
#include "mongo/unittest/assert.h"
#include "mongo/unittest/framework.h"
#include "mongo/util/assert_util.h"

namespace mongo {
namespace {

using namespace fmt::literals;

class InitializerTest : public unittest::Test {
public:
    enum State {
        kUnset = 0,
        kInitialized = 1,
        kDeinitialized = 2,
    };

    struct Graph {
        struct Node {
            std::string name;
            std::vector<size_t> prereqs;
        };

        /**
         * The dependency graph expressed as a vector of vectors.
         * Each row is a vector of the corresponding node's dependencies.
         */
        auto prerequisites() const {
            std::vector<std::vector<size_t>> result;
            for (const auto& node : nodes)
                result.push_back(node.prereqs);
            return result;
        }

        /** Invert the prereq edges. */
        auto dependents() const {
            std::vector<std::vector<size_t>> result(nodes.size());
            for (size_t i = 0; i != nodes.size(); ++i)
                for (auto& r : nodes[i].prereqs)
                    result[r].push_back(i);
            return result;
        }

        size_t size() const {
            return nodes.size();
        }

        std::vector<Node> nodes;
    };

    /*
     * Unless otherwise specified, all tests herein use the following
     * dependency graph.
     */
    static inline const Graph graph{{
        {"n0", {}},         // 0
                            // |
        {"n1", {}},         // |  1
                            // |  |
        {"n2", {0, 1}},     // +--+->2
                            // |  |  |
        {"n3", {0, 2}},     // +-----+->3
                            //    |  |  |
        {"n4", {1, 2}},     //    +--+---->4
                            //          |  |
        {"n5", {3, 4}},     //          +--+->5
                            //          |  |  |
        {"n6", {4}},        //          |  +---->6
                            //          |     |  |
        {"n7", {3}},        //          +---------->7
                            //                |  |  |
        {"n8", {5, 6, 7}},  //                +--+--+->8
    }};

    /** The arguments for an addInitializer call. */
    struct NodeSpec {
        std::string name;
        std::function<void(InitializerContext*)> init;
        std::function<void(DeinitializerContext*)> deinit;
        std::vector<std::string> prerequisites;
        std::vector<std::string> dependents;
    };

    void initImpl(size_t idx) {
        auto reqs = graph.prerequisites()[idx];
        for (auto req : reqs)
            if (states[req] != kInitialized)
                uasserted(ErrorCodes::UnknownError,
                          "(init{0}) {1} not already initialized"_format(idx, req));
        states[idx] = kInitialized;
    }

    void deinitImpl(size_t idx) {
        if (states[idx] != kInitialized)
            uasserted(ErrorCodes::UnknownError, "(deinit{0}) {0} not initialized"_format(idx));
        auto deps = graph.dependents()[idx];
        for (auto dep : deps)
            if (states[dep] != kDeinitialized)
                uasserted(ErrorCodes::UnknownError,
                          "(deinit{0}) {1} not already deinitialized"_format(idx, dep));
        states[idx] = kDeinitialized;
    }

    static void initNoop(InitializerContext*) {}
    static void deinitNoop(DeinitializerContext*) {}

    std::vector<NodeSpec> makeDependencyGraphSpecs(const Graph& graph) {
        std::vector<NodeSpec> specs;
        for (size_t idx = 0; idx != graph.size(); ++idx) {
            std::vector<std::string> reqNames;
            for (auto&& req : graph.nodes[idx].prereqs)
                reqNames.push_back(graph.nodes[req].name);
            specs.push_back({graph.nodes[idx].name,
                             [this, idx](InitializerContext*) { initImpl(idx); },
                             [this, idx](DeinitializerContext*) { deinitImpl(idx); },
                             reqNames,
                             {}});
        }
        return specs;
    }

    void constructDependencyGraph(Initializer& initializer,
                                  const std::vector<NodeSpec>& nodeSpecs) {
        for (const auto& n : nodeSpecs)
            initializer.addInitializer(n.name, n.init, n.deinit, n.prerequisites, n.dependents);
    }

    void constructDependencyGraph(Initializer& initializer) {
        constructDependencyGraph(initializer, makeDependencyGraphSpecs(graph));
    }

    std::vector<State> states = std::vector<State>(graph.size(), kUnset);
};

TEST_F(InitializerTest, SuccessfulInitializationAndDeinitialization) {
    Initializer initializer;
    constructDependencyGraph(initializer);

    initializer.executeInitializers({});
    for (size_t i = 0; i != states.size(); ++i)
        ASSERT_EQ(states[i], kInitialized) << i;

    initializer.executeDeinitializers();
    for (size_t i = 0; i != states.size(); ++i)
        ASSERT_EQ(states[i], kDeinitialized) << i;
}

TEST_F(InitializerTest, Init5Misimplemented) {
    auto specs = makeDependencyGraphSpecs(graph);
    for (auto&& spec : specs)
        spec.deinit = deinitNoop;
    specs[5].init = initNoop;
    Initializer initializer;
    constructDependencyGraph(initializer, specs);

    ASSERT_THROWS_CODE(initializer.executeInitializers({}), DBException, ErrorCodes::UnknownError);

    std::vector<State> expected{
        kInitialized,
        kInitialized,
        kInitialized,
        kInitialized,
        kInitialized,
        kUnset,  // 5: noop init
        kInitialized,
        kInitialized,
        kUnset,  // 8: depends on states[5] == kIninitialized, so fails.
    };
    for (size_t i = 0; i != states.size(); ++i)
        ASSERT_EQ(states[i], expected[i]) << i;
}

TEST_F(InitializerTest, Deinit2Misimplemented) {
    auto specs = makeDependencyGraphSpecs(graph);
    specs[2].deinit = deinitNoop;
    Initializer initializer;
    constructDependencyGraph(initializer, specs);

    initializer.executeInitializers({});
    for (size_t i = 0; i != states.size(); ++i)
        ASSERT_EQ(states[i], kInitialized) << i;

    ASSERT_THROWS_CODE(initializer.executeDeinitializers(), DBException, ErrorCodes::UnknownError);

    // Since [2]'s deinit has been replaced with deinitNoop, it does not set states[2]
    // to kDeinitialized. Its dependents [0] and [1] will check for this and fail
    // with UnknownError, also remaining in the kInitialized state themselves.
    std::vector<State> expected{
        kInitialized,  // 0: depends on states[2] == kDeinitialized, so fails
        kInitialized,  // 1: depends on states[2] == kDeinitialized, so fails
        kInitialized,  // 2: noop deinit
        kDeinitialized,
        kDeinitialized,
        kDeinitialized,
        kDeinitialized,
        kDeinitialized,
        kDeinitialized,
    };
    for (size_t i = 0; i != states.size(); ++i)
        ASSERT_EQ(states[i], expected[i]) << i;
}

TEST_F(InitializerTest, InsertNullFunctionFails) {
    Initializer initializer;
    ASSERT_THROWS_CODE(initializer.addInitializer("A", nullptr, nullptr, {}, {}),
                       DBException,
                       ErrorCodes::BadValue);
}

TEST_F(InitializerTest, CannotAddInitializerAfterInitializing) {
    Initializer initializer;
    constructDependencyGraph(initializer);
    initializer.executeInitializers({});
    ASSERT_THROWS_CODE(initializer.addInitializer("test", initNoop, deinitNoop, {}, {}),
                       DBException,
                       ErrorCodes::CannotMutateObject);
}

TEST_F(InitializerTest, CannotDoubleInitialize) {
    Initializer initializer;
    constructDependencyGraph(initializer);
    initializer.executeInitializers({});
    ASSERT_THROWS_CODE(
        initializer.executeInitializers({}), DBException, ErrorCodes::IllegalOperation);
}

TEST_F(InitializerTest, RepeatingInitializerCycle) {
    Initializer initializer;
    constructDependencyGraph(initializer);
    initializer.executeInitializers({});
    initializer.executeDeinitializers();
    initializer.executeInitializers({});
    initializer.executeDeinitializers();
}

TEST_F(InitializerTest, CannotDeinitializeWithoutInitialize) {
    Initializer initializer;
    constructDependencyGraph(initializer);
    ASSERT_THROWS_CODE(
        initializer.executeDeinitializers(), DBException, ErrorCodes::IllegalOperation);
}

TEST_F(InitializerTest, CannotDoubleDeinitialize) {
    Initializer initializer;
    constructDependencyGraph(initializer);
    initializer.executeInitializers({});
    initializer.executeDeinitializers();
    ASSERT_THROWS_CODE(
        initializer.executeDeinitializers(), DBException, ErrorCodes::IllegalOperation);
}

TEST_F(InitializerTest, CannotAddWhenFrozen) {
    Initializer initializer;
    constructDependencyGraph(initializer);
    initializer.executeInitializers({});
    initializer.executeDeinitializers();
    ASSERT_THROWS_CODE(initializer.addInitializer("A", initNoop, nullptr, {}, {}),
                       DBException,
                       ErrorCodes::CannotMutateObject);
}

}  // namespace
}  // namespace mongo
