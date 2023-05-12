package types

import "time"

// GitHubApp represents a GitHub App.
type GitHubApp struct {
	ID            int
	AppID         int
	Name          string
	Slug          string
	BaseURL       string
	AppURL        string
	ClientID      string
	ClientSecret  string
	WebhookSecret string
	WebhookID     *int
	PrivateKey    string
	EncryptionKey string
	Logo          string
	CreatedAt     time.Time
	UpdatedAt     time.Time
}
