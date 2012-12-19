/*!

Generic communication channels for things that can be represented as,
or transformed to and from, byte vectors.

The `FlatPort` and `FlatChan` types implement the generic channel and
port interface for arbitrary types and transport strategies. It can
particularly be used to send and recieve serializable types over I/O
streams.

`FlatPort` and `FlatChan` implement the same comm traits as pipe-based
ports and channels.

# Example

This example sends boxed integers across tasks using serialization.

~~~
let (port, chan) = serial::pipe_stream();

do task::spawn |move chan| {
    for int::range(0, 10) |i| {
        chan.send(@i)
    }
}

for int::range(0, 10) |i| {
    assert @i == port.recv()
}
~~~

# Safety Note

Flat pipes created from `io::Reader`s and `io::Writer`s share the same
blocking properties as the underlying stream. Since some implementations
block the scheduler thread, so will their pipes.

*/

// The basic send/recv interface FlatChan and PortChan will implement
use core::pipes::GenericChan;
use core::pipes::GenericPort;

use core::sys::size_of;

/**
A FlatPort, consisting of a `BytePort` that recieves byte vectors,
and an `Unflattener` that converts the bytes to a value.

Create using the constructors in the `serial` and `pod` modules.
*/
pub struct FlatPort<T, U: Unflattener<T>, P: BytePort> {
    unflattener: U,
    byte_port: P
}

/**
A FlatChan, consisting of a `Flattener` that converts values to
byte vectors, and a `ByteChan` that transmits the bytes.

Create using the constructors in the `serial` and `pod` modules.
*/
pub struct FlatChan<T, F: Flattener<T>, C: ByteChan> {
    flattener: F,
    byte_chan: C
}

/**
Constructors for flat pipes that using serialization-based flattening.
*/
pub mod serial {

    pub use DefaultEncoder = ebml::writer::Encoder;
    pub use DefaultDecoder = ebml::reader::Decoder;

    use core::io::{Reader, Writer};
    use core::pipes::{Port, Chan};
    use serialize::{Decodable, Encodable};
    use flatpipes::flatteners::{DeserializingUnflattener,
                                SerializingFlattener};
    use flatpipes::flatteners::{deserialize_buffer, serialize_value};
    use flatpipes::bytepipes::{ReaderBytePort, WriterByteChan};
    use flatpipes::bytepipes::{PipeBytePort, PipeByteChan};

    pub type ReaderPort<T, R> = FlatPort<
        T, DeserializingUnflattener<DefaultDecoder, T>,
        ReaderBytePort<R>>;
    pub type WriterChan<T, W> = FlatChan<
        T, SerializingFlattener<DefaultEncoder, T>, WriterByteChan<W>>;
    pub type PipePort<T> = FlatPort<
        T, DeserializingUnflattener<DefaultDecoder, T>, PipeBytePort>;
    pub type PipeChan<T> = FlatChan<
        T, SerializingFlattener<DefaultEncoder, T>, PipeByteChan>;

    /// Create a `FlatPort` from a `Reader`
    pub fn reader_port<T: Decodable<DefaultDecoder>,
                       R: Reader>(reader: R) -> ReaderPort<T, R> {
        let unflat: DeserializingUnflattener<DefaultDecoder, T> =
            DeserializingUnflattener::new(
                deserialize_buffer::<DefaultDecoder, T>);
        let byte_port = ReaderBytePort::new(move reader);
        FlatPort::new(move unflat, move byte_port)
    }

    /// Create a `FlatChan` from a `Writer`
    pub fn writer_chan<T: Encodable<DefaultEncoder>,
                       W: Writer>(writer: W) -> WriterChan<T, W> {
        let flat: SerializingFlattener<DefaultEncoder, T> =
            SerializingFlattener::new(
                serialize_value::<DefaultEncoder, T>);
        let byte_chan = WriterByteChan::new(move writer);
        FlatChan::new(move flat, move byte_chan)
    }

    /// Create a `FlatPort` from a `Port<~[u8]>`
    pub fn pipe_port<T: Decodable<DefaultDecoder>>(
        port: Port<~[u8]>
    ) -> PipePort<T> {
        let unflat: DeserializingUnflattener<DefaultDecoder, T> =
            DeserializingUnflattener::new(
                deserialize_buffer::<DefaultDecoder, T>);
        let byte_port = PipeBytePort::new(move port);
        FlatPort::new(move unflat, move byte_port)
    }

    /// Create a `FlatChan` from a `Chan<~[u8]>`
    pub fn pipe_chan<T: Encodable<DefaultEncoder>>(
        chan: Chan<~[u8]>
    ) -> PipeChan<T> {
        let flat: SerializingFlattener<DefaultEncoder, T> =
            SerializingFlattener::new(
                serialize_value::<DefaultEncoder, T>);
        let byte_chan = PipeByteChan::new(move chan);
        FlatChan::new(move flat, move byte_chan)
    }

    /// Create a pair of `FlatChan` and `FlatPort`, backed by pipes
    pub fn pipe_stream<T: Encodable<DefaultEncoder>
                          Decodable<DefaultDecoder>>(
                          ) -> (PipePort<T>, PipeChan<T>) {
        let (port, chan) = pipes::stream();
        return (pipe_port(move port), pipe_chan(move chan));
    }

}

// FIXME #4074 this doesn't correctly enforce POD bounds
/**
Constructors for flat pipes that send POD types using memcpy.

# Safety Note

This module is currently unsafe because it uses `Copy Owned` as a type
parameter bounds meaning POD (plain old data), but `Copy Owned` and
POD are not equivelant.

*/
pub mod pod {

    use core::io::{Reader, Writer};
    use core::pipes::{Port, Chan};
    use flatpipes::flatteners::{PodUnflattener, PodFlattener};
    use flatpipes::bytepipes::{ReaderBytePort, WriterByteChan};
    use flatpipes::bytepipes::{PipeBytePort, PipeByteChan};

    pub type ReaderPort<T: Copy Owned, R> =
        FlatPort<T, PodUnflattener<T>, ReaderBytePort<R>>;
    pub type WriterChan<T: Copy Owned, W> =
        FlatChan<T, PodFlattener<T>, WriterByteChan<W>>;
    pub type PipePort<T: Copy Owned> =
        FlatPort<T, PodUnflattener<T>, PipeBytePort>;
    pub type PipeChan<T: Copy Owned> =
        FlatChan<T, PodFlattener<T>, PipeByteChan>;

    /// Create a `FlatPort` from a `Reader`
    pub fn reader_port<T: Copy Owned, R: Reader>(
        reader: R
    ) -> ReaderPort<T, R> {
        let unflat: PodUnflattener<T> = PodUnflattener::new();
        let byte_port = ReaderBytePort::new(move reader);
        FlatPort::new(move unflat, move byte_port)
    }

    /// Create a `FlatChan` from a `Writer`
    pub fn writer_chan<T: Copy Owned, W: Writer>(
        writer: W
    ) -> WriterChan<T, W> {
        let flat: PodFlattener<T> = PodFlattener::new();
        let byte_chan = WriterByteChan::new(move writer);
        FlatChan::new(move flat, move byte_chan)
    }

    /// Create a `FlatPort` from a `Port<~[u8]>`
    pub fn pipe_port<T: Copy Owned>(port: Port<~[u8]>) -> PipePort<T> {
        let unflat: PodUnflattener<T> = PodUnflattener::new();
        let byte_port = PipeBytePort::new(move port);
        FlatPort::new(move unflat, move byte_port)
    }

    /// Create a `FlatChan` from a `Chan<~[u8]>`
    pub fn pipe_chan<T: Copy Owned>(chan: Chan<~[u8]>) -> PipeChan<T> {
        let flat: PodFlattener<T> = PodFlattener::new();
        let byte_chan = PipeByteChan::new(move chan);
        FlatChan::new(move flat, move byte_chan)
    }

    /// Create a pair of `FlatChan` and `FlatPort`, backed by pipes
    pub fn pipe_stream<T: Copy Owned>() -> (PipePort<T>, PipeChan<T>) {
        let (port, chan) = pipes::stream();
        return (pipe_port(move port), pipe_chan(move chan));
    }

}

/**
Flatteners present a value as a byte vector
*/
pub trait Flattener<T> {
    fn flatten(&self, val: T) -> ~[u8];
}

/**
Unflatteners convert a byte vector to a value
*/
pub trait Unflattener<T> {
    fn unflatten(&self, buf: ~[u8]) -> T;
}

/**
BytePorts are a simple interface for receiving a specified number
*/
pub trait BytePort {
    fn try_recv(&self, count: uint) -> Option<~[u8]>;
}

/**
ByteChans are a simple interface for sending bytes
*/
pub trait ByteChan {
    fn send(&self, val: ~[u8]);
}

const CONTINUE: [u8 * 4] = [0xAA, 0xBB, 0xCC, 0xDD];

impl<T, U: Unflattener<T>, P: BytePort> FlatPort<T, U, P>: GenericPort<T> {
    fn recv() -> T {
        match self.try_recv() {
            Some(move val) => move val,
            None => fail ~"port is closed"
        }
    }
    fn try_recv() -> Option<T> {
        let command = match self.byte_port.try_recv(CONTINUE.len()) {
            Some(move c) => move c,
            None => {
                warn!("flatpipe: broken pipe");
                return None;
            }
        };

        if vec::eq(command, CONTINUE) {
            let msg_len = match self.byte_port.try_recv(size_of::<u64>()) {
                Some(bytes) => {
                    io::u64_from_be_bytes(bytes, 0, size_of::<u64>())
                },
                None => {
                    warn!("flatpipe: broken pipe");
                    return None;
                }
            };

            let msg_len = msg_len as uint;

            match self.byte_port.try_recv(msg_len) {
                Some(move bytes) => {
                    Some(self.unflattener.unflatten(move bytes))
                }
                None => {
                    warn!("flatpipe: broken pipe");
                    return None;
                }
            }
        }
        else {
            fail ~"flatpipe: unrecognized command";
        }
    }
}

impl<T, F: Flattener<T>, C: ByteChan> FlatChan<T, F, C>: GenericChan<T> {
    fn send(val: T) {
        self.byte_chan.send(CONTINUE.to_vec());
        let bytes = self.flattener.flatten(move val);
        let len = bytes.len() as u64;
        do io::u64_to_be_bytes(len, size_of::<u64>()) |len_bytes| {
            self.byte_chan.send(len_bytes.to_vec());
        }
        self.byte_chan.send(move bytes);
    }
}

impl<T, U: Unflattener<T>, P: BytePort> FlatPort<T, U, P> {
    static fn new(u: U, p: P) -> FlatPort<T, U, P> {
        FlatPort {
            unflattener: move u,
            byte_port: move p
        }
    }
}

impl<T, F: Flattener<T>, C: ByteChan> FlatChan<T, F, C> {
    static fn new(f: F, c: C) -> FlatChan<T, F, C> {
        FlatChan {
            flattener: move f,
            byte_chan: move c
        }
    }
}


pub mod flatteners {

    use core::sys::size_of;

    use serialize::{Encoder, Decoder,
                        Encodable, Decodable};

    use core::io::{Writer, Reader, BytesWriter, ReaderUtil};
    use flatpipes::util::BufReader;

    // XXX: Is copy/send equivalent to pod?
    pub struct PodUnflattener<T: Copy Owned> {
        bogus: ()
    }

    pub struct PodFlattener<T: Copy Owned> {
        bogus: ()
    }

    pub impl<T: Copy Owned> PodUnflattener<T>: Unflattener<T> {
        fn unflatten(&self, buf: ~[u8]) -> T {
            assert size_of::<T>() != 0;
            assert size_of::<T>() == buf.len();
            let addr_of_init: &u8 = unsafe { &*vec::raw::to_ptr(buf) };
            let addr_of_value: &T = unsafe { cast::transmute(addr_of_init) };
            copy *addr_of_value
        }
    }

    pub impl<T: Copy Owned> PodFlattener<T>: Flattener<T> {
        fn flatten(&self, val: T) -> ~[u8] {
            assert size_of::<T>() != 0;
            let val: *T = ptr::to_unsafe_ptr(&val);
            let byte_value = val as *u8;
            unsafe { vec::from_buf(byte_value, size_of::<T>()) }
        }
    }

    pub impl<T: Copy Owned> PodUnflattener<T> {
        static fn new() -> PodUnflattener<T> {
            PodUnflattener {
                bogus: ()
            }
        }
    }

    pub impl<T: Copy Owned> PodFlattener<T> {
        static fn new() -> PodFlattener<T> {
            PodFlattener {
                bogus: ()
            }
        }
    }


    pub type DeserializeBuffer<T> = ~fn(buf: &[u8]) -> T;

    pub struct DeserializingUnflattener<D: Decoder,
                                        T: Decodable<D>> {
        deserialize_buffer: DeserializeBuffer<T>
    }

    pub type SerializeValue<T> = ~fn(val: &T) -> ~[u8];

    pub struct SerializingFlattener<S: Encoder, T: Encodable<S>> {
        serialize_value: SerializeValue<T>
    }

    pub impl<D: Decoder, T: Decodable<D>>
        DeserializingUnflattener<D, T>: Unflattener<T> {
        fn unflatten(&self, buf: ~[u8]) -> T {
            (self.deserialize_buffer)(buf)
        }
    }

    pub impl<S: Encoder, T: Encodable<S>>
        SerializingFlattener<S, T>: Flattener<T> {
        fn flatten(&self, val: T) -> ~[u8] {
            (self.serialize_value)(&val)
        }
    }

    pub impl<D: Decoder, T: Decodable<D>>
        DeserializingUnflattener<D, T> {

        static fn new(deserialize_buffer: DeserializeBuffer<T>
                     ) -> DeserializingUnflattener<D, T> {
            DeserializingUnflattener {
                deserialize_buffer: move deserialize_buffer
            }
        }
    }

    pub impl<S: Encoder, T: Encodable<S>>
        SerializingFlattener<S, T> {

        static fn new(serialize_value: SerializeValue<T>
                     ) -> SerializingFlattener<S, T> {
            SerializingFlattener {
                serialize_value: move serialize_value
            }
        }
    }

    /*
    Implementations of the serialization functions required by
    SerializingFlattener
    */

    pub fn deserialize_buffer<D: Decoder FromReader,
                          T: Decodable<D>>(buf: &[u8]) -> T {
        let buf = vec::from_slice(buf);
        let buf_reader = @BufReader::new(move buf);
        let reader = buf_reader as @Reader;
        let deser: D = FromReader::from_reader(reader);
        Decodable::decode(&deser)
    }

    pub fn serialize_value<D: Encoder FromWriter,
                       T: Encodable<D>>(val: &T) -> ~[u8] {
        let bytes_writer = @BytesWriter();
        let writer = bytes_writer as @Writer;
        let ser = FromWriter::from_writer(writer);
        val.encode(&ser);
        let bytes = bytes_writer.bytes.check_out(|bytes| move bytes);
        return move bytes;
    }

    pub trait FromReader {
        static fn from_reader(r: Reader) -> self;
    }

    pub trait FromWriter {
        static fn from_writer(w: Writer) -> self;
    }

    impl json::Decoder: FromReader {
        static fn from_reader(r: Reader) -> json::Decoder {
            match json::from_reader(r) {
                Ok(move json) => {
                    json::Decoder(move json)
                }
                Err(e) => fail fmt!("flatpipe: can't parse json: %?", e)
            }
        }
    }

    impl json::Encoder: FromWriter {
        static fn from_writer(w: Writer) -> json::Encoder {
            json::Encoder(move w)
        }
    }

    impl ebml::reader::Decoder: FromReader {
        static fn from_reader(r: Reader) -> ebml::reader::Decoder {
            let buf = @r.read_whole_stream();
            let doc = ebml::reader::Doc(buf);
            ebml::reader::Decoder(move doc)
        }
    }

    impl ebml::writer::Encoder: FromWriter {
        static fn from_writer(w: Writer) -> ebml::writer::Encoder {
            ebml::writer::Encoder(move w)
        }
    }

}

pub mod bytepipes {

    use core::io::{Writer, Reader, ReaderUtil};
    use core::pipes::{Port, Chan};

    pub struct ReaderBytePort<R: Reader> {
        reader: R
    }

    pub struct WriterByteChan<W: Writer> {
        writer: W
    }

    pub impl<R: Reader> ReaderBytePort<R>: BytePort {
        fn try_recv(&self, count: uint) -> Option<~[u8]> {
            let mut left = count;
            let mut bytes = ~[];
            while !self.reader.eof() && left > 0 {
                assert left <= count;
                assert left > 0;
                let new_bytes = self.reader.read_bytes(left);
                bytes.push_all(new_bytes);
                assert new_bytes.len() <= left;
                left -= new_bytes.len();
            }

            if left == 0 {
                return Some(move bytes);
            } else {
                warn!("flatpipe: dropped %? broken bytes", left);
                return None;
            }
        }
    }

    pub impl<W: Writer> WriterByteChan<W>: ByteChan {
        fn send(&self, val: ~[u8]) {
            self.writer.write(val);
        }
    }

    pub impl<R: Reader> ReaderBytePort<R> {
        static fn new(r: R) -> ReaderBytePort<R> {
            ReaderBytePort {
                reader: move r
            }
        }
    }

    pub impl<W: Writer> WriterByteChan<W> {
        static fn new(w: W) -> WriterByteChan<W> {
            WriterByteChan {
                writer: move w
            }
        }
    }

    pub struct PipeBytePort {
        port: pipes::Port<~[u8]>,
        mut buf: ~[u8]
    }

    pub struct PipeByteChan {
        chan: pipes::Chan<~[u8]>
    }

    pub impl PipeBytePort: BytePort {
        fn try_recv(&self, count: uint) -> Option<~[u8]> {
            if self.buf.len() >= count {
                let mut bytes = core::util::replace(&mut self.buf, ~[]);
                self.buf = bytes.slice(count, bytes.len());
                bytes.truncate(count);
                return Some(bytes);
            } else if self.buf.len() > 0 {
                let mut bytes = core::util::replace(&mut self.buf, ~[]);
                assert count > bytes.len();
                match self.try_recv(count - bytes.len()) {
                    Some(move rest) => {
                        bytes.push_all(rest);
                        return Some(move bytes);
                    }
                    None => return None
                }
            } else if self.buf.is_empty() {
                match self.port.try_recv() {
                    Some(move buf) => {
                        assert buf.is_not_empty();
                        self.buf = move buf;
                        return self.try_recv(count);
                    }
                    None => return None
                }
            } else {
                core::util::unreachable()
            }
        }
    }

    pub impl PipeByteChan: ByteChan {
        fn send(&self, val: ~[u8]) {
            self.chan.send(move val)
        }
    }

    pub impl PipeBytePort {
        static fn new(p: Port<~[u8]>) -> PipeBytePort {
            PipeBytePort {
                port: move p,
                buf: ~[]
            }
        }
    }

    pub impl PipeByteChan {
        static fn new(c: Chan<~[u8]>) -> PipeByteChan {
            PipeByteChan {
                chan: move c
            }
        }
    }

}

// XXX: This belongs elsewhere
mod util {

    use io::{Reader, BytesReader};

    pub struct BufReader {
        buf: ~[u8],
        mut pos: uint
    }

    pub impl BufReader {
        static pub fn new(v: ~[u8]) -> BufReader {
            BufReader {
                buf: move v,
                pos: 0
            }
        }

        priv fn as_bytes_reader<A>(f: &fn(&BytesReader) -> A) -> A {
            // Recreating the BytesReader state every call since
            // I can't get the borrowing to work correctly
            let bytes_reader = BytesReader {
                bytes: core::util::id::<&[u8]>(self.buf),
                pos: self.pos
            };

            let res = f(&bytes_reader);

            // XXX: This isn't correct if f fails
            self.pos = bytes_reader.pos;

            return move res;
        }
    }

    impl BufReader: Reader {
        fn read(bytes: &[mut u8], len: uint) -> uint {
            self.as_bytes_reader(|r| r.read(bytes, len) )
        }
        fn read_byte() -> int {
            self.as_bytes_reader(|r| r.read_byte() )
        }
        fn eof() -> bool {
            self.as_bytes_reader(|r| r.eof() )
        }
        fn seek(offset: int, whence: io::SeekStyle) {
            self.as_bytes_reader(|r| r.seek(offset, whence) )
        }
        fn tell() -> uint {
            self.as_bytes_reader(|r| r.tell() )
        }
    }

}

#[cfg(test)]
mod test {

    // XXX: json::Decoder doesn't work because of problems related to
    // its interior pointers
    //use DefaultEncoder = json::Encoder;
    //use DefaultDecoder = json::Decoder;
    use DefaultEncoder = ebml::writer::Encoder;
    use DefaultDecoder = ebml::reader::Decoder;

    use flatpipes::flatteners::*;
    use flatpipes::bytepipes::*;

    use core::dvec::DVec;
    use io::BytesReader;
    use util::BufReader;
    use net::tcp::TcpSocketBuf;

    #[test]
    fn test_serializing_memory_stream() {
        let writer = BytesWriter();
        let chan = serial::writer_chan(move writer);

        chan.send(10);

        let bytes = chan.byte_chan.writer.bytes.get();

        let reader = BufReader::new(move bytes);
        let port = serial::reader_port(move reader);

        let res: int = port.recv();
        assert res == 10i;
    }

    #[test]
    fn test_serializing_pipes() {
        let (port, chan) = serial::pipe_stream();

        do task::spawn |move chan| {
            for int::range(0, 10) |i| {
                chan.send(i)
            }
        }

        for int::range(0, 10) |i| {
            assert i == port.recv()
        }
    }

    #[test]
    fn test_serializing_boxes() {
        let (port, chan) = serial::pipe_stream();

        do task::spawn |move chan| {
            for int::range(0, 10) |i| {
                chan.send(@i)
            }
        }

        for int::range(0, 10) |i| {
            assert @i == port.recv()
        }
    }

    #[test]
    fn test_pod_memory_stream() {
        let writer = BytesWriter();
        let chan = pod::writer_chan(move writer);

        chan.send(10);

        let bytes = chan.byte_chan.writer.bytes.get();

        let reader = BufReader::new(move bytes);
        let port = pod::reader_port(move reader);

        let res: int = port.recv();
        assert res == 10;
    }

    #[test]
    fn test_pod_pipes() {
        let (port, chan) = pod::pipe_stream();

        do task::spawn |move chan| {
            for int::range(0, 10) |i| {
                chan.send(i)
            }
        }

        for int::range(0, 10) |i| {
            assert i == port.recv()
        }
    }

    // XXX: Networking doesn't work on x86
    #[test]
    #[cfg(target_arch = "x86_64")]
    fn test_pod_tcp_stream() {
        fn reader_port(buf: TcpSocketBuf
                      ) -> pod::ReaderPort<int, TcpSocketBuf> {
            pod::reader_port(move buf)
        }
        fn writer_chan(buf: TcpSocketBuf
                      ) -> pod::WriterChan<int, TcpSocketBuf> {
            pod::writer_chan(move buf)
        }
        test_some_tcp_stream(reader_port, writer_chan, 9666);
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn test_serializing_tcp_stream() {
        fn reader_port(buf: TcpSocketBuf
                      ) -> serial::ReaderPort<int, TcpSocketBuf> {
            serial::reader_port(move buf)
        }
        fn writer_chan(buf: TcpSocketBuf
                      ) -> serial::WriterChan<int, TcpSocketBuf> {
            serial::writer_chan(move buf)
        }
        test_some_tcp_stream(reader_port, writer_chan, 9667);
    }

    type ReaderPortFactory<U: Unflattener<int>> =
        ~fn(TcpSocketBuf) -> FlatPort<int, U, ReaderBytePort<TcpSocketBuf>>;
    type WriterChanFactory<F: Flattener<int>> =
        ~fn(TcpSocketBuf) -> FlatChan<int, F, WriterByteChan<TcpSocketBuf>>;

    fn test_some_tcp_stream<U: Unflattener<int>, F: Flattener<int>>(
        reader_port: ReaderPortFactory<U>,
        writer_chan: WriterChanFactory<F>,
        port: uint) {

        use net::tcp;
        use net::ip;
        use cell::Cell;
        use net::tcp::TcpSocket;

        // Indicate to the client task that the server is listening
        let (begin_connect_port, begin_connect_chan) = pipes::stream();
        // The connection is sent from the server task to the receiver task
        // to handle the connection
        let (accept_port, accept_chan) = pipes::stream();
        // The main task will wait until the test is over to proceed
        let (finish_port, finish_chan) = pipes::stream();

        let addr = ip::v4::parse_addr("127.0.0.1");
        let iotask = uv::global_loop::get();

        let begin_connect_chan = Cell(move begin_connect_chan);
        let accept_chan = Cell(move accept_chan);

        // The server task
        do task::spawn |copy addr, move begin_connect_chan,
                        move accept_chan| {
            let begin_connect_chan = begin_connect_chan.take();
            let accept_chan = accept_chan.take();
            let listen_res = do tcp::listen(
                copy addr, port, 128, iotask,
                |move begin_connect_chan, _kill_ch| {
                    // Tell the sender to initiate the connection
                    debug!("listening");
                    begin_connect_chan.send(())
                }) |move accept_chan, new_conn, kill_ch| {

                // Incoming connection. Send it to the receiver task to accept
                let (res_port, res_chan) = pipes::stream();
                accept_chan.send((move new_conn, move res_chan));
                // Wait until the connection is accepted
                res_port.recv();

                // Stop listening
                kill_ch.send(None)
            };

            assert listen_res.is_ok();
        }

        // Client task
        do task::spawn |copy addr, move begin_connect_port,
                        move writer_chan| {

            // Wait for the server to start listening
            begin_connect_port.recv();

            debug!("connecting");
            let connect_result = tcp::connect(copy addr, port, iotask);
            assert connect_result.is_ok();
            let sock = result::unwrap(move connect_result);
            let socket_buf: tcp::TcpSocketBuf = tcp::socket_buf(move sock);

            // TcpSocketBuf is a Writer!
            let chan = writer_chan(move socket_buf);

            for int::range(0, 10) |i| {
                debug!("sending %?", i);
                chan.send(i)
            }
        }

        // Reciever task
        do task::spawn |move accept_port, move finish_chan,
                        move reader_port| {

            // Wait for a connection
            let (conn, res_chan) = accept_port.recv();

            debug!("accepting connection");
            let accept_result = tcp::accept(conn);
            debug!("accepted");
            assert accept_result.is_ok();
            let sock = result::unwrap(move accept_result);
            res_chan.send(());

            let socket_buf: tcp::TcpSocketBuf = tcp::socket_buf(move sock);

            // TcpSocketBuf is a Reader!
            let port = reader_port(move socket_buf);

            for int::range(0, 10) |i| {
                let j = port.recv();
                debug!("receieved %?", j);
                assert i == j;
            }

            // The test is over!
            finish_chan.send(());
        }

        finish_port.recv();
    }

    // Tests that the different backends behave the same when the
    // binary streaming protocol is broken
    mod broken_protocol {
        type PortLoader<P: BytePort> =
            ~fn(~[u8]) -> FlatPort<int, PodUnflattener<int>, P>;

        fn reader_port_loader(bytes: ~[u8]
                             ) -> pod::ReaderPort<int, BufReader> {
            let reader = BufReader::new(move bytes);
            pod::reader_port(move reader)
        }

        fn pipe_port_loader(bytes: ~[u8]
                           ) -> pod::PipePort<int> {
            let (port, chan) = pipes::stream();
            if bytes.is_not_empty() {
                chan.send(move bytes);
            }
            pod::pipe_port(move port)
        }

        fn test_try_recv_none1<P: BytePort>(loader: PortLoader<P>) {
            let bytes = ~[];
            let port = loader(move bytes);
            let res: Option<int> = port.try_recv();
            assert res.is_none();
        }

        #[test]
        fn test_try_recv_none1_reader() {
            test_try_recv_none1(reader_port_loader);
        }
        #[test]
        fn test_try_recv_none1_pipe() {
            test_try_recv_none1(pipe_port_loader);
        }

        fn test_try_recv_none2<P: BytePort>(loader: PortLoader<P>) {
            // The control word in the protocol is interrupted
            let bytes = ~[0];
            let port = loader(move bytes);
            let res: Option<int> = port.try_recv();
            assert res.is_none();
        }

        #[test]
        fn test_try_recv_none2_reader() {
            test_try_recv_none2(reader_port_loader);
        }
        #[test]
        fn test_try_recv_none2_pipe() {
            test_try_recv_none2(pipe_port_loader);
        }

        fn test_try_recv_none3<P: BytePort>(loader: PortLoader<P>) {
            const CONTINUE: [u8 * 4] = [0xAA, 0xBB, 0xCC, 0xDD];
            // The control word is followed by garbage
            let bytes = CONTINUE.to_vec() + ~[0];
            let port = loader(move bytes);
            let res: Option<int> = port.try_recv();
            assert res.is_none();
        }

        #[test]
        fn test_try_recv_none3_reader() {
            test_try_recv_none3(reader_port_loader);
        }
        #[test]
        fn test_try_recv_none3_pipe() {
            test_try_recv_none3(pipe_port_loader);
        }

        fn test_try_recv_none4<P: BytePort>(+loader: PortLoader<P>) {
            assert do task::try |move loader| {
                const CONTINUE: [u8 * 4] = [0xAA, 0xBB, 0xCC, 0xDD];
                // The control word is followed by a valid length,
                // then undeserializable garbage
                let len_bytes = do io::u64_to_be_bytes(
                    1, sys::size_of::<u64>()) |len_bytes| {
                    len_bytes.to_vec()
                };
                let bytes = CONTINUE.to_vec() + len_bytes + ~[0, 0, 0, 0];

                let port = loader(move bytes);

                let _res: Option<int> = port.try_recv();
            }.is_err();
        }

        #[test]
        #[ignore(cfg(windows))]
        fn test_try_recv_none4_reader() {
            test_try_recv_none4(reader_port_loader);
        }
        #[test]
        #[ignore(cfg(windows))]
        fn test_try_recv_none4_pipe() {
            test_try_recv_none4(pipe_port_loader);
        }
    }

}
