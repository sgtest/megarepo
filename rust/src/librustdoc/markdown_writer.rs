// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use core::prelude::*;

use config;
use config::Config;
use doc::ItemUtils;
use doc;
use pass::Pass;

use core::io::ReaderUtil;
use core::io;
use core::libc;
use core::os;
use core::comm;
use core::result;
use core::run;
use core::str;
use core::task;
use core::comm::*;
use std::future;
use syntax;

pub enum WriteInstr {
    Write(~str),
    Done
}

pub type Writer = ~fn(v: WriteInstr);
pub type WriterFactory = ~fn(page: doc::Page) -> Writer;

pub trait WriterUtils {
    fn write_str(&self, +str: ~str);
    fn write_line(&self, +str: ~str);
    fn write_done(&self);
}

impl WriterUtils for Writer {
    fn write_str(&self, str: ~str) {
        (*self)(Write(str));
    }

    fn write_line(&self, str: ~str) {
        self.write_str(str + ~"\n");
    }

    fn write_done(&self) {
        (*self)(Done)
    }
}

pub fn make_writer_factory(config: config::Config) -> WriterFactory {
    match config.output_format {
      config::Markdown => {
        markdown_writer_factory(config)
      }
      config::PandocHtml => {
        pandoc_writer_factory(config)
      }
    }
}

fn markdown_writer_factory(config: config::Config) -> WriterFactory {
    let result: ~fn(page: doc::Page) -> Writer = |page| {
        markdown_writer(copy config, page)
    };
    result
}

fn pandoc_writer_factory(config: config::Config) -> WriterFactory {
    let result: ~fn(doc::Page) -> Writer = |page| {
        pandoc_writer(copy config, page)
    };
    result
}

fn markdown_writer(
    config: config::Config,
    page: doc::Page
) -> Writer {
    let filename = make_local_filename(config, page);
    do generic_writer |markdown| {
        write_file(&filename, markdown);
    }
}

fn pandoc_writer(
    config: config::Config,
    page: doc::Page
) -> Writer {
    assert config.pandoc_cmd.is_some();
    let pandoc_cmd = (&config.pandoc_cmd).get();
    let filename = make_local_filename(config, page);

    let pandoc_args = ~[
        ~"--standalone",
        ~"--section-divs",
        ~"--from=markdown",
        ~"--to=html",
        ~"--css=rust.css",
        ~"--output=" + filename.to_str()
    ];

    do generic_writer |markdown| {
        use core::io::WriterUtil;

        debug!("pandoc cmd: %s", pandoc_cmd);
        debug!("pandoc args: %s", str::connect(pandoc_args, ~" "));

        let pipe_in = os::pipe();
        let pipe_out = os::pipe();
        let pipe_err = os::pipe();
        let pid = run::spawn_process(
            pandoc_cmd, pandoc_args, &None, &None,
            pipe_in.in, pipe_out.out, pipe_err.out);

        let writer = io::fd_writer(pipe_in.out, false);
        writer.write_str(markdown);

        os::close(pipe_in.in);
        os::close(pipe_out.out);
        os::close(pipe_err.out);
        os::close(pipe_in.out);

        let (stdout_po, stdout_ch) = comm::stream();
        do task::spawn_sched(task::SingleThreaded) || {
            stdout_ch.send(readclose(pipe_out.in));
        }

        let (stderr_po, stderr_ch) = comm::stream();
        do task::spawn_sched(task::SingleThreaded) || {
            stderr_ch.send(readclose(pipe_err.in));
        }
        let stdout = stdout_po.recv();
        let stderr = stderr_po.recv();

        let status = run::waitpid(pid);
        debug!("pandoc result: %i", status);
        if status != 0 {
            error!("pandoc-out: %s", stdout);
            error!("pandoc-err: %s", stderr);
            fail!(~"pandoc failed");
        }
    }
}

fn readclose(fd: libc::c_int) -> ~str {
    // Copied from run::program_output
    unsafe {
        let file = os::fdopen(fd);
        let reader = io::FILE_reader(file, false);
        let buf = io::with_bytes_writer(|writer| {
            let mut bytes = [0, ..4096];
            while !reader.eof() {
                let nread = reader.read(bytes, bytes.len());
                writer.write(bytes.view(0, nread));
            }
        });
        os::fclose(file);
        str::from_bytes(buf)
    }
}

fn generic_writer(process: ~fn(markdown: ~str)) -> Writer {
    let (po, ch) = stream::<WriteInstr>();
    do task::spawn || {
        let mut markdown = ~"";
        let mut keep_going = true;
        while keep_going {
            match po.recv() {
              Write(s) => markdown += s,
              Done => keep_going = false
            }
        }
        process(markdown);
    };
    let result: ~fn(instr: WriteInstr) = |instr| ch.send(instr);
    result
}

fn make_local_filename(
    config: config::Config,
    page: doc::Page
) -> Path {
    let filename = make_filename(copy config, page);
    config.output_dir.push_rel(&filename)
}

pub fn make_filename(
    config: config::Config,
    page: doc::Page
) -> Path {
    let filename = {
        match page {
          doc::CratePage(doc) => {
            if config.output_format == config::PandocHtml &&
                config.output_style == config::DocPerMod {
                ~"index"
            } else {
                assert doc.topmod.name() != ~"";
                doc.topmod.name()
            }
          }
          doc::ItemPage(doc) => {
            str::connect(doc.path() + ~[doc.name()], ~"_")
          }
        }
    };
    let ext = match config.output_format {
      config::Markdown => ~"md",
      config::PandocHtml => ~"html"
    };

    Path(filename).with_filetype(ext)
}

#[test]
fn should_use_markdown_file_name_based_off_crate() {
    let config = Config {
        output_dir: Path("output/dir"),
        output_format: config::Markdown,
        output_style: config::DocPerCrate,
        .. config::default_config(&Path("input/test.rc"))
    };
    let doc = test::mk_doc(~"test", ~"");
    let page = doc::CratePage(doc.CrateDoc());
    let filename = make_local_filename(config, page);
    assert filename.to_str() == ~"output/dir/test.md";
}

#[test]
fn should_name_html_crate_file_name_index_html_when_doc_per_mod() {
    let config = Config {
        output_dir: Path("output/dir"),
        output_format: config::PandocHtml,
        output_style: config::DocPerMod,
        .. config::default_config(&Path("input/test.rc"))
    };
    let doc = test::mk_doc(~"", ~"");
    let page = doc::CratePage(doc.CrateDoc());
    let filename = make_local_filename(config, page);
    assert filename.to_str() == ~"output/dir/index.html";
}

#[test]
fn should_name_mod_file_names_by_path() {
    let config = Config {
        output_dir: Path("output/dir"),
        output_format: config::PandocHtml,
        output_style: config::DocPerMod,
        .. config::default_config(&Path("input/test.rc"))
    };
    let doc = test::mk_doc(~"", ~"mod a { mod b { } }");
    let modb = copy doc.cratemod().mods()[0].mods()[0];
    let page = doc::ItemPage(doc::ModTag(modb));
    let filename = make_local_filename(config, page);
    assert  filename == Path("output/dir/a_b.html");
}

#[cfg(test)]
mod test {
    use astsrv;
    use doc;
    use extract;
    use path_pass;

    pub fn mk_doc(name: ~str, source: ~str) -> doc::Doc {
        do astsrv::from_str(source) |srv| {
            let doc = extract::from_srv(srv.clone(), copy name);
            let doc = (path_pass::mk_pass().f)(srv.clone(), doc);
            doc
        }
    }
}

fn write_file(path: &Path, s: ~str) {
    use core::io::WriterUtil;

    match io::file_writer(path, ~[io::Create, io::Truncate]) {
      result::Ok(writer) => {
        writer.write_str(s);
      }
      result::Err(e) => fail!(e)
    }
}

pub fn future_writer_factory(
) -> (WriterFactory, Port<(doc::Page, ~str)>) {
    let (markdown_po, markdown_ch) = stream();
    let markdown_ch = SharedChan(markdown_ch);
    let writer_factory: WriterFactory = |page| {
        let (writer_po, writer_ch) = comm::stream();
        let markdown_ch = markdown_ch.clone();
        do task::spawn || {
            let (writer, future) = future_writer();
            writer_ch.send(writer);
            let s = future.get();
            markdown_ch.send((copy page, s));
        }
        writer_po.recv()
    };

    (writer_factory, markdown_po)
}

fn future_writer() -> (Writer, future::Future<~str>) {
    let (port, chan) = comm::stream();
    let writer: ~fn(instr: WriteInstr) = |instr| chan.send(copy instr);
    let future = do future::from_fn || {
        let mut res = ~"";
        loop {
            match port.recv() {
              Write(s) => res += s,
              Done => break
            }
        }
        res
    };
    (writer, future)
}
