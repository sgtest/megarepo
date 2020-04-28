package mocks

//go:generate env GOBIN=$PWD/.bin GO111MODULE=on go install github.com/efritz/go-mockgen
//go:generate $PWD/.bin/go-mockgen -f github.com/sourcegraph/sourcegraph/internal/codeintel/bundles/client -i BundleManagerClient -o mock_bundle_manager_client.go
//go:generate $PWD/.bin/go-mockgen -f github.com/sourcegraph/sourcegraph/internal/codeintel/bundles/client -i BundleClient -o mock_bundle_client.go
//go:generate $PWD/.bin/go-mockgen -f github.com/sourcegraph/sourcegraph/internal/codeintel/bundles/reader -i Reader -o mock_bundle_reader.go
//go:generate $PWD/.bin/go-mockgen -f github.com/sourcegraph/sourcegraph/internal/codeintel/bundles/writer -i Writer -o mock_bundle_writer.go
//go:generate $PWD/.bin/go-mockgen -f github.com/sourcegraph/sourcegraph/internal/codeintel/bundles/serializer -i Serializer -o mock_bundle_serializer.go
