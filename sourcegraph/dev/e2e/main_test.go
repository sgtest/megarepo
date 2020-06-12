// +build e2e

package main

import (
	"flag"
	"log"
	"os"
	"strings"
	"testing"

	"github.com/inconshreveable/log15"
	jsoniter "github.com/json-iterator/go"

	"github.com/sourcegraph/sourcegraph/internal/e2eutil"
)

/*
	NOTE: For easier testing, run Sourcegraph instance without volume:
			docker run --publish 7080:7080 --rm sourcegraph/server:insiders
*/

var client *e2eutil.Client

var (
	baseURL  = flag.String("base-url", "http://127.0.0.1:7080", "The base URL of the Sourcegraph instance")
	email    = flag.String("email", "e2e@sourcegraph.com", "The email of the admin user")
	username = flag.String("username", "e2e-admin", "The username of the admin user")
	password = flag.String("password", "supersecurepassword", "The password of the admin user")

	githubToken = flag.String("github-token", os.Getenv("GITHUB_TOKEN"), "The GitHub personal access token that will be used to authenticate a GitHub external service")
)

func TestMain(m *testing.M) {
	flag.Parse()

	*baseURL = strings.TrimSuffix(*baseURL, "/")

	needsSiteInit, err := e2eutil.NeedsSiteInit(*baseURL)
	if err != nil {
		log.Fatal("Failed to check if site needs init: ", err)
	}

	if needsSiteInit {
		client, err = e2eutil.SiteAdminInit(*baseURL, *email, *username, *password)
		if err != nil {
			log.Fatal("Failed to create site admin: ", err)
		}
		log.Println("Site admin has been created:", *username)
	} else {
		client, err = e2eutil.SignIn(*baseURL, *email, *password)
		if err != nil {
			log.Fatal("Failed to sign in:", err)
		}
		log.Println("Site admin authenticated:", *username)
	}

	if !testing.Verbose() {
		log15.Root().SetHandler(log15.DiscardHandler())
	}
	os.Exit(m.Run())
}

func mustMarshalJSONString(v interface{}) string {
	str, err := jsoniter.MarshalToString(v)
	if err != nil {
		panic(err)
	}
	return str
}
