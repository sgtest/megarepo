package historian

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strconv"
	"time"

	"github.com/grafana/grafana/pkg/infra/log"
	"github.com/grafana/grafana/pkg/services/ngalert/client"
	"github.com/grafana/grafana/pkg/services/ngalert/metrics"
	"github.com/grafana/grafana/pkg/setting"
)

const defaultPageSize = 1000
const maximumPageSize = 5000

func NewRequester() client.Requester {
	return &http.Client{}
}

// encoder serializes log streams to some byte format.
type encoder interface {
	// encode serializes a set of log streams to bytes.
	encode(s []Stream) ([]byte, error)
	// headers returns a set of HTTP-style headers that describes the encoding scheme used.
	headers() map[string]string
}

type LokiConfig struct {
	ReadPathURL       *url.URL
	WritePathURL      *url.URL
	BasicAuthUser     string
	BasicAuthPassword string
	TenantID          string
	ExternalLabels    map[string]string
	Encoder           encoder
}

func NewLokiConfig(cfg setting.UnifiedAlertingStateHistorySettings) (LokiConfig, error) {
	read, write := cfg.LokiReadURL, cfg.LokiWriteURL
	if read == "" {
		read = cfg.LokiRemoteURL
	}
	if write == "" {
		write = cfg.LokiRemoteURL
	}

	if read == "" {
		return LokiConfig{}, fmt.Errorf("either read path URL or remote Loki URL must be provided")
	}
	if write == "" {
		return LokiConfig{}, fmt.Errorf("either write path URL or remote Loki URL must be provided")
	}

	readURL, err := url.Parse(read)
	if err != nil {
		return LokiConfig{}, fmt.Errorf("failed to parse loki remote read URL: %w", err)
	}
	writeURL, err := url.Parse(write)
	if err != nil {
		return LokiConfig{}, fmt.Errorf("failed to parse loki remote write URL: %w", err)
	}

	return LokiConfig{
		ReadPathURL:       readURL,
		WritePathURL:      writeURL,
		BasicAuthUser:     cfg.LokiBasicAuthUsername,
		BasicAuthPassword: cfg.LokiBasicAuthPassword,
		TenantID:          cfg.LokiTenantID,
		ExternalLabels:    cfg.ExternalLabels,
		// Snappy-compressed protobuf is the default, same goes for Promtail.
		Encoder: SnappyProtoEncoder{},
	}, nil
}

type HttpLokiClient struct {
	client  client.Requester
	encoder encoder
	cfg     LokiConfig
	metrics *metrics.Historian
	log     log.Logger
}

// Kind of Operation (=, !=, =~, !~)
type Operator string

const (
	// Equal operator (=)
	Eq Operator = "="
	// Not Equal operator (!=)
	Neq Operator = "!="
	// Equal operator supporting RegEx (=~)
	EqRegEx Operator = "=~"
	// Not Equal operator supporting RegEx (!~)
	NeqRegEx Operator = "!~"
)

func NewLokiClient(cfg LokiConfig, req client.Requester, metrics *metrics.Historian, logger log.Logger) *HttpLokiClient {
	tc := client.NewTimedClient(req, metrics.WriteDuration)
	return &HttpLokiClient{
		client:  tc,
		encoder: cfg.Encoder,
		cfg:     cfg,
		metrics: metrics,
		log:     logger.New("protocol", "http"),
	}
}

func (c *HttpLokiClient) Ping(ctx context.Context) error {
	uri := c.cfg.ReadPathURL.JoinPath("/loki/api/v1/labels")
	req, err := http.NewRequest(http.MethodGet, uri.String(), nil)
	if err != nil {
		return fmt.Errorf("error creating request: %w", err)
	}
	c.setAuthAndTenantHeaders(req)

	req = req.WithContext(ctx)
	res, err := c.client.Do(req)
	if res != nil {
		defer func() {
			if err := res.Body.Close(); err != nil {
				c.log.Warn("Failed to close response body", "err", err)
			}
		}()
	}
	if err != nil {
		return fmt.Errorf("error sending request: %w", err)
	}

	if res.StatusCode < 200 || res.StatusCode >= 300 {
		return fmt.Errorf("ping request to loki endpoint returned a non-200 status code: %d", res.StatusCode)
	}
	c.log.Debug("Ping request to Loki endpoint succeeded", "status", res.StatusCode)
	return nil
}

type Stream struct {
	Stream map[string]string `json:"stream"`
	Values []Sample          `json:"values"`
}

type Sample struct {
	T time.Time
	V string
}

func (r *Sample) MarshalJSON() ([]byte, error) {
	return json.Marshal([2]string{
		fmt.Sprintf("%d", r.T.UnixNano()), r.V,
	})
}

func (r *Sample) UnmarshalJSON(b []byte) error {
	// A Loki stream sample is formatted like a list with two elements, [At, Val]
	// At is a string wrapping a timestamp, in nanosecond unix epoch.
	// Val is a string containing the log line.
	var tuple [2]string
	if err := json.Unmarshal(b, &tuple); err != nil {
		return fmt.Errorf("failed to deserialize sample in Loki response: %w", err)
	}
	nano, err := strconv.ParseInt(tuple[0], 10, 64)
	if err != nil {
		return fmt.Errorf("timestamp in Loki sample not convertible to nanosecond epoch: %v", tuple[0])
	}
	r.T = time.Unix(0, nano)
	r.V = tuple[1]
	return nil
}

func (c *HttpLokiClient) Push(ctx context.Context, s []Stream) error {
	enc, err := c.encoder.encode(s)
	if err != nil {
		return err
	}

	uri := c.cfg.WritePathURL.JoinPath("/loki/api/v1/push")
	req, err := http.NewRequest(http.MethodPost, uri.String(), bytes.NewBuffer(enc))
	if err != nil {
		return fmt.Errorf("failed to create Loki request: %w", err)
	}

	c.setAuthAndTenantHeaders(req)
	for k, v := range c.encoder.headers() {
		req.Header.Add(k, v)
	}

	c.metrics.BytesWritten.Add(float64(len(enc)))
	req = req.WithContext(ctx)
	resp, err := c.client.Do(req)
	if resp != nil {
		defer func() {
			if err := resp.Body.Close(); err != nil {
				c.log.Warn("Failed to close response body", "err", err)
			}
		}()
	}
	if err != nil {
		return fmt.Errorf("failed to send request: %w", err)
	}

	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		byt, _ := io.ReadAll(resp.Body)
		if len(byt) > 0 {
			c.log.Error("Error response from Loki", "response", string(byt), "status", resp.StatusCode)
		} else {
			c.log.Error("Error response from Loki with an empty body", "status", resp.StatusCode)
		}
		return fmt.Errorf("received a non-200 response from loki, status: %d", resp.StatusCode)
	}
	return nil
}

func (c *HttpLokiClient) setAuthAndTenantHeaders(req *http.Request) {
	if c.cfg.BasicAuthUser != "" || c.cfg.BasicAuthPassword != "" {
		req.SetBasicAuth(c.cfg.BasicAuthUser, c.cfg.BasicAuthPassword)
	}

	if c.cfg.TenantID != "" {
		req.Header.Add("X-Scope-OrgID", c.cfg.TenantID)
	}
}

func (c *HttpLokiClient) RangeQuery(ctx context.Context, logQL string, start, end, limit int64) (QueryRes, error) {
	// Run the pre-flight checks for the query.
	if start > end {
		return QueryRes{}, fmt.Errorf("start time cannot be after end time")
	}
	if limit < 1 {
		limit = defaultPageSize
	}
	if limit > maximumPageSize {
		limit = maximumPageSize
	}

	queryURL := c.cfg.ReadPathURL.JoinPath("/loki/api/v1/query_range")

	values := url.Values{}
	values.Set("query", logQL)
	values.Set("start", fmt.Sprintf("%d", start))
	values.Set("end", fmt.Sprintf("%d", end))
	values.Set("limit", fmt.Sprintf("%d", limit))

	queryURL.RawQuery = values.Encode()

	req, err := http.NewRequest(http.MethodGet,
		queryURL.String(), nil)
	if err != nil {
		return QueryRes{}, fmt.Errorf("error creating request: %w", err)
	}

	req = req.WithContext(ctx)
	c.setAuthAndTenantHeaders(req)

	res, err := c.client.Do(req)
	if err != nil {
		return QueryRes{}, fmt.Errorf("error executing request: %w", err)
	}

	defer func() {
		_ = res.Body.Close()
	}()

	data, err := io.ReadAll(res.Body)
	if err != nil {
		return QueryRes{}, fmt.Errorf("error reading request response: %w", err)
	}

	if res.StatusCode < 200 || res.StatusCode >= 300 {
		if len(data) > 0 {
			c.log.Error("Error response from Loki", "response", string(data), "status", res.StatusCode)
		} else {
			c.log.Error("Error response from Loki with an empty body", "status", res.StatusCode)
		}
		return QueryRes{}, fmt.Errorf("received a non-200 response from loki, status: %d", res.StatusCode)
	}

	result := QueryRes{}
	err = json.Unmarshal(data, &result)
	if err != nil {
		fmt.Println(string(data))
		return QueryRes{}, fmt.Errorf("error parsing request response: %w", err)
	}

	return result, nil
}

type QueryRes struct {
	Data QueryData `json:"data"`
}

type QueryData struct {
	Result []Stream `json:"result"`
}
