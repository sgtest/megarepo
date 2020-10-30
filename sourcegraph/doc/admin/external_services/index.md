# Using external services with Sourcegraph

> NOTE: Using Sourcegraph with an external service is a [paid feature](https://about.sourcegraph.com/pricing). [Contact us](https://about.sourcegraph.com/contact/sales) to get a trial license.

Sourcegraph by default provides versions of services it needs to operate, including:

- A [PostgreSQL](https://www.postgresql.org/) instance for storing long-term information, such as user information when using Sourcegraph's built-in authentication provider instead of an external one.
- A second PostgreSQL instance for storing large-volume precise code intelligence data.
- A [Redis](https://redis.io/) instance for storing short-term information such as user sessions.
- A second Redis instance for storing cache data.
- A [MinIO](https://min.io/) instance that serves as a local S3-compatible object storage to hold user uploads before they can be processed.

Your Sourcegraph instance can be configured to use an external or managed version of these services. Using a managed version of PostgreSQL can make backups and recovery easier to manage and perform. Using a managed object storage service may decrease your hosting costs as persistent volumes are often more expensive than object storage space.

See the following guides to use an external or managed version of each service type.

- See [Using your own PostgreSQL server](./postgres.md) to replace the bundled PostgreSQL instances.
- See [Using your own Redis server](./redis.md) to replace the bundled Redis instances.
- See [Using a managed object storage service (S3 or GCS)](./object_storage.md) to replace the bundled MinIO instance.
