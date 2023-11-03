package entity

//-----------------------------------------------------------------------------------------------------
// NOTE: the object store is in heavy development, and the locations will likely continue to move
//-----------------------------------------------------------------------------------------------------

import (
	"context"
)

const (
	StandardKindDashboard   = "dashboard"
	StandardKindPlaylist    = "playlist"
	StandardKindSnapshot    = "snapshot"
	StandardKindFolder      = "folder"
	StandardKindPreferences = "preferences"

	// StandardKindDataSource: not a real kind yet, but used to define references from dashboards
	// Types: influx, prometheus, testdata, ...
	StandardKindDataSource = "ds"

	// StandardKindPanel: only used for searchV2 right now
	// Standalone panel is not an object kind yet -- library panel, or nested in dashboard
	StandardKindPanel = "panel"

	// entity.StandardKindSVG SVG file support
	StandardKindSVG = "svg"

	// StandardKindPNG PNG file support
	StandardKindPNG = "png"

	// StandardKindGeoJSON represents spatial data
	StandardKindGeoJSON = "geojson"

	// StandardKindDataFrame data frame
	StandardKindDataFrame = "frame"

	// StandardKindJSONObj generic json object
	StandardKindJSONObj = "jsonobj"

	// StandardKindQuery early development on panel query library
	// the kind may need to change to better encapsulate { targets:[], transforms:[] }
	StandardKindQuery = "query"

	// StandardKindAlertRule is not a real kind. It's used to refer to alert rules, for instance
	// in the folder registry service.
	StandardKindAlertRule = "alertrule"

	// StandardKindLibraryPanel is for library panels
	StandardKindLibraryPanel = "librarypanel"

	//----------------------------------------
	// References are referenced from objects
	//----------------------------------------

	// ExternalEntityReferencePlugin: requires a plugin to be installed
	ExternalEntityReferencePlugin = "plugin"

	// ExternalEntityReferenceRuntime: frontend runtime requirements
	ExternalEntityReferenceRuntime = "runtime"

	// ExternalEntityReferenceRuntime_Transformer is a "type" under runtime
	// UIDs include: joinByField, organize, seriesToColumns, etc
	ExternalEntityReferenceRuntime_Transformer = "transformer"
)

// EntityKindInfo describes information needed from the object store
// All non-raw types will have a schema that can be used to validate
type EntityKindInfo struct {
	// Unique short id for this kind
	ID string `json:"id,omitempty"`

	// Display name (may be equal to the ID)
	Name string `json:"name,omitempty"`

	// Kind description
	Description string `json:"description,omitempty"`

	// The format is not controlled by a schema
	IsRaw bool `json:"isRaw,omitempty"`

	// The preferred save extension (svg, png, parquet, etc) if one exists
	FileExtension string `json:"fileExtension,omitempty"`

	// The correct mime-type to return for raw objects
	MimeType string `json:"mimeType,omitempty"`
}

// EntitySummaryBuilder will read an object, validate it, and return a summary, sanitized payload, or an error
// This should not include values that depend on system state, only the raw object
type EntitySummaryBuilder = func(ctx context.Context, uid string, body []byte) (*EntitySummary, []byte, error)
