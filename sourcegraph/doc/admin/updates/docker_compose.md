# Updating a Docker Compose Sourcegraph instance

This document describes the exact changes needed to update a Docker Compose Sourcegraph instance.

Each section comprehensively describes the steps needed to upgrade, and any manual migration steps you must perform.

## v3.17.2 -> 3.18.0

No manual migration required.

Please upgrade to the [`v3.18.0` tag of deploy-sourcegraph-docker](https://github.com/sourcegraph/deploy-sourcegraph-docker/tree/3.18/docker-compose) by following the [standard upgrade procedure](#standard-upgrade-procedure).

## v3.16.0 -> v3.17.2

No manual migration is required. 

Please upgrade to the [`v3.17.2` tag of deploy-sourcegraph-docker](https://github.com/sourcegraph/deploy-sourcegraph-docker/tree/v3.16.0/docker-compose) by following the [standard upgrade procedure](#standard-upgrade-procedure).

## v3.15.1 -> v3.16.0

No manual migration is required. 

Please upgrade to the [`v3.16.0` tag of deploy-sourcegraph-docker](https://github.com/sourcegraph/deploy-sourcegraph-docker/tree/v3.16.0/docker-compose) by following the [standard upgrade procedure](#standard-upgrade-procedure).

## (v3.14.2, v3.14.4) -> v3.15.1

No manual migration is required. 

Please upgrade to the [`v3.15.1` tag of deploy-sourcegraph-docker](https://github.com/sourcegraph/deploy-sourcegraph-docker/tree/v3.15.1/docker-compose) by following the [standard upgrade procedure](#standard-upgrade-procedure).

### (Optional) Keeping LSIF data

If your users have uploaded LSIF precise code intelligence data, you may keep it by running the following command after you have ran `docker-compose up` with the new v3.15.1 version:

```
docker run --rm -it -v /var/lib/docker:/docker alpine:latest sh -c 'cp -R /docker/volumes/docker-compose_lsif-server/_data/* /docker/volumes/docker-compose_precise-code-intel-bundle-manager/_data/'
```

(No restart is required after running the above command.)

## v3.14.2 -> v3.14.4

No manual migration is required. 

Please upgrade to the [`v3.14.4` tag of deploy-sourcegraph-docker](https://github.com/sourcegraph/deploy-sourcegraph-docker/tree/v3.14.4/docker-compose) by following the [standard upgrade procedure](#standard-upgrade-procedure).

## v3.14.0 -> v3.14.2

No manual migration is required. 

Please upgrade to the [`v3.14.2` tag of deploy-sourcegraph-docker](https://github.com/sourcegraph/deploy-sourcegraph-docker/tree/v3.14.2/docker-compose) by following the [standard upgrade procedure](#standard-upgrade-procedure).

## v3.13 -> 3.14

No manual migration is required.

Please be sure to upgrade to the [`v3.14.0-1` tag of deploy-sourcegraph-docker](https://github.com/sourcegraph/deploy-sourcegraph-docker/tree/v3.14.0-1/docker-compose) by following the [standard upgrade procedure](#standard-upgrade-procedure).

If you have upgrade to `v3.14.0` already (not the `v3.14.0-1` version) and are experiencing restarts of lsif-server, please run the following on the host machine to correct it:

```sh
docker run --rm -it -v /var/lib/docker:/docker alpine:latest sh -c 'chown -R 100:101 /docker/volumes/docker-compose_lsif-server'
docker restart lsif-server
```

## v3.12 -> v3.13

A manual migration is required. Please follow the [standard upgrade procedure](#standard-upgrade-procedure) to take down the current deployment, perform the manual migration, and then upgrade using the [`v3.13.2` tag of deploy-sourcegraph-docker](https://github.com/sourcegraph/deploy-sourcegraph-docker/tree/v3.13.2/docker-compose).

### Manual migration step: adjust file permissions

Please adjust the redis-store and redis-cache volume permissions by running the following on the host machine:

```
docker run --rm -it -v /var/lib/docker:/docker alpine:latest sh -c 'chown -R 999:1000 /docker/volumes/docker-compose_redis-store /docker/volumes/docker-compose_redis-cache'
```

### Standard upgrade procedure

In your fork of [the deploy-sourcegraph-docker](https://github.com/sourcegraph/deploy-sourcegraph-docker) repository, merge the new version into the `release` branch if you maintain any changes (see: [storing customizations in a fork](../install/docker-compose.md#optional-recommended-store-customizations-in-a-fork)):

```sh
cd docker-compose/
git fetch upstream
git merge upstream $NEW_VERSION
# Address any merge conflicts you may have.
```

Then on your server:

```sh
cd deploy-sourcegraph-docker/docker-compose/
docker-compose down
git pull
docker-compose up -d
```
