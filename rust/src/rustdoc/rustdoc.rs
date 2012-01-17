/* rustdoc: rust -> markdown translator
 * Copyright 2011 Google Inc.
 */

use std;
use rustc;

type pass = fn~(srv: astsrv::seq_srv, doc: doc::cratedoc) -> doc::cratedoc;

fn run_passes(
    srv: astsrv::seq_srv,
    doc: doc::cratedoc,
    passes: [pass]
) -> doc::cratedoc {
    vec::foldl(doc, passes) {|doc, pass|
        pass(srv, doc)
    }
}

#[test]
fn test_run_passes() {
    import astsrv::seq_srv;
    fn pass1(
        _srv: astsrv::seq_srv,
        doc: doc::cratedoc
    ) -> doc::cratedoc {
        ~{
            topmod: ~{
                name: doc.topmod.name + "two",
                mods: doc::modlist([]),
                fns: doc::fnlist([])
            }
        }
    }
    fn pass2(
        _srv: astsrv::seq_srv,
        doc: doc::cratedoc
    ) -> doc::cratedoc {
        ~{
            topmod: ~{
                name: doc.topmod.name + "three",
                mods: doc::modlist([]),
                fns: doc::fnlist([])
            }
        }
    }
    let source = "";
    let srv = astsrv::mk_seq_srv_from_str(source);
    let passes = [pass1, pass2];
    let doc = extract::from_srv(srv, "one");
    let doc = run_passes(srv, doc, passes);
    assert doc.topmod.name == "onetwothree";
}

#[doc(
  brief = "Main function.",
  desc = "Command-line arguments:

*  argv[1]: crate file name",
  args(argv = "Command-line arguments.")
)]
fn main(argv: [str]) {

    if vec::len(argv) != 2u {
        std::io::println(#fmt("usage: %s <input>", argv[0]));
        ret;
    }

    let source_file = argv[1];
    let default_name = source_file;
    let crate = parse::from_file(source_file);
    let doc = extract::extract(crate, default_name);
    let doc = tystr_pass::run(doc, crate);
    gen::write_markdown(doc, std::io::stdout());
}
