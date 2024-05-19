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

import SCons


def _update_builder(env, builder):
    old_scanner = builder.target_scanner
    old_path_function = old_scanner.path_function

    def new_scanner(node, env, path=()):
        results = old_scanner.function(node, env, path)
        origin = getattr(node.attributes, "debug_file_for", None)
        if origin is not None:
            origin_results = old_scanner(origin, env, path)
            for origin_result in origin_results:
                origin_result_debug_files = getattr(
                    origin_result.attributes, "separate_debug_files", None
                )
                if origin_result_debug_files is not None:
                    results.extend(origin_result_debug_files)
        return results

    builder.target_scanner = SCons.Scanner.Scanner(
        function=new_scanner,
        path_function=old_path_function,
    )

    base_action = builder.action
    if not isinstance(base_action, SCons.Action.ListAction):
        base_action = SCons.Action.ListAction([base_action])

    # TODO: Make variables for dsymutil and strip, and for the action
    # strings. We should really be running these tools as found by
    # xcrun by default. We should achieve that by upgrading the
    # site_scons/site_tools/xcode.py tool to search for these for
    # us. We could then also remove a lot of the compiler and sysroot
    # setup from the etc/scons/xcode_*.vars files, which would be a
    # win as well.
    if env.TargetOSIs("darwin"):
        base_action.list.extend(
            [
                SCons.Action.Action(
                    "$DSYMUTIL -num-threads 1 $TARGET -o ${TARGET}$SEPDBG_SUFFIX",
                    "$DSYMUTILCOMSTR",
                ),
                SCons.Action.Action(
                    "$STRIP -S ${TARGET}",
                    "$DEBUGSTRIPCOMSTR",
                ),
            ]
        )
    elif env.TargetOSIs("posix"):
        base_action.list.extend(
            [
                SCons.Action.Action(
                    "$OBJCOPY --only-keep-debug $TARGET ${TARGET}$SEPDBG_SUFFIX",
                    "$OBJCOPY_ONLY_KEEP_DEBUG_COMSTR",
                ),
                SCons.Action.Action(
                    "$OBJCOPY --strip-debug --add-gnu-debuglink ${TARGET}$SEPDBG_SUFFIX ${TARGET}",
                    "$DEBUGSTRIPCOMSTR",
                ),
            ]
        )
    else:
        pass

    builder.action = base_action

    base_emitter = builder.emitter

    def new_emitter(target, source, env):
        debug_files = []
        if env.TargetOSIs("darwin"):
            # There isn't a lot of great documentation about the structure of dSYM bundles.
            # For general bundles, see:
            #
            # https://developer.apple.com/library/archive/documentation/CoreFoundation/Conceptual/CFBundles/BundleTypes/BundleTypes.html
            #
            # But we expect to find two files in the bundle. An
            # Info.plist file under Contents, and a file with the same
            # name as the target under Contents/Resources/DWARF.

            target0 = env.File(target[0])
            dsym_dir_name = str(target[0]) + ".dSYM"
            dsym_dir = env.Dir(dsym_dir_name, directory=target0.get_dir())

            plist_file = env.File("Contents/Info.plist", directory=dsym_dir)
            setattr(plist_file.attributes, "aib_effective_suffix", ".dSYM")
            setattr(
                plist_file.attributes,
                "aib_additional_directory",
                "{}/Contents".format(dsym_dir_name),
            )

            dwarf_dir = env.Dir("Contents/Resources/DWARF", directory=dsym_dir)

            dwarf_file = env.File(target0.name, directory=dwarf_dir)
            setattr(dwarf_file.attributes, "aib_effective_suffix", ".dSYM")
            setattr(
                dwarf_file.attributes,
                "aib_additional_directory",
                "{}/Contents/Resources/DWARF".format(dsym_dir_name),
            )

            debug_files.extend([plist_file, dwarf_file])

        elif env.TargetOSIs("posix"):
            debug_file = env.File(f"{target[0]}$SEPDBG_SUFFIX")
            debug_files.append(debug_file)
        elif env.TargetOSIs("windows"):
            debug_file = env.File(env.subst("${PDB}", target=target))
            debug_files.append(debug_file)
        else:
            pass

        # Establish bidirectional linkages between the target and each debug file by setting
        # attributes on th nodes. We use these in the scanner above to ensure that transitive
        # dependencies among libraries are projected into transitive dependencies between
        # debug files.
        for debug_file in debug_files:
            setattr(debug_file.attributes, "debug_file_for", target[0])
        setattr(target[0].attributes, "separate_debug_files", debug_files)

        # On Windows, we don't want to emit the PDB. The mslink.py
        # SCons tool is already doing so, so otherwise it would be
        # emitted twice. This tool only needed to adorn the node with
        # the various attributes as needed, so we just return the
        # original target, source tuple.
        if env.TargetOSIs("windows"):
            return target, source

        return (target + debug_files, source)

    new_emitter = SCons.Builder.ListEmitter([base_emitter, new_emitter])
    builder.emitter = new_emitter


def generate(env):
    if not exists(env):
        return

    if env.TargetOSIs("darwin"):
        env["SEPDBG_SUFFIX"] = ".dSYM"

        if env.get("DSYMUTIL", None) is None:
            env["DSYMUTIL"] = env.WhereIs("dsymutil")

        if env.get("STRIP", None) is None:
            env["STRIP"] = env.WhereIs("strip")

        if not env.Verbose():
            env.Append(
                DSYMUTILCOMSTR="Generating debug info for $TARGET into ${TARGET}.dSYM",
                DEBUGSTRIPCOMSTR="Stripping debug info from ${TARGET}",
            )

    elif env.TargetOSIs("posix"):
        env["SEPDBG_SUFFIX"] = env.get("SEPDBG_SUFFIX", ".debug")
        if env.get("OBJCOPY", None) is None:
            env["OBJCOPY"] = env.Whereis("objcopy")

        if not env.Verbose():
            env.Append(
                OBJCOPY_ONLY_KEEP_DEBUG_COMSTR="Generating debug info for $TARGET into ${TARGET}${SEPDBG_SUFFIX}",
                DEBUGSTRIPCOMSTR="Stripping debug info from ${TARGET} and adding .gnu.debuglink to ${TARGET}${SEPDBG_SUFFIX}",
            )

    for builder in ["Program", "SharedLibrary", "LoadableModule"]:
        _update_builder(env, env["BUILDERS"][builder])


def exists(env):
    if env.TargetOSIs("darwin"):
        if env.get("DSYMUTIL", None) is None and env.WhereIs("dsymutil") is None:
            return False
        if env.get("STRIP", None) is None and env.WhereIs("strip") is None:
            return False
    elif env.TargetOSIs("posix"):
        if env.get("OBJCOPY", None) is None and env.WhereIs("objcopy") is None:
            return False
    return True
