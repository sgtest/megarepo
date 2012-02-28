#[doc = "Sorts items by type"];

export mk_pass;

fn mk_pass() -> pass {
    sort_pass::mk_pass("sort_item_type") { |item1, item2|
        fn score(item: doc::itemtag) -> int {
            alt item {
              doc::consttag(_) { 0 }
              doc::tytag(_) { 1 }
              doc::enumtag(_) { 2 }
              doc::restag(_) { 3 }
              doc::ifacetag(_) { 4 }
              doc::impltag(_) { 5 }
              doc::fntag(_) { 6 }
              doc::modtag(_) { 7 }
              doc::nmodtag(_) { 8 }
            }
        }

        score(item1) <= score(item2)
    }
}

#[test]
fn test() {
    let source =
        "mod imod { } \
         native mod inmod { } \
         const iconst: int = 0; \
         fn ifn() { } \
         enum ienum { ivar } \
         resource ires(a: bool) { } \
         iface iiface { fn a(); } \
         impl iimpl for int { fn a() { } } \
         type itype = int;";
    astsrv::from_str(source) {|srv|
        let doc = extract::from_srv(srv, "");
        let doc = mk_pass().f(srv, doc);
        assert doc.topmod.items[0].name() == "iconst";
        assert doc.topmod.items[1].name() == "itype";
        assert doc.topmod.items[2].name() == "ienum";
        assert doc.topmod.items[3].name() == "ires";
        assert doc.topmod.items[4].name() == "iiface";
        assert doc.topmod.items[5].name() == "iimpl";
        assert doc.topmod.items[6].name() == "ifn";
        assert doc.topmod.items[7].name() == "imod";
        assert doc.topmod.items[8].name() == "inmod";
    }
}
