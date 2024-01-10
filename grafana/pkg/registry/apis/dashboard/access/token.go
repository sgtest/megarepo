package access

import (
	"fmt"
	"strconv"
	"strings"

	"github.com/grafana/grafana/pkg/util"
)

type continueToken struct {
	orgId  int64
	id     int64  // the internal id (sort by!)
	folder string // from the query
	size   int64
}

func readContinueToken(q *DashboardQuery) (continueToken, error) {
	var err error
	token := continueToken{}
	if q.ContinueToken == "" {
		return token, nil
	}
	parts := strings.Split(q.ContinueToken, "/")
	if len(parts) < 3 {
		return token, fmt.Errorf("invalid continue token (too few parts)")
	}
	sub := strings.Split(parts[0], ":")
	if sub[0] != "org" {
		return token, fmt.Errorf("expected org in first slug")
	}
	token.orgId, err = strconv.ParseInt(sub[1], 10, 64)
	if err != nil {
		return token, fmt.Errorf("error parsing orgid")
	}

	sub = strings.Split(parts[1], ":")
	if sub[0] != "start" {
		return token, fmt.Errorf("expected internal ID in second slug")
	}
	token.id, err = strconv.ParseInt(sub[1], 10, 64)
	if err != nil {
		return token, fmt.Errorf("error parsing updated")
	}

	sub = strings.Split(parts[2], ":")
	if sub[0] != "folder" {
		return token, fmt.Errorf("expected folder UID in third slug")
	}
	token.folder = sub[1]

	// Check if the folder filter is the same from the previous query
	if token.folder != q.FolderUID {
		return token, fmt.Errorf("invalid token, the folder must match previous query")
	}

	return token, err
}

func (r *continueToken) String() string {
	return fmt.Sprintf("org:%d/start:%d/folder:%s/%s",
		r.orgId, r.id, r.folder, util.ByteCountSI(r.size))
}
