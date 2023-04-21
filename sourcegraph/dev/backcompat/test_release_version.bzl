"""Defines the minimum ugradeable version of Sourcegraph.

This designates the mininum version from which we guarantees the newest database
schema can run.

See https://docs.sourcegraph.com/dev/background-information/sql/migrations
"""

# Defines which version we target with the backward compatibility tests.
MINIMUM_UPGRADEABLE_VERSION = "5.0.0"

# Defines a reproducible reference to clone Sourcegraph at to run those tests.
MINIMUM_UPGRADEABLE_VERSION_REF = "196e8d2884a8c20a4c4a22e2c03faff08a329a30"
