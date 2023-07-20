---
keywords:
  - grafana
  - schema
labels:
  products:
    - cloud
    - enterprise
    - oss
title: NewsPanelCfg kind
---
> Both documentation generation and kinds schemas are in active development and subject to change without prior notice.

## NewsPanelCfg

#### Maturity: [experimental](../../../maturity/#experimental)
#### Version: 0.0



| Property  | Type               | Required | Default | Description |
|-----------|--------------------|----------|---------|-------------|
| `Options` | [object](#options) | **Yes**  |         |             |

### Options

| Property    | Type    | Required | Default | Description                                |
|-------------|---------|----------|---------|--------------------------------------------|
| `feedUrl`   | string  | No       |         | empty/missing will default to grafana blog |
| `showImage` | boolean | No       | `true`  |                                            |


