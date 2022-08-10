#![allow(macro_expanded_macro_exports_accessed_by_absolute_paths)]

#[macro_use]
extern crate rocket;

use rocket::serde::json::{json, Json, Value as JsonValue};
use sg_syntax::{ScipHighlightQuery, SourcegraphQuery};

#[post("/", format = "application/json", data = "<q>")]
fn syntect(q: Json<SourcegraphQuery>) -> JsonValue {
    // TODO(slimsag): In an ideal world we wouldn't be relying on catch_unwind
    // and instead Syntect would return Result types when failures occur. This
    // will require some non-trivial work upstream:
    // https://github.com/trishume/syntect/issues/98
    let result = std::panic::catch_unwind(|| sg_syntax::syntect_highlight(q.into_inner()));
    match result {
        Ok(v) => v,
        Err(_) => json!({"error": "panic while highlighting code", "code": "panic"}),
    }
}

// TODO: Once we're confident we don't need this anymore, we can remove this entirely
// and just have the `scip` endpoint. But I figured I would make it available at least
// for now, since I'm working on doing that.
#[post("/lsif", format = "application/json", data = "<q>")]
fn lsif(q: Json<SourcegraphQuery>) -> JsonValue {
    match sg_syntax::lsif_highlight(q.into_inner()) {
        Ok(v) => v,
        Err(err) => err,
    }
}

#[post("/scip", format = "application/json", data = "<q>")]
fn scip(q: Json<ScipHighlightQuery>) -> JsonValue {
    match sg_syntax::scip_highlight(q.into_inner()) {
        Ok(v) => v,
        Err(err) => err,
    }
}

#[get("/health")]
fn health() -> &'static str {
    "OK"
}

#[catch(404)]
fn not_found() -> JsonValue {
    json!({"error": "resource not found", "code": "resource_not_found"})
}

#[launch]
fn rocket() -> _ {
    // Only list features if QUIET != "true"
    match std::env::var("QUIET") {
        Ok(v) if v == "true" => {}
        _ => sg_syntax::list_features(),
    };

    rocket::build()
        .mount("/", routes![syntect, lsif, scip, health])
        .register("/", catchers![not_found])
}
