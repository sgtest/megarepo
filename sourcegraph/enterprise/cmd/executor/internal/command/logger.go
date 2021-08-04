package command

import (
	"bytes"
	"context"
	"strings"
	"sync"
	"time"

	"github.com/inconshreveable/log15"

	"github.com/sourcegraph/sourcegraph/enterprise/internal/executor"
	"github.com/sourcegraph/sourcegraph/internal/workerutil"
)

type executionLogEntryStore interface {
	AddExecutionLogEntry(ctx context.Context, id int, entry workerutil.ExecutionLogEntry) (int, error)
	UpdateExecutionLogEntry(ctx context.Context, id, entryID int, entry workerutil.ExecutionLogEntry) error
}

// entryHandle is returned by (*Logger).Log and implements the io.WriteCloser
// interface to allow clients to update the Out field of the ExecutionLogEntry.
//
// The Close() method *must* be called once the client is done writing log
// output to flush the entry to the database.
type entryHandle struct {
	logEntry workerutil.ExecutionLogEntry
	replacer *strings.Replacer

	done chan struct{}

	mu  sync.Mutex
	buf *bytes.Buffer
}

func (h *entryHandle) Write(p []byte) (n int, err error) {
	h.mu.Lock()
	defer h.mu.Unlock()
	return h.buf.Write(p)
}

func (h *entryHandle) Read() string {
	h.mu.Lock()
	defer h.mu.Unlock()
	return h.buf.String()
}

func (h *entryHandle) Close() error {
	close(h.done)
	return nil
}

func (h *entryHandle) CurrentLogEntry() workerutil.ExecutionLogEntry {
	logEntry := h.logEntry
	logEntry.Out = h.Read()
	redact(&logEntry, h.replacer)
	return logEntry
}

// Logger tracks command invocations and stores the command's output and
// error stream values.
type Logger struct {
	store   executionLogEntryStore
	done    chan struct{}
	handles chan *entryHandle

	job      executor.Job
	recordID int

	replacer *strings.Replacer
}

// logEntryBufSize is the maximum number of log entries that are logged by the
// task execution but not yet written to the database.
const logEntryBufsize = 50

// NewLogger creates a new logger instance with the given store, job, record,
// and replacement map.
// When the log messages are serialized, any occurrence of sensitive values are
// replace with a non-sensitive value.
// Each log message is written to the store in a goroutine. The Flush method
// must be called to ensure all entries are written.
func NewLogger(store executionLogEntryStore, job executor.Job, recordID int, replacements map[string]string) *Logger {
	oldnew := make([]string, 0, len(replacements)*2)
	for k, v := range replacements {
		oldnew = append(oldnew, k, v)
	}

	l := &Logger{
		store:    store,
		job:      job,
		recordID: recordID,
		done:     make(chan struct{}),
		handles:  make(chan *entryHandle, logEntryBufsize),
		replacer: strings.NewReplacer(oldnew...),
	}

	go l.writeEntries()

	return l
}

// Flush waits until all entries have been written to the store and all
// background goroutines that watch a log entry and possibly update it have
// exited.
func (l *Logger) Flush() {
	close(l.handles)
	<-l.done
}

// Log redacts secrets from the given log entry and stores it.
func (l *Logger) Log(logEntry *workerutil.ExecutionLogEntry) *entryHandle {
	handle := &entryHandle{logEntry: *logEntry, replacer: l.replacer, buf: &bytes.Buffer{}, done: make(chan struct{})}
	l.handles <- handle
	return handle
}

func (l *Logger) writeEntries() {
	wg := &sync.WaitGroup{}
	defer func() {
		wg.Wait()
		close(l.done)
	}()

	for handle := range l.handles {
		log15.Info("Writing log entry", "jobID", l.job.ID, "repositoryName", l.job.RepositoryName, "commit", l.job.Commit)

		entryID, err := l.store.AddExecutionLogEntry(context.Background(), l.recordID, handle.CurrentLogEntry())
		if err != nil {
			// If there is a timeout or cancellation error we don't want to skip
			// writing these logs as users will often want to see how far something
			// progressed prior to a timeout.
			log15.Warn("Failed to upload executor log entry for job", "id", l.recordID, "repositoryName", l.job.RepositoryName, "commit", l.job.Commit, "error", err)
			continue
		}

		wg.Add(1)
		go func(handle *entryHandle, entryID int) {
			defer wg.Done()

			l.syncLogEntry(handle, entryID)
		}(handle, entryID)
	}
}

const syncLogEntryInterval = 1 * time.Second

func (l *Logger) syncLogEntry(handle *entryHandle, entryID int) {
	lastWrite := false
	old := handle.logEntry

	for !lastWrite {
		select {
		case <-handle.done:
			lastWrite = true
		case <-time.After(syncLogEntryInterval):
		}

		current := handle.CurrentLogEntry()
		if !entryWasUpdated(old, current) {
			continue
		}

		log15.Info(
			"Updating executor log entry",
			"jobID", l.job.ID,
			"repositoryName", l.job.RepositoryName,
			"commit", l.job.Commit,
			"entryID", entryID,
		)

		if err := l.store.UpdateExecutionLogEntry(context.Background(), l.recordID, entryID, current); err != nil {
			logMethod := log15.Warn
			if lastWrite {
				logMethod = log15.Error
			}

			logMethod(
				"Failed to update executor log entry for job",
				"jobID", l.job.ID,
				"repositoryName", l.job.RepositoryName,
				"commit", l.job.Commit,
				"entryID", entryID,
				"lastWrite", lastWrite,
				"error", err,
			)
		} else {
			old = current
		}
	}
}

// If old didn't have exit code or duration and current does, update; we're finished.
// Otherwise, update if the log text has changed since the last write to the API.
func entryWasUpdated(old, current workerutil.ExecutionLogEntry) bool {
	return (current.ExitCode != nil && old.ExitCode == nil) || (current.DurationMs != nil && old.DurationMs == nil) || current.Out != old.Out
}

func redact(entry *workerutil.ExecutionLogEntry, replacer *strings.Replacer) {
	for i, arg := range entry.Command {
		entry.Command[i] = replacer.Replace(arg)
	}
	entry.Out = replacer.Replace(entry.Out)
}
