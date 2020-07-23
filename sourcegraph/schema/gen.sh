#!/usr/bin/env bash

set -e

export GOBIN="$PWD/.bin"
export GO111MODULE=on

go install github.com/sourcegraph/go-jsonschema/cmd/go-jsonschema-compiler

# shellcheck disable=SC2010
schemas="$(ls -- *.schema.json | grep -v json-schema-draft)"

# shellcheck disable=SC2086
"$GOBIN"/go-jsonschema-compiler -o schema.go -pkg schema $schemas

stringdata() {
  # shellcheck disable=SC2039
  target="${1/.schema.json/_stringdata.go}"
  go run stringdata.go -i "$1" -name "$2" -pkg schema -o "$target"
}

stringdata aws_codecommit.schema.json AWSCodeCommitSchemaJSON
stringdata bitbucket_cloud.schema.json BitbucketCloudSchemaJSON
stringdata bitbucket_server.schema.json BitbucketServerSchemaJSON
stringdata campaign_spec.schema.json CampaignSpecSchemaJSON
stringdata changeset_spec.schema.json ChangesetSpecSchemaJSON
stringdata github.schema.json GitHubSchemaJSON
stringdata gitlab.schema.json GitLabSchemaJSON
stringdata gitolite.schema.json GitoliteSchemaJSON
stringdata other_external_service.schema.json OtherExternalServiceSchemaJSON
stringdata phabricator.schema.json PhabricatorSchemaJSON
stringdata settings.schema.json SettingsSchemaJSON
stringdata site.schema.json SiteSchemaJSON

gofmt -s -w ./*.go
