package codemonitors

import (
	"testing"

	"github.com/google/go-cmp/cmp"
)

func TestAllRecipientsForEmailIDInt64(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}

	ctx, db, s := newTestStore(t)
	_, id, _, userCTX := newTestUser(ctx, t, db)
	_, err := s.insertTestMonitor(userCTX, t)
	if err != nil {
		t.Fatal(err)
	}
	var (
		wantEmailID     int64 = 1
		wantRecipientID int64 = 1
	)
	rs, err := s.ListRecipients(ctx, ListRecipientsOpts{EmailID: &wantEmailID})
	if err != nil {
		t.Fatal(err)
	}
	if diff := cmp.Diff(rs, []*Recipient{{
		ID:              wantRecipientID,
		Email:           wantEmailID,
		NamespaceUserID: &id,
		NamespaceOrgID:  nil,
	}}); diff != "" {
		t.Fatalf("diff: %s", diff)
	}
}
