#!/usr/bin/env bash
set -eu

GCLOUD_APP_CREDENTIALS_FILE=${GCLOUD_APP_CREDENTIALS_FILE-$HOME/.config/gcloud/application_default_credentials.json}
cd "$(dirname "${BASH_SOURCE[0]}")"/../../.. || exit 1

./enterprise/dev/app/build-backend.sh
./enterprise/dev/app/tauri-build.sh
