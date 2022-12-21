package webhooks

import (
	"bytes"
	"context"
	"crypto/hmac"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"testing"

	gh "github.com/google/go-github/v43/github"
	"github.com/sourcegraph/log/logtest"
	"github.com/stretchr/testify/assert"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/external/globals"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/webhooks"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/dbtest"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/httpcli"
	"github.com/sourcegraph/sourcegraph/internal/repos"
	"github.com/sourcegraph/sourcegraph/internal/repoupdater"
	"github.com/sourcegraph/sourcegraph/internal/repoupdater/protocol"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/lib/errors"
	"github.com/sourcegraph/sourcegraph/schema"
)

func TestGitHubWebhookHandle(t *testing.T) {
	ctx := context.Background()
	logger := logtest.Scoped(t)

	db := database.NewDB(logger, dbtest.NewDB(logger, t))
	store := repos.NewStore(logger, db)
	repoStore := store.RepoStore()
	esStore := store.ExternalServiceStore()

	repo := &types.Repo{
		ID:   1,
		Name: "ghe.sgdev.org/milton/test",
	}
	if err := repoStore.Create(ctx, repo); err != nil {
		t.Fatal(err)
	}

	conn := schema.GitHubConnection{
		Url:      "https://github.com",
		Token:    "token",
		Repos:    []string{"owner/name"},
		Webhooks: []*schema.GitHubWebhook{{Org: "ghe.sgdev.org", Secret: "secret"}},
	}

	config, err := json.Marshal(conn)
	if err != nil {
		t.Fatal(err)
	}

	svc := &types.ExternalService{
		Kind:        extsvc.KindGitHub,
		DisplayName: "TestService",
		Config:      extsvc.NewUnencryptedConfig(string(config)),
	}
	if err := esStore.Upsert(ctx, svc); err != nil {
		t.Fatal(err)
	}

	handler := NewGitHubWebhookHandler()
	router := &webhooks.GitHubWebhook{
		WebhookRouter: &webhooks.WebhookRouter{
			DB: db,
		},
	}
	handler.Register(router.WebhookRouter)

	mux := http.NewServeMux()
	mux.HandleFunc("/enqueue-repo-update", func(w http.ResponseWriter, r *http.Request) {
		reqBody, err := io.ReadAll(r.Body)
		if err != nil {
			http.Error(w, err.Error(), http.StatusBadRequest)
		}

		var req protocol.RepoUpdateRequest
		if err := json.Unmarshal(reqBody, &req); err != nil {
			http.Error(w, err.Error(), http.StatusBadRequest)
		}

		repos, err := repoStore.List(ctx, database.ReposListOptions{Names: []string{string(req.Repo)}})
		if err != nil {
			http.Error(w, err.Error(), http.StatusNotFound)
		}
		if len(repos) != 1 {
			http.Error(w, fmt.Sprintf("expected 1 repo, got %v", len(repos)), http.StatusNotFound)
		}

		repo := repos[0]
		res := &protocol.RepoUpdateResponse{
			ID:   repo.ID,
			Name: string(repo.Name),
		}

		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusOK)
		json.NewEncoder(w).Encode(res)
	})

	server := httptest.NewServer(mux)
	defer server.Close()

	cf := httpcli.NewExternalClientFactory()
	opts := []httpcli.Opt{}
	doer, err := cf.Doer(opts...)
	if err != nil {
		t.Fatal(err)
	}

	repoupdater.DefaultClient = &repoupdater.Client{
		URL:        server.URL,
		HTTPClient: doer,
	}

	payload, err := os.ReadFile(filepath.Join("testdata", "github-push.json"))
	if err != nil {
		t.Fatal(err)
	}

	targetURL := fmt.Sprintf("%s/github-webhooks", globals.ExternalURL())
	req, err := http.NewRequest("POST", targetURL, bytes.NewReader(payload))
	if err != nil {
		t.Fatal(err)
	}
	req.Header.Set("X-GitHub-Event", "push")
	req.Header.Set("X-Hub-Signature", sign(t, payload, []byte("secret")))

	rec := httptest.NewRecorder()
	router.ServeHTTP(rec, req)
	resp := rec.Result()

	if resp.StatusCode != http.StatusOK {
		t.Fatalf("expected status code: 200, got %v", resp.StatusCode)
	}
}

func sign(t *testing.T, message, secret []byte) string {
	t.Helper()

	mac := hmac.New(sha256.New, secret)
	_, err := mac.Write(message)
	if err != nil {
		t.Fatalf("writing hmac message failed: %s", err)
	}

	return "sha256=" + hex.EncodeToString(mac.Sum(nil))
}

func TestGithubNameFromEvent(t *testing.T) {
	tests := []struct {
		name    string
		event   *gh.PushEvent
		want    api.RepoName
		wantErr error
	}{
		{
			name: "valid event",
			event: &gh.PushEvent{
				Repo: &gh.PushEventRepository{
					URL: stringp("https://github.com/sourcegraph/sourcegraph"),
				},
			},
			want: api.RepoName("github.com/sourcegraph/sourcegraph"),
		},
		{
			name:    "nil event",
			event:   nil,
			want:    api.RepoName(""),
			wantErr: errors.New("URL for repository not found"),
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := githubNameFromEvent(tt.event)
			if tt.wantErr != nil {
				assert.EqualError(t, tt.wantErr, err.Error())
				return
			}
			assert.Equal(t, tt.want, got)
		})
	}
}

func stringp(s string) *string {
	return &s
}
