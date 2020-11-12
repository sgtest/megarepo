package uploadstore

import (
	"context"
	"fmt"
	"io"
	"strings"
	"sync"
	"time"

	"github.com/aws/aws-sdk-go/aws"
	"github.com/aws/aws-sdk-go/aws/awserr"
	"github.com/aws/aws-sdk-go/aws/credentials"
	"github.com/aws/aws-sdk-go/aws/session"
	"github.com/aws/aws-sdk-go/service/s3"
	"github.com/aws/aws-sdk-go/service/s3/s3manager"
	"github.com/inconshreveable/log15"
	"github.com/opentracing/opentracing-go/log"
	"github.com/pkg/errors"
	"github.com/sourcegraph/sourcegraph/internal/env"
	"github.com/sourcegraph/sourcegraph/internal/goroutine"
	"github.com/sourcegraph/sourcegraph/internal/observation"
)

type s3Store struct {
	bucket       string
	ttl          time.Duration
	manageBucket bool
	client       s3API
	uploader     s3Uploader
	operations   *operations
}

var _ Store = &s3Store{}

type S3Config struct {
	Region          string
	Endpoint        string
	AccessKeyID     string
	SecretAccessKey string
	SessionToken    string
}

func (c *S3Config) load(parent *env.BaseConfig) {
	c.Region = parent.Get("PRECISE_CODE_INTEL_UPLOAD_AWS_REGION", "us-east-1", "The target AWS region.")
	c.Endpoint = parent.Get("PRECISE_CODE_INTEL_UPLOAD_AWS_ENDPOINT", "http://minio:9000", "The target AWS endpoint.")
	c.AccessKeyID = parent.Get("PRECISE_CODE_INTEL_UPLOAD_AWS_ACCESS_KEY_ID", "AKIAIOSFODNN7EXAMPLE", "An AWS access key associated with a user with access to S3.")
	c.SecretAccessKey = parent.Get("PRECISE_CODE_INTEL_UPLOAD_AWS_SECRET_ACCESS_KEY", "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY", "An AWS secret key associated with a user with access to S3.")
	c.SessionToken = parent.GetOptional("PRECISE_CODE_INTEL_UPLOAD_AWS_SESSION_TOKEN", "An optional AWS session token associated with a user with access to S3.")
}

// newS3FromConfig creates a new store backed by AWS Simple Storage Service.
func newS3FromConfig(ctx context.Context, config *Config, operations *operations) (Store, error) {
	sess, err := session.NewSessionWithOptions(s3SessionOptions(config.Backend, config.S3))
	if err != nil {
		return nil, err
	}

	s3Client := s3.New(sess)
	api := &s3APIShim{s3Client}
	uploader := &s3UploaderShim{s3manager.NewUploaderWithClient(s3Client)}
	return newS3WithClients(api, uploader, config.Bucket, config.TTL, config.ManageBucket, operations), nil
}

func newS3WithClients(client s3API, uploader s3Uploader, bucket string, ttl time.Duration, manageBucket bool, operations *operations) *s3Store {
	return &s3Store{
		bucket:       bucket,
		ttl:          ttl,
		manageBucket: manageBucket,
		client:       client,
		uploader:     uploader,
		operations:   operations,
	}
}

func (s *s3Store) Init(ctx context.Context) error {
	if !s.manageBucket {
		return nil
	}

	if err := s.create(ctx); err != nil {
		return errors.Wrap(err, "failed to create bucket")
	}

	if err := s.update(ctx); err != nil {
		return errors.Wrap(err, "failed to update bucket attributes")
	}

	return nil
}

func (s *s3Store) Get(ctx context.Context, key string, skipBytes int64) (_ io.ReadCloser, err error) {
	ctx, endObservation := s.operations.get.With(ctx, &err, observation.Args{LogFields: []log.Field{
		log.String("key", key),
		log.Int64("skipBytes", skipBytes),
	}})
	defer endObservation(1, observation.Args{})

	var bytesRange *string
	if skipBytes > 0 {
		bytesRange = aws.String(fmt.Sprintf("bytes=%d-", skipBytes))
	}

	resp, err := s.client.GetObject(ctx, &s3.GetObjectInput{
		Bucket: aws.String(s.bucket),
		Key:    aws.String(key),
		Range:  bytesRange,
	})
	if err != nil {
		return nil, errors.Wrap(err, "failed to get object")
	}

	return resp.Body, nil
}

func (s *s3Store) Upload(ctx context.Context, key string, r io.Reader) (_ int64, err error) {
	ctx, endObservation := s.operations.upload.With(ctx, &err, observation.Args{LogFields: []log.Field{
		log.String("key", key),
	}})
	defer endObservation(1, observation.Args{})

	cr := &countingReader{r: r}

	if err := s.uploader.Upload(ctx, &s3manager.UploadInput{
		Bucket: aws.String(s.bucket),
		Key:    aws.String(key),
		Body:   cr,
	}); err != nil {
		return 0, errors.Wrap(err, "failed to upload object")
	}

	return int64(cr.n), nil
}

func (s *s3Store) Compose(ctx context.Context, destination string, sources ...string) (_ int64, err error) {
	ctx, endObservation := s.operations.compose.With(ctx, &err, observation.Args{LogFields: []log.Field{
		log.String("destination", destination),
		log.String("sources", strings.Join(sources, ", ")),
	}})
	defer endObservation(1, observation.Args{})

	multipartUpload, err := s.client.CreateMultipartUpload(ctx, &s3.CreateMultipartUploadInput{
		Bucket: aws.String(s.bucket),
		Key:    aws.String(destination),
	})
	if err != nil {
		return 0, errors.Wrap(err, "failed to create multipart upload")
	}

	defer func() {
		if err == nil {
			// Delete sources on success
			if err := s.deleteSources(ctx, *multipartUpload.Bucket, sources); err != nil {
				log15.Error("Failed to delete source objects", "error", err)
			}
		} else {
			// On failure, try to clean up copied then orphaned parts
			if _, err := s.client.AbortMultipartUpload(ctx, &s3.AbortMultipartUploadInput{
				Bucket:   multipartUpload.Bucket,
				Key:      multipartUpload.Key,
				UploadId: multipartUpload.UploadId,
			}); err != nil {
				log15.Error("Failed to abort multipart upload", "error", err)
			}
		}
	}()

	var m sync.Mutex
	etags := map[int]*string{}

	if err := goroutine.RunWorkersOverStrings(sources, func(index int, source string) error {
		partNumber := index + 1

		copyResult, err := s.client.UploadPartCopy(ctx, &s3.UploadPartCopyInput{
			Bucket:     multipartUpload.Bucket,
			Key:        multipartUpload.Key,
			UploadId:   multipartUpload.UploadId,
			PartNumber: aws.Int64(int64(partNumber)),
			CopySource: aws.String(fmt.Sprintf("%s/%s", s.bucket, source)),
		})
		if err != nil {
			return errors.Wrap(err, "failed to upload part")
		}

		m.Lock()
		etags[partNumber] = copyResult.CopyPartResult.ETag
		m.Unlock()

		return nil
	}); err != nil {
		return 0, err
	}

	var parts []*s3.CompletedPart
	for i := 0; i < len(sources); i++ {
		partNumber := i + 1

		parts = append(parts, &s3.CompletedPart{
			ETag:       etags[partNumber],
			PartNumber: aws.Int64(int64(partNumber)),
		})
	}

	if _, err := s.client.CompleteMultipartUpload(ctx, &s3.CompleteMultipartUploadInput{
		Bucket:          multipartUpload.Bucket,
		Key:             multipartUpload.Key,
		UploadId:        multipartUpload.UploadId,
		MultipartUpload: &s3.CompletedMultipartUpload{Parts: parts},
	}); err != nil {
		return 0, errors.Wrap(err, "failed to complete multipart upload")
	}

	obj, err := s.client.HeadObject(ctx, &s3.HeadObjectInput{
		Bucket: multipartUpload.Bucket,
		Key:    multipartUpload.Key,
	})
	if err != nil {
		return 0, errors.Wrap(err, "failed to stat composed object")
	}

	return *obj.ContentLength, nil
}

func (s *s3Store) Delete(ctx context.Context, key string) (err error) {
	ctx, endObservation := s.operations.delete.With(ctx, &err, observation.Args{LogFields: []log.Field{
		log.String("key", key),
	}})
	defer endObservation(1, observation.Args{})

	_, err = s.client.DeleteObject(ctx, &s3.DeleteObjectInput{
		Bucket: aws.String(s.bucket),
		Key:    aws.String(key),
	})

	return errors.Wrap(err, "failed to delete object")
}

func (s *s3Store) create(ctx context.Context) error {
	_, err := s.client.CreateBucket(ctx, &s3.CreateBucketInput{
		Bucket: aws.String(s.bucket),
	})

	codes := []string{
		s3.ErrCodeBucketAlreadyExists,
		s3.ErrCodeBucketAlreadyOwnedByYou,
	}

	for _, code := range codes {
		if aerr, ok := err.(awserr.Error); ok && aerr.Code() == code {
			return nil
		}
	}

	return err
}

func (s *s3Store) update(ctx context.Context) error {
	configureRequest := &s3.PutBucketLifecycleConfigurationInput{
		Bucket:                 aws.String(s.bucket),
		LifecycleConfiguration: s.lifecycle(),
	}

	_, err := s.client.PutBucketLifecycleConfiguration(ctx, configureRequest)
	return err
}

func (s *s3Store) lifecycle() *s3.BucketLifecycleConfiguration {
	days := aws.Int64(int64(s.ttl / (time.Hour * 24)))

	return &s3.BucketLifecycleConfiguration{
		Rules: []*s3.LifecycleRule{
			{
				ID:         aws.String("Expiration Rule"),
				Status:     aws.String("Enabled"),
				Filter:     &s3.LifecycleRuleFilter{Prefix: aws.String("")},
				Expiration: &s3.LifecycleExpiration{Days: days},
			},
			{
				ID:                             aws.String("Abort Incomplete Multipart Upload Rule"),
				Status:                         aws.String("Enabled"),
				Filter:                         &s3.LifecycleRuleFilter{Prefix: aws.String("")},
				AbortIncompleteMultipartUpload: &s3.AbortIncompleteMultipartUpload{DaysAfterInitiation: days},
			},
		},
	}
}

func (s *s3Store) deleteSources(ctx context.Context, bucket string, sources []string) error {
	return goroutine.RunWorkersOverStrings(sources, func(index int, source string) error {
		if _, err := s.client.DeleteObject(ctx, &s3.DeleteObjectInput{
			Bucket: aws.String(bucket),
			Key:    aws.String(source),
		}); err != nil {
			return errors.Wrap(err, "failed to delete source object")
		}

		return nil
	})
}

type countingReader struct {
	r io.Reader
	n int
}

func (r *countingReader) Read(p []byte) (n int, err error) {
	n, err = r.r.Read(p)
	r.n += n
	return n, err
}

func s3SessionOptions(backend string, config S3Config) session.Options {
	creds := credentials.NewStaticCredentials(
		config.AccessKeyID,
		config.SecretAccessKey,
		config.SessionToken,
	)

	awsConfig := aws.Config{
		Credentials: creds,
		Region:      aws.String(config.Region),
	}

	if backend == "minio" {
		awsConfig.Endpoint = aws.String(config.Endpoint)
		awsConfig.S3ForcePathStyle = aws.Bool(true)
	}

	return session.Options{Config: awsConfig}
}
