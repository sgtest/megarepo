package commitgraph

//go:generate ../../../../../dev/mockgen.sh github.com/sourcegraph/sourcegraph/internal/codeintel/uploads/background/commitgraph -i DBStore -i Locker -i GitserverClient -o mock_iface_test.go
