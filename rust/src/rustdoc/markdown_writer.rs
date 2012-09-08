use doc::item_utils;
use io::ReaderUtil;

export writeinstr;
export writer;
export writer_factory;
export writer_util;
export writer_utils;
export make_writer_factory;
export future_writer_factory;
export make_filename;

enum writeinstr {
    write(~str),
    done
}

type writer = fn~(+writeinstr);
type writer_factory = fn~(page: doc::page) -> writer;

trait writer_utils {
    fn write_str(str: ~str);
    fn write_line(str: ~str);
    fn write_done();
}

impl writer: writer_utils {
    fn write_str(str: ~str) {
        self(write(str));
    }

    fn write_line(str: ~str) {
        self.write_str(str + ~"\n");
    }

    fn write_done() {
        self(done)
    }
}

fn make_writer_factory(config: config::config) -> writer_factory {
    match config.output_format {
      config::markdown => {
        markdown_writer_factory(config)
      }
      config::pandoc_html => {
        pandoc_writer_factory(config)
      }
    }
}

fn markdown_writer_factory(config: config::config) -> writer_factory {
    fn~(page: doc::page) -> writer {
        markdown_writer(config, page)
    }
}

fn pandoc_writer_factory(config: config::config) -> writer_factory {
    fn~(page: doc::page) -> writer {
        pandoc_writer(config, page)
    }
}

fn markdown_writer(
    config: config::config,
    page: doc::page
) -> writer {
    let filename = make_local_filename(config, page);
    do generic_writer |markdown| {
        write_file(&filename, markdown);
    }
}

fn pandoc_writer(
    config: config::config,
    page: doc::page
) -> writer {
    assert option::is_some(config.pandoc_cmd);
    let pandoc_cmd = option::get(config.pandoc_cmd);
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
        import io::WriterUtil;

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

        let stdout_po = comm::Port();
        let stdout_ch = comm::Chan(stdout_po);
        do task::spawn_sched(task::SingleThreaded) {
            comm::send(stdout_ch, readclose(pipe_out.in));
        }
        let stdout = comm::recv(stdout_po);

        let stderr_po = comm::Port();
        let stderr_ch = comm::Chan(stderr_po);
        do task::spawn_sched(task::SingleThreaded) {
            comm::send(stderr_ch, readclose(pipe_err.in));
        }
        let stderr = comm::recv(stderr_po);

        let status = run::waitpid(pid);
        debug!("pandoc result: %i", status);
        if status != 0 {
            error!("pandoc-out: %s", stdout);
            error!("pandoc-err: %s", stderr);
            fail ~"pandoc failed";
        }
    }
}

fn readclose(fd: libc::c_int) -> ~str {
    // Copied from run::program_output
    let file = os::fdopen(fd);
    let reader = io::FILE_reader(file, false);
    let mut buf = ~"";
    while !reader.eof() {
        let bytes = reader.read_bytes(4096u);
        buf += str::from_bytes(bytes);
    }
    os::fclose(file);
    return buf;
}

fn generic_writer(+process: fn~(markdown: ~str)) -> writer {
    let ch = do task::spawn_listener |po: comm::Port<writeinstr>| {
        let mut markdown = ~"";
        let mut keep_going = true;
        while keep_going {
            match comm::recv(po) {
              write(s) => markdown += s,
              done => keep_going = false
            }
        }
        process(markdown);
    };

    fn~(+instr: writeinstr) {
        comm::send(ch, instr);
    }
}

fn make_local_filename(
    config: config::config,
    page: doc::page
) -> Path {
    let filename = make_filename(config, page);
    config.output_dir.push_rel(&filename)
}

fn make_filename(
    config: config::config,
    page: doc::page
) -> Path {
    let filename = {
        match page {
          doc::cratepage(doc) => {
            if config.output_format == config::pandoc_html &&
                config.output_style == config::doc_per_mod {
                ~"index"
            } else {
                assert doc.topmod.name() != ~"";
                doc.topmod.name()
            }
          }
          doc::itempage(doc) => {
            str::connect(doc.path() + ~[doc.name()], ~"_")
          }
        }
    };
    let ext = match config.output_format {
      config::markdown => ~"md",
      config::pandoc_html => ~"html"
    };

    Path(filename).with_filetype(ext)
}

#[test]
fn should_use_markdown_file_name_based_off_crate() {
    let config = {
        output_dir: Path("output/dir"),
        output_format: config::markdown,
        output_style: config::doc_per_crate,
        .. config::default_config(&Path("input/test.rc"))
    };
    let doc = test::mk_doc(~"test", ~"");
    let page = doc::cratepage(doc.cratedoc());
    let filename = make_local_filename(config, page);
    assert filename.to_str() == ~"output/dir/test.md";
}

#[test]
fn should_name_html_crate_file_name_index_html_when_doc_per_mod() {
    let config = {
        output_dir: Path("output/dir"),
        output_format: config::pandoc_html,
        output_style: config::doc_per_mod,
        .. config::default_config(&Path("input/test.rc"))
    };
    let doc = test::mk_doc(~"", ~"");
    let page = doc::cratepage(doc.cratedoc());
    let filename = make_local_filename(config, page);
    assert filename.to_str() == ~"output/dir/index.html";
}

#[test]
fn should_name_mod_file_names_by_path() {
    let config = {
        output_dir: Path("output/dir"),
        output_format: config::pandoc_html,
        output_style: config::doc_per_mod,
        .. config::default_config(&Path("input/test.rc"))
    };
    let doc = test::mk_doc(~"", ~"mod a { mod b { } }");
    let modb = doc.cratemod().mods()[0].mods()[0];
    let page = doc::itempage(doc::modtag(modb));
    let filename = make_local_filename(config, page);
    assert  filename == Path("output/dir/a_b.html");
}

#[cfg(test)]
mod test {
    fn mk_doc(name: ~str, source: ~str) -> doc::doc {
        do astsrv::from_str(source) |srv| {
            let doc = extract::from_srv(srv, name);
            let doc = path_pass::mk_pass().f(srv, doc);
            doc
        }
    }
}

fn write_file(path: &Path, s: ~str) {
    import io::WriterUtil;

    match io::file_writer(path, ~[io::Create, io::Truncate]) {
      result::Ok(writer) => {
        writer.write_str(s);
      }
      result::Err(e) => fail e
    }
}

fn future_writer_factory(
) -> (writer_factory, comm::Port<(doc::page, ~str)>) {
    let markdown_po = comm::Port();
    let markdown_ch = comm::Chan(markdown_po);
    let writer_factory = fn~(page: doc::page) -> writer {
        let writer_po = comm::Port();
        let writer_ch = comm::Chan(writer_po);
        do task::spawn {
            let (writer, future) = future_writer();
            comm::send(writer_ch, writer);
            let s = future::get(&future);
            comm::send(markdown_ch, (page, s));
        }
        comm::recv(writer_po)
    };

    (writer_factory, markdown_po)
}

fn future_writer() -> (writer, future::Future<~str>) {
    let (chan, port) = pipes::stream();
    let writer = fn~(+instr: writeinstr) {
        chan.send(copy instr);
    };
    let future = do future::from_fn {
        let mut res = ~"";
        loop {
            match port.recv() {
              write(s) => res += s,
              done => break
            }
        }
        res
    };
    (writer, future)
}
