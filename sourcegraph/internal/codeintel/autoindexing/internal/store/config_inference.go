package store

import (
	"context"
	"strings"

	"github.com/keegancsmith/sqlf"
	"go.opentelemetry.io/otel/attribute"

	"github.com/sourcegraph/sourcegraph/internal/database/basestore"
	"github.com/sourcegraph/sourcegraph/internal/observation"
)

func (s *store) GetInferenceScript(ctx context.Context) (_ string, err error) {
	ctx, _, endObservation := s.operations.getInferenceScript.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})

	script, _, err := basestore.ScanFirstNullString(s.db.Query(ctx, sqlf.Sprintf(getInferenceScriptQuery)))
	if err != nil {
		return "", err
	}
	if script == "" {
		script = strings.TrimSpace(defaultScript) + "\n"
	}

	return script, nil
}

const getInferenceScriptQuery = `
SELECT script
FROM codeintel_inference_scripts
ORDER BY insert_timestamp DESC
LIMIT 1
`

func (s *store) SetInferenceScript(ctx context.Context, script string) (err error) {
	ctx, _, endObservation := s.operations.setInferenceScript.With(ctx, &err, observation.Args{Attrs: []attribute.KeyValue{
		attribute.Int("scriptSize", len(script)),
	}})
	defer endObservation(1, observation.Args{})

	return s.db.Exec(ctx, sqlf.Sprintf(setInferenceScriptQuery, script))
}

const setInferenceScriptQuery = `
INSERT INTO codeintel_inference_scripts (script)
VALUES(%s)
`

//
//

const defaultScript = `
local path = require("path")
local pattern = require("sg.autoindex.patterns")
local recognizer = require("sg.autoindex.recognizer")

local custom_recognizer = recognizer.new_path_recognizer {
	patterns = {
		pattern.new_path_basename("acme-custom.yaml")
	},

	-- Invoked with paths matching acme-custom.yaml anywhere in repo
	generate = function(_, paths)
		local jobs = {}
		for i = 1, #paths do
			table.insert(jobs, {
				steps = {},
				root = path.dirname(paths[i]),
				indexer = "acme/custom-indexer",
				indexer_args = {},
				outfile = "",
			})
		end

		return jobs
	end,
}

return require("sg.autoindex.config").new({
	-- ["sg.clang"] = false,
	-- ["sg.go"] = false,
	-- ["sg.java"] = false,
	-- ["sg.python"] = false,
	-- ["sg.ruby"] = false,
	-- ["sg.rust"] = false,
	-- ["sg.typescript"] = false,
	["acme.custom"] = custom_recognizer,
})
`
