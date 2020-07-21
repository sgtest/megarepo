# Install Sourcegraph with Docker Compose

| Deployment Type                                          | Suggested for                                       | Setup time | Multi-machine? | Auto healing? | Monitoring? |
| -------------------------------------------------------- | --------------------------------------------------- | ---------- | -------------- | ------------- | ----------- |
| [Single-container server](../../install/docker/index.md) | Local testing                                       | 60 seconds | Impossible     | No            | No          |
| [Docker Compose](../../install/docker-compose/index.md)  | Small & medium production deployments               | 5 minutes  | Possible       | No            | Yes         |
| [Kubernetes](../../install/cluster.md)                   | Medium & large highly-available cluster deployments | 30 minutes | Easily         | Yes           | Yes         |

* If you're just starting out, we recommend [running Sourcegraph locally](../docker/index.md). It takes only a few minutes and lets you try out all of the features.
* If you want to migrate from the single-container server (`sourcegraph/server`) to the Docker Compose deployment, refer to this [migration guide](./migrate.md).
* If you need scalability and high-availability beyond what a single-node [Docker Compose](https://docs.docker.com/compose/) can offer, use the [Kubernetes cluster deployment option](https://github.com/sourcegraph/deploy-sourcegraph), instead.

---

It takes less than 5 minutes to run and install Sourcegraph using Docker Compose:

```bash
git clone git@github.com:sourcegraph/deploy-sourcegraph-docker.git
cd deploy-sourcegraph-docker/docker-compose
git checkout v3.18.0
docker-compose up -d
```

Once the server is ready (the `sourcegraph-frontend-0` service is healthy when running `docker ps`), navigate to the hostname or IP address on port `80`.  Create the admin account, then you'll be guided through setting up Sourcegraph for code searching and navigation.

For next steps and further configuration options, visit the [site administration documentation](../../index.md).

> NOTE: If you get stuck or need help, [file an issue](https://github.com/sourcegraph/sourcegraph/issues/new?&title=Improve+Sourcegraph+quickstart+guide), [tweet (@srcgraph)](https://twitter.com/srcgraph) or [email](mailto:support@sourcegraph.com?subject=Sourcegraph%20quickstart%20guide).

## (optional, recommended) Store customizations in a fork

We **strongly** recommend that you create your own fork of [sourcegraph/deploy-sourcegraph-docker](https://github.com/sourcegraph/deploy-sourcegraph-docker/) to track customizations to the [Sourcegraph Docker Compose yaml](https://github.com/sourcegraph/deploy-sourcegraph-docker/blob/master/docker-compose/docker-compose.yaml). This will make upgrades far easier.

* Fork [sourcegraph/deploy-sourcegraph-docker](https://github.com/sourcegraph/deploy-sourcegraph-docker/)
  * The fork can be public **unless** you plan to store secrets (SSL certificates, external Postgres credentials, etc.) in the repository itself.

* Create a `release` branch (to track all of your customizations to Sourcegraph. When you upgrade Sourcegraph's Docker Compose definition, you will merge upstream into this branch.

```bash
SOURCEGRAPH_VERSION="v3.18.0"
git checkout $SOURCEGRAPH_VERSION -b release
```

* Commit customizations to the [Sourcegraph Docker Compose yaml](https://github.com/sourcegraph/deploy-sourcegraph-docker/blob/master/docker-compose/docker-compose.yaml) to your `release` branch

## Storage

The [Sourcegraph Docker Compose definition](https://github.com/sourcegraph/deploy-sourcegraph-docker/blob/master/docker-compose/docker-compose.yaml) uses [Docker volumes](https://docs.docker.com/storage/volumes/) to store its data. These volumes are stored at `/var/lib/docker/volumes` by [default on Linux](https://docs.docker.com/storage/#choose-the-right-type-of-mount). 

## Resource estimator

Use the [resource estimator](../resource_estimator.md) to find a good starting point for your deployment.

## Cloud installation guides

Cloud specific Sourcegraph installation guides for AWS, Google Cloud and Digital Ocean.

- [Install Sourcegraph with Docker Compose on AWS](../../install/docker-compose/aws.md)
- [Install Sourcegraph with Docker Compose on Google Cloud](../../install/docker-compose/google_cloud.md)
- [Install Sourcegraph with Docker Compose on DigitalOcean](../../install/docker-compose/digitalocean.md)

## Insiders build

To test new development builds of Sourcegraph (triggered by commits to master), change all `index.docker.io/sourcegraph/*` Docker image semver tags in [docker-compose.yaml](https://github.com/sourcegraph/deploy-sourcegraph-docker/blob/master/docker-compose/docker-compose.yaml) to `insiders` (e.g., `index.docker.io/sourcegraph/frontend:1.2.3` to `index.docker.io/sourcegraph/frontend:insiders`).

> WARNING: `insiders` builds may be unstable, so back up Sourcegraph's data and config beforehand.

To keep this up to date, run `docker-compose pull` to pull in the latest images, and run `docker-compose restart` to restart all container to access new changes.

## Next steps

- [Configuring Sourcegraph](../../config/index.md)
- [Upgrading Sourcegraph](../../updates.md)
- [Site administration documentation](../../index.md)
