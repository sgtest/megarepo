# Copyright 2020 MongoDB Inc.
#
# Permission is hereby granted, free of charge, to any person obtaining
# a copy of this software and associated documentation files (the
# "Software"), to deal in the Software without restriction, including
# without limitation the rights to use, copy, modify, merge, publish,
# distribute, sublicense, and/or sell copies of the Software, and to
# permit persons to whom the Software is furnished to do so, subject to
# the following conditions:
#
# The above copyright notice and this permission notice shall be included
# in all copies or substantial portions of the Software.
#
# THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY
# KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE
# WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
# NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE
# LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
# OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION
# WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
#
"""Pseudo-builders for building and registering unit tests."""
from SCons.Script import Action

from site_scons.mongo import insort_wrapper

LAST_TEST_GROUP = 0

TEST_GROUPS = ['first', 'second', 'third', 'fourth']


def exists(env):
    return True


def build_cpp_unit_test(env, target, source, **kwargs):

    global LAST_TEST_GROUP

    if not isinstance(target, list):
        target = [target]

    for t in target:
        if not t.endswith('_test'):
            env.ConfError(f"CppUnitTest target `{t}' does not end in `_test'")

    test_group = TEST_GROUPS[LAST_TEST_GROUP]
    LAST_TEST_GROUP += 1
    if LAST_TEST_GROUP > len(TEST_GROUPS) - 1:
        LAST_TEST_GROUP = 0

    if not kwargs.get("UNITTEST_HAS_CUSTOM_MAINLINE", False):
        libdeps = kwargs.get("LIBDEPS", env.get("LIBDEPS", [])).copy()
        insort_wrapper(libdeps, "$BUILD_DIR/mongo/unittest/unittest_main")
        kwargs["LIBDEPS"] = libdeps

    unit_test_components = {"tests", "unittests"}
    primary_component = kwargs.get("AIB_COMPONENT", env.get("AIB_COMPONENT", ""))
    if primary_component and not primary_component.endswith("-test"):
        kwargs["AIB_COMPONENT"] = primary_component + "-test"
    elif primary_component:
        kwargs["AIB_COMPONENT"] = primary_component
    else:
        kwargs["AIB_COMPONENT"] = f"{test_group}-quarter-unittests"

    if "AIB_COMPONENTS_EXTRA" in kwargs:
        kwargs["AIB_COMPONENTS_EXTRA"] = set(
            kwargs["AIB_COMPONENTS_EXTRA"]).union(unit_test_components)
    else:
        kwargs["AIB_COMPONENTS_EXTRA"] = list(unit_test_components)

    result = env.Program(target, source, **kwargs)
    env.RegisterTest("$UNITTEST_LIST", result[0])
    env.Alias("$UNITTEST_ALIAS", result[0])

    env.RegisterTest(f"$BUILD_ROOT/{test_group}_quarter_unittests.txt", result[0],
                     generate_alias=False)
    env.Alias(f"install-{test_group}-quarter-unittests", result[0])

    return result


def generate(env):
    for test_group in TEST_GROUPS:
        env.TestList(f"$BUILD_ROOT/{test_group}_quarter_unittests.txt", source=[])
        env.Alias(f"install-{test_group}-quarter-unittests",
                  f"$BUILD_ROOT/{test_group}_quarter_unittests.txt")
    env.TestList("$UNITTEST_LIST", source=[])
    env.AddMethod(build_cpp_unit_test, "CppUnitTest")
    env.Alias("$UNITTEST_ALIAS", "$UNITTEST_LIST")
