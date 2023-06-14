load("@rules_oci//oci:pull.bzl", "oci_pull")

# Quick script to get the latest tags for each of the base images:
#
# grep 'image = ' ./dev/oci_deps.bzl | while read -r str ; do
#   str_no_spaces="${str#"${str%%[![:space:]]*}"}"  # remove leading spaces
#   url="${str_no_spaces#*\"}"  # remove prefix until first quote
#   url="${url%%\"*}"  # remove suffix from first quote
#
#   IMAGE_DETAILS=$(gcloud container images list-tags $url --limit=1 --sort-by=~timestamp --format=json)
#   TAG=$(echo $IMAGE_DETAILS | jq -r '.[0].tags[0]')
#   DIGEST=$(echo $IMAGE_DETAILS | jq -r '.[0].digest')
#
#   echo $url
#   echo $DIGEST
# done

def oci_deps():
    oci_pull(
        name = "wolfi_base",
        digest = "sha256:8d80271a8d8f7b8fa7ff62b2e009ab3f0f81c5407872144db8cb30b396801853",
        image = "us.gcr.io/sourcegraph-dev/wolfi-sourcegraph-base",
    )

    oci_pull(
        name = "wolfi_cadvisor_base",
        digest = "sha256:2e5aec9ba5a4835b4c35103bd27ad2ad3e65a064ec5001a35168044dd8c06a4a",
        image = "us.gcr.io/sourcegraph-dev/wolfi-cadvisor-base",
    )

    oci_pull(
        name = "wolfi_symbols_base",
        digest = "sha256:a5d6a10698466e1a7198ca17e41a3c6c8cd7228ae562352abbdac829e539fc75",
        image = "us.gcr.io/sourcegraph-dev/wolfi-symbols-base",
    )

    oci_pull(
        name = "wolfi_server_base",
        digest = "sha256:b4f5d5927d4e53937fdaf8fc9b54414ee87c2e2f288e566cc25bb98771e88008",
        image = "us.gcr.io/sourcegraph-dev/wolfi-server-base",
    )

    oci_pull(
        name = "wolfi_gitserver_base",
        digest = "sha256:eae7c238c7c33d59752973b6bcb678b25dce1a759a0cece6d8350e4230d4ea49",
        image = "us.gcr.io/sourcegraph-dev/wolfi-gitserver-base",
    )

    oci_pull(
        name = "wolfi_grafana_base",
        digest = "sha256:ec1049f35ff7e4ab6ff7b4cc6790996ad74d196b8dcee8ea5283fca759156637",
        image = "us.gcr.io/sourcegraph-dev/wolfi-grafana",
    )

    oci_pull(
        name = "wolfi_postgres_exporter_base",
        digest = "sha256:b51ae2b70cd7cd7883e8057d69a74c959fd5f03d723538908ea8f47a0a322e02",
        image = "us.gcr.io/sourcegraph-dev/wolfi-postgres-exporter-base",
    )

    oci_pull(
        name = "wolfi_jaeger_all_in_one_base",
        digest = "sha256:6d978aa3cc31e3410088ef4a3220fe419878c0b53e604c9323b343d0044ed9d3",
        image = "us.gcr.io/sourcegraph-dev/wolfi-jaeger-all-in-one-base",
    )

    oci_pull(
        name = "wolfi_jaeger_agent_base",
        digest = "sha256:a7dd18fa67c5c02f1f6ba6f503a2249a1fe103bfe47775a2faa041b16895c59c",
        image = "us.gcr.io/sourcegraph-dev/wolfi-jaeger-agent-base",
    )

    oci_pull(
        name = "wolfi_redis_base",
        digest = "sha256:d72b41d737473226ddf3a752bec885caaf1bd93adaecbb33dc0cce693f261b5e",
        image = "us.gcr.io/sourcegraph-dev/wolfi-redis-base",
    )

    oci_pull(
        name = "wolfi_redis_exporter_base",
        digest = "sha256:97924b18f530386f524df14b8172963c54d1378727cea72004bef8ae2490e871",
        image = "us.gcr.io/sourcegraph-dev/wolfi-redis-exporter-base",
    )

    oci_pull(
        name = "wolfi_syntax_highlighter_base",
        digest = "sha256:06ce2e349550d2e99c96a5610746fa2a3b743790bd0c16d896847434551afead",
        image = "us.gcr.io/sourcegraph-dev/wolfi-syntax-highlighter-base",
    )

    oci_pull(
        name = "wolfi_search_indexer_base",
        digest = "sha256:7a3f1327e75de7d3ace2240e650b82a44f4a70bd988548786880c3eebb02143e",
        image = "us.gcr.io/sourcegraph-dev/wolfi-search-indexer-base",
    )

    oci_pull(
        name = "wolfi_repo_updater_base",
        digest = "sha256:2e49220a8e69a8f1f92fe1c2da08efd35a9d7226e76220a5b39c124d8231092b",
        image = "us.gcr.io/sourcegraph-dev/wolfi-repo-updater-base",
    )

    oci_pull(
        name = "wolfi_searcher_base",
        digest = "sha256:3029998bad3b614efde5ff2dbe8287b4fa5e38cbf1a22c40b37f97f6257aed16",
        image = "us.gcr.io/sourcegraph-dev/wolfi-searcher-base",
    )

    oci_pull(
        name = "wolfi_executor_base",
        digest = "sha256:03c0e699760fda087702baa090b0827471395cbf891807b1f73b48280f345041",
        image = "us.gcr.io/sourcegraph-dev/wolfi-executor-base",
    )

    oci_pull(
        name = "wolfi_bundled_executor_base",
        digest = "sha256:b9a217e4f71e767a19bed1e3d39618ed7258ea726d339776ddf1523267452c8c",
        image = "us.gcr.io/sourcegraph-dev/wolfi-bundled-executor-base",
    )

    oci_pull(
        name = "wolfi_executor_kubernetes_base",
        digest = "sha256:0cb7a64371b29f2689ab18f41a71cab51f0976de1a3b850a2d468601f8ab9c48",
        image = "us.gcr.io/sourcegraph-dev/wolfi-executor-kubernetes-base",
    )

    oci_pull(
        name = "wolfi_batcheshelper_base",
        digest = "sha256:3c6c8b6ef31d062c4b9faa461d4533bf0589fab7de9b89040b03e27ca25a4176",
        image = "us.gcr.io/sourcegraph-dev/wolfi-batcheshelper-base",
    )

    oci_pull(
        name = "wolfi_prometheus_base",
        digest = "sha256:5089836fad63b647d0a1c1dbb3a10d7abdeea2f0fc76f4c977df21d26d70cf06",
        image = "us.gcr.io/sourcegraph-dev/wolfi-prometheus-base",
    )

    oci_pull(
        name = "wolfi_postgresql-12_base",
        digest = "sha256:e3f597e118056f6c555dbb284b59bf6c29b8ebbd3a4fc6c3df7889db368855a9",
        image = "us.gcr.io/sourcegraph-dev/wolfi-postgresql-12-base",
    )

    oci_pull(
        name = "wolfi_postgresql-12-codeinsights_base",
        digest = "sha256:78061eee8c728a9d732c1bfd6012baf5f4ad2f087acd18c17a6d749f7a0d459f",
        image = "us.gcr.io/sourcegraph-dev/wolfi-postgresql-12-codeinsights-base",
    )

    oci_pull(
        name = "wolfi_node_exporter_base",
        digest = "sha256:9f7149d05afad6e3581a7a4bc13c60cad5d314bab7307e1dcd47d1c6bb42c497",
        image = "us.gcr.io/sourcegraph-dev/wolfi-node-exporter-base",
    )

    oci_pull(
        name = "wolfi_opentelemetry_collector_base",
        digest = "sha256:2b410f0807c8db91ac324edf48b9b657bf7ddabfe7553d0d32d2f5e77db23a7e",
        image = "us.gcr.io/sourcegraph-dev/wolfi-opentelemetry-collector-base",
    )

    oci_pull(
        name = "wolfi_searcher_base",
        digest = "sha256:3029998bad3b614efde5ff2dbe8287b4fa5e38cbf1a22c40b37f97f6257aed16",
        image = "us.gcr.io/sourcegraph-dev/wolfi-searcher-base",
    )

    oci_pull(
        name = "wolfi_s3proxy_base",
        digest = "sha256:4299634c0e403059a5a2aeda323181feb8189648c23fd69d0b5d057e0e7966eb",
        image = "us.gcr.io/sourcegraph-dev/wolfi-blobstore-base",
    )
