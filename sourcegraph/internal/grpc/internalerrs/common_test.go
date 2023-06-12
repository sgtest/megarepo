package internalerrs

import (
	"errors"
	"sort"
	"strings"
	"testing"

	"github.com/google/go-cmp/cmp/cmpopts"
	newspb "github.com/sourcegraph/sourcegraph/internal/grpc/testprotos/news/v1"
	"google.golang.org/protobuf/proto"
	"google.golang.org/protobuf/types/known/timestamppb"

	"github.com/google/go-cmp/cmp"
	"google.golang.org/grpc"
	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/status"
)

func TestCallBackClientStream(t *testing.T) {
	t.Run("SendMsg calls postMessageSend with message and error", func(t *testing.T) {
		sentinelMessage := struct{}{}
		sentinelErr := errors.New("send error")

		var called bool
		stream := callBackClientStream{
			ClientStream: &mockClientStream{
				sendErr: sentinelErr,
			},
			postMessageSend: func(message any, err error) {
				called = true

				if diff := cmp.Diff(message, sentinelMessage); diff != "" {
					t.Errorf("postMessageSend called with unexpected message (-want +got):\n%s", diff)
				}
				if !errors.Is(err, sentinelErr) {
					t.Errorf("got %v, want %v", err, sentinelErr)
				}
			},
		}

		sendErr := stream.SendMsg(sentinelMessage)
		if !called {
			t.Error("postMessageSend not called")
		}

		if !errors.Is(sendErr, sentinelErr) {
			t.Errorf("got %v, want %v", sendErr, sentinelErr)
		}
	})

	t.Run("RecvMsg calls postMessageReceive with message and error", func(t *testing.T) {
		sentinelMessage := struct{}{}
		sentinelErr := errors.New("receive error")

		var called bool
		stream := callBackClientStream{
			ClientStream: &mockClientStream{
				recvErr: sentinelErr,
			},
			postMessageReceive: func(message any, err error) {
				called = true

				if diff := cmp.Diff(message, sentinelMessage); diff != "" {
					t.Errorf("postMessageReceive called with unexpected message (-want +got):\n%s", diff)
				}
				if !errors.Is(err, sentinelErr) {
					t.Errorf("got %v, want %v", err, sentinelErr)
				}
			},
		}

		receiveErr := stream.RecvMsg(sentinelMessage)
		if !called {
			t.Error("postMessageReceive not called")
		}

		if !errors.Is(receiveErr, sentinelErr) {
			t.Errorf("got %v, want %v", receiveErr, sentinelErr)
		}
	})
}

// mockClientStream is a grpc.ClientStream that returns a given error on SendMsg and RecvMsg.
type mockClientStream struct {
	grpc.ClientStream
	sendErr error
	recvErr error
}

func (s *mockClientStream) SendMsg(any) error {
	return s.sendErr
}

func (s *mockClientStream) RecvMsg(any) error {
	return s.recvErr
}

func TestProbablyInternalGRPCError(t *testing.T) {
	checker := func(s *status.Status) bool {
		return strings.HasPrefix(s.Message(), "custom error")
	}

	testCases := []struct {
		status     *status.Status
		checkers   []internalGRPCErrorChecker
		wantResult bool
	}{
		{
			status:     status.New(codes.OK, ""),
			checkers:   []internalGRPCErrorChecker{func(*status.Status) bool { return true }},
			wantResult: false,
		},
		{
			status:     status.New(codes.Internal, "custom error message"),
			checkers:   []internalGRPCErrorChecker{checker},
			wantResult: true,
		},
		{
			status:     status.New(codes.Internal, "some other error"),
			checkers:   []internalGRPCErrorChecker{checker},
			wantResult: false,
		},
	}

	for _, tc := range testCases {
		gotResult := probablyInternalGRPCError(tc.status, tc.checkers)
		if gotResult != tc.wantResult {
			t.Errorf("probablyInternalGRPCError(%v, %v) = %v, want %v", tc.status, tc.checkers, gotResult, tc.wantResult)
		}
	}
}

func TestGRPCResourceExhaustedChecker(t *testing.T) {
	testCases := []struct {
		status     *status.Status
		expectPass bool
	}{
		{
			status:     status.New(codes.ResourceExhausted, "trying to send message larger than max (1024 vs 2)"),
			expectPass: true,
		},
		{
			status:     status.New(codes.ResourceExhausted, "some other error"),
			expectPass: false,
		},
		{
			status:     status.New(codes.OK, "trying to send message larger than max (1024 vs 5)"),
			expectPass: false,
		},
	}

	for _, tc := range testCases {
		actual := gRPCResourceExhaustedChecker(tc.status)
		if actual != tc.expectPass {
			t.Errorf("gRPCResourceExhaustedChecker(%v) got %t, want %t", tc.status, actual, tc.expectPass)
		}
	}
}

func TestGRPCPrefixChecker(t *testing.T) {
	tests := []struct {
		status *status.Status
		want   bool
	}{
		{
			status: status.New(codes.OK, "not a grpc error"),
			want:   false,
		},
		{
			status: status.New(codes.Internal, "grpc: internal server error"),
			want:   true,
		},
		{
			status: status.New(codes.Unavailable, "some other error"),
			want:   false,
		},
	}
	for _, test := range tests {
		got := gRPCPrefixChecker(test.status)
		if got != test.want {
			t.Errorf("gRPCPrefixChecker(%v) = %v, want %v", test.status, got, test.want)
		}
	}
}

func TestSplitMethodName(t *testing.T) {
	testCases := []struct {
		name string

		fullMethod  string
		wantService string
		wantMethod  string
	}{
		{
			name: "full method with service and method",

			fullMethod:  "/package.service/method",
			wantService: "package.service",
			wantMethod:  "method",
		},
		{
			name: "method without leading slash",

			fullMethod:  "package.service/method",
			wantService: "package.service",
			wantMethod:  "method",
		},
		{
			name: "service without method",

			fullMethod:  "/package.service/",
			wantService: "package.service",
			wantMethod:  "",
		},
		{
			name: "empty input",

			fullMethod:  "",
			wantService: "unknown",
			wantMethod:  "unknown",
		},
	}

	for _, tc := range testCases {
		t.Run(tc.name, func(t *testing.T) {
			service, method := splitMethodName(tc.fullMethod)
			if diff := cmp.Diff(service, tc.wantService); diff != "" {
				t.Errorf("splitMethodName(%q) service (-want +got):\n%s", tc.fullMethod, diff)
			}

			if diff := cmp.Diff(method, tc.wantMethod); diff != "" {
				t.Errorf("splitMethodName(%q) method (-want +got):\n%s", tc.fullMethod, diff)
			}
		})
	}
}

func TestFindNonUTF8StringFields(t *testing.T) {
	// Create instances of the BinaryAttachment and KeyValueAttachment messages
	invalidBinaryAttachment := &newspb.BinaryAttachment{
		Name: "inval\x80id_binary",
		Data: []byte("sample data"),
	}

	invalidKeyValueAttachment := &newspb.KeyValueAttachment{
		Name: "inval\x80id_key_value",
		Data: map[string]string{
			"key1": "value1",
			"key2": "inval\x80id_value",
		},
	}

	// Create a sample Article message with invalid UTF-8 strings
	article := &newspb.Article{
		Author:  "inval\x80id_author",
		Date:    &timestamppb.Timestamp{Seconds: 1234567890},
		Title:   "valid_title",
		Content: "valid_content",
		Status:  newspb.Article_STATUS_PUBLISHED,
		Attachments: []*newspb.Attachment{
			{Contents: &newspb.Attachment_BinaryAttachment{BinaryAttachment: invalidBinaryAttachment}},
			{Contents: &newspb.Attachment_KeyValueAttachment{KeyValueAttachment: invalidKeyValueAttachment}},
		},
	}

	tests := []struct {
		name          string
		message       proto.Message
		expectedPaths []string
	}{
		{
			name:    "Article with invalid UTF-8 strings",
			message: article,
			expectedPaths: []string{
				"author",
				"attachments[0].binary_attachment.name",
				"attachments[1].key_value_attachment.name",
				`attachments[1].key_value_attachment.data["key2"]`,
			},
		},
		{
			name:          "nil message",
			message:       nil,
			expectedPaths: []string{},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			invalidFields, err := findNonUTF8StringFields(tt.message)
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}

			sort.Strings(invalidFields)
			sort.Strings(tt.expectedPaths)

			if diff := cmp.Diff(tt.expectedPaths, invalidFields, cmpopts.EquateEmpty()); diff != "" {
				t.Fatalf("unexpected invalid fields (-want +got):\n%s", diff)
			}
		})
	}
}
