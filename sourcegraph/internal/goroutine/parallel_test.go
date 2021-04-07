package goroutine

import (
	"fmt"
	"strings"
	"testing"
	"time"
)

func TestParallel(t *testing.T) {
	ch1 := make(chan error)
	ch2 := make(chan error)
	ch3 := make(chan error)
	defer close(ch1)
	defer close(ch2)
	defer close(ch3)

	errs := make(chan error)
	defer close(errs)

	go func() {
		errs <- Parallel(
			func() error { return <-ch1 },
			func() error { return nil },
			func() error { return <-ch2 },
			func() error { return nil },
			func() error { return <-ch3 },
			func() error { return nil },
		)
	}()

	ch3 <- fmt.Errorf("C")
	ch2 <- fmt.Errorf("B")
	ch1 <- fmt.Errorf("A")

	select {
	case <-time.After(time.Second):
		t.Fatal("timeout")

	case err := <-errs:
		if !strings.Contains(err.Error(), "3 errors occurred") {
			t.Fatalf("expected a multi-error, got %s", err)
		}
	}

}
