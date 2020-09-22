# Migrate from the single Docker image to Docker Compose

Since Sourcegraph 3.13, deploying via Docker Compose is the recommended method for production deployments as it provides resource isolation between Sourcegraph services which makes it more scalable and stable. This page describes how to migrate from a single Docker image deployment to the Docker Compose deployment method.

Sourcegraph's core data (including user accounts, configuration, repository-metadata, etc.), can be migrated from the single Docker image (`sourcegraph/server`) to the Docker Compose deployment by dumping and restoring the Postgres database.

## Notes before you begin

### Version requirements

* This migration can only be done with Sourcegraph v3.13.1+. If you are not currently on at least this version, please upgrade first.
* Do NOT attempt to upgrade at the same time as migrating to docker-compose. Use the docker-compose version corresponding to your current version. For example, if you are running `sourcegraph/server:3.19.2` you must follow this guide using the Docker Compose deployment version `v3.19.2`.

### Storage location change

After migration, Sourcegraph's data will be stored in Docker volumes instead of `~/.sourcegraph/`. For more information, see the cloud-provider documentation referred to in ["Create the new Docker Compose instance"](#create-the-new-docker-compose-instance).

### Only core data will be migrated

The migration will bring over core data including user accounts, configuration, repository-metadata, etc. Other data will be regenerated automatically:

* Repositories will be re-cloned
* Search indexes will be rebuilt from scratch

The above may take awhile if you have a lot of repositories. In the meantime, searches may be slow or return incomplete results. Usually this process will not take longer than 6 hours.

### Monthly-usage based pricing

If you are on a monthly-based usage pricing model, please check first with your Sourcegraph point of contact before continuing with these migration steps.

## Migration guide

### Backup single Docker image database

#### Find single Docker image's `CONTAINER_ID`

* `ssh` from your local machine into the instance hosting the `sourcegraph/server` container
* Run the following command to find the `sourcegraph/server`'s `CONTAINER_ID`:
  
```bash
> docker ps
CONTAINER ID        IMAGE
...                 sourcegraph/server
```

#### Generate database dump

* Dump Postgres database to `/tmp/db.out`

```bash
# Use the CONTAINER_ID found in the previous step
docker exec -it "$CONTAINER_ID" sh -c 'pg_dumpall --verbose --username=postgres' > /tmp/db.out
```

* Copy Postgres dump from the `sourcegraph/server` container to the host machine

```bash
docker cp "$CONTAINER_ID":/tmp/db.out /tmp/db.out
```

#### Copy database dump to your local machine

* End your `ssh` session with the `sourcegraph/server` host machine

* Copy the Postgres dump from the `sourcegraph/server` host to your local machine:

```bash
# Modify this command with your authentication information
scp example_user@example_docker_host.com:/tmp/db.out db.out
```

* Run `less "/tmp/db.out"` and verify that the database dump has contents that you expect (e.g. that some of your repository names appear)

### Create the new Docker Compose instance

Follow your cloud provider's installation guide to create the new Docker Compose instance:

* [Install Sourcegraph with Docker Compose on AWS](../../install/docker-compose/aws.md)
* [Install Sourcegraph with Docker Compose on Google Cloud](../../install/docker-compose/google_cloud.md)
* [Install Sourcegraph with Docker Compose on DigitalOcean](../../install/docker-compose/digitalocean.md)

Once you have finished the above, come back here for directions on how to copy over the database from your old `sourcegraph/server` instance.

### Restore database backup to the Docker Compose instance

#### Prepare the Postgres instance

* `ssh` from your local machine into the new instance running the Docker Compose deployment

* Navigate to the directory containing the Docker Compose definition:

```bash
# Refer to the script in your cloud provider's installation guide
# to find the value for "DEPLOY_SOURCEGRAPH_DOCKER_CHECKOUT"

cd "$DEPLOY_SOURCEGRAPH_DOCKER_CHECKOUT"/docker-compose
```

* Tear down the existing Docker Compose containers (and associated volumes) so that we avoid conflicting transactions while modifying the database

```bash
 docker-compose down --volumes
```

* Start the Postgres instance on its own

```bash
docker-compose -f pgsql-only-migrate.docker-compose.yaml up -d
```

* End your `ssh` session with the new Docker Compose deployment host

#### Apply database dump to Postgres instance

* Copy the Postgres dump from your local machine to the Docker Compose host:

```bash
# Modify this command with your authentication information
scp db.out example_user@example_docker_compose_host.com:/tmp/db.out
```

* `ssh` from your local machine into the Docker Compose deployment host

* Copy database dump from the Docker Compose host to the Postgres container

```bash
docker cp /tmp/db.out pgsql:/tmp/db.out
```

* Create a shell session inside the Postgres container

```bash
docker exec -it pgsql /bin/sh
```

* Restore the database dump

```bash
psql --username=sg -f /tmp/db.out postgres
```

* Open up a psql session inside the Postgres container

```bash
psql --username=sg postgres
```

* Apply the following tweaks to transform the single Docker image's database schema into Docker Compose's

```postgres
DROP DATABASE sg;
ALTER DATABASE sourcegraph RENAME TO sg;
ALTER DATABASE sg OWNER TO sg;
```

* End your `psql` session

```bash
\q
```

* End your Postgres container shell session

```bash
exit
```

#### Start the rest of the Sourcegraph containers

```bash
docker-compose -f docker-compose.yaml up -d
```

## Conclusion

The migration process is now complete.

You should be able to log into your instance and verify that previous users and configuration are still present. Repositories may take awhile to clone and index, but their names should be immediately visible in the site admin repositories list. Wait for repositories to clone and verify the new Sourcegraph instance works as expected.

After verifying the new instance is functional, you can tear down the old `sourcegraph/server` single Docker container Sourcegraph instance.
