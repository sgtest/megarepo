package entity_server_tests

import (
	_ "embed"
	"encoding/json"
	"fmt"
	"reflect"
	"testing"
	"time"

	"github.com/stretchr/testify/require"
	"google.golang.org/grpc/metadata"

	"github.com/grafana/grafana/pkg/infra/grn"
	"github.com/grafana/grafana/pkg/services/store"
	"github.com/grafana/grafana/pkg/services/store/entity"
	"github.com/grafana/grafana/pkg/util"
)

var (
	//go:embed testdata/dashboard-with-tags-b-g.json
	dashboardWithTagsBlueGreen string
	//go:embed testdata/dashboard-with-tags-r-g.json
	dashboardWithTagsRedGreen string
)

type rawEntityMatcher struct {
	grn          *grn.GRN
	createdRange []time.Time
	updatedRange []time.Time
	createdBy    string
	updatedBy    string
	body         []byte
	version      *string
}

type objectVersionMatcher struct {
	updatedRange []time.Time
	updatedBy    string
	version      *string
	etag         *string
	comment      *string
}

func timestampInRange(ts int64, tsRange []time.Time) bool {
	low := tsRange[0].UnixMilli() - 1
	high := tsRange[1].UnixMilli() + 1
	return ts >= low && ts <= high
}

func requireEntityMatch(t *testing.T, obj *entity.Entity, m rawEntityMatcher) {
	t.Helper()
	require.NotNil(t, obj)

	mismatches := ""
	if m.grn != nil {
		if m.grn.TenantID > 0 && m.grn.TenantID != obj.GRN.TenantID {
			mismatches += fmt.Sprintf("expected tenant: %d, actual: %d\n", m.grn.TenantID, obj.GRN.TenantID)
		}
		if m.grn.ResourceKind != "" && m.grn.ResourceKind != obj.GRN.ResourceKind {
			mismatches += fmt.Sprintf("expected ResourceKind: %s, actual: %s\n", m.grn.ResourceKind, obj.GRN.ResourceKind)
		}
		if m.grn.ResourceIdentifier != "" && m.grn.ResourceIdentifier != obj.GRN.ResourceIdentifier {
			mismatches += fmt.Sprintf("expected ResourceIdentifier: %s, actual: %s\n", m.grn.ResourceIdentifier, obj.GRN.ResourceIdentifier)
		}
	}

	if len(m.createdRange) == 2 && !timestampInRange(obj.CreatedAt, m.createdRange) {
		mismatches += fmt.Sprintf("expected Created range: [from %s to %s], actual created: %s\n", m.createdRange[0], m.createdRange[1], time.UnixMilli(obj.CreatedAt))
	}

	if len(m.updatedRange) == 2 && !timestampInRange(obj.UpdatedAt, m.updatedRange) {
		mismatches += fmt.Sprintf("expected Updated range: [from %s to %s], actual updated: %s\n", m.updatedRange[0], m.updatedRange[1], time.UnixMilli(obj.UpdatedAt))
	}

	if m.createdBy != "" && m.createdBy != obj.CreatedBy {
		mismatches += fmt.Sprintf("createdBy: expected:%s, found:%s\n", m.createdBy, obj.CreatedBy)
	}

	if m.updatedBy != "" && m.updatedBy != obj.UpdatedBy {
		mismatches += fmt.Sprintf("updatedBy: expected:%s, found:%s\n", m.updatedBy, obj.UpdatedBy)
	}

	if len(m.body) > 0 {
		if json.Valid(m.body) {
			require.JSONEq(t, string(m.body), string(obj.Body), "expecting same body")
		} else if !reflect.DeepEqual(m.body, obj.Body) {
			mismatches += fmt.Sprintf("expected body len: %d, actual body len: %d\n", len(m.body), len(obj.Body))
		}
	}

	if m.version != nil && *m.version != obj.Version {
		mismatches += fmt.Sprintf("expected version: %s, actual version: %s\n", *m.version, obj.Version)
	}

	require.True(t, len(mismatches) == 0, mismatches)
}

func requireVersionMatch(t *testing.T, obj *entity.Entity, m objectVersionMatcher) {
	t.Helper()
	mismatches := ""

	if m.etag != nil && *m.etag != obj.ETag {
		mismatches += fmt.Sprintf("expected etag: %s, actual etag: %s\n", *m.etag, obj.ETag)
	}

	if len(m.updatedRange) == 2 && !timestampInRange(obj.UpdatedAt, m.updatedRange) {
		mismatches += fmt.Sprintf("expected updatedRange range: [from %s to %s], actual updated: %s\n", m.updatedRange[0], m.updatedRange[1], time.UnixMilli(obj.UpdatedAt))
	}

	if m.updatedBy != "" && m.updatedBy != obj.UpdatedBy {
		mismatches += fmt.Sprintf("updatedBy: expected:%s, found:%s\n", m.updatedBy, obj.UpdatedBy)
	}

	if m.version != nil && *m.version != obj.Version {
		mismatches += fmt.Sprintf("expected version: %s, actual version: %s\n", *m.version, obj.Version)
	}

	require.True(t, len(mismatches) == 0, mismatches)
}

func TestIntegrationEntityServer(t *testing.T) {
	if true {
		// FIXME
		t.Skip()
	}

	if testing.Short() {
		t.Skip("skipping integration test")
	}

	testCtx := createTestContext(t)
	ctx := metadata.AppendToOutgoingContext(testCtx.ctx, "authorization", fmt.Sprintf("Bearer %s", testCtx.authToken))

	fakeUser := store.GetUserIDString(testCtx.user)
	firstVersion := "1"
	kind := entity.StandardKindJSONObj
	testGrn := &grn.GRN{
		ResourceKind:       kind,
		ResourceIdentifier: "my-test-entity",
	}
	body := []byte("{\"name\":\"John\"}")

	t.Run("should not retrieve non-existent objects", func(t *testing.T) {
		resp, err := testCtx.client.Read(ctx, &entity.ReadEntityRequest{
			GRN: testGrn,
		})
		require.NoError(t, err)

		require.NotNil(t, resp)
		require.Nil(t, resp.GRN)
	})

	t.Run("should be able to read persisted objects", func(t *testing.T) {
		before := time.Now()
		writeReq := &entity.WriteEntityRequest{
			Entity: &entity.Entity{
				GRN:     testGrn,
				Body:    body,
				Message: "first entity!",
			},
		}
		writeResp, err := testCtx.client.Write(ctx, writeReq)
		require.NoError(t, err)

		versionMatcher := objectVersionMatcher{
			updatedRange: []time.Time{before, time.Now()},
			updatedBy:    fakeUser,
			version:      &firstVersion,
			comment:      &writeReq.Entity.Message,
		}
		requireVersionMatch(t, writeResp.Entity, versionMatcher)

		readResp, err := testCtx.client.Read(ctx, &entity.ReadEntityRequest{
			GRN:      testGrn,
			Version:  "",
			WithBody: true,
		})
		require.NoError(t, err)
		require.NotNil(t, readResp)

		foundGRN := readResp.GRN
		require.NotNil(t, foundGRN)
		require.Equal(t, testCtx.user.OrgID, foundGRN.TenantID) // orgId becomes the tenant id when not set
		require.Equal(t, testGrn.ResourceKind, foundGRN.ResourceKind)
		require.Equal(t, testGrn.ResourceIdentifier, foundGRN.ResourceIdentifier)

		objectMatcher := rawEntityMatcher{
			grn:          testGrn,
			createdRange: []time.Time{before, time.Now()},
			updatedRange: []time.Time{before, time.Now()},
			createdBy:    fakeUser,
			updatedBy:    fakeUser,
			body:         body,
			version:      &firstVersion,
		}
		requireEntityMatch(t, readResp, objectMatcher)

		deleteResp, err := testCtx.client.Delete(ctx, &entity.DeleteEntityRequest{
			GRN:             testGrn,
			PreviousVersion: writeResp.Entity.Version,
		})
		require.NoError(t, err)
		require.Equal(t, deleteResp.Status, entity.DeleteEntityResponse_DELETED)

		readRespAfterDelete, err := testCtx.client.Read(ctx, &entity.ReadEntityRequest{
			GRN:      testGrn,
			Version:  "",
			WithBody: true,
		})
		require.NoError(t, err)
		require.Nil(t, readRespAfterDelete.GRN)
	})

	t.Run("should be able to update an object", func(t *testing.T) {
		before := time.Now()
		testGrn := &grn.GRN{
			ResourceKind:       kind,
			ResourceIdentifier: util.GenerateShortUID(),
		}

		writeReq1 := &entity.WriteEntityRequest{
			Entity: &entity.Entity{
				GRN:     testGrn,
				Body:    body,
				Message: "first entity!",
			},
		}
		writeResp1, err := testCtx.client.Write(ctx, writeReq1)
		require.NoError(t, err)
		require.Equal(t, entity.WriteEntityResponse_CREATED, writeResp1.Status)

		body2 := []byte("{\"name\":\"John2\"}")

		writeReq2 := &entity.WriteEntityRequest{
			Entity: &entity.Entity{
				GRN:     testGrn,
				Body:    body2,
				Message: "update1",
			},
		}
		writeResp2, err := testCtx.client.Write(ctx, writeReq2)
		require.NoError(t, err)
		require.NotEqual(t, writeResp1.Entity.Version, writeResp2.Entity.Version)

		// Duplicate write (no change)
		writeDupRsp, err := testCtx.client.Write(ctx, writeReq2)
		require.NoError(t, err)
		require.Nil(t, writeDupRsp.Error)
		require.Equal(t, entity.WriteEntityResponse_UNCHANGED, writeDupRsp.Status)
		require.Equal(t, writeResp2.Entity.Version, writeDupRsp.Entity.Version)
		require.Equal(t, writeResp2.Entity.ETag, writeDupRsp.Entity.ETag)

		body3 := []byte("{\"name\":\"John3\"}")
		writeReq3 := &entity.WriteEntityRequest{
			Entity: &entity.Entity{
				GRN:     testGrn,
				Body:    body3,
				Message: "update3",
			},
		}
		writeResp3, err := testCtx.client.Write(ctx, writeReq3)
		require.NoError(t, err)
		require.NotEqual(t, writeResp3.Entity.Version, writeResp2.Entity.Version)

		latestMatcher := rawEntityMatcher{
			grn:          testGrn,
			createdRange: []time.Time{before, time.Now()},
			updatedRange: []time.Time{before, time.Now()},
			createdBy:    fakeUser,
			updatedBy:    fakeUser,
			body:         body3,
			version:      &writeResp3.Entity.Version,
		}
		readRespLatest, err := testCtx.client.Read(ctx, &entity.ReadEntityRequest{
			GRN:      testGrn,
			Version:  "", // latest
			WithBody: true,
		})
		require.NoError(t, err)
		requireEntityMatch(t, readRespLatest, latestMatcher)

		readRespFirstVer, err := testCtx.client.Read(ctx, &entity.ReadEntityRequest{
			GRN:      testGrn,
			Version:  writeResp1.Entity.Version,
			WithBody: true,
		})

		require.NoError(t, err)
		require.NotNil(t, readRespFirstVer)
		requireEntityMatch(t, readRespFirstVer, rawEntityMatcher{
			grn:          testGrn,
			createdRange: []time.Time{before, time.Now()},
			updatedRange: []time.Time{before, time.Now()},
			createdBy:    fakeUser,
			updatedBy:    fakeUser,
			body:         body,
			version:      &firstVersion,
		})

		history, err := testCtx.client.History(ctx, &entity.EntityHistoryRequest{
			GRN: testGrn,
		})
		require.NoError(t, err)
		require.Equal(t, []*entity.Entity{
			writeResp3.Entity,
			writeResp2.Entity,
			writeResp1.Entity,
		}, history.Versions)

		deleteResp, err := testCtx.client.Delete(ctx, &entity.DeleteEntityRequest{
			GRN:             testGrn,
			PreviousVersion: writeResp3.Entity.Version,
		})
		require.NoError(t, err)
		require.Equal(t, deleteResp.Status, entity.DeleteEntityResponse_DELETED)
	})

	t.Run("should be able to list objects", func(t *testing.T) {
		uid2 := "uid2"
		uid3 := "uid3"
		uid4 := "uid4"
		kind2 := entity.StandardKindPlaylist
		w1, err := testCtx.client.Write(ctx, &entity.WriteEntityRequest{
			Entity: &entity.Entity{
				GRN:  testGrn,
				Body: body,
			},
		})
		require.NoError(t, err)

		w2, err := testCtx.client.Write(ctx, &entity.WriteEntityRequest{
			Entity: &entity.Entity{
				GRN: &grn.GRN{
					ResourceIdentifier: uid2,
					ResourceKind:       kind,
				},
				Body: body,
			},
		})
		require.NoError(t, err)

		w3, err := testCtx.client.Write(ctx, &entity.WriteEntityRequest{
			Entity: &entity.Entity{
				GRN: &grn.GRN{
					ResourceIdentifier: uid3,
					ResourceKind:       kind2,
				},
				Body: body,
			},
		})
		require.NoError(t, err)

		w4, err := testCtx.client.Write(ctx, &entity.WriteEntityRequest{
			Entity: &entity.Entity{
				GRN: &grn.GRN{
					ResourceIdentifier: uid4,
					ResourceKind:       kind2,
				},
				Body: body,
			},
		})
		require.NoError(t, err)

		resp, err := testCtx.client.List(ctx, &entity.EntityListRequest{
			Kind:     []string{kind, kind2},
			WithBody: false,
		})
		require.NoError(t, err)

		require.NotNil(t, resp)
		uids := make([]string, 0, len(resp.Results))
		kinds := make([]string, 0, len(resp.Results))
		version := make([]string, 0, len(resp.Results))
		for _, res := range resp.Results {
			uids = append(uids, res.GRN.ResourceIdentifier)
			kinds = append(kinds, res.GRN.ResourceKind)
			version = append(version, res.Version)
		}
		require.Equal(t, []string{"my-test-entity", "uid2", "uid3", "uid4"}, uids)
		require.Equal(t, []string{"jsonobj", "jsonobj", "playlist", "playlist"}, kinds)
		require.Equal(t, []string{
			w1.Entity.Version,
			w2.Entity.Version,
			w3.Entity.Version,
			w4.Entity.Version,
		}, version)

		// Again with only one kind
		respKind1, err := testCtx.client.List(ctx, &entity.EntityListRequest{
			Kind: []string{kind},
		})
		require.NoError(t, err)
		uids = make([]string, 0, len(respKind1.Results))
		kinds = make([]string, 0, len(respKind1.Results))
		version = make([]string, 0, len(respKind1.Results))
		for _, res := range respKind1.Results {
			uids = append(uids, res.GRN.ResourceIdentifier)
			kinds = append(kinds, res.GRN.ResourceKind)
			version = append(version, res.Version)
		}
		require.Equal(t, []string{"my-test-entity", "uid2"}, uids)
		require.Equal(t, []string{"jsonobj", "jsonobj"}, kinds)
		require.Equal(t, []string{
			w1.Entity.Version,
			w2.Entity.Version,
		}, version)
	})

	t.Run("should be able to filter objects based on their labels", func(t *testing.T) {
		kind := entity.StandardKindDashboard
		_, err := testCtx.client.Write(ctx, &entity.WriteEntityRequest{
			Entity: &entity.Entity{
				GRN: &grn.GRN{
					ResourceKind:       kind,
					ResourceIdentifier: "blue-green",
				},
				Body: []byte(dashboardWithTagsBlueGreen),
			},
		})
		require.NoError(t, err)

		_, err = testCtx.client.Write(ctx, &entity.WriteEntityRequest{
			Entity: &entity.Entity{
				GRN: &grn.GRN{
					ResourceKind:       kind,
					ResourceIdentifier: "red-green",
				},
				Body: []byte(dashboardWithTagsRedGreen),
			},
		})
		require.NoError(t, err)

		resp, err := testCtx.client.List(ctx, &entity.EntityListRequest{
			Kind:       []string{kind},
			WithBody:   false,
			WithLabels: true,
			Labels: map[string]string{
				"red": "",
			},
		})
		require.NoError(t, err)
		require.NotNil(t, resp)
		require.Len(t, resp.Results, 1)
		require.Equal(t, resp.Results[0].GRN.ResourceIdentifier, "red-green")

		resp, err = testCtx.client.List(ctx, &entity.EntityListRequest{
			Kind:       []string{kind},
			WithBody:   false,
			WithLabels: true,
			Labels: map[string]string{
				"red":   "",
				"green": "",
			},
		})
		require.NoError(t, err)
		require.NotNil(t, resp)
		require.Len(t, resp.Results, 1)
		require.Equal(t, resp.Results[0].GRN.ResourceIdentifier, "red-green")

		resp, err = testCtx.client.List(ctx, &entity.EntityListRequest{
			Kind:       []string{kind},
			WithBody:   false,
			WithLabels: true,
			Labels: map[string]string{
				"red": "invalid",
			},
		})
		require.NoError(t, err)
		require.NotNil(t, resp)
		require.Len(t, resp.Results, 0)

		resp, err = testCtx.client.List(ctx, &entity.EntityListRequest{
			Kind:       []string{kind},
			WithBody:   false,
			WithLabels: true,
			Labels: map[string]string{
				"green": "",
			},
		})
		require.NoError(t, err)
		require.NotNil(t, resp)
		require.Len(t, resp.Results, 2)

		resp, err = testCtx.client.List(ctx, &entity.EntityListRequest{
			Kind:       []string{kind},
			WithBody:   false,
			WithLabels: true,
			Labels: map[string]string{
				"yellow": "",
			},
		})
		require.NoError(t, err)
		require.NotNil(t, resp)
		require.Len(t, resp.Results, 0)
	})
}
