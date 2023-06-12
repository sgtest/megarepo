"""
This module returns a Drone step and pipeline for linting with shellcheck.
"""

load("scripts/drone/steps/lib.star", "compile_build_cmd")
load(
    "scripts/drone/utils/utils.star",
    "pipeline",
)
load(
    "scripts/drone/utils/images.star",
    "images",
)

trigger = {
    "event": [
        "pull_request",
    ],
    "paths": {
        "exclude": [
            "*.md",
            "docs/**",
            "latest.json",
        ],
        "include": ["scripts/**/*.sh"],
    },
}

def shellcheck_step():
    return {
        "name": "shellcheck",
        "image": images["build_image"],
        "depends_on": [
            "compile-build-cmd",
        ],
        "commands": [
            "./bin/build shellcheck",
        ],
    }

def shellcheck_pipeline():
    environment = {"EDITION": "oss"}
    steps = [
        compile_build_cmd(),
        shellcheck_step(),
    ]
    return pipeline(
        name = "pr-shellcheck",
        edition = "oss",
        trigger = trigger,
        services = [],
        steps = steps,
        environment = environment,
    )
