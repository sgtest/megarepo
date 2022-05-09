package honey

import (
	"github.com/honeycombio/libhoney-go"
	"github.com/opentracing/opentracing-go/log"
)

// Event represents a mockable/noop-able single event in Honeycomb terms, as per
// https://docs.honeycomb.io/getting-started/events-metrics-logs/#structured-events.
type Event interface {
	// Dataset returns the destination dataset of this event
	Dataset() string
	// AddField adds a single key-value pair to this event.
	AddField(key string, val any)
	// AddLogFields adds each opentracing-go/log key-value field to this event.
	AddLogFields(fields []log.Field)
	// Add adds a complex type to the event. For structs, it adds each exported field.
	// For maps, it adds each key/value. Add will error on all other types.
	Add(data any) error
	// Fields returns all the added fields of the event. The returned map is not safe to
	// be modified concurrently with calls Add/AddField/AddLogFields.
	Fields() map[string]any
	// SetSampleRate overrides the global sample rate for this event. Default is 1,
	// meaning no sampling. If you want to send one event out of every 250 times
	// Send() is called, you would specify 250 here.
	SetSampleRate(rate uint)
	// Send dispatches the event to be sent to Honeycomb, sampling if necessary.
	Send() error
}

type eventWrapper struct {
	event *libhoney.Event
	// contains a map of keys whose values have been slice wrapped aka
	// added more than once already. If theres no entry in sliceWrapped
	// but there is in event for a key, then the to-be-added value is
	// sliceWrapped before insertion and true inserted into sliceWrapped for that key
	sliceWrapped map[string]bool
}

var _ Event = eventWrapper{}

func (w eventWrapper) Dataset() string {
	return w.event.Dataset
}

func (w eventWrapper) AddField(name string, val any) {
	data, ok := w.Fields()[name]
	if !ok {
		data = val
	} else if ok && !w.sliceWrapped[name] {
		data = sliceWrapper{data, val}
		w.sliceWrapped[name] = true
	} else {
		data = append(data.(sliceWrapper), val)
	}
	w.event.AddField(name, data)
}

func (w eventWrapper) AddLogFields(fields []log.Field) {
	for _, field := range fields {
		w.AddField(field.Key(), field.Value())
	}
}

func (w eventWrapper) Add(data any) error {
	return w.event.Add(data)
}

func (w eventWrapper) Fields() map[string]any {
	return w.event.Fields()
}

func (w eventWrapper) SetSampleRate(rate uint) {
	w.event.SampleRate = rate
}

func (w eventWrapper) Send() error {
	return w.event.Send()
}

// NewEvent creates an event for logging to dataset. If Enabled() would return false,
// NewEvent returns a noop event. NewEvent.Send will only work if
// Enabled() returns true.
func NewEvent(dataset string) Event {
	if !Enabled() {
		return noopEvent{}
	}
	ev := libhoney.NewEvent()
	ev.Dataset = dataset + suffix
	return eventWrapper{
		event:        ev,
		sliceWrapped: map[string]bool{},
	}
}

// NewEventWithFields creates an event for logging to the given dataset. The given
// fields are assigned to the event.
func NewEventWithFields(dataset string, fields map[string]any) Event {
	if !Enabled() {
		return noopEvent{}
	}
	ev := NewEvent(dataset)
	for key, value := range fields {
		ev.AddField(key, value)
	}

	return ev
}
