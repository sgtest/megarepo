# How to use feature flags

This document will take you through how to add, remove, and modify feature flags.

## When to use feature flags

Feature flags, as opposed to experimental features, are intended to be strictly short-lived.
They are designed to be useful for A/B testing, and the values of all active feature flags
are added to every event log for the purpose of analytics.

## How it works

Each feature flag is either a static feature flag, or a "rollout" flag. 
- A static feature flag has a single value (currently only "true" or "false") for all users that haven't overriden it.
- A rollout flag assigns a random (but stable) value to each user. Each rollout flag is created with a percentage of users that should be randomly assigned the value "true".

A user is identified either by their user ID (if logged in), or by an anonymous user ID in local 
storage. 

The set of evaluated feature flags is appended to each event log so they can be queried against
for analytics.

## Example lifecycle of a feature flag (for frontend A/B testing)

The standard use of a feature flag for A/B testing a frontend feature will look like the following:

1) Implement the feature that you want to be behind a feature flag
2) Deploy to sourcegraph.com
3) Create the feature flag through the GraphQL API
4) Measure the effect of the feature flag
5) Disable or delete the feature flag
6) Remove the code that references the feature flag

Feature flags can also be used in the backend through `featureflag.FromContext()`, but this
example specifically applies to frontend flags.


### Implement the feature flag

In the frontend, evaluated feature flags for the current user are available on 
the SourcegraphWebAppState. These can be prop-drilled into the components that need access to them.

Ensure that a default value is set for feature flags so that 
(i) code can be deployed before creating the feature flag, (ii) deleting the feature flag is safe before removing referenced code, (iii) enterprise deployments continue to work as
expected. 

For example:
```typescript
let myFeatureFlagValue = this.state.featureFlags.myFeatureFlag || false
```

### Create the feature flag through the GraphQL API

To create a rollout feature flag, currently the best way is to use the GraphQL API.

Go to `sourcegraph.com/api/console`, then create a feature flag like the following:
```graphql
mutation CreateFeatureFlag{
  createFeatureFlag(
    name: "myFeatureFlag",
    rolloutBasisPoints: 5000,
  ){
    __typename
  }
}
```

The value of `rolloutBasisPoints` is measured in increments of 0.01% (a basis point).
To create a feature flag that applies to 50% of users, set `rolloutBasisPoints` 
to 5000.

### Measure the effect of the feature flag

Feature flags are added as a column to all event logs, so in order to measure any 
effect, there needs to be a related event for it. For example, to compare the number of
`ShareButtonClicked` events between groups where `myFeatureFlag` is enabled and disabled,
you could use a query like the following:

```sql
SELECT 
	JSON_VALUE(feature_flags, '$.myFeatureFlag') AS my_flag, 
	count(*) 
FROM `telligentsourcegraph.dotcom_events.events` 
WHERE name = 'ShareButtonClicked' 
GROUP BY my_flag;
```

### Disable or delete the feature flag

In most cases, after an A/B test is performed, a feature flag should be deleted.
That can be done with a GraphQL query like the following:
```graphql
mutation DeleteFeatureFlag{
  deleteFeatureFlag(
    name: "myFeatureFlag",
  ){
    __typename
  }
}
```

Once a feature flag is deleted, it will no longer be added to events as metadata,
so removing the code path that uses it will not change any measurements.

### Remove the code that references the feature flag

Once the feature flag is deleted, the code that references it can be safely deleted
without changing any of the measurements. 

## Feature flag overrides

In addition to feature flags as described above, you can also create feature flag
overrides. This is useful if you'd like to test a feature flag locally by assigning
your user a specific value, or if you'd like to do an A/B test on members of the 
Sourcegraph org. 

Overrides can either apply to a single user or an entire org. If both are set, a user
override takes precedence over an org override.

If an override for a feature flag exists for a user (or the user's org), the value of 
the override will be used instead of the value that would have been randomly selected for a user.

### Creating an override

To create a feature flag override, you can use a graphql query like the following:

```graphql
mutation CreateFeatureFlagOverride{
  createFeatureFlagOverride(
    namespace: "Vx528v=", 
    flagName: "myFeatureFlag",
    value: false,
  ){
    __typename
  }
}
```

The `namespace` argument is the graphql ID of either a user or an organization.

## Further reading

- Initial RFC [#286](https://docs.google.com/document/d/1aT8uI3mUXpm9IK9_WbXhFM5ahHj9KQeQ521hd9EE5U8/edit)
