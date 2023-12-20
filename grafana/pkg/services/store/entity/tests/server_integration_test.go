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

	"github.com/grafana/grafana/pkg/services/store"
	"github.com/grafana/grafana/pkg/services/store/entity"
)

var (
	//go:embed testdata/dashboard-with-tags-b-g.json
	dashboardWithTagsBlueGreen string
	//go:embed testdata/dashboard-with-tags-r-g.json
	dashboardWithTagsRedGreen string
)

type rawEntityMatcher struct {
	key          string
	createdRange []time.Time
	updatedRange []time.Time
	createdBy    string
	updatedBy    string
	body         []byte
	version      int64
}

type objectVersionMatcher struct {
	updatedRange []time.Time
	updatedBy    string
	version      int64
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
	if m.key != "" && m.key != obj.Key {
		mismatches += fmt.Sprintf("expected key: %s, actual: %s\n", m.key, obj.Key)
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

	if m.version != 0 && m.version != obj.ResourceVersion {
		mismatches += fmt.Sprintf("expected version: %d, actual version: %d\n", m.version, obj.ResourceVersion)
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

	if m.version != 0 && m.version != obj.ResourceVersion {
		mismatches += fmt.Sprintf("expected version: %d, actual version: %d\n", m.version, obj.ResourceVersion)
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
	firstVersion := int64(0)
	group := "test.grafana.app"
	resource := "jsonobjs"
	resource2 := "playlists"
	namespace := "default"
	name := "my-test-entity"
	testKey := "/" + group + "/" + resource + "/" + namespace + "/" + name
	body := []byte("{\"name\":\"John\"}")

	t.Run("should not retrieve non-existent objects", func(t *testing.T) {
		resp, err := testCtx.client.Read(ctx, &entity.ReadEntityRequest{
			Key: testKey,
		})
		require.NoError(t, err)

		require.NotNil(t, resp)
		require.Empty(t, resp.Key)
	})

	t.Run("should be able to read persisted objects", func(t *testing.T) {
		before := time.Now()
		createReq := &entity.CreateEntityRequest{
			Entity: &entity.Entity{
				Key:       testKey,
				Group:     group,
				Resource:  resource,
				Namespace: namespace,
				Name:      name,
				Body:      body,
				Message:   "first entity!",
			},
		}
		createResp, err := testCtx.client.Create(ctx, createReq)
		require.NoError(t, err)

		versionMatcher := objectVersionMatcher{
			updatedRange: []time.Time{before, time.Now()},
			updatedBy:    fakeUser,
			version:      firstVersion,
			comment:      &createReq.Entity.Message,
		}
		requireVersionMatch(t, createResp.Entity, versionMatcher)

		readResp, err := testCtx.client.Read(ctx, &entity.ReadEntityRequest{
			Key:             testKey,
			ResourceVersion: 0,
			WithBody:        true,
		})
		require.NoError(t, err)
		require.NotNil(t, readResp)

		require.Equal(t, testKey, readResp.Key)
		require.Equal(t, namespace, readResp.Namespace) // orgId becomes the tenant id when not set
		require.Equal(t, resource, readResp.Resource)
		require.Equal(t, name, readResp.Name)

		objectMatcher := rawEntityMatcher{
			key:          testKey,
			createdRange: []time.Time{before, time.Now()},
			updatedRange: []time.Time{before, time.Now()},
			createdBy:    fakeUser,
			updatedBy:    fakeUser,
			body:         body,
			version:      firstVersion,
		}
		requireEntityMatch(t, readResp, objectMatcher)

		deleteResp, err := testCtx.client.Delete(ctx, &entity.DeleteEntityRequest{
			Key:             testKey,
			PreviousVersion: readResp.ResourceVersion,
		})
		require.NoError(t, err)
		require.Equal(t, deleteResp.Status, entity.DeleteEntityResponse_DELETED)

		readRespAfterDelete, err := testCtx.client.Read(ctx, &entity.ReadEntityRequest{
			Key:             testKey,
			ResourceVersion: 0,
			WithBody:        true,
		})
		require.NoError(t, err)
		require.Empty(t, readRespAfterDelete.Key)
	})

	t.Run("should be able to update an object", func(t *testing.T) {
		before := time.Now()

		createReq := &entity.CreateEntityRequest{
			Entity: &entity.Entity{
				Key:       testKey,
				Group:     group,
				Resource:  resource,
				Namespace: namespace,
				Name:      name,
				Body:      body,
				Message:   "first entity!",
			},
		}
		createResp, err := testCtx.client.Create(ctx, createReq)
		require.NoError(t, err)
		require.Equal(t, entity.CreateEntityResponse_CREATED, createResp.Status)

		body2 := []byte("{\"name\":\"John2\"}")

		updateReq := &entity.UpdateEntityRequest{
			Entity: &entity.Entity{
				Key:     testKey,
				Body:    body2,
				Message: "update1",
			},
		}
		updateResp, err := testCtx.client.Update(ctx, updateReq)
		require.NoError(t, err)
		require.NotEqual(t, createResp.Entity.ResourceVersion, updateResp.Entity.ResourceVersion)

		// Duplicate write (no change)
		writeDupRsp, err := testCtx.client.Update(ctx, updateReq)
		require.NoError(t, err)
		require.Nil(t, writeDupRsp.Error)
		require.Equal(t, entity.UpdateEntityResponse_UNCHANGED, writeDupRsp.Status)
		require.Equal(t, updateResp.Entity.ResourceVersion, writeDupRsp.Entity.ResourceVersion)
		require.Equal(t, updateResp.Entity.ETag, writeDupRsp.Entity.ETag)

		body3 := []byte("{\"name\":\"John3\"}")
		writeReq3 := &entity.UpdateEntityRequest{
			Entity: &entity.Entity{
				Key:     testKey,
				Body:    body3,
				Message: "update3",
			},
		}
		writeResp3, err := testCtx.client.Update(ctx, writeReq3)
		require.NoError(t, err)
		require.NotEqual(t, writeResp3.Entity.ResourceVersion, updateResp.Entity.ResourceVersion)

		latestMatcher := rawEntityMatcher{
			key:          testKey,
			createdRange: []time.Time{before, time.Now()},
			updatedRange: []time.Time{before, time.Now()},
			createdBy:    fakeUser,
			updatedBy:    fakeUser,
			body:         body3,
			version:      writeResp3.Entity.ResourceVersion,
		}
		readRespLatest, err := testCtx.client.Read(ctx, &entity.ReadEntityRequest{
			Key:             testKey,
			ResourceVersion: 0, // latest
			WithBody:        true,
		})
		require.NoError(t, err)
		requireEntityMatch(t, readRespLatest, latestMatcher)

		readRespFirstVer, err := testCtx.client.Read(ctx, &entity.ReadEntityRequest{
			Key:             testKey,
			ResourceVersion: createResp.Entity.ResourceVersion,
			WithBody:        true,
		})

		require.NoError(t, err)
		require.NotNil(t, readRespFirstVer)
		requireEntityMatch(t, readRespFirstVer, rawEntityMatcher{
			key:          testKey,
			createdRange: []time.Time{before, time.Now()},
			updatedRange: []time.Time{before, time.Now()},
			createdBy:    fakeUser,
			updatedBy:    fakeUser,
			body:         body,
			version:      0,
		})

		history, err := testCtx.client.History(ctx, &entity.EntityHistoryRequest{
			Key: testKey,
		})
		require.NoError(t, err)
		require.Equal(t, []*entity.Entity{
			writeResp3.Entity,
			updateResp.Entity,
			createResp.Entity,
		}, history.Versions)

		deleteResp, err := testCtx.client.Delete(ctx, &entity.DeleteEntityRequest{
			Key:             testKey,
			PreviousVersion: writeResp3.Entity.ResourceVersion,
		})
		require.NoError(t, err)
		require.Equal(t, deleteResp.Status, entity.DeleteEntityResponse_DELETED)
	})

	t.Run("should be able to list objects", func(t *testing.T) {
		w1, err := testCtx.client.Create(ctx, &entity.CreateEntityRequest{
			Entity: &entity.Entity{
				Key:  testKey + "1",
				Body: body,
			},
		})
		require.NoError(t, err)

		w2, err := testCtx.client.Create(ctx, &entity.CreateEntityRequest{
			Entity: &entity.Entity{
				Key:  testKey + "2",
				Body: body,
			},
		})
		require.NoError(t, err)

		w3, err := testCtx.client.Create(ctx, &entity.CreateEntityRequest{
			Entity: &entity.Entity{
				Key:  testKey + "3",
				Body: body,
			},
		})
		require.NoError(t, err)

		w4, err := testCtx.client.Create(ctx, &entity.CreateEntityRequest{
			Entity: &entity.Entity{
				Key:  testKey + "4",
				Body: body,
			},
		})
		require.NoError(t, err)

		resp, err := testCtx.client.List(ctx, &entity.EntityListRequest{
			Resource: []string{resource, resource2},
			WithBody: false,
		})
		require.NoError(t, err)

		require.NotNil(t, resp)
		names := make([]string, 0, len(resp.Results))
		kinds := make([]string, 0, len(resp.Results))
		version := make([]int64, 0, len(resp.Results))
		for _, res := range resp.Results {
			names = append(names, res.Name)
			kinds = append(kinds, res.Resource)
			version = append(version, res.ResourceVersion)
		}
		require.Equal(t, []string{"my-test-entity", "name2", "name3", "name4"}, names)
		require.Equal(t, []string{"jsonobj", "jsonobj", "playlist", "playlist"}, kinds)
		require.Equal(t, []int64{
			w1.Entity.ResourceVersion,
			w2.Entity.ResourceVersion,
			w3.Entity.ResourceVersion,
			w4.Entity.ResourceVersion,
		}, version)

		// Again with only one kind
		respKind1, err := testCtx.client.List(ctx, &entity.EntityListRequest{
			Resource: []string{resource},
		})
		require.NoError(t, err)
		names = make([]string, 0, len(respKind1.Results))
		kinds = make([]string, 0, len(respKind1.Results))
		version = make([]int64, 0, len(respKind1.Results))
		for _, res := range respKind1.Results {
			names = append(names, res.Name)
			kinds = append(kinds, res.Resource)
			version = append(version, res.ResourceVersion)
		}
		require.Equal(t, []string{"my-test-entity", "name2"}, names)
		require.Equal(t, []string{"jsonobj", "jsonobj"}, kinds)
		require.Equal(t, []int64{
			w1.Entity.ResourceVersion,
			w2.Entity.ResourceVersion,
		}, version)
	})

	t.Run("should be able to filter objects based on their labels", func(t *testing.T) {
		kind := entity.StandardKindDashboard
		_, err := testCtx.client.Create(ctx, &entity.CreateEntityRequest{
			Entity: &entity.Entity{
				Key:  "/grafana/dashboards/blue-green",
				Body: []byte(dashboardWithTagsBlueGreen),
			},
		})
		require.NoError(t, err)

		_, err = testCtx.client.Create(ctx, &entity.CreateEntityRequest{
			Entity: &entity.Entity{
				Key:  "/grafana/dashboards/red-green",
				Body: []byte(dashboardWithTagsRedGreen),
			},
		})
		require.NoError(t, err)

		resp, err := testCtx.client.List(ctx, &entity.EntityListRequest{
			Key:      []string{kind},
			WithBody: false,
			Labels: map[string]string{
				"red": "",
			},
		})
		require.NoError(t, err)
		require.NotNil(t, resp)
		require.Len(t, resp.Results, 1)
		require.Equal(t, resp.Results[0].Name, "red-green")

		resp, err = testCtx.client.List(ctx, &entity.EntityListRequest{
			Key:      []string{kind},
			WithBody: false,
			Labels: map[string]string{
				"red":   "",
				"green": "",
			},
		})
		require.NoError(t, err)
		require.NotNil(t, resp)
		require.Len(t, resp.Results, 1)
		require.Equal(t, resp.Results[0].Name, "red-green")

		resp, err = testCtx.client.List(ctx, &entity.EntityListRequest{
			Key:      []string{kind},
			WithBody: false,
			Labels: map[string]string{
				"red": "invalid",
			},
		})
		require.NoError(t, err)
		require.NotNil(t, resp)
		require.Len(t, resp.Results, 0)

		resp, err = testCtx.client.List(ctx, &entity.EntityListRequest{
			Key:      []string{kind},
			WithBody: false,
			Labels: map[string]string{
				"green": "",
			},
		})
		require.NoError(t, err)
		require.NotNil(t, resp)
		require.Len(t, resp.Results, 2)

		resp, err = testCtx.client.List(ctx, &entity.EntityListRequest{
			Key:      []string{kind},
			WithBody: false,
			Labels: map[string]string{
				"yellow": "",
			},
		})
		require.NoError(t, err)
		require.NotNil(t, resp)
		require.Len(t, resp.Results, 0)
	})
}
