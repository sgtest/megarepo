package repos

import (
	"context"
	"crypto/hmac"
	"crypto/sha256"
	"fmt"
	"net/http"
	"net/url"
	"reflect"
	"sync"
	"time"

	"github.com/aws/aws-sdk-go-v2/aws"
	"github.com/aws/aws-sdk-go-v2/aws/defaults"
	"github.com/aws/aws-sdk-go-v2/aws/endpoints"
	"github.com/aws/aws-sdk-go-v2/service/sts"
	multierror "github.com/hashicorp/go-multierror"
	"github.com/pkg/errors"
	"github.com/sourcegraph/sourcegraph/pkg/api"
	"github.com/sourcegraph/sourcegraph/pkg/atomicvalue"
	"github.com/sourcegraph/sourcegraph/pkg/conf"
	"github.com/sourcegraph/sourcegraph/pkg/conf/reposource"
	"github.com/sourcegraph/sourcegraph/pkg/extsvc/awscodecommit"
	"github.com/sourcegraph/sourcegraph/pkg/httpcli"
	"github.com/sourcegraph/sourcegraph/pkg/repoupdater/protocol"
	"github.com/sourcegraph/sourcegraph/schema"
	log15 "gopkg.in/inconshreveable/log15.v2"
)

var awsCodeCommitConnections = func() *atomicvalue.Value {
	c := atomicvalue.New()
	c.Set(func() interface{} {
		return []*awsCodeCommitConnection{}
	})
	return c
}()

// SyncAWSCodeCommitConnections periodically syncs connections from
// the Frontend API.
func SyncAWSCodeCommitConnections(ctx context.Context) {
	t := time.NewTicker(configWatchInterval)
	var lastConfig []*schema.AWSCodeCommitConnection
	for range t.C {
		config, err := conf.AWSCodeCommitConfigs(ctx)
		if err != nil {
			log15.Error("unable to fetch AWS CodeCommit configs", "err", err)
			continue
		}

		if reflect.DeepEqual(config, lastConfig) {
			continue
		}
		lastConfig = config

		var conns []*awsCodeCommitConnection
		for _, c := range config {
			conn, err := newAWSCodeCommitConnection(c, nil)
			if err != nil {
				log15.Error("Error processing configured AWS CodeCommit connection. Skipping it.", "region", c.Region, "error", err)
				continue
			}
			conns = append(conns, conn)
		}

		awsCodeCommitConnections.Set(func() interface{} {
			return conns
		})

		awsCodeCommitRepositorySyncWorker.restart()
	}
}

// GetAWSCodeCommitRepositoryMock is set by tests that need to mock GetAWSCodeCommitRepository.
var GetAWSCodeCommitRepositoryMock func(args protocol.RepoLookupArgs) (repo *protocol.RepoInfo, authoritative bool, err error)

// GetAWSCodeCommitRepository queries a configured AWS CodeCommit connection endpoint for
// information about the specified repository.
//
// If args.Repo refers to a repository that is not known to be on a configured AWS CodeCommit
// connection's host, it returns authoritative == false.
func GetAWSCodeCommitRepository(ctx context.Context, args protocol.RepoLookupArgs) (repo *protocol.RepoInfo, authoritative bool, err error) {
	if GetAWSCodeCommitRepositoryMock != nil {
		return GetAWSCodeCommitRepositoryMock(args)
	}

	log15.Debug("GetAWSCodeCommitRepository", "repo", args.Repo, "externalRepo", args.ExternalRepo)

	if args.ExternalRepo != nil && args.ExternalRepo.ServiceType == awscodecommit.ServiceType {
		// Look up by external repository spec.
		var err error
		for _, conn := range awsCodeCommitConnections.Get().([]*awsCodeCommitConnection) {
			var serviceID string
			serviceID, err = conn.getServiceID()
			if serviceID != "" && args.ExternalRepo.ServiceID == serviceID {
				ccrepo, err := conn.client.GetRepository(ctx, args.ExternalRepo.ID)
				if ccrepo != nil {
					remoteURL := conn.authenticatedRemoteURL(ccrepo)
					webURL := fmt.Sprintf("https://%s.console.aws.amazon.com/codecommit/home#/repository/%s", conn.awsRegion.ID(), ccrepo.Name)
					repo = &protocol.RepoInfo{
						Name:         awsCodeCommitRepositoryToRepoPath(conn, ccrepo),
						ExternalRepo: awscodecommit.ExternalRepoSpec(ccrepo, serviceID),
						Description:  ccrepo.Description,
						VCS:          protocol.VCSInfo{URL: remoteURL},
						Links: &protocol.RepoLinks{
							Root:   webURL,
							Tree:   webURL + "/browse/{rev}/--/{path}",
							Blob:   webURL + "/browse/{rev}/--/{path}",
							Commit: webURL + "/commit/{commit}",
						},
					}
				}
				return repo, true, errors.Wrap(err, "GetRepository")
			}
		}
		return nil, true, errors.Wrap(err, "getServiceID")
	}

	// Unlike other code hosts (e.g., GitHub and GitLab), looking up by repository name is not
	// supported because it's far less likely to be useful for AWS CodeCommit, which usually has a
	// more limited universe of repositories.
	return nil, false, nil
}

var awsCodeCommitRepositorySyncWorker = &worker{
	work: func(ctx context.Context, shutdown chan struct{}) {
		awsCodeCommitConnections := awsCodeCommitConnections.Get().([]*awsCodeCommitConnection)
		if len(awsCodeCommitConnections) == 0 {
			return
		}
		for _, c := range awsCodeCommitConnections {
			go func(c *awsCodeCommitConnection) {
				// Hit the AWS API to determine our account ID (which is a fixed value but not derivable
				// from the values in the Sourcegraph site config). Be robust to the API being
				// unreachable when we start up.
				const retryInterval = 20 * time.Second
				for {
					_, err := c.tryPopulateAWSAccountID()
					if err == nil {
						break
					}
					log15.Error("Unable to reach AWS CodeCommit API to determine AWS account ID.", "region", c.config.Region, "error", err, "retryInterval", retryInterval)
					select {
					case <-shutdown:
						return
					case <-time.After(retryInterval):
					}
				}

				for {
					updateAWSCodeCommitRepositories(ctx, c)
					awsCodeCommitUpdateTime.WithLabelValues(c.awsAccountID).Set(float64(time.Now().Unix()))
					select {
					case <-shutdown:
						return
					case <-time.After(GetUpdateInterval()):
					}
				}
			}(c)
		}
	},
}

// RunAWSCodeCommitRepositorySyncWorker runs the worker that syncs repositories from the configured AWSCodeCommit and AWSCodeCommit
// Enterprise instances to Sourcegraph.
func RunAWSCodeCommitRepositorySyncWorker(ctx context.Context) {
	awsCodeCommitRepositorySyncWorker.start(ctx)
}

func awsCodeCommitRepositoryToRepoPath(conn *awsCodeCommitConnection, repo *awscodecommit.Repository) api.RepoName {
	return reposource.AWSRepoName(conn.config.RepositoryPathPattern, repo.Name)
}

// updateAWSCodeCommitRepositories ensures that all provided repositories have been added and updated on Sourcegraph.
func updateAWSCodeCommitRepositories(ctx context.Context, conn *awsCodeCommitConnection) {
	repos, err := conn.listAllRepositories(ctx)
	if err != nil {
		log15.Error("failed to list some AWS CodeCommit repos", "error", err.Error())
	}

	repoChan := make(chan repoCreateOrUpdateRequest)
	defer close(repoChan)
	go createEnableUpdateRepos(ctx, fmt.Sprintf("aws:%s", conn.config.AccessKeyID), repoChan)
	for _, repo := range repos {
		// log15.Debug("awscodecommit sync: create/enable/update repo", "repo", repo.Name)
		remoteURL := conn.authenticatedRemoteURL(repo)
		repoChan <- repoCreateOrUpdateRequest{
			RepoCreateOrUpdateRequest: api.RepoCreateOrUpdateRequest{
				RepoName:     awsCodeCommitRepositoryToRepoPath(conn, repo),
				ExternalRepo: awscodecommit.ExternalRepoSpec(repo, awscodecommit.ServiceID(conn.awsPartition, conn.awsRegion, repo.AccountID)),
				Description:  repo.Description,
				Enabled:      conn.config.InitialRepositoryEnablement,
			},
			URL: remoteURL,
		}
	}
}

func newAWSCodeCommitConnection(config *schema.AWSCodeCommitConnection, cf *httpcli.Factory) (*awsCodeCommitConnection, error) {
	awsConfig := defaults.Config()
	awsConfig.Region = config.Region
	awsConfig.Credentials = aws.StaticCredentialsProvider{
		Value: aws.Credentials{
			AccessKeyID:     config.AccessKeyID,
			SecretAccessKey: config.SecretAccessKey,
			Source:          "sourcegraph-site-configuration",
		},
	}

	if cf == nil {
		cf = NewHTTPClientFactory()
	}

	cli, err := cf.Client(func(c *http.Client) error {
		*c = *awsConfig.HTTPClient
		return nil
	})
	if err != nil {
		return nil, err
	}
	awsConfig.HTTPClient = cli

	exclude := make(map[string]bool, len(config.Exclude))
	for _, r := range config.Exclude {
		if r.Name != "" {
			exclude[r.Name] = true
		}

		if r.Id != "" {
			exclude[r.Id] = true
		}
	}

	conn := &awsCodeCommitConnection{
		config:    config,
		awsConfig: awsConfig,
		exclude:   exclude,
	}
	conn.client = awscodecommit.NewClient(conn.awsConfig)

	var ok bool
	conn.awsPartition, ok = endpoints.DefaultPartitions().ForRegion(config.Region)
	if ok {
		conn.awsRegion, ok = conn.awsPartition.Regions()[config.Region]
	}
	if !ok {
		return nil, fmt.Errorf("unrecognized AWS region name: %q", config.Region)
	}

	return conn, nil
}

type awsCodeCommitConnection struct {
	config       *schema.AWSCodeCommitConnection
	awsConfig    aws.Config
	awsPartition endpoints.Partition // "aws", "aws-cn", "aws-us-gov"
	awsRegion    endpoints.Region
	client       *awscodecommit.Client

	mu           sync.Mutex
	awsAccountID string

	exclude map[string]bool
}

func (c *awsCodeCommitConnection) getServiceID() (string, error) {
	awsAccountID, err := c.tryPopulateAWSAccountID()
	if err != nil {
		return "", err
	}
	if awsAccountID == "" {
		return "", nil
	}
	return awscodecommit.ServiceID(c.awsPartition, c.awsRegion, c.awsAccountID), nil
}

func (c *awsCodeCommitConnection) tryPopulateAWSAccountID() (string, error) {
	c.mu.Lock()
	awsAccountID := c.awsAccountID
	c.mu.Unlock()
	if awsAccountID != "" {
		return awsAccountID, nil
	}

	result, err := sts.New(c.awsConfig).GetCallerIdentityRequest(&sts.GetCallerIdentityInput{}).Send()
	if err != nil {
		return "", err
	}
	if result.Account == nil {
		return "", errors.New("AWS STS GetCallerIdentity unexpectedly returned empty AWS account ID")
	}

	c.mu.Lock()
	defer c.mu.Unlock()
	c.awsAccountID = *result.Account
	return c.awsAccountID, nil
}

// authenticatedRemoteURL returns the repository's Git remote URL with the
// configured AWS CodeCommit Git credentials inserted in the URL userinfo, for
// repositories needing authentication.
func (c *awsCodeCommitConnection) authenticatedRemoteURL(repo *awscodecommit.Repository) string {
	u, err := url.Parse(repo.HTTPCloneURL)
	if err != nil {
		log15.Warn("Error adding authentication to AWS CodeCommit repository Git remote URL.", "url", repo.HTTPCloneURL, "error", err)
		return repo.HTTPCloneURL
	}

	username := c.config.GitCredentials.Username
	password := c.config.GitCredentials.Password

	u.User = url.UserPassword(username, password)
	return u.String()
}

func makeHMAC(key []byte, data []byte) []byte {
	hash := hmac.New(sha256.New, key)
	_, _ = hash.Write(data)
	return hash.Sum(nil)
}

func makeSHA256(data []byte) []byte {
	hash := sha256.New()
	_, _ = hash.Write(data)
	return hash.Sum(nil)
}

func (c *awsCodeCommitConnection) listAllRepositories(ctx context.Context) ([]*awscodecommit.Repository, error) {
	repos := []*awscodecommit.Repository{}
	errs := new(multierror.Error)

	var nextToken string
	for {
		batch, token, err := c.client.ListRepositories(ctx, nextToken)
		if err != nil {
			errs = multierror.Append(errs, err)
			break
		}

		for _, r := range batch {
			if !c.excludes(r) {
				repos = append(repos, r)
			}
		}

		if len(batch) == 0 || token == "" {
			break // last page
		}

		nextToken = token
	}

	return repos, errs.ErrorOrNil()
}

func (c *awsCodeCommitConnection) excludes(r *awscodecommit.Repository) bool {
	return c.exclude[r.Name] || c.exclude[r.ID]
}
