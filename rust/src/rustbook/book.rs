// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Basic data structures for representing a book.

use std::io::BufferedReader;
use std::iter;
use std::iter::AdditiveIterator;
use regex::Regex;

pub struct BookItem {
    pub title: String,
    pub path: Path,
    pub path_to_root: Path,
    pub children: Vec<BookItem>,
}

pub struct Book {
    pub chapters: Vec<BookItem>,
}

/// A depth-first iterator over a book.
pub struct BookItems<'a> {
    cur_items: &'a [BookItem],
    cur_idx: usize,
    stack: Vec<(&'a [BookItem], usize)>,
}

impl<'a> Iterator for BookItems<'a> {
    type Item = (String, &'a BookItem);

    fn next(&mut self) -> Option<(String, &'a BookItem)> {
        loop {
            if self.cur_idx >= self.cur_items.len() {
                match self.stack.pop() {
                    None => return None,
                    Some((parent_items, parent_idx)) => {
                        self.cur_items = parent_items;
                        self.cur_idx = parent_idx + 1;
                    }
                }
            } else {
                let cur = self.cur_items.get(self.cur_idx).unwrap();

                let mut section = "".to_string();
                for &(_, idx) in self.stack.iter() {
                    section.push_str(&(idx + 1).to_string()[]);
                    section.push('.');
                }
                section.push_str(&(self.cur_idx + 1).to_string()[]);
                section.push('.');

                self.stack.push((self.cur_items, self.cur_idx));
                self.cur_items = &cur.children[];
                self.cur_idx = 0;
                return Some((section, cur))
            }
        }
    }
}

impl Book {
    pub fn iter(&self) -> BookItems {
        BookItems {
            cur_items: &self.chapters[],
            cur_idx: 0,
            stack: Vec::new(),
        }
    }
}

/// Construct a book by parsing a summary (markdown table of contents).
pub fn parse_summary<R: Reader>(input: R, src: &Path) -> Result<Book, Vec<String>> {
    fn collapse(stack: &mut Vec<BookItem>,
                top_items: &mut Vec<BookItem>,
                to_level: usize) {
        loop {
            if stack.len() < to_level { return }
            if stack.len() == 1 {
                top_items.push(stack.pop().unwrap());
                return;
            }

            let tip = stack.pop().unwrap();
            let last = stack.len() - 1;
            stack[last].children.push(tip);
        }
    }

    let regex = r"(?P<indent>[\t ]*)\*[:space:]*\[(?P<title>.*)\]\((?P<path>.*)\)";
    let item_re = Regex::new(regex).unwrap();
    let mut top_items = vec!();
    let mut stack = vec!();
    let mut errors = vec!();

    // always include the introduction
    top_items.push(BookItem {
        title: "Introduction".to_string(),
        path: Path::new("README.md"),
        path_to_root: Path::new("."),
        children: vec!(),
    });

    for line_result in BufferedReader::new(input).lines() {
        let line = match line_result {
            Ok(line) => line,
            Err(err) => {
                errors.push(err.desc.to_string()); // FIXME: include detail
                return Err(errors);
            }
        };

        item_re.captures(&line[]).map(|cap| {
            let given_path = cap.name("path");
            let title = cap.name("title").unwrap().to_string();

            let path_from_root = match src.join(given_path.unwrap()).path_relative_from(src) {
                Some(p) => p,
                None => {
                    errors.push(format!("Paths in SUMMARY.md must be relative, \
                                         but path '{}' for section '{}' is not.",
                                         given_path.unwrap(), title));
                    Path::new("")
                }
            };
            let path_to_root = Path::new(iter::repeat("../")
                                             .take(path_from_root.components().count() - 1)
                                             .collect::<String>());
            let item = BookItem {
                title: title,
                path: path_from_root,
                path_to_root: path_to_root,
                children: vec!(),
            };
            let level = cap.name("indent").unwrap().chars().map(|c| {
                match c {
                    ' ' => 1us,
                    '\t' => 4,
                    _ => unreachable!()
                }
            }).sum() / 4 + 1;

            if level > stack.len() + 1 {
                // FIXME: better error message
                errors.push(format!("Section '{}' is indented too many levels.", item.title));
            } else if level <= stack.len() {
                collapse(&mut stack, &mut top_items, level);
            }
            stack.push(item)
        });
    }

    if errors.is_empty() {
        collapse(&mut stack, &mut top_items, 1);
        Ok(Book { chapters: top_items })
    } else {
        Err(errors)
    }
}
