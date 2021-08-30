# Search-Blitz

The purpose of Search-Blitz is to provide a baseline for our search performance.
Search-Blitz calls the stream and GraphQL API of Sourcegraph.com for typical
queries in regular intervals. Sourcegraph recognizes the Search-Blitz's
`User-Agent` and sends metrics to Prometheus.

The dashboard is accessible on
[Grafana](https://sourcegraph.com/-/debug/grafana/d/frontend/frontend?orgId=1),
section "Sentinel queries".

In addition to the dashboard that we ship with Sourcegraph, Search-Blitz is
deployed with a dedicated instance of Prometheus and Grafana.

## How to track a query

Add the query to [`config.yaml`](https://github.com/sourcegraph/sourcegraph/blob/main/internal/cmd/search-blitz/config.yaml).

## How to deploy

1. Merge your changes to _main_
2. Build and upload a new docker image:
   ```
   ./scripts/build.sh <next-version, e.g. 0.0.2>
   ```
3. Update the image tag in [deploy-sourcegraph-dot-com](https://github.com/sourcegraph/deploy-sourcegraph-dot-com/blob/release/configure/search-blitz/search-blitz.StatefulSet.yaml#L36)

4. (Optional) Apply the new manifest

```
kubectl apply -f ./configure/search-blitz
```

## How to access Search-Blitz's dedicated Grafana

```
kubectl port-forward search-blitz-0 3000:3000 -n monitoring
```

open http://localhost:3000
