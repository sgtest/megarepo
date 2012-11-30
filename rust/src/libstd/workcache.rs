use core::cmp::Eq;
use send_map::linear::LinearMap;
use pipes::{recv, oneshot, PortOne, send_one};
use either::{Right,Left,Either};

use json;
use sha1;
use serialization::{Serializer,Serializable,
                    Deserializer,Deserializable,
                    deserialize};

/**
*
* This is a loose clone of the fbuild build system, made a touch more
* generic (not wired to special cases on files) and much less metaprogram-y
* due to rust's comparative weakness there, relative to python.
*
* It's based around _imperative bulids_ that happen to have some function
* calls cached. That is, it's _just_ a mechanism for describing cached
* functions. This makes it much simpler and smaller than a "build system"
* that produces an IR and evaluates it. The evaluation order is normal
* function calls. Some of them just return really quickly.
*
* A cached function consumes and produces a set of _works_. A work has a
* name, a kind (that determines how the value is to be checked for
* freshness) and a value. Works must also be (de)serializable. Some
* examples of works:
*
*    kind   name    value
*   ------------------------
*    cfg    os      linux
*    file   foo.c   <sha1>
*    url    foo.com <etag>
*
* Works are conceptually single units, but we store them most of the time
* in maps of the form (type,name) => value. These are WorkMaps.
*
* A cached function divides the works it's interested up into inputs and
* outputs, and subdivides those into declared (input and output) works and
* discovered (input and output) works.
*
* A _declared_ input or output is one that is given to the workcache before
* any work actually happens, in the "prep" phase. Even when a function's
* work-doing part (the "exec" phase) never gets called, it has declared
* inputs and outputs, which can be checked for freshness (and potentially
* used to determine that the function can be skipped).
*
* The workcache checks _all_ works for freshness, but uses the set of
* discovered outputs from the _previous_ exec (which it will re-discover
* and re-record each time the exec phase runs).
*
* Therefore the discovered works cached in the db might be a
* mis-approximation of the current discoverable works, but this is ok for
* the following reason: we assume that if an artifact A changed from
* depending on B,C,D to depending on B,C,D,E, then A itself changed (as
* part of the change-in-dependencies), so we will be ok.
*
* Each function has a single discriminated output work called its _result_.
* This is only different from other works in that it is returned, by value,
* from a call to the cacheable function; the other output works are used in
* passing to invalidate dependencies elsewhere in the cache, but do not
* otherwise escape from a function invocation. Most functions only have one
* output work anyways.
*
* A database (the central store of a workcache) stores a mappings:
*
* (fn_name,{declared_input}) => ({declared_output},{discovered_input},
*                                {discovered_output},result)
*
*/

struct WorkKey {
    kind: ~str,
    name: ~str
}

impl WorkKey: to_bytes::IterBytes {
    #[inline(always)]
    pure fn iter_bytes(&self, lsb0: bool, f: to_bytes::Cb) {
        let mut flag = true;
        self.kind.iter_bytes(lsb0, |bytes| {flag = f(bytes); flag});
        if !flag { return; }
        self.name.iter_bytes(lsb0, f);
    }
}

impl WorkKey {
    static fn new(kind: &str, name: &str) -> WorkKey {
    WorkKey { kind: kind.to_owned(), name: name.to_owned() }
    }
}

impl WorkKey: core::cmp::Eq {
    pure fn eq(&self, other: &WorkKey) -> bool {
        self.kind == other.kind && self.name == other.name
    }
    pure fn ne(&self, other: &WorkKey) -> bool {
        self.kind != other.kind || self.name != other.name
    }
}

type WorkMap = LinearMap<WorkKey, ~str>;

struct Database {
    // XXX: Fill in.
    a: ()
}

impl Database {
    pure fn prepare(_fn_name: &str,
                    _declared_inputs: &const WorkMap) ->
        Option<(WorkMap, WorkMap, WorkMap, ~str)> {
        // XXX: load
        None
    }
    pure fn cache(_fn_name: &str,
             _declared_inputs: &WorkMap,
             _declared_outputs: &WorkMap,
             _discovered_inputs: &WorkMap,
             _discovered_outputs: &WorkMap,
             _result: &str) {
        // XXX: store
    }
}

struct Logger {
    // XXX: Fill in
    a: ()
}

struct Context {
    db: @Database,
    logger: @Logger,
    cfg: @json::Object,
    freshness: LinearMap<~str,~fn(&str,&str)->bool>
}

struct Prep {
    ctxt: @Context,
    fn_name: ~str,
    declared_inputs: WorkMap,
    declared_outputs: WorkMap
}

struct Exec {
    discovered_inputs: WorkMap,
    discovered_outputs: WorkMap
}

struct Work<T:Send> {
    prep: @mut Prep,
    res: Option<Either<T,PortOne<(Exec,T)>>>
}

fn digest<T:Serializable<json::Serializer>
            Deserializable<json::Deserializer>>(t: &T) -> ~str {
    let sha = sha1::sha1();
    let s = do io::with_str_writer |wr| {
        // XXX: sha1 should be a writer itself, shouldn't
        // go via strings.
        t.serialize(&json::Serializer(wr));
    };
    sha.input_str(s);
    sha.result_str()
}

fn digest_file(path: &Path) -> ~str {
    let sha = sha1::sha1();
    let s = io::read_whole_file_str(path);
    sha.input_str(*s.get_ref());
    sha.result_str()
}

impl Context {

    static fn new(db: @Database, lg: @Logger,
                  cfg: @json::Object) -> Context {
        Context {db: db, logger: lg, cfg: cfg, freshness: LinearMap()}
    }

    fn prep<T:Send
              Serializable<json::Serializer>
              Deserializable<json::Deserializer>>(
                  @self,
                  fn_name:&str,
                  blk: fn((@mut Prep))->Work<T>) -> Work<T> {
        let p = @mut Prep {ctxt: self,
                           fn_name: fn_name.to_owned(),
                           declared_inputs: LinearMap(),
                           declared_outputs: LinearMap()};
        blk(p)
    }
}

impl Prep {
    fn declare_input(&mut self, kind:&str, name:&str, val:&str) {
        self.declared_inputs.insert(WorkKey::new(kind, name),
                                    val.to_owned());
    }

    fn declare_output(&mut self, kind:&str, name:&str, val:&str) {
        self.declared_outputs.insert(WorkKey::new(kind, name),
                                     val.to_owned());
    }

    fn exec<T:Send
              Serializable<json::Serializer>
              Deserializable<json::Deserializer>>(
                  @mut self, blk: ~fn(&Exec) -> T) -> Work<T> {
        let cached = self.ctxt.db.prepare(self.fn_name,
                                          &self.declared_inputs);

        match move cached {
            None => (),
            Some((move _decl_out,
                  move _disc_in,
                  move _disc_out,
                  move res)) => {
                // XXX: check deps for freshness, only return if fresh.
                let v : T = do io::with_str_reader(res) |rdr| {
                    let j = result::unwrap(json::from_reader(rdr));
                    deserialize(&json::Deserializer(move j))
                };
                return Work::new(self, move Left(move v));
            }
        }

        let (chan, port) = oneshot::init();

        let chan = ~mut Some(move chan);
        do task::spawn |move blk, move chan| {
            let exe = Exec { discovered_inputs: LinearMap(),
                             discovered_outputs: LinearMap() };
            let chan = option::swap_unwrap(&mut *chan);
            let v = blk(&exe);
            send_one(move chan, (move exe, move v));
        }

        Work::new(self, move Right(move port))
    }
}

impl<T:Send
       Serializable<json::Serializer>
       Deserializable<json::Deserializer>>
    Work<T> {
    static fn new(p: @mut Prep, e: Either<T,PortOne<(Exec,T)>>) -> Work<T> {
        move Work { prep: p, res: Some(move e) }
    }
}

// FIXME (#3724): movable self. This should be in impl Work.
fn unwrap<T:Send
            Serializable<json::Serializer>
            Deserializable<json::Deserializer>>(w: Work<T>) -> T {

    let mut ww = move w;
    let mut s = None;

    ww.res <-> s;

    match move s {
        None => fail,
        Some(Left(move v)) => move v,
        Some(Right(move port)) => {

            let (exe, v) = match recv(move port) {
                oneshot::send(move data) => move data
            };

            let s = do io::with_str_writer |wr| {
                v.serialize(&json::Serializer(wr));
            };

            ww.prep.ctxt.db.cache(ww.prep.fn_name,
                                  &ww.prep.declared_inputs,
                                  &ww.prep.declared_outputs,
                                  &exe.discovered_inputs,
                                  &exe.discovered_outputs,
                                  s);
            move v
        }
    }
}

#[test]
fn test() {
    use io::WriterUtil;
    let db = @Database { a: () };
    let lg = @Logger { a: () };
    let cfg = @LinearMap();
    let cx = @Context::new(db, lg, cfg);
    let w:Work<~str> = do cx.prep("test1") |prep| {
        let pth = Path("foo.c");
        {
            let file = io::file_writer(&pth, [io::Create]).get();
            file.write_str("void main() { }");
        }

        prep.declare_input("file", pth.to_str(), digest_file(&pth));
        do prep.exec |_exe| {
            let out = Path("foo.o");
            run::run_program("gcc", [~"foo.c", ~"-o", out.to_str()]);
            move out.to_str()
        }
    };
    let s = unwrap(move w);
    io::println(s);
}
