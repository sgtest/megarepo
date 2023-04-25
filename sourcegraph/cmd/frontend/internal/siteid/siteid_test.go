package siteid

import (
	"fmt"
	"testing"

	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

func TestNotInited(t *testing.T) {
	if inited {
		t.Fatal("one of this test package's imports called Init, but these tests require that it has not yet been called")
	}
}

func TestGet(t *testing.T) {
	reset := func() {
		inited = false
		siteID = ""
		conf.Mock(nil)
	}

	{
		origFatalln := fatalln
		fatalln = func(v ...any) { panic(v) }
		defer func() { fatalln = origFatalln }()
	}

	tryInit := func(db database.DB) (err error) {
		defer func() {
			if e := recover(); e != nil {
				err = errors.Errorf("panic: %v", e)
			}
		}()
		Init(db)
		return nil
	}

	t.Run("from DB", func(t *testing.T) {
		defer reset()
		gss := database.NewMockGlobalStateStore()
		gss.GetFunc.SetDefaultReturn(database.GlobalState{SiteID: "a"}, nil)

		db := database.NewMockDB()
		db.GlobalStateFunc.SetDefaultReturn(gss)

		if err := tryInit(db); err != nil {
			t.Fatal(err)
		}
		if !inited {
			t.Error("!inited")
		}
		if got, want := Get(), "a"; got != want {
			t.Errorf("got %q, want %q", got, want)
		}
	})

	t.Run("panics if DB unavailable", func(t *testing.T) {
		defer reset()
		gss := database.NewMockGlobalStateStore()
		gss.GetFunc.SetDefaultReturn(database.GlobalState{}, errors.New("x"))

		db := database.NewMockDB()
		db.GlobalStateFunc.SetDefaultReturn(gss)

		want := errors.Errorf("panic: [Error initializing global state: x]")
		if err := tryInit(db); fmt.Sprint(err) != fmt.Sprint(want) {
			t.Errorf("got error %q, want %q", err, want)
		}
		if inited {
			t.Error("inited")
		}
		if siteID != "" {
			t.Error("siteID is set")
		}
	})

	t.Run("from env var", func(t *testing.T) {
		defer reset()
		t.Setenv("TRACKING_APP_ID", "a")

		db := database.NewMockDB()

		if err := tryInit(db); err != nil {
			t.Fatal(err)
		}
		if !inited {
			t.Error("!inited")
		}
		if got, want := Get(), "a"; got != want {
			t.Errorf("got %q, want %q", got, want)
		}
	})

	t.Run("env var takes precedence over DB", func(t *testing.T) {
		defer reset()
		t.Setenv("TRACKING_APP_ID", "a")

		gss := database.NewMockGlobalStateStore()
		gss.GetFunc.SetDefaultReturn(database.GlobalState{SiteID: "b"}, nil)

		db := database.NewMockDB()
		db.GlobalStateFunc.SetDefaultReturn(gss)

		if err := tryInit(db); err != nil {
			t.Fatal(err)
		}
		if !inited {
			t.Error("!inited")
		}
		if got, want := Get(), "a"; got != want {
			t.Errorf("got %q, want %q", got, want)
		}
	})
}
