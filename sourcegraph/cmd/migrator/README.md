# Migrator

The migrator service is deployed ahead of a Sourcegraph version upgrade to synchronously run database migrations required by the next version. Successful exit of the migrator denotes that the new version can be deployed. Database migrations are written to be backwards-compatible so that running the migrator for the next upgrade does not cause issues with a working instance.

At this point in time, this service no-ops on invocation and is meant to be a development placeholder while we modify CI pipelines and upgrade instructions.
