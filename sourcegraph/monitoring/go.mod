module github.com/sourcegraph/sourcegraph/monitoring

go 1.19

require (
	github.com/grafana-tools/sdk v0.0.0-20220919052116-6562121319fc
	github.com/grafana/regexp v0.0.0-20221123153739-15dc172cd2db
	github.com/hashicorp/hcl v1.0.0
	github.com/prometheus/common v0.37.0
	github.com/prometheus/prometheus v0.40.5
	github.com/sourcegraph/log v0.0.0-20221206163500-7d93c6ad7037
	github.com/sourcegraph/sourcegraph/lib v0.0.0-20230613175844-f031949c72f5
	github.com/stretchr/testify v1.8.2
	github.com/urfave/cli/v2 v2.23.7
	golang.org/x/text v0.9.0
	gopkg.in/yaml.v2 v2.4.0
)

replace (
	// Pending a release cut of https://github.com/prometheus/alertmanager/pull/3010
	github.com/prometheus/common => github.com/prometheus/common v0.32.1
	// When developing, this replace uses the local package instead of a pushed version.
	github.com/sourcegraph/sourcegraph/lib => ../lib
)

require (
	github.com/benbjohnson/clock v1.3.0 // indirect
	github.com/beorn7/perks v1.0.1 // indirect
	github.com/cespare/xxhash/v2 v2.1.2 // indirect
	github.com/cockroachdb/errors v1.9.1 // indirect
	github.com/cockroachdb/logtags v0.0.0-20230118201751-21c54148d20b // indirect
	github.com/cockroachdb/redact v1.1.4 // indirect
	github.com/cpuguy83/go-md2man/v2 v2.0.2 // indirect
	github.com/davecgh/go-spew v1.1.1 // indirect
	github.com/dennwc/varint v1.0.0 // indirect
	github.com/fatih/color v1.13.0 // indirect
	github.com/getsentry/sentry-go v0.21.0 // indirect
	github.com/go-kit/log v0.2.1 // indirect
	github.com/go-logfmt/logfmt v0.5.1 // indirect
	github.com/gobwas/ws v1.1.0 // indirect
	github.com/gogo/protobuf v1.3.2 // indirect
	github.com/golang/protobuf v1.5.2 // indirect
	github.com/google/go-cmp v0.5.9 // indirect
	github.com/google/uuid v1.3.0 // indirect
	github.com/gosimple/slug v1.12.0 // indirect
	github.com/gosimple/unidecode v1.0.1 // indirect
	github.com/jackc/chunkreader/v2 v2.0.1 // indirect
	github.com/jackc/pgconn v1.10.1 // indirect
	github.com/jackc/pgio v1.0.0 // indirect
	github.com/jackc/pgpassfile v1.0.0 // indirect
	github.com/jackc/pgproto3/v2 v2.1.1 // indirect
	github.com/jackc/pgservicefile v0.0.0-20200714003250-2b9c44734f2b // indirect
	github.com/kr/pretty v0.3.1 // indirect
	github.com/kr/text v0.2.0 // indirect
	github.com/mattn/go-colorable v0.1.13 // indirect
	github.com/mattn/go-isatty v0.0.17 // indirect
	github.com/matttproud/golang_protobuf_extensions v1.0.4 // indirect
	github.com/pkg/errors v0.9.1 // indirect
	github.com/pmezard/go-difflib v1.0.0 // indirect
	github.com/prometheus/client_golang v1.14.0 // indirect
	github.com/prometheus/client_model v0.3.0 // indirect
	github.com/prometheus/procfs v0.8.0 // indirect
	github.com/rogpeppe/go-internal v1.10.0 // indirect
	github.com/russross/blackfriday/v2 v2.1.0 // indirect
	github.com/xrash/smetrics v0.0.0-20201216005158-039620a65673 // indirect
	go.uber.org/atomic v1.10.0 // indirect
	go.uber.org/goleak v1.2.0 // indirect
	go.uber.org/multierr v1.8.0 // indirect
	go.uber.org/zap v1.24.0 // indirect
	golang.org/x/crypto v0.7.0 // indirect
	golang.org/x/exp v0.0.0-20230425010034-47ecfdc1ba53 // indirect
	golang.org/x/oauth2 v0.2.0 // indirect
	golang.org/x/sys v0.8.0 // indirect
	google.golang.org/protobuf v1.29.1 // indirect
	gopkg.in/yaml.v3 v3.0.1 // indirect
)
