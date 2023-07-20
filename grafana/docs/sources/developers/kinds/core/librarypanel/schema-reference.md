---
keywords:
  - grafana
  - schema
labels:
  products:
    - cloud
    - enterprise
    - oss
title: LibraryPanel kind
---
> Both documentation generation and kinds schemas are in active development and subject to change without prior notice.

## LibraryPanel

#### Maturity: [experimental](../../../maturity/#experimental)
#### Version: 0.0

A standalone panel

| Property   | Type                | Required | Default | Description                                                                                                                                                                                                                                                                    |
|------------|---------------------|----------|---------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `metadata` | [object](#metadata) | **Yes**  |         | metadata contains embedded CommonMetadata and can be extended with custom string fields<br/>TODO: use CommonMetadata instead of redefining here; currently needs to be defined here<br/>without external reference as using the CommonMetadata reference breaks thema codegen. |
| `spec`     | [object](#spec)     | **Yes**  |         |                                                                                                                                                                                                                                                                                |
| `status`   | [object](#status)   | **Yes**  |         |                                                                                                                                                                                                                                                                                |

### Metadata

metadata contains embedded CommonMetadata and can be extended with custom string fields
TODO: use CommonMetadata instead of redefining here; currently needs to be defined here
without external reference as using the CommonMetadata reference breaks thema codegen.

It extends [_kubeObjectMetadata](#_kubeobjectmetadata).

| Property            | Type                   | Required | Default | Description                                                                                                                             |
|---------------------|------------------------|----------|---------|-----------------------------------------------------------------------------------------------------------------------------------------|
| `createdBy`         | string                 | **Yes**  |         |                                                                                                                                         |
| `creationTimestamp` | string                 | **Yes**  |         | *(Inherited from [_kubeObjectMetadata](#_kubeobjectmetadata))*                                                                          |
| `extraFields`       | [object](#extrafields) | **Yes**  |         | extraFields is reserved for any fields that are pulled from the API server metadata but do not have concrete fields in the CUE metadata |
| `finalizers`        | string[]               | **Yes**  |         | *(Inherited from [_kubeObjectMetadata](#_kubeobjectmetadata))*                                                                          |
| `labels`            | map[string]string      | **Yes**  |         | *(Inherited from [_kubeObjectMetadata](#_kubeobjectmetadata))*                                                                          |
| `resourceVersion`   | string                 | **Yes**  |         | *(Inherited from [_kubeObjectMetadata](#_kubeobjectmetadata))*                                                                          |
| `uid`               | string                 | **Yes**  |         | *(Inherited from [_kubeObjectMetadata](#_kubeobjectmetadata))*                                                                          |
| `updateTimestamp`   | string                 | **Yes**  |         |                                                                                                                                         |
| `updatedBy`         | string                 | **Yes**  |         |                                                                                                                                         |
| `deletionTimestamp` | string                 | No       |         | *(Inherited from [_kubeObjectMetadata](#_kubeobjectmetadata))*                                                                          |

### _kubeObjectMetadata

_kubeObjectMetadata is metadata found in a kubernetes object's metadata field.
It is not exhaustive and only includes fields which may be relevant to a kind's implementation,
As it is also intended to be generic enough to function with any API Server.

| Property            | Type              | Required | Default | Description |
|---------------------|-------------------|----------|---------|-------------|
| `creationTimestamp` | string            | **Yes**  |         |             |
| `finalizers`        | string[]          | **Yes**  |         |             |
| `labels`            | map[string]string | **Yes**  |         |             |
| `resourceVersion`   | string            | **Yes**  |         |             |
| `uid`               | string            | **Yes**  |         |             |
| `deletionTimestamp` | string            | No       |         |             |

### ExtraFields

extraFields is reserved for any fields that are pulled from the API server metadata but do not have concrete fields in the CUE metadata

| Property | Type | Required | Default | Description |
|----------|------|----------|---------|-------------|

### Spec

| Property        | Type                                            | Required | Default | Description                                                                                                                          |
|-----------------|-------------------------------------------------|----------|---------|--------------------------------------------------------------------------------------------------------------------------------------|
| `model`         | [object](#model)                                | **Yes**  |         | TODO: should be the same panel schema defined in dashboard<br/>Typescript: Omit<Panel, 'gridPos' &#124; 'id' &#124; 'libraryPanel'>; |
| `name`          | string                                          | **Yes**  |         | Panel name (also saved in the model)<br/>Constraint: `length >=1`.                                                                   |
| `type`          | string                                          | **Yes**  |         | The panel type (from inside the model)<br/>Constraint: `length >=1`.                                                                 |
| `uid`           | string                                          | **Yes**  |         | Library element UID                                                                                                                  |
| `version`       | integer                                         | **Yes**  |         | panel version, incremented each time the dashboard is updated.                                                                       |
| `description`   | string                                          | No       |         | Panel description                                                                                                                    |
| `folderUid`     | string                                          | No       |         | Folder UID                                                                                                                           |
| `meta`          | [LibraryElementDTOMeta](#libraryelementdtometa) | No       |         |                                                                                                                                      |
| `schemaVersion` | uint16                                          | No       |         | Dashboard version when this was saved (zero if unknown)                                                                              |

### LibraryElementDTOMeta

| Property              | Type                                                    | Required | Default | Description |
|-----------------------|---------------------------------------------------------|----------|---------|-------------|
| `connectedDashboards` | integer                                                 | **Yes**  |         |             |
| `createdBy`           | [LibraryElementDTOMetaUser](#libraryelementdtometauser) | **Yes**  |         |             |
| `created`             | string                                                  | **Yes**  |         |             |
| `folderName`          | string                                                  | **Yes**  |         |             |
| `folderUid`           | string                                                  | **Yes**  |         |             |
| `updatedBy`           | [LibraryElementDTOMetaUser](#libraryelementdtometauser) | **Yes**  |         |             |
| `updated`             | string                                                  | **Yes**  |         |             |

### LibraryElementDTOMetaUser

| Property    | Type    | Required | Default | Description |
|-------------|---------|----------|---------|-------------|
| `avatarUrl` | string  | **Yes**  |         |             |
| `id`        | integer | **Yes**  |         |             |
| `name`      | string  | **Yes**  |         |             |

### Model

TODO: should be the same panel schema defined in dashboard
Typescript: Omit<Panel, 'gridPos' | 'id' | 'libraryPanel'>;

| Property | Type | Required | Default | Description |
|----------|------|----------|---------|-------------|

### Status

| Property           | Type                                                       | Required | Default | Description                                                                                                                                                                |
|--------------------|------------------------------------------------------------|----------|---------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `additionalFields` | [object](#additionalfields)                                | No       |         | additionalFields is reserved for future use                                                                                                                                |
| `operatorStates`   | map[string][status.#OperatorState](#status.#operatorstate) | No       |         | operatorStates is a map of operator ID to operator state evaluations.<br/>Any operator which consumes this kind SHOULD add its state evaluation information to this field. |

### AdditionalFields

additionalFields is reserved for future use

| Property | Type | Required | Default | Description |
|----------|------|----------|---------|-------------|

### Status.#OperatorState

| Property           | Type               | Required | Default | Description                                                                                                                                                                      |
|--------------------|--------------------|----------|---------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `lastEvaluation`   | string             | **Yes**  |         | lastEvaluation is the ResourceVersion last evaluated                                                                                                                             |
| `state`            | string             | **Yes**  |         | state describes the state of the lastEvaluation.<br/>It is limited to three possible states for machine evaluation.<br/>Possible values are: `success`, `in_progress`, `failed`. |
| `descriptiveState` | string             | No       |         | descriptiveState is an optional more descriptive state field which has no requirements on format                                                                                 |
| `details`          | [object](#details) | No       |         | details contains any extra information that is operator-specific                                                                                                                 |

### Details

details contains any extra information that is operator-specific

| Property | Type | Required | Default | Description |
|----------|------|----------|---------|-------------|


