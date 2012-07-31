import result::result;
import std::getopts;

export output_format;
export output_style;
export config;
export default_config;
export parse_config;
export usage;
export markdown, pandoc_html;
export doc_per_crate, doc_per_mod;

/// The type of document to output
enum output_format {
    /// Markdown
    markdown,
    /// HTML, via markdown and pandoc
    pandoc_html
}

/// How to organize the output
enum output_style {
    /// All in a single document
    doc_per_crate,
    /// Each module in its own document
    doc_per_mod
}

/// The configuration for a rustdoc session
type config = {
    input_crate: ~str,
    output_dir: ~str,
    output_format: output_format,
    output_style: output_style,
    pandoc_cmd: option<~str>
};

fn opt_output_dir() -> ~str { ~"output-dir" }
fn opt_output_format() -> ~str { ~"output-format" }
fn opt_output_style() -> ~str { ~"output-style" }
fn opt_pandoc_cmd() -> ~str { ~"pandoc-cmd" }
fn opt_help() -> ~str { ~"h" }

fn opts() -> ~[(getopts::opt, ~str)] {
    ~[
        (getopts::optopt(opt_output_dir()),
         ~"--output-dir <val>     put documents here"),
        (getopts::optopt(opt_output_format()),
         ~"--output-format <val>  either 'markdown' or 'html'"),
        (getopts::optopt(opt_output_style()),
         ~"--output-style <val>   either 'doc-per-crate' or 'doc-per-mod'"),
        (getopts::optopt(opt_pandoc_cmd()),
         ~"--pandoc-cmd <val>     the command for running pandoc"),
        (getopts::optflag(opt_help()),
         ~"-h                     print help")
    ]
}

fn usage() {
    import io::println;

    println(~"Usage: rustdoc ~[options] <cratefile>\n");
    println(~"Options:\n");
    for opts().each |opt| {
        println(fmt!{"    %s", opt.second()});
    }
    println(~"");
}

fn default_config(input_crate: ~str) -> config {
    {
        input_crate: input_crate,
        output_dir: ~".",
        output_format: pandoc_html,
        output_style: doc_per_mod,
        pandoc_cmd: none
    }
}

type program_output = fn~(~str, ~[~str]) ->
    {status: int, out: ~str, err: ~str};

fn mock_program_output(_prog: ~str, _args: ~[~str]) -> {
    status: int, out: ~str, err: ~str
} {
    {
        status: 0,
        out: ~"",
        err: ~""
    }
}

fn parse_config(args: ~[~str]) -> result<config, ~str> {
    parse_config_(args, run::program_output)
}

fn parse_config_(
    args: ~[~str],
    program_output: program_output
) -> result<config, ~str> {
    let args = vec::tail(args);
    let opts = vec::unzip(opts()).first();
    alt getopts::getopts(args, opts) {
        result::ok(match) {
            if vec::len(match.free) == 1u {
                let input_crate = vec::head(match.free);
                config_from_opts(input_crate, match, program_output)
            } else if vec::is_empty(match.free) {
                result::err(~"no crates specified")
            } else {
                result::err(~"multiple crates specified")
            }
        }
        result::err(f) {
            result::err(getopts::fail_str(f))
        }
    }
}

fn config_from_opts(
    input_crate: ~str,
    match: getopts::match,
    program_output: program_output
) -> result<config, ~str> {

    let config = default_config(input_crate);
    let result = result::ok(config);
    let result = do result::chain(result) |config| {
        let output_dir = getopts::opt_maybe_str(match, opt_output_dir());
        result::ok({
            output_dir: option::get_default(output_dir, config.output_dir)
            with config
        })
    };
    let result = do result::chain(result) |config| {
        let output_format = getopts::opt_maybe_str(
            match, opt_output_format());
        do option::map_default(output_format, result::ok(config))
            |output_format| {
            do result::chain(parse_output_format(output_format))
                |output_format| {

                result::ok({
                    output_format: output_format
                    with config
                })
            }
        }
    };
    let result = do result::chain(result) |config| {
        let output_style = getopts::opt_maybe_str(match, opt_output_style());
        do option::map_default(output_style, result::ok(config))
            |output_style| {
            do result::chain(parse_output_style(output_style))
                |output_style| {
                result::ok({
                    output_style: output_style
                    with config
                })
            }
        }
    };
    let result = do result::chain(result) |config| {
        let pandoc_cmd = getopts::opt_maybe_str(match, opt_pandoc_cmd());
        let pandoc_cmd = maybe_find_pandoc(
            config, pandoc_cmd, program_output);
        do result::chain(pandoc_cmd) |pandoc_cmd| {
            result::ok({
                pandoc_cmd: pandoc_cmd
                with config
            })
        }
    };
    ret result;
}

fn parse_output_format(output_format: ~str) -> result<output_format, ~str> {
    alt output_format {
      ~"markdown" { result::ok(markdown) }
      ~"html" { result::ok(pandoc_html) }
      _ { result::err(fmt!{"unknown output format '%s'", output_format}) }
    }
}

fn parse_output_style(output_style: ~str) -> result<output_style, ~str> {
    alt output_style {
      ~"doc-per-crate" { result::ok(doc_per_crate) }
      ~"doc-per-mod" { result::ok(doc_per_mod) }
      _ { result::err(fmt!{"unknown output style '%s'", output_style}) }
    }
}

fn maybe_find_pandoc(
    config: config,
    maybe_pandoc_cmd: option<~str>,
    program_output: program_output
) -> result<option<~str>, ~str> {
    if config.output_format != pandoc_html {
        ret result::ok(maybe_pandoc_cmd);
    }

    let possible_pandocs = alt maybe_pandoc_cmd {
      some(pandoc_cmd) { ~[pandoc_cmd] }
      none {
        ~[~"pandoc"] + alt os::homedir() {
          some(dir) {
            ~[path::connect(dir, ~".cabal/bin/pandoc")]
          }
          none { ~[] }
        }
      }
    };

    let pandoc = do vec::find(possible_pandocs) |pandoc| {
        let output = program_output(pandoc, ~[~"--version"]);
        debug!{"testing pandoc cmd %s: %?", pandoc, output};
        output.status == 0
    };

    if option::is_some(pandoc) {
        result::ok(pandoc)
    } else {
        result::err(~"couldn't find pandoc")
    }
}

#[test]
fn should_find_pandoc() {
    let config = {
        output_format: pandoc_html
        with default_config(~"test")
    };
    let mock_program_output = fn~(_prog: ~str, _args: ~[~str]) -> {
        status: int, out: ~str, err: ~str
    } {
        {
            status: 0, out: ~"pandoc 1.8.2.1", err: ~""
        }
    };
    let result = maybe_find_pandoc(config, none, mock_program_output);
    assert result == result::ok(some(~"pandoc"));
}

#[test]
fn should_error_with_no_pandoc() {
    let config = {
        output_format: pandoc_html
        with default_config(~"test")
    };
    let mock_program_output = fn~(_prog: ~str, _args: ~[~str]) -> {
        status: int, out: ~str, err: ~str
    } {
        {
            status: 1, out: ~"", err: ~""
        }
    };
    let result = maybe_find_pandoc(config, none, mock_program_output);
    assert result == result::err(~"couldn't find pandoc");
}

#[cfg(test)]
mod test {
    fn parse_config(args: ~[~str]) -> result<config, ~str> {
        parse_config_(args, mock_program_output)
    }
}

#[test]
fn should_error_with_no_crates() {
    let config = test::parse_config(~[~"rustdoc"]);
    assert result::get_err(config) == ~"no crates specified";
}

#[test]
fn should_error_with_multiple_crates() {
    let config =
        test::parse_config(~[~"rustdoc", ~"crate1.rc", ~"crate2.rc"]);
    assert result::get_err(config) == ~"multiple crates specified";
}

#[test]
fn should_set_output_dir_to_cwd_if_not_provided() {
    let config = test::parse_config(~[~"rustdoc", ~"crate.rc"]);
    assert result::get(config).output_dir == ~".";
}

#[test]
fn should_set_output_dir_if_provided() {
    let config = test::parse_config(~[
        ~"rustdoc", ~"crate.rc", ~"--output-dir", ~"snuggles"
    ]);
    assert result::get(config).output_dir == ~"snuggles";
}

#[test]
fn should_set_output_format_to_pandoc_html_if_not_provided() {
    let config = test::parse_config(~[~"rustdoc", ~"crate.rc"]);
    assert result::get(config).output_format == pandoc_html;
}

#[test]
fn should_set_output_format_to_markdown_if_requested() {
    let config = test::parse_config(~[
        ~"rustdoc", ~"crate.rc", ~"--output-format", ~"markdown"
    ]);
    assert result::get(config).output_format == markdown;
}

#[test]
fn should_set_output_format_to_pandoc_html_if_requested() {
    let config = test::parse_config(~[
        ~"rustdoc", ~"crate.rc", ~"--output-format", ~"html"
    ]);
    assert result::get(config).output_format == pandoc_html;
}

#[test]
fn should_error_on_bogus_format() {
    let config = test::parse_config(~[
        ~"rustdoc", ~"crate.rc", ~"--output-format", ~"bogus"
    ]);
    assert result::get_err(config) == ~"unknown output format 'bogus'";
}

#[test]
fn should_set_output_style_to_doc_per_mod_by_default() {
    let config = test::parse_config(~[~"rustdoc", ~"crate.rc"]);
    assert result::get(config).output_style == doc_per_mod;
}

#[test]
fn should_set_output_style_to_one_doc_if_requested() {
    let config = test::parse_config(~[
        ~"rustdoc", ~"crate.rc", ~"--output-style", ~"doc-per-crate"
    ]);
    assert result::get(config).output_style == doc_per_crate;
}

#[test]
fn should_set_output_style_to_doc_per_mod_if_requested() {
    let config = test::parse_config(~[
        ~"rustdoc", ~"crate.rc", ~"--output-style", ~"doc-per-mod"
    ]);
    assert result::get(config).output_style == doc_per_mod;
}

#[test]
fn should_error_on_bogus_output_style() {
    let config = test::parse_config(~[
        ~"rustdoc", ~"crate.rc", ~"--output-style", ~"bogus"
    ]);
    assert result::get_err(config) == ~"unknown output style 'bogus'";
}

#[test]
fn should_set_pandoc_command_if_requested() {
    let config = test::parse_config(~[
        ~"rustdoc", ~"crate.rc", ~"--pandoc-cmd", ~"panda-bear-doc"
    ]);
    assert result::get(config).pandoc_cmd == some(~"panda-bear-doc");
}

#[test]
fn should_set_pandoc_command_when_using_pandoc() {
    let config = test::parse_config(~[~"rustdoc", ~"crate.rc"]);
    assert result::get(config).pandoc_cmd == some(~"pandoc");
}
