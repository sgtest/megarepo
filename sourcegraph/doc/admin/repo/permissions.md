# Repository permissions

Sourcegraph can be configured to enforce repository permissions from code hosts.

Currently, GitLab permissions are supported. Check the [roadmap](../../dev/roadmap.md) for plans to
support other code hosts. If your desired code host is not yet on the roadmap, please [open a
feature request](https://github.com/sourcegraph/sourcegraph/issues/new?template=feature_request.md).

## GitLab

Enabling GitLab repository permissions on Sourcegraph requires the following:

* A GitLab access token with `api` and `sudo` scope.
* Single sign-on (SSO) configured for both Sourcegraph and GitLab. The same SSO provider should be
  used to authenticate to both.

Sourcegraph uses the above to associated a GitLab user account with each Sourcegraph user. It then
uses the GitLab API to determine the set of repositories that are accessible to each user. Note that
Sourcegraph admin users will have access to **all** repositories on Sourcegraph regardless of what
permissions are associated with their GitLab user.

To enable GitLab permissions, set the `authorization` field in the GitLab site configuration:

1. If you haven't done so already, [add a GitLab connection
   object](../../integration/gitlab.md#gitlab-configuration) in the [site configuration
   editor](../../admin/site_config/index.md).
1. Add an `authorization` field to that object and fill in the fields. The site config will contain a
   snippet like the following:
   
   ```
   {
     "auth.providers": [
       {
         "type": "$AUTHENTICATION_TYPE",
         "configID": "$USER_SPECIFIED_AUTHENTICATION_ID"
         ...
       }
     ],
     "gitlab": [
       {
         "url": "$GITLAB_URL",
         "token": "$GITLAB_TOKEN",  // Token should have both `sudo` and `api` scope
         "authorization": {
           "authnProvider": {
             "configID": "$USER_SPECIFIED_AUTHENTICATION_ID",
             "type": "$AUTHENTICATION_TYPE",
             "gitlabProvider": "$GITLAB_PROVIDER"
           },
           "ttl": "1h"
         }
       }
     ],
     ...
   }
   ```

   Note that the `configID` and `type` fields in the GitLab `authorization.authnProvider` object
   must match the `configID` and `type` of exactly one element of `auth.providers`.

See the [site configuration
documentation](https://docs.sourcegraph.com/admin/site_config/all#gitlabconnection-object) for the
meaning of specific fields.
