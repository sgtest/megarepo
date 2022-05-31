package sources

//go:generate ../../../../dev/mockgen.sh github.com/sourcegraph/sourcegraph/enterprise/internal/batches/sources -i ChangesetSource -i SourcerStore -o mock_iface_test.go
//go:generate ../../../../dev/mockgen.sh github.com/sourcegraph/sourcegraph/internal/extsvc/bitbucketcloud -i Client --prefix BitbucketCloud -o mock_bitbucketcloud_client_test.go
