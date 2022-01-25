# Using your own PostgreSQL server

You can use your own PostgreSQL v12+ server with Sourcegraph if you wish. For example, you may prefer this if you already have existing backup infrastructure around your own PostgreSQL server, wish to use Amazon RDS, etc.

> NOTE: Only the frontend (_pgsql_) and code intelligence (_codeintel-db_) databases are supported to run externally at this time.

## General recommendations

If you choose to set up your own PostgreSQL server, please note **we strongly recommend each database to be set up in different servers and/or hosts**. We suggest either:

1. Deploy _codeintel-db_ alongside the other Sourcegraph containers, i.e. not as a managed PostgreSQL instance.
2. Deploy a separate PostgreSQL instance. The primary reason to not use the same Postgres instance for this data is because precise code intelligence data can take up a significant of space (given the amount of indexed repositories is large) and the performance of the database may impact the performance of the general application database. You'll most likely want to be able to scale their resources independently.

We also recommend having backups for the _codeintel-db_ as a best practice. The reason behind this recommendation is that _codeintel-db_ data is uploaded via CI systems. If data is lost, Sourcegraph cannot automatically rebuild it from the repositories, which means you'd have to wait until it is re-uploaded from your CI systems.

## Instructions

The addition of `PG*` environment variables to your Sourcegraph deployment files will instruct Sourcegraph to target an external PostgreSQL server. To externalize the _frontend database_, use the following standard `PG*` variables:

- `PGHOST`
- `PGPORT`
- `PGUSER`
- `PGPASSWORD`
- `PGDATABASE`
- `PGSSLMODE`

To externalize the _code intelligence database_, use the following prefixed `CODEINTEL_PG*` variables:

- `CODEINTEL_PGHOST`
- `CODEINTEL_PGPORT`
- `CODEINTEL_PGUSER`
- `CODEINTEL_PGPASSWORD`
- `CODEINTEL_PGDATABASE`
- `CODEINTEL_PGSSLMODE`

> NOTE: ⚠️ If you have configured both the frontend (pgsql) and code intelligence (codeintel-db) databases with the same values, the Sourcegraph instance will refuse to start. Each database should either be configured to point to distinct hosts (recommended), or configured to point to distinct databases on the same host.

### sourcegraph/server

Add the following to your `docker run` command:

<!--
  DO NOT CHANGE THIS TO A CODEBLOCK.
  We want line breaks for readability, but backslashes to escape them do not work cross-platform.
  This uses line breaks that are rendered but not copy-pasted to the clipboard.
-->

<pre class="pre-wrap start-sourcegraph-command"><code>docker run [...]<span class="virtual-br"></span> -e PGHOST=psql1.mycompany.org<span class="virtual-br"></span> -e PGUSER=sourcegraph<span class="virtual-br"></span> -e PGPASSWORD=secret<span class="virtual-br"></span> -e PGDATABASE=sourcegraph<span class="virtual-br"></span> -e PGSSLMODE=require<span class="virtual-br"> -e CODEINTEL_PGHOST=psql2.mycompany.org<span class="virtual-br"></span> -e CODEINTEL_PGUSER=sourcegraph<span class="virtual-br"></span> -e CODEINTEL_PGPASSWORD=secret<span class="virtual-br"></span> -e CODEINTEL_PGDATABASE=sourcegraph-codeintel<span class="virtual-br"></span> -e CODEINTEL_PGSSLMODE=require<span class="virtual-br"></span> sourcegraph/server:3.36.2</code></pre>

### Docker Compose

1. Add/modify the following environment variables to all of the `sourcegraph-frontend-*` services and the `sourcegraph-frontend-internal` service in [docker-compose.yaml](https://github.com/sourcegraph/deploy-sourcegraph-docker/blob/3.21/docker-compose/docker-compose.yaml):

    ```
    sourcegraph-frontend-0:
      # ...
      environment:
        # ...
        - 'PGHOST=psql1.mycompany.org'
        - 'PGUSER=sourcegraph'
        - 'PGPASSWORD=secret'
        - 'PGDATABASE=sourcegraph'
        - 'PGSSLMODE=require'
        - 'CODEINTEL_PGHOST=psql2.mycompany.org'
        - 'CODEINTEL_PGUSER=sourcegraph'
        - 'CODEINTEL_PGPASSWORD=secret'
        - 'CODEINTEL_PGDATABASE=sourcegraph-codeintel'
        - 'CODEINTEL_PGSSLMODE=require'
      # ...
    ```

    See ["Environment variables in Compose"](https://docs.docker.com/compose/environment-variables/) for other ways to pass these environment variables to the relevant services (including from the command line, a `.env` file, etc.).

1. Comment out / remove the internal `pgsql` and `codeintel-db` services in [docker-compose.yaml](https://github.com/sourcegraph/deploy-sourcegraph-docker/blob/3.21/docker-compose/docker-compose.yaml) since Sourcegraph is using the external one now.

    ```
    # # Description: PostgreSQL database for various data.
    # #
    # # Disk: 128GB / persistent SSD
    # # Ports exposed to other Sourcegraph services: 5432/TCP 9187/TCP
    # # Ports exposed to the public internet: none
    # #
    # pgsql:
    # container_name: pgsql
    # image: 'index.docker.io/sourcegraph/postgres-11.4:19-11-14_b084311b@sha256:072481559d559cfd9a53ad77c3688b5cf583117457fd452ae238a20405923297'
    # cpus: 4
    # mem_limit: '2g'
    # healthcheck:
    #    test: '/liveness.sh'
    #    interval: 10s
    #    timeout: 1s
    #    retries: 3
    #    start_period: 15s
    # volumes:
    #    - 'pgsql:/data/'
    # networks:
    #     - sourcegraph
    # restart: always

    # # Description: PostgreSQL database for code intelligence data.
    # #
    # # Disk: 128GB / persistent SSD
    # # Ports exposed to other Sourcegraph services: 5432/TCP 9187/TCP
    # # Ports exposed to the public internet: none
    # #
    # codeintel-db:
    #   container_name: codeintel-db
    #   image: 'index.docker.io/sourcegraph/codeintel-db@sha256:63090799b34b3115a387d96fe2227a37999d432b774a1d9b7966b8c5d81b56ad'
    #   cpus: 4
    #   mem_limit: '2g'
    #   healthcheck:
    #     test: '/liveness.sh'
    #     interval: 10s
    #     timeout: 1s
    #     retries: 3
    #     start_period: 15s
    #   volumes:
    #     - 'codeintel-db:/data/'
    #   networks:
    #     - sourcegraph
    #   restart: always
    ```

### Kubernetes

Update the `PG*` and `CODEINTEL_PG*` environment variables in the `sourcegraph-frontend` deployment YAML file to point to the external frontend (`pgsql`) and code intelligence (`codeintel-db`) PostgreSQL instances, respectively. Again, these must not point to the same database or the Sourcegraph instance will refuse to start.

You are then free to remove the now unused `pgsql` and `codeintel-db` services and deployments from your cluster.

### Version requirements

Please refer to our [Postgres](https://docs.sourcegraph.com/admin/postgres) documentation to learn about version requirements.

### Caveats

> NOTE: If your PostgreSQL server does not support SSL, set `PGSSLMODE=disable` instead of `PGSSLMODE=require`. Note that this is potentially insecure.

Most standard PostgreSQL environment variables may be specified (`PGPORT`, etc). See http://www.postgresql.org/docs/current/static/libpq-envars.html for a full list.

> NOTE: On Mac/Windows, if trying to connect to a PostgreSQL server on the same host machine, remember that Sourcegraph is running inside a Docker container inside of the Docker virtual machine. You may need to specify your actual machine IP address and not `localhost` or `127.0.0.1` as that refers to the Docker VM itself.
