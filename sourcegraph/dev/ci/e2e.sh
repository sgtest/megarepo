#!/usr/bin/env bash

cd $(dirname "${BASH_SOURCE[0]}")/../..
set -ex

if [ -z "$IMAGE" ]; then
    echo "Must specify \$IMAGE."
    exit 1
fi

echo "Running a daemonized $IMAGE as the test subject..."
CONTAINER="$(docker container run --rm -d $IMAGE)"
trap 'kill $(jobs -p)'" ; docker container stop $CONTAINER" EXIT

docker exec "$CONTAINER" apk add --no-cache socat
# Connect the server container's port 7080 to localhost:7080 so that e2e tests
# can hit it. This is similar to port-forwarding via SSH tunneling, but uses
# docker exec as the transport.
socat tcp-listen:7080,reuseaddr,fork system:"docker exec -i $CONTAINER socat stdio 'tcp:localhost:7080'" &

URL="http://localhost:7080"

timeout 30s bash -c "until curl --output /dev/null --silent --head --fail $URL; do
    echo Waiting 5s for $URL...
    sleep 5
done"
echo "Waiting for $URL... done"

pushd web
env SOURCEGRAPH_BASE_URL="$URL" yarn run test-e2e
popd
