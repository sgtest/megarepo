# Sourcegraph Deployment Best Practices

## What does "best practice" mean to us?

Sourcegraph is a highly scalable and configurable application. As an open source company we hope our customers will feel empowered to customize Sourcegraph to meet their unique needs, but we cannot guarantee whether deviations from the below guidelines will work or be supportable by Sourcegraph. If in doubt, please contact your Customer Engineer or reach out to support.

## Sourcegraph Performance Dependencies

- number of users associated to instance
- user's engagement level
- number and size of code repositories synced to Sourcegraph.

_To get a better idea of your resource requirements for your instance use our_ [_resource estimator_](https://docs.sourcegraph.com/admin/install/resource_estimator)_._

## Deployment Best Practices

## Docker Compose

Docker Compose Sourcegraph may be customized by forking our [repo](https://github.com/sourcegraph/deploy-sourcegraph-docker) and altering our [standard](https://sourcegraph.com/github.com/sourcegraph/deploy-sourcegraph-docker@master/-/blob/docker-compose/docker-compose.yaml)[_ **docker-compose.yaml** _](https://sourcegraph.com/github.com/sourcegraph/deploy-sourcegraph-docker@master/-/blob/docker-compose/docker-compose.yaml)[file](https://sourcegraph.com/github.com/sourcegraph/deploy-sourcegraph-docker@master/-/blob/docker-compose/docker-compose.yaml), we consider the following best practice:

- The version argument in the .yaml file must be the same as in the standard deployment
- Users should only alter the .yaml file to adjust resource limits, or duplicate container entries to add more container replicas
- Minimum Docker version: v20.10.0 ([https://docs.docker.com/engine/release-notes/#20100](https://docs.docker.com/engine/release-notes/#20100))
- Minimum version of Docker Compose: v1.22.0 ([https://docs.docker.com/compose/release-notes/#1220](https://docs.docker.com/compose/release-notes/#1220)) - this is first version that supports Docker Compose format `2.4`
- Docker Compose deployments should only be deployed with `docker-compose up`, and not Docker Swarm

## Kubernetes

Kubernetes deployments may be customized in a variety of ways, we consider the following best practice:

- Users should use our [standard deployment](https://github.com/sourcegraph/deploy-sourcegraph) as a base, users may customize deployments via:
  - Kustomize [overlays](https://github.com/sourcegraph/deploy-sourcegraph/tree/master/overlays)
- The suggested Kubernetes version is the current [GKE Stable release version](https://cloud.google.com/kubernetes-engine/docs/release-notes-stable)
- We attempt to support new versions of Kubernetes 2-3 months after their release.
- Users are expected to run a compliant Kubernetes version ([a CNCF certified Kubernetes distribution](https://github.com/cncf/k8s-conformance))
- The cluster must have access to persistent SSD storage
- We test against Google Kubernetes Engine

_Unless scale, resiliency, or some other legitimate need exists that necessitates the use of Kubernetes (over a much simpler Docker Compose installation), it's recommended that Docker-Compose be used._

_Any major modifications outside of what we ship in the [standard deployment](https://github.com/sourcegraph/deploy-sourcegraph) are the responsibility of the user to manage, including but not limited to: Helm templates, Terraform configuration, and other ops/infrastructure tooling._

## Sourcegraph Server (single Docker container)

Sourcegraph Server is best used for trying out Sourcegraph. It&#39;s not intended for enterprise production deployments for the following reasons:

- Limited logging information for debugging
- Performance issues with:
  - more than 100 repositories
  - more than 10 active users
- Some Sourcegraph features do not have full functionality (Ex: Code Insights)

_It is possible to migrate your data to a Docker-Compose or Kubernetes deployment, contact your Customer Engineer or reach out to support and we&#39;ll be happy to assist you in upgrading your deployment._

## Additional Support Information

### LSIF and Batch Changes

- The list of languages currently supported for precise code intelligence (LSIF indexers) can be found [here](https://docs.sourcegraph.com/code_intelligence/references/indexers)
- Requirements to set-up Batch Changes can be found [here](https://docs.sourcegraph.com/batch_changes/references/requirements)

### Browsers Extensions

- Sourcegraph and its extensions are supported on the latest versions of Chrome, Firefox, and Safari.

### Editor Extensions

Only the latest versions of IDEs are generally supported, but most versions within a few months up-to-date generally work.

- VS code: [https://github.com/sourcegraph/sourcegraph-vscode](https://github.com/sourcegraph/sourcegraph-vscode); we don&#39;t yet support VSCodium
- Atom: [https://github.com/sourcegraph/sourcegraph-atom](https://github.com/sourcegraph/sourcegraph-atom)
- Sublime Text 3: [https://github.com/sourcegraph/sourcegraph-sublime](https://github.com/sourcegraph/sourcegraph-sublime); we don&#39;t yet support Sublime Text 2 or 4
- Jetbrains IDEs: [https://github.com/sourcegraph/sourcegraph-jetbrains](https://github.com/sourcegraph/sourcegraph-jetbrains) - we only test with IntelliJ IDEA, but it should work with no issues in all Jetbrains IDEs:
  - IntelliJ IDEA
  - IntelliJ IDEA Community Edition
  - PhpStorm
  - WebStorm
  - PyCharm
  - PyCharm Community Edition
  - RubyMine
  - AppCode
  - CLion
  - GoLand
  - DataGrip
  - Rider
  - Android Studio
