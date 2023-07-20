---
keywords:
  - grafana
  - schema
labels:
  products:
    - cloud
    - enterprise
    - oss
title: AlertGroupsPanelCfg kind
---
> Both documentation generation and kinds schemas are in active development and subject to change without prior notice.

## AlertGroupsPanelCfg

#### Maturity: [merged](../../../maturity/#merged)
#### Version: 0.0



| Property  | Type               | Required | Default | Description |
|-----------|--------------------|----------|---------|-------------|
| `Options` | [object](#options) | **Yes**  |         |             |

### Options

| Property       | Type    | Required | Default | Description                                                 |
|----------------|---------|----------|---------|-------------------------------------------------------------|
| `alertmanager` | string  | **Yes**  |         | Name of the alertmanager used as a source for alerts        |
| `expandAll`    | boolean | **Yes**  |         | Expand all alert groups by default                          |
| `labels`       | string  | **Yes**  |         | Comma-separated list of values used to filter alert results |


