#[doc = "Generate markdown from a document tree"];

import std::io;
import std::io::writer_util;

export mk_pass;

fn mk_pass(
    writer: fn~() -> io::writer
) -> pass {
    ret fn~(
        _srv: astsrv::srv,
        doc: doc::cratedoc
    ) -> doc::cratedoc {
        write_markdown(doc, writer());
        doc
    };
}

type ctxt = {
    w: io::writer,
    mutable depth: uint
};

fn write_markdown(
    doc: doc::cratedoc,
    writer: io::writer
) {
    let ctxt = {
        w: writer,
        mutable depth: 1u
    };

    write_crate(ctxt, doc);
}

fn write_header(ctxt: ctxt, title: str) {
    let hashes = str::from_chars(vec::init_elt('#', ctxt.depth));
    ctxt.w.write_line(#fmt("%s %s", hashes, title));
    ctxt.w.write_line("");
}

fn subsection(ctxt: ctxt, f: fn&()) {
    ctxt.depth += 1u;
    f();
    ctxt.depth -= 1u;
}

fn write_crate(
    ctxt: ctxt,
    doc: doc::cratedoc
) {
    write_header(ctxt, #fmt("Crate %s", doc.topmod.name));
    write_top_module(ctxt, doc.topmod);
}

fn write_top_module(
    ctxt: ctxt,
    moddoc: doc::moddoc
) {
    write_mod_contents(ctxt, moddoc);
}

fn write_mod(
    ctxt: ctxt,
    moddoc: doc::moddoc
) {
    write_header(ctxt, #fmt("Module `%s`", moddoc.name));
    write_mod_contents(ctxt, moddoc);
}

fn write_mod_contents(
    ctxt: ctxt,
    moddoc: doc::moddoc
) {
    for fndoc in *moddoc.fns {
        subsection(ctxt) {||
            write_fn(ctxt, fndoc);
        }
    }

    for moddoc in *moddoc.mods {
        subsection(ctxt) {||
            write_mod(ctxt, moddoc);
        }
    }
}

fn write_fn(
    ctxt: ctxt,
    doc: doc::fndoc
) {
    write_header(ctxt, #fmt("Function `%s`", doc.name));
    write_brief(ctxt, doc.brief);
    write_desc(ctxt, doc.desc);
    write_args(ctxt, doc.args);
    write_return(ctxt, doc.return);
}

fn write_brief(
    ctxt: ctxt,
    brief: option<str>
) {
    alt brief {
      some(brief) {
        ctxt.w.write_line(brief);
        ctxt.w.write_line("");
      }
      none. { }
    }
}

fn write_desc(
    ctxt: ctxt,
    desc: option<str>
) {
    alt desc {
        some(_d) {
            ctxt.w.write_line("");
            ctxt.w.write_line(_d);
            ctxt.w.write_line("");
        }
        none. { }
    }
}

fn write_args(
    ctxt: ctxt,
    args: [(str, str)]
) {
    for (arg, desc) in args {
        ctxt.w.write_str("### Argument `" + arg + "`: ");
        ctxt.w.write_str(desc)
    }
}

fn write_return(
    ctxt: ctxt,
    return: option<doc::retdoc>
) {
    alt return {
      some(doc) {
        alt doc.ty {
          some(ty) {
            ctxt.w.write_line("### Returns `" + ty + "`");
            alt doc.desc {
              some(d) {
                ctxt.w.write_line(d);
              }
              none. { }
            }
          }
          none. { fail "unimplemented"; }
        }
      }
      none. { }
    }
}

#[cfg(test)]
mod tests {
    fn render(source: str) -> str {
        let srv = astsrv::mk_srv_from_str(source);
        let doc = extract::from_srv(srv, "");
        let doc = attr_pass::mk_pass()(srv, doc);
        write_markdown_str(doc)
    }

    fn write_markdown_str(
        doc: doc::cratedoc
    ) -> str {
        let buffer = io::mk_mem_buffer();
        let writer = io::mem_buffer_writer(buffer);
        write_markdown(doc, writer);
        ret io::mem_buffer_str(buffer);
    }

    #[test]
    fn write_markdown_should_write_crate_header() {
        let srv = astsrv::mk_srv_from_str("");
        let doc = extract::from_srv(srv, "belch");
        let doc = attr_pass::mk_pass()(srv, doc);
        let markdown = write_markdown_str(doc);
        assert str::contains(markdown, "# Crate belch");
    }

    #[test]
    fn write_markdown_should_write_function_header() {
        let markdown = render("fn func() { }");
        assert str::contains(markdown, "## Function `func`");
    }

    #[test]
    fn write_markdown_should_write_mod_headers() {
        let markdown = render("mod moo { }");
        assert str::contains(markdown, "## Module `moo`");
    }

    #[test]
    fn should_leave_blank_line_after_header() {
        let markdown = render("mod morp { }");
        assert str::contains(markdown, "Module `morp`\n\n");
    }

    #[test]
    fn should_leave_blank_line_between_fn_header_and_brief() {
        let markdown = render("#[doc(brief = \"brief\")] fn a() { }");
        assert str::contains(markdown, "Function `a`\n\nbrief");
    }

    #[test]
    fn should_leve_blank_line_after_brief() {
        let markdown = render("#[doc(brief = \"brief\")] fn a() { }");
        assert str::contains(markdown, "brief\n\n");
    }
}