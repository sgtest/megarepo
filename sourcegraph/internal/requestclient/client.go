package requestclient

import (
	"context"
)

type clientKey struct{}

// Client carries information about the original client of a request.
type Client struct {
	// IP identifies the IP of the client.
	IP string
	// ForwardedFor identifies the originating IP address of a client.
	//
	// Note: This header can be spoofed and relies on trusted clients/proxies.
	// For sourcegraph.com we use cloudflare headers to avoid spoofing.
	ForwardedFor string
}

// FromContext retrieves the client IP, if available, from context.
func FromContext(ctx context.Context) *Client {
	ip, ok := ctx.Value(clientKey{}).(*Client)
	if !ok || ip == nil {
		return nil
	}
	return ip
}

// WithClient adds client IP information to context for propagation.
func WithClient(ctx context.Context, client *Client) context.Context {
	return context.WithValue(ctx, clientKey{}, client)
}
