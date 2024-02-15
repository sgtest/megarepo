"""Extension to SCons providing advanced static library dependency tracking.

These modifications to a build environment, which can be attached to
StaticLibrary and Program builders via a call to setup_environment(env),
cause the build system to track library dependencies through static libraries,
and to add them to the link command executed when building programs.

For example, consider a program 'try' that depends on a lib 'tc', which in
turn uses a symbol from a lib 'tb' which in turn uses a library from 'ta'.

Without this package, the Program declaration for "try" looks like this:

Program('try', ['try.c', 'path/to/${LIBPREFIX}tc${LIBSUFFIX}',
                'path/to/${LIBPREFIX}tb${LIBSUFFIX}',
                'path/to/${LIBPREFIX}ta${LIBSUFFIX}',])

With this library, we can instead write the following

Program('try', ['try.c'], LIBDEPS=['path/to/tc'])
StaticLibrary('tc', ['c.c'], LIBDEPS=['path/to/tb'])
StaticLibrary('tb', ['b.c'], LIBDEPS=['path/to/ta'])
StaticLibrary('ta', ['a.c'])

And the build system will figure out that it needs to link libta.a and libtb.a
when building 'try'.

A StaticLibrary S may also declare programs or libraries, [L1, ...] to be dependent
upon S by setting LIBDEPS_DEPENDENTS=[L1, ...], using the same syntax as is used
for LIBDEPS, except that the libraries and programs will not have LIBPREFIX/LIBSUFFIX
automatically added when missing.
"""

# Copyright (c) 2010, Corensic Inc., All Rights Reserved.
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

from collections import defaultdict
from functools import partial
import enum
import copy
import json
import os
import sys
import glob
import textwrap
import hashlib
import json
import fileinput
import subprocess

try:
    import networkx
    from buildscripts.libdeps.libdeps.graph import EdgeProps, NodeProps, LibdepsGraph
except ImportError:
    pass

import SCons.Errors
import SCons.Scanner
import SCons.Util
import SCons
from SCons.Script import COMMAND_LINE_TARGETS


class Constants:
    Libdeps = "LIBDEPS"
    LibdepsCached = "LIBDEPS_cached"
    LibdepsDependents = "LIBDEPS_DEPENDENTS"
    LibdepsGlobal = "LIBDEPS_GLOBAL"
    LibdepsNoInherit = "LIBDEPS_NO_INHERIT"
    LibdepsInterface = "LIBDEPS_INTERFACE"
    LibdepsPrivate = "LIBDEPS_PRIVATE"
    LibdepsTags = "LIBDEPS_TAGS"
    LibdepsTagExpansion = "LIBDEPS_TAG_EXPANSIONS"
    MissingLibdep = "MISSING_LIBDEP_"
    ProgdepsDependents = "PROGDEPS_DEPENDENTS"
    SysLibdeps = "SYSLIBDEPS"
    SysLibdepsCached = "SYSLIBDEPS_cached"
    SysLibdepsPrivate = "SYSLIBDEPS_PRIVATE"


class deptype(tuple, enum.Enum):
    Global: tuple = (0, 'GLOBAL')
    Public: tuple = (1, 'PUBLIC')
    Private: tuple = (2, 'PRIVATE')
    Interface: tuple = (3, 'INTERFACE')

    def __lt__(self, other):
        if self.__class__ is other.__class__:
            return self.value[0] < other.value[0]
        return NotImplemented

    def __str__(self):
        return self.value[1]

    def __int__(self):
        return self.value[0]


class dependency:
    def __init__(self, value, deptype, listed_name):
        self.target_node = value
        self.dependency_type = deptype
        self.listed_name = listed_name

    def __str__(self):
        return str(self.target_node)


class FlaggedLibdep:
    """
    Utility class used for processing prefix and postfix flags on libdeps. The class
    can keep track of separate lists for prefix and postfix as well separators,
    allowing for modifications to the lists and then re-application of the flags with
    modifications to a larger list representing the link line.
    """

    def __init__(self, libnode=None, env=None, start_index=None):
        """
        The libnode should be a Libdep SCons node, and the env is the target env in
        which the target has a dependency on the libdep. The start_index is important as
        it determines where this FlaggedLibdep starts in the larger list of libdeps.

        The start_index will cut the larger list, and then re-apply this libdep with flags
        at that location. This class will exract the prefix and postfix flags
        from the Libdep nodes env.
        """
        self.libnode = libnode
        self.env = env

        # We need to maintain our own copy so as not to disrupt the env's original list.
        try:
            self.prefix_flags = copy.copy(getattr(libnode.attributes, 'libdeps_prefix_flags', []))
            self.postfix_flags = copy.copy(getattr(libnode.attributes, 'libdeps_postfix_flags', []))
        except AttributeError:
            self.prefix_flags = []
            self.postfix_flags = []

        self.start_index = start_index

    def __str__(self):
        return str(self.libnode)

    def add_lib_to_result_list(self, result):
        """
        This function takes in the current list of libdeps for a given target, and will
        apply the libdep taking care of the prefix, postfix and any required separators when
        adding to the list.
        """
        if self.start_index != None:
            result[:] = result[:self.start_index]
        self._add_lib_and_flags(result)

    def _get_separators(self, flags):

        separated_list = []

        for flag in flags:
            separators = self.env.get('LIBDEPS_FLAG_SEPARATORS', {}).get(flag, {})
            separated_list.append(separators.get('prefix', ' '))
            separated_list.append(flag)
            separated_list.append(separators.get('suffix', ' '))

        return separated_list

    def _get_lib_with_flags(self):

        lib_and_flags = []

        lib_and_flags += self._get_separators(self.prefix_flags)
        lib_and_flags += [str(self)]
        lib_and_flags += self._get_separators(self.postfix_flags)

        return lib_and_flags

    def _add_lib_and_flags(self, result):
        """
        This function will clean up the flags for the link line after extracting everything
        from the environment. This will mostly look for separators that are just a space, and
        remove them from the list, as the final link line will add spaces back for each item
        in the list. It will take to concat flags where the separators don't allow for a space.
        """
        next_contig_str = ''

        for item in self._get_lib_with_flags():
            if item != ' ':
                next_contig_str += item
            else:
                if next_contig_str:
                    result.append(next_contig_str)
                next_contig_str = ''

        if next_contig_str:
            result.append(next_contig_str)


class LibdepLinter:
    """
    This class stores the rules for linting the libdeps. Using a decorator,
    new rules can easily be added to the class, and will be called when
    linting occurs. Each rule is run on each libdep.

    When a rule is broken, a LibdepLinterError exception will be raised.
    Optionally the class can be configured to print the error message and
    keep going with the build.

    Each rule should provide a method to skip that rule on a given node,
    by supplying the correct flag in the LIBDEPS_TAG environment var for
    that node.

    """

    skip_linting = False
    print_linter_errors = False

    linting_time = 0
    linting_infractions = 0
    linting_rules_run = 0
    registered_linting_time = False

    dangling_dep_dependents = set()

    @staticmethod
    def _make_linter_decorator():
        """
        This is used for gathering the functions
        by decorator that will be used for linting a given libdep.
        """

        funcs = {}

        def linter_rule_func(func):
            funcs[func.__name__] = func
            return func

        linter_rule_func.all = funcs
        return linter_rule_func

    linter_rule = _make_linter_decorator.__func__()
    linter_final_check = _make_linter_decorator.__func__()

    @classmethod
    def _skip_linting(cls):
        return cls.skip_linting

    @classmethod
    def _start_timer(cls):
        # Record time spent linting if we are in print mode.
        if cls.print_linter_errors:
            from timeit import default_timer as timer
            return timer()

    @classmethod
    def _stop_timer(cls, start, num_rules):
        # Record time spent linting if we are in print mode.
        if cls.print_linter_errors:
            from timeit import default_timer as timer
            cls.linting_time += timer() - start
            cls.linting_rules_run += num_rules

    def __init__(self, env, target=None):
        self.env = env
        self.target = target
        self.unique_libs = set()
        self._libdeps_types_previous = dict()

        # If we are in print mode, we will record some linting metrics,
        # and print the results at the end of the build.
        if self.__class__.print_linter_errors and not self.__class__.registered_linting_time:
            import atexit

            def print_linting_time():
                print(f"Spent {self.__class__.linting_time} seconds linting libdeps.")
                print(
                    f"Found {self.__class__.linting_infractions} issues out of {self.__class__.linting_rules_run} libdeps rules checked."
                )

            atexit.register(print_linting_time)
            self.__class__.registered_linting_time = True

    def lint_libdeps(self, libdeps):
        """
        Lint the given list of libdeps for all
        rules.
        """

        # Build performance optimization if you
        # are sure your build is clean.
        if self._skip_linting():
            return
        start = self._start_timer()

        linter_rules = [getattr(self, linter_rule) for linter_rule in self.linter_rule.all]

        for libdep in libdeps:
            for linter_rule in linter_rules:
                linter_rule(libdep)

        self._stop_timer(start, len(linter_rules) * len(libdeps))

    def final_checks(self):
        # Build performance optimization if you
        # are sure your build is clean.
        if self._skip_linting():
            return
        start = self._start_timer()

        linter_rules = [
            getattr(self.__class__, rule) for rule in self.__class__.linter_final_check.all
        ]

        for linter_rule in linter_rules:
            linter_rule(self)

        self._stop_timer(start, len(linter_rules))

    def _raise_libdep_lint_exception(self, message):
        """
        Raises the LibdepLinterError exception or if configure
        to do so, just prints the error.
        """
        prefix = "LibdepLinter: \n\t"
        message = prefix + message.replace('\n', '\n\t') + '\n'
        if self.__class__.print_linter_errors:
            self.__class__.linting_infractions += 1
            print(message)
        else:
            raise LibdepLinterError(message)

    def _check_for_lint_tags(self, lint_tag, env=None, inclusive_tag=False):
        """
        Used to get the lint tag from the environment,
        and if printing instead of raising exceptions,
        will ignore the tags.
        """

        # If print mode is on, we want to make sure to bypass checking
        # exclusive tags so we can make sure the exceptions are not excluded
        # and are printed. If it's an inclusive tag, we want to ignore this
        # early return completely, because we want to make sure the node
        # gets included for checking, and the exception gets printed.
        if not inclusive_tag and self.__class__.print_linter_errors:
            return False

        target_env = env if env else self.env

        if lint_tag in target_env.get(Constants.LibdepsTags, []):
            return True

    def _get_deps_dependents(self, env=None):
        """ util function to get all types of DEPS_DEPENDENTS"""
        target_env = env if env else self.env
        deps_dependents = target_env.get(Constants.LibdepsDependents, []).copy()
        deps_dependents += target_env.get(Constants.ProgdepsDependents, [])
        return deps_dependents

    def _get_deps_dependents_with_types(self, builder, type):
        return [(dependent[0], builder) if isinstance(dependent, tuple) else (dependent, builder)
                for dependent in self.env.get(type, [])]

    @linter_rule
    def linter_rule_leaf_node_no_deps(self, libdep):
        """
        LIBDEP RULE:
            Nodes marked explicitly as a leaf node should not have any dependencies,
            unless those dependencies are explicitly marked as allowed as leaf node
            dependencies.
        """
        if not self._check_for_lint_tags('lint-leaf-node-no-deps', inclusive_tag=True):
            return

        # Ignore dependencies that explicitly exempt themselves.
        if self._check_for_lint_tags('lint-leaf-node-allowed-dep', libdep.target_node.env):
            return

        # Global dependencies will apply to leaf nodes, so they should
        # be automatically exempted.
        if libdep.dependency_type == deptype.Global:
            return

        target_type = self.target[0].builder.get_name(self.env)
        lib = os.path.basename(str(libdep))
        self._raise_libdep_lint_exception(
            textwrap.dedent(f"""\
                {target_type} '{self.target[0]}' has dependency '{lib}' and is marked explicitly as a leaf node,
                and '{lib}' does not exempt itself as an exception to the rule."""))

    @linter_rule
    def linter_rule_no_dangling_deps(self, libdep):
        """
        LIBDEP RULE:
            All reverse dependency edges must point to a node which will be built.
        """
        if self._check_for_lint_tags('lint-allow-dangling-dep-dependent'):
            return

        # Gather the DEPS_DEPENDENTS and store them for a final check to make sure they were
        # eventually defined as being built by some builder
        libdep_libbuilder = self.target[0].builder.get_name(self.env)
        deps_depends = self._get_deps_dependents_with_types(libdep_libbuilder,
                                                            Constants.LibdepsDependents)
        deps_depends += self._get_deps_dependents_with_types("Program",
                                                             Constants.ProgdepsDependents)
        deps_depends = [(_get_node_with_ixes(self.env, dep[0], dep[1]), dep[1])
                        for dep in deps_depends]
        self.__class__.dangling_dep_dependents.update(deps_depends)

    @linter_final_check
    def linter_rule_no_dangling_dep_final_check(self):
        # At this point the SConscripts have defined all the build items,
        # and so we can go check any DEPS_DEPENDENTS listed and make sure a builder
        # was instantiated to build them.
        for dep_dependent in self.__class__.dangling_dep_dependents:
            if not dep_dependent[0].has_builder():
                self._raise_libdep_lint_exception(
                    textwrap.dedent(f"""\
                        Found reverse dependency linked to node '{dep_dependent[0]}'
                        which will never be built by any builder.
                        Remove the reverse dependency or add a way to build it."""))

    @linter_rule
    def linter_rule_no_public_deps(self, libdep):
        """
        LIBDEP RULE:
            Nodes explicitly marked as not allowed to have public dependencies, should not
            have public dependencies, unless the dependency is explicitly marked as allowed.
        """
        if not self._check_for_lint_tags('lint-no-public-deps', inclusive_tag=True):
            return

        if libdep.dependency_type not in (deptype.Global, deptype.Private):
            # Check if the libdep exempts itself from this rule.
            if self._check_for_lint_tags('lint-public-dep-allowed', libdep.target_node.env):
                return

            target_type = self.target[0].builder.get_name(self.env)
            lib = os.path.basename(str(libdep))
            self._raise_libdep_lint_exception(
                textwrap.dedent(f"""\
                    {target_type} '{self.target[0]}' has public dependency '{lib}'
                    while being marked as not allowed to have public dependencies
                    and '{lib}' does not exempt itself."""))

    @linter_rule
    def linter_rule_no_dups(self, libdep):
        """
        LIBDEP RULE:
            A given node shall not link the same LIBDEP across public, private
            or interface dependency types because it is ambiguous and unnecessary.
        """
        if self._check_for_lint_tags('lint-allow-dup-libdeps'):
            return

        if str(libdep) in self.unique_libs:
            target_type = self.target[0].builder.get_name(self.env)
            lib = os.path.basename(str(libdep))
            self._raise_libdep_lint_exception(
                f"{target_type} '{self.target[0]}' links '{lib}' multiple times.")

        self.unique_libs.add(str(libdep))

    @linter_rule
    def linter_rule_alphabetic_deps(self, libdep):
        """
        LIBDEP RULE:
            Libdeps shall be listed alphabetically by type in the SCons files.
        """

        if self._check_for_lint_tags('lint-allow-non-alphabetic'):
            return

        # Start checking order after the first item in the list is recorded to compare with.
        if libdep.dependency_type in self._libdeps_types_previous:
            if self._libdeps_types_previous[libdep.dependency_type] > libdep.listed_name:
                target_type = self.target[0].builder.get_name(self.env)
                self._raise_libdep_lint_exception(
                    f"{target_type} '{self.target[0]}' has '{libdep.listed_name}' listed in {dep_type_to_env_var[libdep.dependency_type]} out of alphabetical order."
                )

        self._libdeps_types_previous[libdep.dependency_type] = libdep.listed_name

    @linter_rule
    def linter_rule_programs_link_private(self, libdep):
        """
        LIBDEP RULE:
            All Programs shall only have public dependency's
            because a Program will never be a dependency of another Program
            or Library, and LIBDEPS transitiveness does not apply. Public
            transitiveness has no meaning in this case and is used just as default.
        """
        if self._check_for_lint_tags('lint-allow-program-links-private'):
            return

        if (self.target[0].builder.get_name(self.env) == "Program"
                and libdep.dependency_type not in (deptype.Global, deptype.Public)):

            lib = os.path.basename(str(libdep))
            self._raise_libdep_lint_exception(
                textwrap.dedent(f"""\
                    Program '{self.target[0]}' links non-public library '{lib}'
                    A 'Program' can only have {Constants.Libdeps} libs,
                    not {Constants.LibdepsPrivate} or {Constants.LibdepsInterface}."""))

    @linter_rule
    def linter_rule_no_bidirectional_deps(self, libdep):
        """
        LIBDEP RULE:
            And Library which issues reverse dependencies, shall not be directly
            linked to by another node, to prevent forward and reverse linkages existing
            at the same node. Instead the content of the library that needs to issue reverse
            dependency needs to be separated from content that needs direct linkage into two
            separate libraries, which can be linked correctly respectively.
        """

        if not libdep.target_node.env:
            return
        elif self._check_for_lint_tags('lint-allow-bidirectional-edges', libdep.target_node.env):
            return
        elif len(self._get_deps_dependents(libdep.target_node.env)) > 0:

            target_type = self.target[0].builder.get_name(self.env)
            lib = os.path.basename(str(libdep))
            self._raise_libdep_lint_exception(
                textwrap.dedent(f"""\
                    {target_type} '{self.target[0]}' links directly to a reverse dependency node '{lib}'
                    No node can link directly to a node that has {Constants.LibdepsDependents} or {Constants.ProgdepsDependents}."""
                                ))

    @linter_rule
    def linter_rule_nonprivate_on_deps_dependents(self, libdep):
        """
        LIBDEP RULE:
            A Library that issues reverse dependencies, shall not link libraries
            with any kind of transitiveness, and will only link libraries privately.
            This is because functionality that requires reverse dependencies should
            not be transitive.
        """
        if self._check_for_lint_tags('lint-allow-nonprivate-on-deps-dependents'):
            return

        if (libdep.dependency_type != deptype.Private and libdep.dependency_type != deptype.Global
                and len(self._get_deps_dependents()) > 0):

            target_type = self.target[0].builder.get_name(self.env)
            lib = os.path.basename(str(libdep))
            self._raise_libdep_lint_exception(
                textwrap.dedent(f"""\
                {target_type} '{self.target[0]}' links non-private libdep '{lib}' and has a reverse dependency.
                A {target_type} can only have {Constants.LibdepsPrivate} depends if it has {Constants.LibdepsDependents} or {Constants.ProgdepsDependents}."""
                                ))

    @linter_rule
    def linter_rule_libdeps_must_be_list(self, libdep):
        """
        LIBDEP RULE:
            LIBDEPS, LIBDEPS_PRIVATE, and LIBDEPS_INTERFACE must be set as lists in the
            environment.
        """
        if self._check_for_lint_tags('lint-allow-nonlist-libdeps'):
            return

        libdeps_vars = list(dep_type_to_env_var.values()) + [
            Constants.LibdepsDependents,
            Constants.ProgdepsDependents,
        ]

        for dep_type_val in libdeps_vars:

            libdeps_list = self.env.get(dep_type_val, [])
            if not SCons.Util.is_List(libdeps_list):

                target_type = self.target[0].builder.get_name(self.env)
                self._raise_libdep_lint_exception(
                    textwrap.dedent(f"""\
                    Found non-list type '{libdeps_list}' while evaluating {dep_type_val[1]} for {target_type} '{self.target[0]}'
                    {dep_type_val[1]} must be setup as a list."""))


dependency_visibility_ignored = {
    deptype.Global: deptype.Public,
    deptype.Interface: deptype.Public,
    deptype.Public: deptype.Public,
    deptype.Private: deptype.Public,
}

dependency_visibility_honored = {
    deptype.Global: deptype.Private,
    deptype.Interface: deptype.Interface,
    deptype.Public: deptype.Public,
    deptype.Private: deptype.Private,
}

dep_type_to_env_var = {
    deptype.Global: Constants.LibdepsGlobal,
    deptype.Interface: Constants.LibdepsInterface,
    deptype.Public: Constants.Libdeps,
    deptype.Private: Constants.LibdepsPrivate,
}


class DependencyCycleError(SCons.Errors.UserError):
    """Exception representing a cycle discovered in library dependencies."""

    def __init__(self, first_node):
        super(DependencyCycleError, self).__init__()
        self.cycle_nodes = [first_node]

    def __str__(self):
        return "Library dependency cycle detected: " + " => ".join(str(n) for n in self.cycle_nodes)


class LibdepLinterError(SCons.Errors.UserError):
    """Exception representing a discongruent usages of libdeps"""


class MissingSyslibdepError(SCons.Errors.UserError):
    """Exception representing a discongruent usages of libdeps"""


def _get_sorted_direct_libdeps(node):
    direct_sorted = getattr(node.attributes, "libdeps_direct_sorted", None)
    if direct_sorted is None:
        direct = getattr(node.attributes, "libdeps_direct", [])
        direct_sorted = sorted(direct, key=lambda t: str(t.target_node))
        setattr(node.attributes, "libdeps_direct_sorted", direct_sorted)
    return direct_sorted


class LibdepsVisitationMark(enum.IntEnum):
    UNMARKED = 0
    MARKED_PRIVATE = 1
    MARKED_PUBLIC = 2


def _libdeps_visit_private(n, marked, walking, debug=False):
    if marked[n.target_node] >= LibdepsVisitationMark.MARKED_PRIVATE:
        return

    if n.target_node in walking:
        raise DependencyCycleError(n.target_node)

    walking.add(n.target_node)

    try:
        for child in _get_sorted_direct_libdeps(n.target_node):
            _libdeps_visit_private(child, marked, walking)

        marked[n.target_node] = LibdepsVisitationMark.MARKED_PRIVATE

    except DependencyCycleError as e:
        if len(e.cycle_nodes) == 1 or e.cycle_nodes[0] != e.cycle_nodes[-1]:
            e.cycle_nodes.insert(0, n.target_node)
        raise

    finally:
        walking.remove(n.target_node)


def _libdeps_visit(n, tsorted, marked, walking, debug=False):
    # The marked dictionary tracks which sorts of visitation a node
    # has received. Values for a given node can be UNMARKED/absent,
    # MARKED_PRIVATE, or MARKED_PUBLIC. These are to be interpreted as
    # follows:
    #
    # 0/UNMARKED: Node is not not marked.
    #
    # MARKED_PRIVATE: Node has only been explored as part of looking
    # for cycles under a LIBDEPS_PRIVATE edge.
    #
    # MARKED_PUBLIC: Node has been explored and any of its transiive
    # dependencies have been incorporated into `tsorted`.
    #
    # The __libdeps_visit_private function above will only mark things
    # at with MARKED_PRIVATE, while __libdeps_visit will mark things
    # MARKED_PUBLIC.
    if marked[n.target_node] == LibdepsVisitationMark.MARKED_PUBLIC:
        return

    # The walking set is used for cycle detection. We record all our
    # predecessors in our depth-first search, and if we observe one of
    # our predecessors as a child, we know we have a cycle.
    if n.target_node in walking:
        raise DependencyCycleError(n.target_node)

    walking.add(n.target_node)

    if debug:
        print(f"    * {n.dependency_type} => {n.listed_name}")

    try:
        children = _get_sorted_direct_libdeps(n.target_node)

        # We first walk all of our public dependencies so that we can
        # put full marks on anything that is in our public transitive
        # graph. We then do a second walk into any private nodes to
        # look for cycles. While we could do just one walk over the
        # children, it is slightly faster to do two passes, since if
        # the algorithm walks into a private edge early, it would do a
        # lot of non-productive (except for cycle checking) walking
        # and marking, but if another public path gets into that same
        # subtree, then it must walk and mark it again to raise it to
        # the public mark level. Whereas, if the algorithm first walks
        # the whole public tree, then those are all productive marks
        # and add to tsorted, and then the private walk will only need
        # to examine those things that are only reachable via private
        # edges.

        for child in children:
            if child.dependency_type != deptype.Private:
                _libdeps_visit(child, tsorted, marked, walking, debug)

        for child in children:
            if child.dependency_type == deptype.Private:
                _libdeps_visit_private(child, marked, walking, debug)

        marked[n.target_node] = LibdepsVisitationMark.MARKED_PUBLIC
        tsorted.append(n.target_node)

    except DependencyCycleError as e:
        if len(e.cycle_nodes) == 1 or e.cycle_nodes[0] != e.cycle_nodes[-1]:
            e.cycle_nodes.insert(0, n.target_node)
        raise

    finally:
        walking.remove(n.target_node)


def _get_libdeps(node, debug=False):
    """Given a SCons Node, return its library dependencies, topologically sorted.

    Computes the dependencies if they're not already cached.
    """

    cache = getattr(node.attributes, Constants.LibdepsCached, None)
    if cache is not None:
        if debug:
            print("  Cache:")
            for dep in cache:
                print(f"    * {str(dep)}")
        return cache

    if debug:
        print(f"  Edges:")

    tsorted = []

    marked = defaultdict(lambda: LibdepsVisitationMark.UNMARKED)
    walking = set()

    for child in _get_sorted_direct_libdeps(node):
        if child.dependency_type != deptype.Interface:
            _libdeps_visit(child, tsorted, marked, walking, debug=debug)
    tsorted.reverse()

    setattr(node.attributes, Constants.LibdepsCached, tsorted)
    return tsorted


def _missing_syslib(name):
    return Constants.MissingLibdep + name


def update_scanner(env, builder_name=None, debug=False):
    """Update the scanner for "builder" to also scan library dependencies."""

    builder = env["BUILDERS"][builder_name]
    old_scanner = builder.target_scanner

    if old_scanner:
        path_function = old_scanner.path_function
    else:
        path_function = None

    def new_scanner(node, env, path=()):
        if debug:
            print(f"LIBDEPS SCANNER: {str(node)}")
            print(f"  Declared dependencies:")
            print(f"    global: {env.get(Constants.LibdepsGlobal, None)}")
            print(f"    private: {env.get(Constants.LibdepsPrivate, None)}")
            print(f"    public: {env.get(Constants.Libdeps, None)}")
            print(f"    interface: {env.get(Constants.LibdepsInterface, None)}")
            print(f"    no_inherit: {env.get(Constants.LibdepsNoInherit, None)}")

        if old_scanner:
            result = old_scanner.function(node, env, path)
        else:
            result = []
        result.extend(_get_libdeps(node, debug=debug))
        if debug:
            print(f"  Build dependencies:")
            print('\n'.join(['    * ' + str(t) for t in result]))
            print('\n')
        return result

    builder.target_scanner = SCons.Scanner.Scanner(function=new_scanner,
                                                   path_function=path_function)


def get_libdeps(source, target, env, for_signature, debug=False):
    """Implementation of the special _LIBDEPS environment variable.

    Expands to the library dependencies for a target.
    """

    target = env.Flatten([target])
    return _get_libdeps(target[0], debug=debug)


def get_libdeps_objs(source, target, env, for_signature, debug=False):
    objs = []
    for lib in get_libdeps(source, target, env, for_signature, debug=debug):
        # This relies on Node.sources being order stable build-to-build.
        objs.extend(lib.sources)
    return objs


def stringify_deps(env, deps):
    lib_link_prefix = env.subst("$LIBLINKPREFIX")
    lib_link_suffix = env.subst("$LIBLINKSUFFIX")

    # Elements of libdeps are either strings (str or unicode), or they're File objects.
    # If they're File objects, they can be passed straight through.  If they're strings,
    # they're believed to represent library short names, that should be prefixed with -l
    # or the compiler-specific equivalent.  I.e., 'm' becomes '-lm', but 'File("m.a") is passed
    # through whole cloth.
    return [f"{lib_link_prefix}{d}{lib_link_suffix}" if isinstance(d, str) else d for d in deps]


def get_syslibdeps(source, target, env, for_signature, debug=False, shared=True):
    """ Given a SCons Node, return its system library dependencies.

    These are the dependencies listed with SYSLIBDEPS, and are linked using -l.
    """

    deps = getattr(target[0].attributes, Constants.SysLibdepsCached, None)
    if deps is None:

        # Get the syslibdeps for the current node
        deps = target[0].get_env().Flatten(
            copy.copy(target[0].get_env().get(Constants.SysLibdepsPrivate)) or [])
        deps += target[0].get_env().Flatten(target[0].get_env().get(Constants.SysLibdeps) or [])

        for lib in _get_libdeps(target[0]):

            # For each libdep get its syslibdeps, and then check to see if we can
            # add it to the deps list. For static build we will also include private
            # syslibdeps to be transitive. For a dynamic build we will only make
            # public libdeps transitive.
            syslibs = []
            if not shared:
                syslibs += lib.get_env().get(Constants.SysLibdepsPrivate) or []
            syslibs += lib.get_env().get(Constants.SysLibdeps) or []

            # Validate the libdeps, a configure check has already checked what
            # syslibdeps are available so we can hard fail here if a syslibdep
            # is being attempted to be linked with.
            for syslib in syslibs:
                if not syslib:
                    continue

                if isinstance(syslib, str) and syslib.startswith(Constants.MissingLibdep):
                    raise MissingSyslibdepError(
                        textwrap.dedent(f"""\
                        LibdepsError:
                            Target '{str(target[0])}' depends on the availability of a
                            system provided library for '{syslib[len(Constants.MissingLibdep):]}',
                            but no suitable library was found during configuration."""))

                deps.append(syslib)

        setattr(target[0].attributes, Constants.SysLibdepsCached, deps)
    return stringify_deps(env, deps)


def _append_direct_libdeps(node, prereq_nodes):
    # We do not bother to decorate nodes that are not actual Objects
    if type(node) == str:
        return
    if getattr(node.attributes, "libdeps_direct", None) is None:
        node.attributes.libdeps_direct = []
    node.attributes.libdeps_direct.extend(prereq_nodes)


def _get_libdeps_with_link_flags(source, target, env, for_signature):
    for lib in get_libdeps(source, target, env, for_signature):
        # Make sure lib is a Node so we can get the env to check for flags.
        libnode = lib
        if not isinstance(lib, (str, SCons.Node.FS.File, SCons.Node.FS.Entry)):
            libnode = env.File(lib)

        # Virtual libdeps don't appear on the link line
        if 'virtual-libdep' in libnode.get_env().get('LIBDEPS_TAGS', []):
            continue

        # Create a libdep and parse the prefix and postfix (and separators if any)
        # flags from the environment.
        cur_lib = FlaggedLibdep(libnode, env)
        yield cur_lib


def _get_node_with_ixes(env, node, node_builder_type):
    """
    Gets the node passed in node with the correct ixes applied
    for the given builder type.
    """

    if not node:
        return node

    node_builder = env["BUILDERS"][node_builder_type]
    node_factory = node_builder.target_factory or env.File

    # Cache the 'ixes' in a function scope global so we don't need
    # to run SCons performance intensive 'subst' each time
    cache_key = (id(env), node_builder_type)
    try:
        prefix, suffix = _get_node_with_ixes.node_type_ixes[cache_key]
    except KeyError:
        prefix = node_builder.get_prefix(env)
        suffix = node_builder.get_suffix(env)

        # TODO(SERVER-50681): Find a way to do this that doesn't hard
        # code these extensions. See the code review for SERVER-27507
        # for additional discussion.
        if suffix == ".dll":
            suffix = ".lib"

        _get_node_with_ixes.node_type_ixes[cache_key] = (prefix, suffix)

    node_with_ixes = SCons.Util.adjustixes(node, prefix, suffix)
    return node_factory(node_with_ixes)


_get_node_with_ixes.node_type_ixes = dict()


def add_node_from(env, node):

    env.GetLibdepsGraph().add_nodes_from([(
        str(node.abspath),
        {
            NodeProps.bin_type.name: node.builder.get_name(env),
        },
    )])


def add_edge_from(env, from_node, to_node, visibility, direct):

    env.GetLibdepsGraph().add_edges_from([(
        from_node,
        to_node,
        {
            EdgeProps.direct.name: direct,
            EdgeProps.visibility.name: int(visibility),
        },
    )])


def add_libdeps_node(env, target, libdeps):

    if str(target).endswith(env["SHLIBSUFFIX"]):
        node = _get_node_with_ixes(env, str(target.abspath), target.get_builder().get_name(env))
        add_node_from(env, node)

        for libdep in libdeps:
            if str(libdep.target_node).endswith(env["SHLIBSUFFIX"]):
                add_edge_from(
                    env,
                    str(node.abspath),
                    str(libdep.target_node.abspath),
                    visibility=libdep.dependency_type,
                    direct=True,
                )


def get_libdeps_nodes(env, target, builder, debug=False, visibility_map=None):
    if visibility_map is None:
        visibility_map = dependency_visibility_ignored

    if not SCons.Util.is_List(target):
        target = [target]

    # Get the current list of nodes not to inherit on each target
    no_inherit = set(env.get(Constants.LibdepsNoInherit, []))

    # Get all the libdeps from the env so we can
    # can append them to the current target_node.
    libdeps = []
    for dep_type in sorted(visibility_map.keys()):

        if dep_type == deptype.Global:
            if any("conftest" in str(t) for t in target):
                # Ignore global dependencies for conftests
                continue

        # Libraries may not be stored as a list in the env,
        # so we must convert single library strings to a list.
        libs = env.get(dep_type_to_env_var[dep_type], []).copy()
        if not SCons.Util.is_List(libs):
            libs = [libs]

        for lib in libs:
            if not lib:
                continue

            lib_with_ixes = _get_node_with_ixes(env, lib, builder)

            if lib in no_inherit:
                if debug and not any("conftest" in str(t) for t in target):
                    print(f"     {dep_type[1]} =/> {lib}")

            else:
                if debug and not any("conftest" in str(t) for t in target):
                    print(f"     {dep_type[1]} => {lib}")

                libdeps.append(dependency(lib_with_ixes, dep_type, lib))

    return libdeps


def libdeps_emitter(target, source, env, debug=False, builder=None, visibility_map=None,
                    ignore_progdeps=False):
    """SCons emitter that takes values from the LIBDEPS environment variable and
    converts them to File node objects, binding correct path information into
    those File objects.

    Emitters run on a particular "target" node during the initial execution of
    the SConscript file, rather than during the later build phase.  When they
    run, the "env" environment's working directory information is what you
    expect it to be -- that is, the working directory is considered to be the
    one that contains the SConscript file.  This allows specification of
    relative paths to LIBDEPS elements.

    This emitter also adds LIBSUFFIX and LIBPREFIX appropriately.

    NOTE: For purposes of LIBDEPS_DEPENDENTS propagation, only the first member
    of the "target" list is made a prerequisite of the elements of LIBDEPS_DEPENDENTS.
    """

    if visibility_map is None:
        visibility_map = dependency_visibility_ignored

    if debug and not any("conftest" in str(t) for t in target):
        print(f"LIBDEPS EMITTER: {str(target[0])}")
        print(f"  Declared dependencies:")
        print(f"    global: {env.get(Constants.LibdepsGlobal, None)}")
        print(f"    private: {env.get(Constants.LibdepsPrivate, None)}")
        print(f"    public: {env.get(Constants.Libdeps, None)}")
        print(f"    interface: {env.get(Constants.LibdepsInterface, None)}")
        print(f"    no_inherit: {env.get(Constants.LibdepsNoInherit, None)}")
        print(f"  Edges:")

    libdeps = get_libdeps_nodes(env, target, builder, debug, visibility_map)

    if debug and not any("conftest" in str(t) for t in target):
        print(f"\n")

    # Lint the libdeps to make sure they are following the rules.
    # This will skip some or all of the checks depending on the options
    # and LIBDEPS_TAGS used.
    if not any("conftest" in str(t) for t in target):
        LibdepLinter(env, target).lint_libdeps(libdeps)

    if env.get('SYMBOLDEPSSUFFIX', None):
        for t in target:
            add_libdeps_node(env, t, libdeps)

    # We ignored the visibility_map until now because we needed to use
    # original dependency value for linting. Now go back through and
    # use the map to convert to the desired dependencies, for example
    # all Public in the static linking case.
    for libdep in libdeps:
        libdep.dependency_type = visibility_map[libdep.dependency_type]

    for t in target:
        # target[0] must be a Node and not a string, or else libdeps will fail to
        # work properly.
        _append_direct_libdeps(t, libdeps)

    for dependent in env.get(Constants.LibdepsDependents, []):
        if dependent is None:
            continue

        visibility = deptype.Private
        if isinstance(dependent, tuple):
            visibility = dependent[1]
            dependent = dependent[0]

        dependentNode = _get_node_with_ixes(env, dependent, builder)
        _append_direct_libdeps(dependentNode,
                               [dependency(target[0], visibility_map[visibility], dependent)])

    if not ignore_progdeps:
        for dependent in env.get(Constants.ProgdepsDependents, []):
            if dependent is None:
                continue

            visibility = deptype.Public
            if isinstance(dependent, tuple):
                # TODO: Error here? Non-public PROGDEPS_DEPENDENTS probably are meaningless
                visibility = dependent[1]
                dependent = dependent[0]

            dependentNode = _get_node_with_ixes(env, dependent, "Program")
            _append_direct_libdeps(dependentNode,
                                   [dependency(target[0], visibility_map[visibility], dependent)])

    return target, source


def expand_libdeps_tags(source, target, env, for_signature):
    results = []
    for expansion in env.get(Constants.LibdepsTagExpansion, []):
        results.append(expansion(source, target, env, for_signature))
    return results


def expand_libdeps_for_link(source, target, env, for_signature):

    libdeps_with_flags = []

    # Used to make modifications to the previous libdep on the link line
    # if needed. An empty class here will make the switch_flag conditionals
    # below a bit cleaner.
    prev_libdep = None

    for flagged_libdep in _get_libdeps_with_link_flags(source, target, env, for_signature):

        # If there are no flags to process we can move on to the next lib.
        # start_index wont mater in the case because if there are no flags
        # on the previous lib, then we will never need to do the chopping
        # mechanism on the next iteration.
        if not flagged_libdep.prefix_flags and not flagged_libdep.postfix_flags:
            libdeps_with_flags.append(str(flagged_libdep))
            prev_libdep = flagged_libdep
            continue

        # This for loop will go through the previous results and remove the 'off'
        # flag as well as removing the new 'on' flag. For example, let libA and libB
        # both use on and off flags which would normally generate on the link line as:
        #   -Wl--on-flag libA.a -Wl--off-flag -Wl--on-flag libA.a -Wl--off-flag
        # This loop below will spot the cases were the flag was turned off and then
        # immediately turned back on
        for switch_flag in getattr(flagged_libdep.libnode.attributes, 'libdeps_switch_flags', []):
            if (prev_libdep and switch_flag['on'] in flagged_libdep.prefix_flags
                    and switch_flag['off'] in prev_libdep.postfix_flags):

                flagged_libdep.prefix_flags.remove(switch_flag['on'])
                prev_libdep.postfix_flags.remove(switch_flag['off'])

                # prev_lib has had its list modified, and it has a start index
                # from the last iteration, so it will chop of the end the current
                # list and reapply the end with the new flags.
                prev_libdep.add_lib_to_result_list(libdeps_with_flags)

        # Store the information of the len of the current list before adding
        # the next set of flags as that will be the start index for the previous
        # lib next time around in case there are any switch flags to chop off.
        start_index = len(libdeps_with_flags)
        flagged_libdep.add_lib_to_result_list(libdeps_with_flags)

        # Done processing the current lib, so set it to previous for the next iteration.
        prev_libdep = flagged_libdep
        prev_libdep.start_index = start_index

    return libdeps_with_flags


def generate_libdeps_graph(env):
    if env.get('SYMBOLDEPSSUFFIX', None):

        find_symbols = env.Dir("$BUILD_DIR").path + "/libdeps/find_symbols"
        libdeps_graph = env.GetLibdepsGraph()

        symbol_deps = []
        for symbols_file, target_node in env.get('LIBDEPS_SYMBOL_DEP_FILES', []):

            direct_libdeps = []
            for direct_libdep in _get_sorted_direct_libdeps(target_node):
                add_node_from(env, direct_libdep.target_node)
                add_edge_from(
                    env,
                    str(target_node.abspath),
                    str(direct_libdep.target_node.abspath),
                    visibility=int(direct_libdep.dependency_type),
                    direct=True,
                )
                direct_libdeps.append(direct_libdep.target_node.abspath)

            for libdep in _get_libdeps(target_node):
                if libdep.abspath not in direct_libdeps:
                    add_node_from(env, libdep)
                    add_edge_from(
                        env,
                        str(target_node.abspath),
                        str(libdep.abspath),
                        visibility=int(deptype.Public),
                        direct=False,
                    )
            if env['PLATFORM'] == 'darwin':
                sep = ' '
            else:
                sep = ':'
            ld_path = sep.join(
                [os.path.dirname(str(libdep)) for libdep in _get_libdeps(target_node)])
            symbol_deps.append(
                env.Command(
                    target=symbols_file,
                    source=target_node,
                    action=SCons.Action.Action(
                        f'{find_symbols} $SOURCE "{ld_path}" $TARGET',
                        "Generating $SOURCE symbol dependencies" if not env['VERBOSE'] else ""),
                ))

        def write_graph_hash(env, target, source):

            with open(target[0].path, 'w') as f:
                json_str = json.dumps(
                    networkx.readwrite.json_graph.node_link_data(env.GetLibdepsGraph()),
                    sort_keys=True).encode('utf-8')
                f.write(hashlib.sha256(json_str).hexdigest())

        graph_hash = env.Command(
            target="$BUILD_DIR/libdeps/graph_hash.sha256",
            source=symbol_deps,
            action=SCons.Action.FunctionAction(
                write_graph_hash,
                {"cmdstr": None},
            ),
        )
        env.Depends(
            graph_hash,
            [env.File("#SConstruct")] + glob.glob("**/SConscript", recursive=True) +
            [os.path.abspath(__file__),
             env.File('$BUILD_DIR/mongo/util/version_constants.h')],
        )

        graph_node = env.Command(
            target=env.get('LIBDEPS_GRAPH_FILE', None),
            source=symbol_deps,
            action=SCons.Action.FunctionAction(
                generate_graph,
                {"cmdstr": "Generating libdeps graph"},
            ),
        )

        env.Depends(graph_node, [graph_hash] + env.Glob("#buildscripts/libdeps/libdeps/*"))


def generate_graph(env, target, source):

    libdeps_graph = env.GetLibdepsGraph()

    demangled_symbols = {}
    for symbol_deps_file in source:

        with open(str(symbol_deps_file)) as f:
            symbols = {}
            try:
                for symbol, lib in json.load(f).items():
                    # ignore symbols from external libraries,
                    # they will just clutter the graph
                    if lib.startswith(env.Dir("$BUILD_DIR").path):
                        if lib not in symbols:
                            symbols[lib] = []
                        symbols[lib].append(symbol)
            except json.JSONDecodeError:
                env.FatalError(f"Failed processing json file: {str(symbol_deps_file)}")

            demangled_symbols[str(symbol_deps_file)] = symbols

    p1 = subprocess.Popen(['c++filt', '-n'], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                          stderr=subprocess.STDOUT)
    stdout, stderr = p1.communicate(json.dumps(demangled_symbols).encode('utf-8'))
    demangled_symbols = json.loads(stdout.decode("utf-8"))

    for deps_file in demangled_symbols:

        for libdep in demangled_symbols[deps_file]:

            from_node = os.path.abspath(str(deps_file)[:-len(env['SYMBOLDEPSSUFFIX'])])
            to_node = os.path.abspath(libdep).strip()
            libdeps_graph.add_edges_from([(
                from_node,
                to_node,
                {EdgeProps.symbols.name: "\n".join(demangled_symbols[deps_file][libdep])},
            )])
            node = env.File(str(deps_file)[:-len(env['SYMBOLDEPSSUFFIX'])])
            add_node_from(env, node)

    libdeps_graph_file = f"{env.Dir('$BUILD_DIR').path}/libdeps/libdeps.graphml"
    networkx.write_graphml(libdeps_graph, libdeps_graph_file, named_key_ids=True)
    with fileinput.FileInput(libdeps_graph_file, inplace=True) as file:
        for line in file:
            print(line.replace(str(env.Dir("$BUILD_DIR").abspath + os.sep), ''), end='')


def setup_environment(env, emitting_shared=False, debug='off', linting='on'):
    """Set up the given build environment to do LIBDEPS tracking."""

    LibdepLinter.skip_linting = linting == 'off'
    LibdepLinter.print_linter_errors = linting == 'print'

    try:
        env["_LIBDEPS"]
    except KeyError:
        env["_LIBDEPS"] = "$_LIBDEPS_LIBS"

    env["_LIBDEPS_TAGS"] = expand_libdeps_tags
    env["_LIBDEPS_GET_LIBS"] = partial(get_libdeps, debug=debug)
    env["_LIBDEPS_OBJS"] = partial(get_libdeps_objs, debug=debug)
    env["_SYSLIBDEPS"] = partial(get_syslibdeps, debug=debug, shared=emitting_shared)

    env[Constants.Libdeps] = SCons.Util.CLVar()
    env[Constants.SysLibdeps] = SCons.Util.CLVar()

    # Create the alias for graph generation, the existence of this alias
    # on the command line will cause the libdeps-graph generation to be
    # configured.
    env['LIBDEPS_GRAPH_ALIAS'] = env.Alias(
        'generate-libdeps-graph',
        "${BUILD_DIR}/libdeps/libdeps.graphml",
    )[0]

    if str(env['LIBDEPS_GRAPH_ALIAS']) in COMMAND_LINE_TARGETS:

        # Detect if the current system has the tools to perform the generation.
        if env.GetOption('ninja') != 'disabled':
            env.FatalError("Libdeps graph generation is not supported with ninja builds.")
        if not emitting_shared:
            env.FatalError("Libdeps graph generation currently only supports dynamic builds.")

        if env['PLATFORM'] == 'darwin':
            required_bins = ['awk', 'sed', 'otool', 'nm']
        else:
            required_bins = ['awk', 'grep', 'ldd', 'nm']
        for bin in required_bins:
            if not env.WhereIs(bin):
                env.FatalError(f"'{bin}' not found, Libdeps graph generation requires {bin}.")

        # The find_symbols binary is a small fast C binary which will extract the missing
        # symbols from the target library, and discover what linked libraries supply it. This
        # setups the binary to be built.
        find_symbols_env = env.Clone()
        find_symbols_env.VariantDir('${BUILD_DIR}/libdeps', 'buildscripts/libdeps', duplicate=0)
        find_symbols_node = find_symbols_env.Program(
            target='${BUILD_DIR}/libdeps/find_symbols',
            source=['${BUILD_DIR}/libdeps/find_symbols.c'],
            CFLAGS=['-O3'],
        )

        # Here we are setting up some functions which will return single instance of the
        # network graph and symbol deps list. We also setup some environment variables
        # which are used along side the functions.
        symbol_deps = []

        def append_symbol_deps(env, symbol_deps_file):
            env.Depends(env['LIBDEPS_GRAPH_FILE'], symbol_deps_file[0])
            symbol_deps.append(symbol_deps_file)

        env.AddMethod(append_symbol_deps, "AppendSymbolDeps")

        env['LIBDEPS_SYMBOL_DEP_FILES'] = symbol_deps
        env['LIBDEPS_GRAPH_FILE'] = env.File("${BUILD_DIR}/libdeps/libdeps.graphml")
        env['LIBDEPS_GRAPH_SCHEMA_VERSION'] = 4
        env["SYMBOLDEPSSUFFIX"] = '.symbol_deps'

        libdeps_graph = LibdepsGraph()
        libdeps_graph.graph['invocation'] = " ".join([env['ESCAPE'](str(sys.executable))] +
                                                     [env['ESCAPE'](arg) for arg in sys.argv])
        libdeps_graph.graph['git_hash'] = env['MONGO_GIT_HASH']
        libdeps_graph.graph['graph_schema_version'] = env['LIBDEPS_GRAPH_SCHEMA_VERSION']
        libdeps_graph.graph['build_dir'] = env.Dir('$BUILD_DIR').path
        libdeps_graph.graph['deptypes'] = json.dumps({
            key: value[0]
            for key, value in deptype.__members__.items() if isinstance(value, tuple)
        })

        def get_libdeps_graph(env):
            return libdeps_graph

        env.AddMethod(get_libdeps_graph, "GetLibdepsGraph")

        # Now we will setup an emitter, and an additional action for several
        # of the builder involved with dynamic builds.
        def libdeps_graph_emitter(target, source, env):
            if "conftest" not in str(target[0]):
                symbol_deps_file = env.File(str(target[0]) + env['SYMBOLDEPSSUFFIX'])
                env.Depends(symbol_deps_file, '${BUILD_DIR}/libdeps/find_symbols')
                env.AppendSymbolDeps((symbol_deps_file, target[0]))

            return target, source

        for builder_name in ("Program", "SharedLibrary", "LoadableModule"):
            builder = env['BUILDERS'][builder_name]
            base_emitter = builder.emitter
            new_emitter = SCons.Builder.ListEmitter([base_emitter, libdeps_graph_emitter])
            builder.emitter = new_emitter

    env.Append(
        LIBDEPS_LIBEMITTER=partial(
            libdeps_emitter,
            debug=debug,
            builder="StaticLibrary",
        ),
        LIBEMITTER=lambda target, source, env: env["LIBDEPS_LIBEMITTER"](target, source, env),
        LIBDEPS_SHAREMITTER=partial(
            libdeps_emitter,
            debug=debug,
            builder="SharedArchive",
            ignore_progdeps=True,
        ),
        SHAREMITTER=lambda target, source, env: env["LIBDEPS_SHAREMITTER"](target, source, env),
        LIBDEPS_SHLIBEMITTER=partial(
            libdeps_emitter,
            debug=debug,
            builder="SharedLibrary",
            visibility_map=dependency_visibility_honored,
        ),
        SHLIBEMITTER=lambda target, source, env: env["LIBDEPS_SHLIBEMITTER"](target, source, env),
        LIBDEPS_PROGEMITTER=partial(
            libdeps_emitter,
            debug=debug,
            builder="SharedLibrary" if emitting_shared else "StaticLibrary",
        ),
        PROGEMITTER=lambda target, source, env: env["LIBDEPS_PROGEMITTER"](target, source, env),
    )

    env["_LIBDEPS_LIBS_FOR_LINK"] = expand_libdeps_for_link

    env["_LIBDEPS_LIBS"] = ("$LINK_LIBGROUP_START "
                            "$_LIBDEPS_LIBS_FOR_LINK "
                            "$LINK_LIBGROUP_END ")

    env.Prepend(_LIBFLAGS="$_LIBDEPS_TAGS $_LIBDEPS $_SYSLIBDEPS ")
    for builder_name in ("Program", "SharedLibrary", "LoadableModule", "SharedArchive"):
        try:
            update_scanner(env, builder_name, debug=debug)
        except KeyError:
            pass


def setup_conftests(conf):
    def FindSysLibDep(context, name, libs, **kwargs):
        var = "LIBDEPS_" + name.upper() + "_SYSLIBDEP"
        kwargs["autoadd"] = False
        for lib in libs:
            result = context.sconf.CheckLib(lib, **kwargs)
            context.did_show_result = 1
            if result:
                context.env[var] = lib
                context.Result(result)
                return result
        context.env[var] = _missing_syslib(name)
        context.Result(result)
        return result

    conf.AddTest("FindSysLibDep", FindSysLibDep)
