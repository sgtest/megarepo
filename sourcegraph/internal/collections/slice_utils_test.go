package collections

import (
	"math"
	"testing"

	"github.com/google/go-cmp/cmp"
	"github.com/stretchr/testify/require"
)

func Test_Min(t *testing.T) {
	t.Run("Returns first int that is smaller", func(t *testing.T) {
		got := Min(1, 2)
		want := 1
		if got != want {
			t.Errorf("got %v, want %v", got, want)
		}
	})

	t.Run("Returns second int that is smaller", func(t *testing.T) {
		got := Min(2, 1)
		want := 1
		if got != want {
			t.Errorf("got %v, want %v", got, want)
		}
	})

	t.Run("Works with a float as well", func(t *testing.T) {
		got := Min(1.5, 1.52)
		want := 1.5
		if got != want {
			t.Errorf("got %v, want %v", got, want)
		}
	})

	t.Run("Works with infinity", func(t *testing.T) {
		got := Min(1.5, math.Inf(1))
		want := 1.5
		if got != want {
			t.Errorf("got %v, want %v", got, want)
		}
	})

	t.Run("Works with negative infinity", func(t *testing.T) {
		got := Min(1.5, math.Inf(-1))
		want := math.Inf(-1)
		if got != want {
			t.Errorf("got %v, want %v", got, want)
		}
	})
}

func Test_SplitIntoChunks(t *testing.T) {
	t.Run("Splits a slice into chunks of size 3", func(t *testing.T) {
		got, err := SplitIntoChunks([]int{1, 2, 3, 4, 5, 6, 7, 8, 9, 10}, 3)
		require.NoError(t, err)
		want := [][]int{{1, 2, 3}, {4, 5, 6}, {7, 8, 9}, {10}}
		if cmp.Diff(got, want) != "" {
			t.Errorf("got %v, want %v", got, want)
		}
	})

	t.Run("Type of slice does not matter", func(t *testing.T) {
		got, err := SplitIntoChunks([]string{"a", "b", "c", "d", "e", "f", "g", "h", "i", "j"}, 4)
		require.NoError(t, err)
		want := [][]string{{"a", "b", "c", "d"}, {"e", "f", "g", "h"}, {"i", "j"}}
		if cmp.Diff(got, want) != "" {
			t.Errorf("got %v, want %v", got, want)
		}
	})

	t.Run("Splits into 1 chunk if slice is smaller than requested chunk size", func(t *testing.T) {
		got, err := SplitIntoChunks([]int{1, 2, 3}, 4)
		require.NoError(t, err)
		want := [][]int{{1, 2, 3}}
		if cmp.Diff(got, want) != "" {
			t.Errorf("got %v, want %v", got, want)
		}
	})

	t.Run("Works with chunk size of 1", func(t *testing.T) {
		got, err := SplitIntoChunks([]int{1, 2, 3}, 1)
		require.NoError(t, err)
		want := [][]int{{1}, {2}, {3}}
		if cmp.Diff(got, want) != "" {
			t.Errorf("got %v, want %v", got, want)
		}
	})

	t.Run("Works with empty slice", func(t *testing.T) {
		got, err := SplitIntoChunks([]int{}, 4)
		require.NoError(t, err)
		want := make([][]int, 0)
		if cmp.Diff(got, want) != "" {
			t.Errorf("got %v, want %v", got, want)
		}
	})

	t.Run("returns error for chunk size of 0", func(t *testing.T) {
		_, err := SplitIntoChunks([]int{1, 2, 3}, 0)
		require.Error(t, err)
	})

	t.Run("returns error for negative chunk size", func(t *testing.T) {
		_, err := SplitIntoChunks([]int{1, 2, 3}, -2)
		require.Error(t, err)
	})

	t.Run("returns empty result for nil slice", func(t *testing.T) {
		var slice []int
		slice = nil
		got, err := SplitIntoChunks(slice, 2)
		require.NoError(t, err)
		want := [][]int{}
		if cmp.Diff(got, want) != "" {
			t.Errorf("got %v, want %v", got, want)
		}
	})
}
