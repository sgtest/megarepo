// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Breaks rustdocs into sections according to their headers

use core::prelude::*;

use astsrv;
use doc::ItemUtils;
use doc;
use fold::Fold;
use fold;
use pass::Pass;

use core::iterator::IteratorUtil;

pub fn mk_pass() -> Pass {
    Pass {
        name: ~"sectionalize",
        f: run
    }
}

pub fn run(_srv: astsrv::Srv, doc: doc::Doc) -> doc::Doc {
    let fold = Fold {
        fold_item: fold_item,
        fold_trait: fold_trait,
        fold_impl: fold_impl,
        .. fold::default_any_fold(())
    };
    (fold.fold_doc)(&fold, doc)
}

fn fold_item(fold: &fold::Fold<()>, doc: doc::ItemDoc) -> doc::ItemDoc {
    let doc = fold::default_seq_fold_item(fold, doc);
    let (desc, sections) = sectionalize(copy doc.desc);

    doc::ItemDoc {
        desc: desc,
        sections: sections,
        .. doc
    }
}

fn fold_trait(fold: &fold::Fold<()>, doc: doc::TraitDoc) -> doc::TraitDoc {
    let doc = fold::default_seq_fold_trait(fold, doc);

    doc::TraitDoc {
        methods: do doc.methods.map |method| {
            let (desc, sections) = sectionalize(copy method.desc);

            doc::MethodDoc {
                desc: desc,
                sections: sections,
                .. copy *method
            }
        },
        .. doc
    }
}

fn fold_impl(fold: &fold::Fold<()>, doc: doc::ImplDoc) -> doc::ImplDoc {
    let doc = fold::default_seq_fold_impl(fold, doc);

    doc::ImplDoc {
        methods: do doc.methods.map |method| {
            let (desc, sections) = sectionalize(copy method.desc);

            doc::MethodDoc {
                desc: desc,
                sections: sections,
                .. copy *method
            }
        },
        .. doc
    }
}

fn sectionalize(desc: Option<~str>) -> (Option<~str>, ~[doc::Section]) {

    /*!
     * Take a description of the form
     *
     *     General text
     *
     *     # Section header
     *
     *     Section text
     *
     *     # Section header
     *
     *     Section text
     *
     * and remove each header and accompanying text into section records.
     */

    if desc.is_none() {
        return (None, ~[]);
    }

    let mut new_desc = None::<~str>;
    let mut current_section = None;
    let mut sections = ~[];

    for desc.get_ref().any_line_iter().advance |line| {
        match parse_header(line) {
          Some(header) => {
            if current_section.is_some() {
                sections.push(copy *current_section.get_ref());
            }
            current_section = Some(doc::Section {
                header: header.to_owned(),
                body: ~""
            });
          }
          None => {
            match copy current_section {
              Some(section) => {
                current_section = Some(doc::Section {
                    body: fmt!("%s\n%s", section.body, line),
                    .. section
                });
              }
              None => {
                new_desc = match copy new_desc {
                  Some(desc) => {
                    Some(fmt!("%s\n%s", desc, line))
                  }
                  None => {
                    Some(line.to_owned())
                  }
                };
              }
            }
          }
        }
    }

    if current_section.is_some() {
        sections.push(current_section.unwrap());
    }

    (new_desc, sections)
}

fn parse_header<'a>(line: &'a str) -> Option<&'a str> {
    if line.starts_with("# ") {
        Some(line.slice_from(2))
    } else {
        None
    }
}



#[cfg(test)]
mod test {
    use core::prelude::*;

    use astsrv;
    use attr_pass;
    use doc;
    use extract;
    use sectionalize_pass::run;

    fn mk_doc(source: ~str) -> doc::Doc {
        do astsrv::from_str(copy source) |srv| {
            let doc = extract::from_srv(srv.clone(), ~"");
            let doc = (attr_pass::mk_pass().f)(srv.clone(), doc);
            run(srv.clone(), doc)
        }
    }

    #[test]
    fn should_create_section_headers() {
        let doc = mk_doc(
            ~"#[doc = \"\
              # Header\n\
              Body\"]\
              mod a {
}");
        assert!(doc.cratemod().mods()[0].item.sections[0].header.contains("Header"));
    }

    #[test]
    fn should_create_section_bodies() {
        let doc = mk_doc(
            ~"#[doc = \"\
              # Header\n\
              Body\"]\
              mod a {
}");
        assert!(doc.cratemod().mods()[0].item.sections[0].body.contains("Body"));
    }

    #[test]
    fn should_not_create_sections_from_indented_headers() {
        let doc = mk_doc(
            ~"#[doc = \"\n\
              Text\n             # Header\n\
              Body\"]\
              mod a {
}");
        assert!(doc.cratemod().mods()[0].item.sections.is_empty());
    }

    #[test]
    fn should_remove_section_text_from_main_desc() {
        let doc = mk_doc(
            ~"#[doc = \"\
              Description\n\n\
              # Header\n\
              Body\"]\
              mod a {
}");
        assert!(!doc.cratemod().mods()[0].desc().get().contains("Header"));
        assert!(!doc.cratemod().mods()[0].desc().get().contains("Body"));
    }

    #[test]
    fn should_eliminate_desc_if_it_is_just_whitespace() {
        let doc = mk_doc(
            ~"#[doc = \"\
              # Header\n\
              Body\"]\
              mod a {
}");
        assert_eq!(doc.cratemod().mods()[0].desc(), None);
    }

    #[test]
    fn should_sectionalize_trait_methods() {
        let doc = mk_doc(
            ~"trait i {
#[doc = \"\
              # Header\n\
              Body\"]\
              fn a(); }");
        assert_eq!(doc.cratemod().traits()[0].methods[0].sections.len(), 1u);
    }

    #[test]
    fn should_sectionalize_impl_methods() {
        let doc = mk_doc(
            ~"impl bool {
#[doc = \"\
              # Header\n\
              Body\"]\
              fn a() { } }");
        assert_eq!(doc.cratemod().impls()[0].methods[0].sections.len(), 1u);
    }
}
