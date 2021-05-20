package upload

import "time"

// RetryableFunc is a function that returns an error as well as a boolean-value flag indicating
// whether or not the error is retryable.
type RetryableFunc = func() (bool, error)

// makeRetry returns a function that calls retry with the given max attempt and interval values.
func makeRetry(n int, interval time.Duration) func(f RetryableFunc) error {
	return func(f RetryableFunc) error {
		return retry(f, n, interval)
	}
}

// retry will re-invoke the given function until it returns a nil error value, the function returns
// a non-retryable error (as indicated by its boolean return value), or until the maximum number of
// retries have been attempted. The returned error will be the last error to occur.
func retry(f RetryableFunc, n int, interval time.Duration) (err error) {
	var retry bool
	for i := n; i >= 0; i-- {
		if retry, err = f(); err == nil || !retry {
			break
		}

		time.Sleep(interval)
	}

	return err
}
