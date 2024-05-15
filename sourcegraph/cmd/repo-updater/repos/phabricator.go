package repos

import (
	"context"
	"encoding/json"
	"fmt"
	"io/ioutil"
	"net/url"
	"strings"
	"time"

	"golang.org/x/net/context/ctxhttp"

	"github.com/sourcegraph/sourcegraph/pkg/api"
	"github.com/sourcegraph/sourcegraph/pkg/conf"
	"github.com/sourcegraph/sourcegraph/schema"
	log15 "gopkg.in/inconshreveable/log15.v2"
)

type phabRepo struct {
	Fields *struct {
		// e.g. "git"
		VCS string `json:"vcs"`
		// phab unique short name
		Callsign string `json:"callsign"`
		// "active" || "inactive"
		Status string `json:"status"`
	} `json:"fields"`
	Attachments *struct {
		URIs *struct {
			URIs []*struct {
				Fields *struct {
					URI *struct {
						Raw        string `json:"raw"`
						Normalized string `json:"normalized"`
					} `json:"uri"`
					Builtin *struct {
						Protocol   *string `json:"protocol"`
						Identifier *string `json:"identifier"`
					} `json:"builtin"`
				} `json:"fields"`
			} `json:"uris"`
		} `json:"uris"`
	} `json:"attachments"`
}

type phabRepoLookupResponse struct {
	Data   []*phabRepo `json:"data"`
	Cursor *struct {
		Limit  int     `json:"limit"`
		After  *string `json:"after"`
		Before *string `json:"before"`
	} `json:"cursor"`
}

type phabAPIResponse struct {
	Result    *phabRepoLookupResponse `json:"result"`
	Error     *string                 `json:"error_code"`
	ErrorInfo *string                 `json:"error_info"`
}

// RunPhabricatorRepositorySyncWorker runs the worker that syncs repositories from Phabricator to Sourcegraph
func RunPhabricatorRepositorySyncWorker(ctx context.Context) {
	for {
		for i, c := range conf.Get().Phabricator {
			if c.Token == "" {
				continue
			}

			after := ""
			for {
				log15.Info("RunPhabricatorRepositorySyncWorker:fetchPhabRepos", "ith", i, "total", len(conf.Get().Phabricator))
				res, err := fetchPhabRepos(ctx, c, after)
				if err != nil {
					log15.Error("Error fetching Phabricator repos", "err", err)
					break
				}
				err = updatePhabRepos(ctx, c, res.Data)
				if err != nil {
					log15.Error("Error updating Phabricator repos", "err", err)
				}
				phabricatorUpdateTime.WithLabelValues(c.Url).Set(float64(time.Now().Unix()))

				if res.Cursor.After == nil {
					break
				}
				after = *res.Cursor.After
			}

		}
		time.Sleep(getUpdateInterval())
	}
}

func fetchPhabRepos(ctx context.Context, cfg *schema.Phabricator, after string) (*phabRepoLookupResponse, error) {
	form := url.Values{}
	form.Add("output", "json")
	form.Add("params[__conduit__]", `{"token": "`+cfg.Token+`"}`)
	form.Add("params[attachments]", `{"uris": true}`)
	if after != "" {
		form.Add("params[after]", after)
	}
	resp, err := ctxhttp.PostForm(ctx, nil, strings.TrimSuffix(cfg.Url, "/")+"/api/diffusion.repository.search", form)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	respBody, err := ioutil.ReadAll(resp.Body)
	if err != nil {
		return nil, err
	}

	res := phabAPIResponse{}
	err = json.Unmarshal(respBody, &res)
	if err != nil {
		return nil, err
	}

	if res.Error != nil {
		return nil, fmt.Errorf("phab error %s: %s", *res.Error, *res.ErrorInfo)
	}
	return res.Result, nil
}

// updatePhabRepos ensures that all provided repositories exist in the phabricator_repos table.
func updatePhabRepos(ctx context.Context, cfg *schema.Phabricator, repos []*phabRepo) error {
	for _, repo := range repos {
		if repo.Fields.VCS != "git" {
			continue
		}
		if repo.Fields.Status == "inactive" {
			continue
		}
		var uri string
		for _, u := range repo.Attachments.URIs.URIs {
			// Phabricator may list multiple URIs for a repo, some of which are internal Phabricator resources.
			// We select the first URI which doesn't have `builtin` fields (as those only come from internal Phab
			// resources).
			if u.Fields.Builtin != nil && u.Fields.Builtin.Identifier != nil {
				continue
			}
			uri = u.Fields.URI.Normalized
			break
		}
		if uri == "" {
			// some repos have no attachments
			return nil
		}

		err := api.InternalClient.PhabricatorRepoCreate(ctx, api.RepoURI(uri), repo.Fields.Callsign, cfg.Url)
		if err != nil {
			return err
		}
	}

	return nil
}
