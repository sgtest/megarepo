# Postgres

Sourcegraph uses Postgres as its main internal database. We support any version **starting from 9.6**.

## Upgrades

This section describes the possible Postgres upgrade procedures for the different Sourcegraph deployment types.

### Automatic (recommended)

A few different deployment types can automatically handle Postgres data upgrades for you.

#### `sourcegraph/server` automatic upgrades

Existing Postgres data will be automatically migrated when a release of the `sourcegraph/server` Docker image ships with a new version of Postgres.

For the upgrade to proceed, the Docker socket must be mounted the first time the new Docker image is ran.

This is needed to run the [Postgres upgrade containers](https://github.com/tianon/docker-postgres-upgrade) in the
Docker host. When the upgrade is done, the container can be restarted without mounting the Docker socket.

Here's the full invocation when using Docker:

```bash
# Add "--env=SRC_LOG_LEVEL=dbug" below for verbose logging.
docker run -p 7080:7080 -p 2633:2633 --rm \
  -v ~/.sourcegraph/config:/etc/sourcegraph \
  -v ~/.sourcegraph/data:/var/opt/sourcegraph \
  -v /var/run/docker.sock:/var/run/docker.sock:ro \
  sourcegraph/server:3.0.1
```

When using the `sourcegraph/server` image in other environments (e.g. Kubernetes), please refer to official documentation on how to mount the Docker socket for the upgrade procedure.

Alternatively, Postgres can be [upgraded manually](#manual).

#### Kubernetes with https://github.com/sourcegraph/deploy-sourcegraph

The upgrade process is fully automated. However, if you have customized the environment variables `PGUSER`, `PGDATABASE` or `PGDATA` then you are required to specify the corresponding `PG*OLD` and `PG*NEW` environment variables. Below are the defaults as reference:

``` shell
# PGUSEROLD: A user that exists in the old database that can be used
#            to authenticate intermediate upgrade operations.
# PGUSERNEW: A user that must exist in the new database (upgraded or freshly created).
#
# PGDATABASEOLD: A database that exists in the old database that can be used
#                to authenticate intermediate upgrade operations. (e.g `psql -d`)
# PGDATABASENEW: A database that must exist in the new database (upgraded or freshly created).
#
# PGDATAOLD: The data directory containing the files of the old Postgres database to be upgraded.
# PGDATANEW: The data directory containing the upgraded Postgres data files, used by the new version of Postgres
PGUSEROLD=sg
PGUSERNEW=sg
PGDATABASEOLD=sg
PGDATABASENEW=sg
PGDATAOLD=/data/pgdata
PGDATANEW=/data/pgdata-11
```

Additionally the upgrade process assumes it can write to the parent directory of `PGDATAOLD`.

### Manual

These instructions can be followed when manual Postgres upgrades are preferred.

#### `sourcegraph/server` manual upgrades

Assuming Postgres data must be upgraded from `9.6` to `11` and your Sourcegraph directory is at `$HOME/.sourcegraph`, here is how it would be done:

```bash
#!/usr/bin/env bash

set -xeuo pipefail

export OLD=${OLD:-"9.6"}
export NEW=${NEW:-"11"}
export SRC_DIR=${SRC_DIR:-"$HOME/.sourcegraph"}

docker run \
  -w /tmp/upgrade \
  -v "$SRC_DIR/data/postgres-$NEW-upgrade:/tmp/upgrade" \
  -v "$SRC_DIR/data/postgresql:/var/lib/postgresql/$OLD/data" \
  -v "$SRC_DIR/data/postgresql-$NEW:/var/lib/postgresql/$NEW/data" \
  "tianon/postgres-upgrade:$OLD-to-$NEW"

mv "$SRC_DIR/data/"{postgresql,postgresql-$OLD}
mv "$SRC_DIR/data/"{postgresql-$NEW,postgresql}

curl -fsSL -o "$SRC_DIR/data/postgres-$NEW-upgrade/optimize.sh" https://raw.githubusercontent.com/sourcegraph/sourcegraph/master/cmd/server/rootfs/postgres-optimize.sh

docker run \
  --entrypoint "/bin/bash" \
  -w /tmp/upgrade \
  -v "$SRC_DIR/data/postgres-$NEW-upgrade:/tmp/upgrade" \
  -v "$SRC_DIR/data/postgresql:/var/lib/postgresql/data" \
  "postgres:$NEW" \
  -c 'chown -R postgres $PGDATA && gosu postgres bash ./optimize.sh $PGDATA'
```

#### Other setups

##### External Postgres

When running an external Postgres instance please refer to the documentation of your provider on how to perform upgrade procedures.

- [AWS RDS](https://docs.aws.amazon.com/AmazonRDS/latest/UserGuide/USER_UpgradeDBInstance.PostgreSQL.html)
- [AWS Aurora](https://docs.aws.amazon.com/AmazonRDS/latest/AuroraUserGuide/USER_UpgradeDBInstance.Upgrading.html)
- [GCP CloudSQL](https://cloud.google.com/sql/docs/postgres/db-versions)
- [Azure DB](https://docs.microsoft.com/en-us/azure/postgresql/concepts-supported-versions#managing-updates-and-upgrades)
- [Heroku](https://devcenter.heroku.com/articles/upgrading-heroku-postgres-databases)
- [EnterpriseDB](https://www.enterprisedb.com/docs/en/9.6/pg/upgrading.html)
- [Citus](http://docs.citusdata.com/en/v8.1/admin_guide/upgrading_citus.html)
- [Aiven Postgres](https://help.aiven.io/postgresql/operations/how-to-perform-a-postgresql-in-place-major-version-upgrade)
- [Your own Postgres](https://www.postgresql.org/docs/11/pgupgrade.html)

