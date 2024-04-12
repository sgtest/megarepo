package inventory

import (
	"context"
	"io"
	"io/fs"
)

// Context defines the environment in which the inventory is computed.
type Context struct {
	// ReadTree is called to list the immediate children of a tree at path. The returned fs.FileInfo
	// values' Name method must return the full path (that can be passed to another ReadTree or
	// ReadFile call), not just the basename.
	ReadTree func(ctx context.Context, path string) ([]fs.FileInfo, error)

	// NewFileReader is called to get an io.ReadCloser from the file at path.
	NewFileReader func(ctx context.Context, path string) (io.ReadCloser, error)

	// CacheGet, if set, returns the cached inventory and true for the given tree, or false for a cache miss.
	CacheGet func(fs.FileInfo) (Inventory, bool)

	// CacheSet, if set, stores the inventory in the cache for the given tree.
	CacheSet func(fs.FileInfo, Inventory)
}
