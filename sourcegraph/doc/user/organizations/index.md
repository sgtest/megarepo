# Organizations

Organizations are named groups of users with an associated JSON settings file (with search scopes, saved queries, etc.). These settings take effect for all users who are members of the organization.

To create an organization, go to `http(s)://[hostname]/organizations/new` on your Sourcegraph instance (or, from any page, click your username and then **New organization**).

You (and any other organization members) may add members from the organization's members page at `http(s)://[hostname]/organizations/[org-name]/members`.

To automatically join all users on your instance to a specific organization, create the organization first and then set the `auth.userOrgMap` [site configuration](../../admin/config/site_config.md) option:

```json
{
  // ...
  "auth.userOrgMap": {
    // All users ("*") will be automatically joined to the "acme-corp" org.
    // Currently "*" (all users) is the only supported key.
    "*": ["acme-corp"] // The array values refer to org names you've already created.
  }
  // ...
}
```
