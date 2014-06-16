// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::fmt;
use std::io;

#[deriving(Clone)]
pub struct Layout {
    pub logo: String,
    pub favicon: String,
    pub krate: String,
    pub playground_url: String,
}

pub struct Page<'a> {
    pub title: &'a str,
    pub ty: &'a str,
    pub root_path: &'a str,
}

pub fn render<T: fmt::Show, S: fmt::Show>(
    dst: &mut io::Writer, layout: &Layout, page: &Page, sidebar: &S, t: &T)
    -> io::IoResult<()>
{
    write!(dst,
r##"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <meta name="description" content="The {krate} library documentation.">

    <title>{title}</title>

    <link href='http://fonts.googleapis.com/css?family=Source+Code+Pro:400,600'
          rel='stylesheet' type='text/css'>
    <link rel="stylesheet" type="text/css" href="{root_path}main.css">

    {favicon}
</head>
<body>
    <!--[if lte IE 8]>
    <div class="warning">
        This old browser is unsupported and will most likely display funky
        things.
    </div>
    <![endif]-->

    <section class="sidebar">
        {logo}
        {sidebar}
    </section>

    <nav class="sub">
        <form class="search-form js-only">
            <div class="search-container">
                <input class="search-input" name="search"
                       autocomplete="off"
                       placeholder="Search documentation..."
                       type="search">
            </div>
        </form>
    </nav>

    <section id='main' class="content {ty}">{content}</section>
    <section id='search' class="content hidden"></section>

    <section class="footer"></section>

    <div id="help" class="hidden">
        <div class="shortcuts">
            <h1>Keyboard shortcuts</h1>
            <dl>
                <dt>?</dt>
                <dd>Show this help dialog</dd>
                <dt>S</dt>
                <dd>Focus the search field</dd>
                <dt>&uarr;</dt>
                <dd>Move up in search results</dd>
                <dt>&darr;</dt>
                <dd>Move down in search results</dd>
                <dt>&#9166;</dt>
                <dd>Go to active search result</dd>
            </dl>
        </div>
        <div class="infos">
            <h1>Search tricks</h1>
            <p>
                Prefix searches with a type followed by a colon (e.g.
                <code>fn:</code>) to restrict the search to a given type.
            </p>
            <p>
                Accepted types are: <code>fn</code>, <code>mod</code>,
                <code>struct</code> (or <code>str</code>), <code>enum</code>,
                <code>trait</code>, <code>typedef</code> (or
                <code>tdef</code>).
            </p>
        </div>
    </div>

    <script>
        window.rootPath = "{root_path}";
        window.currentCrate = "{krate}";
        window.playgroundUrl = "{play_url}";
    </script>
    <script src="{root_path}jquery.js"></script>
    <script src="{root_path}main.js"></script>
    {play_js}
    <script async src="{root_path}search-index.js"></script>
</body>
</html>"##,
    content   = *t,
    root_path = page.root_path,
    ty        = page.ty,
    logo      = if layout.logo.len() == 0 {
        "".to_string()
    } else {
        format!("<a href='{}{}/index.html'>\
                 <img src='{}' alt='' width='100'></a>",
                page.root_path, layout.krate,
                layout.logo)
    },
    title     = page.title,
    favicon   = if layout.favicon.len() == 0 {
        "".to_string()
    } else {
        format!(r#"<link rel="shortcut icon" href="{}">"#, layout.favicon)
    },
    sidebar   = *sidebar,
    krate     = layout.krate,
    play_url  = layout.playground_url,
    play_js   = if layout.playground_url.len() == 0 {
        "".to_string()
    } else {
        format!(r#"<script src="{}playpen.js"></script>"#, page.root_path)
    },
    )
}

pub fn redirect(dst: &mut io::Writer, url: &str) -> io::IoResult<()> {
    write!(dst,
r##"<!DOCTYPE html>
<html lang="en">
<head>
    <meta http-equiv="refresh" content="0;URL={url}">
</head>
<body>
</body>
</html>"##,
    url = url,
    )
}
