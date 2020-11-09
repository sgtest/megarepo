package api

//go:generate env GOBIN=$PWD/.bin GO111MODULE=on go install github.com/efritz/go-mockgen
//go:generate $PWD/.bin/go-mockgen -f github.com/sourcegraph/sourcegraph/enterprise/cmd/frontend/internal/codeintel/api -i DBStore -i ReferencePager -i LSIFStore -i GitserverClient -o mock_iface_test.go
