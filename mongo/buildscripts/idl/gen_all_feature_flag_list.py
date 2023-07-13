# Copyright (C) 2020-present MongoDB, Inc.
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the Server Side Public License, version 1,
# as published by MongoDB, Inc.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# Server Side Public License for more details.
#
# You should have received a copy of the Server Side Public License
# along with this program. If not, see
# <http://www.mongodb.com/licensing/server-side-public-license>.
#
# As a special exception, the copyright holders give permission to link the
# code of portions of this program with the OpenSSL library under certain
# conditions as described in each individual source file and distribute
# linked combinations including the program with the OpenSSL library. You
# must comply with the Server Side Public License in all respects for
# all of the code used other than as permitted herein. If you modify file(s)
# with this exception, you may extend this exception to your version of the
# file(s), but you are not obligated to do so. If you do not wish to do so,
# delete this exception statement from your version. If you delete this
# exception statement from all source files in the program, then also delete
# it in the license file.
"""
Generate a file containing a list of disabled feature flags.

Used by resmoke.py to run only feature flag tests.
"""

import os
import sys

from typing import List

import yaml

# Permit imports from "buildscripts".
sys.path.append(os.path.normpath(os.path.join(os.path.abspath(__file__), '../../..')))

# pylint: disable=wrong-import-position
from buildscripts.idl import lib
from buildscripts.idl.idl import parser


def is_third_party_idl(idl_path: str) -> bool:
    """Check if an IDL file is under a third party directory."""
    third_party_idl_subpaths = [os.path.join("third_party", "mozjs"), "win32com"]

    for file_name in third_party_idl_subpaths:
        if file_name in idl_path:
            return True

    return False


def gen_all_feature_flags(idl_dirs: List[str] = None):
    """Generate a list of all feature flags."""
    default_idl_dirs = ["src", "buildscripts"]

    if not idl_dirs:
        idl_dirs = default_idl_dirs

    all_flags = []
    for idl_dir in idl_dirs:
        for idl_path in sorted(lib.list_idls(idl_dir)):
            if is_third_party_idl(idl_path):
                continue
            doc = parser.parse_file(open(idl_path), idl_path)
            for feature_flag in doc.spec.feature_flags:
                if feature_flag.default.literal != "true":
                    all_flags.append(feature_flag.name)

    force_disabled_flags = yaml.safe_load(
        open("buildscripts/resmokeconfig/fully_disabled_feature_flags.yml"))

    return list(set(all_flags) - set(force_disabled_flags))


def gen_all_feature_flags_file(filename: str = lib.ALL_FEATURE_FLAG_FILE):
    flags = gen_all_feature_flags()
    with open(filename, "w") as output_file:
        output_file.write("\n".join(flags))
        print("Generated: ", os.path.realpath(output_file.name))


def main():
    """Run the main function."""
    gen_all_feature_flags_file()


if __name__ == '__main__':
    main()
