// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/*!
Pulls a brief description out of a long description.

If the first paragraph of a long description is short enough then it
is interpreted as the brief description.
*/

use doc::ItemUtils;
use doc;
use pass::Pass;
use text_pass;

use core::option::Some;
use core::str;

pub fn mk_pass() -> Pass {
    text_pass::mk_pass(~"trim", |s| str::trim(s) )
}

#[test]
fn should_trim_text() {
    let doc = test::mk_doc(~"#[doc = \" desc \"] \
                            mod m {
                            }");
    assert doc.cratemod().mods()[0].desc() == Some(~"desc");
}

#[cfg(test)]
mod test {
    use astsrv;
    use attr_pass;
    use doc;
    use extract;
    use trim_pass::mk_pass;

    pub fn mk_doc(source: ~str) -> doc::Doc {
        do astsrv::from_str(source) |srv| {
            let doc = extract::from_srv(srv, ~"");
            let doc = (attr_pass::mk_pass().f)(srv, doc);
            (mk_pass().f)(srv, doc)
        }
    }
}
