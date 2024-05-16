#!/usr/bin/env python3
"""
Generate the compile expansions file used by Evergreen as part of the push/release process.

Invoke by specifying an output file.
$ python generate_compile_expansions.py --out compile_expansions.yml
"""

import argparse
import os
import sys
import shlex
import yaml

VERSION_JSON = "version.json"


def generate_expansions():
    """Entry point for the script.

    This calls functions to generate version and scons cache expansions and
    writes them to a file.
    """
    args = parse_args()
    expansions = {}
    expansions.update(generate_scons_cache_expansions())

    with open(args.out, "w") as out:
        print("saving compile expansions to {0}: ({1})".format(args.out, expansions))
        yaml.safe_dump(expansions, out, default_flow_style=False)


def parse_args():
    """Parse program arguments."""
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", required=True)
    return parser.parse_args()


def generate_scons_cache_expansions():
    """Generate scons cache expansions from some files and environment variables."""
    expansions = {}
    if sys.platform.startswith("win"):
        system_id_path = r"c:\mongodb-build-system-id"
        default_cache_path_base = r"z:\data\scons-cache"
    else:
        system_id_path = "/etc/mongodb-build-system-id"
        default_cache_path_base = "/data/scons-cache"

    if os.path.isfile(system_id_path):
        with open(system_id_path, "r") as fh:
            default_cache_path = os.path.join(default_cache_path_base, fh.readline().strip())

            expansions["scons_cache_path"] = default_cache_path

            scons_cache_mode = os.getenv("SCONS_CACHE_MODE")

            if scons_cache_mode in (None, ""):
                scons_cache_mode = "nolinked"

            if os.getenv("USE_SCONS_CACHE") not in (None, False, "false", ""):
                expansions["scons_cache_args"] = (
                    "--cache={0} --cache-signature-mode=validate --cache-dir={1} --cache-show".format(
                        scons_cache_mode, shlex.quote(default_cache_path)
                    )
                )
    return expansions


if __name__ == "__main__":
    generate_expansions()
