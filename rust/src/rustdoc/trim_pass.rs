/*!
Pulls a brief description out of a long description.

If the first paragraph of a long description is short enough then it
is interpreted as the brief description.
*/

use doc::ItemUtils;

export mk_pass;

fn mk_pass() -> Pass {
    text_pass::mk_pass(~"trim", |s| str::trim(s) )
}

#[test]
fn should_trim_text() {
    let doc = test::mk_doc(~"#[doc = \" desc \"] \
                            mod m {
                                #[legacy_exports]; }");
    assert doc.cratemod().mods()[0].desc() == Some(~"desc");
}

#[cfg(test)]
mod test {
    #[legacy_exports];
    fn mk_doc(source: ~str) -> doc::Doc {
        do astsrv::from_str(source) |srv| {
            let doc = extract::from_srv(srv, ~"");
            let doc = attr_pass::mk_pass().f(srv, doc);
            mk_pass().f(srv, doc)
        }
    }
}
