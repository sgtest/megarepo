---
description: Guide for upgrading to Grafana v10.1
keywords:
  - grafana
  - configuration
  - documentation
  - upgrade
title: Upgrade to Grafana v10.1
menutitle: Upgrade to v10.1
weight: 1600
---

# Upgrade to Grafana v10.1

{{< docs/shared "upgrade/upgrade-common-tasks.md" >}}

## Technical notes

### OAuth role mapping enforcement

This change impacts `GitHub` OAuth, `Gitlab` OAuth, `Okta` OAuth and `Generic` OAuth.

Currently if no organization role mapping is found for a user when connecting via OAuth, Grafana doesn’t update the user’s organization role.

With Grafana 10.1, on every login, if the role_attribute_path property does not return a role, then the user is assigned the role specified by the auto_assign_org_role option or the default role for the organization, by default, Viewer.

To avoid overriding manually set roles, enable the skip_org_role_sync option in the Grafana configuration for your OAuth provider before upgrading to Grafana 10.1 and before users log in for the first time in Grafana 10.1.

Example for Generic OAuth2:

```ini
[auth.generic_oauth]
...
skip_org_role_sync = true
```
