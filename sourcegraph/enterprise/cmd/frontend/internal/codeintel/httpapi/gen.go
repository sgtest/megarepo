package httpapi

//go:generate ../../../../../../dev/mockgen.sh github.com/sourcegraph/sourcegraph/enterprise/cmd/frontend/internal/codeintel/httpapi -i DBStore -i GitHubClient -o mock_iface_test.go
