// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::cast;
use std::io::IoError;
use std::io::net::ip::{Ipv4Addr, Ipv6Addr, SocketAddr, IpAddr};
use std::libc::{size_t, ssize_t, c_int, c_void, c_uint, c_char};
use std::libc;
use std::ptr;
use std::rt::rtio;
use std::rt::task::BlockedTask;
use std::str;
use std::vec;

use homing::{HomingIO, HomeHandle};
use stream::StreamWatcher;
use super::{Loop, Request, UvError, Buf, status_to_io_result,
            uv_error_to_io_error, UvHandle, slice_to_uv_buf,
            wait_until_woken_after, wakeup};
use uvio::UvIoFactory;
use uvll;
use uvll::sockaddr;

////////////////////////////////////////////////////////////////////////////////
/// Generic functions related to dealing with sockaddr things
////////////////////////////////////////////////////////////////////////////////

fn socket_addr_as_sockaddr<T>(addr: SocketAddr, f: |*sockaddr| -> T) -> T {
    let malloc = match addr.ip {
        Ipv4Addr(..) => uvll::rust_malloc_ip4_addr,
        Ipv6Addr(..) => uvll::rust_malloc_ip6_addr,
    };

    let ip = addr.ip.to_str();
    let addr = ip.with_c_str(|p| unsafe { malloc(p, addr.port as c_int) });
    (|| {
        f(addr)
    }).finally(|| {
        unsafe { libc::free(addr) };
    })
}

pub fn sockaddr_to_socket_addr(addr: *sockaddr) -> SocketAddr {
    unsafe {
        let ip_size = if uvll::rust_is_ipv4_sockaddr(addr) == 1 {
            4/*groups of*/ * 3/*digits separated by*/ + 3/*periods*/
        } else if uvll::rust_is_ipv6_sockaddr(addr) == 1 {
            8/*groups of*/ * 4/*hex digits separated by*/ + 7 /*colons*/
        } else {
            fail!("unknown address?");
        };
        let ip_name = {
            // apparently there's an off-by-one in libuv?
            let ip_size = ip_size + 1;
            let buf = vec::from_elem(ip_size + 1 /*null terminated*/, 0u8);
            let buf_ptr = buf.as_ptr();
            let ret = if uvll::rust_is_ipv4_sockaddr(addr) == 1 {
                uvll::uv_ip4_name(addr, buf_ptr as *c_char, ip_size as size_t)
            } else {
                uvll::uv_ip6_name(addr, buf_ptr as *c_char, ip_size as size_t)
            };
            if ret != 0 {
                fail!("error parsing sockaddr: {}", UvError(ret).desc());
            }
            buf
        };
        let ip_port = {
            let port = if uvll::rust_is_ipv4_sockaddr(addr) == 1 {
                uvll::rust_ip4_port(addr)
            } else {
                uvll::rust_ip6_port(addr)
            };
            port as u16
        };
        let ip_str = str::from_utf8(ip_name).trim_right_chars(&'\x00');
        let ip_addr = FromStr::from_str(ip_str).unwrap();

        SocketAddr { ip: ip_addr, port: ip_port }
    }
}

#[test]
fn test_ip4_conversion() {
    use std::io::net::ip::{SocketAddr, Ipv4Addr};
    let ip4 = SocketAddr { ip: Ipv4Addr(127, 0, 0, 1), port: 4824 };
    socket_addr_as_sockaddr(ip4, |addr| {
        assert_eq!(ip4, sockaddr_to_socket_addr(addr));
    })
}

#[test]
fn test_ip6_conversion() {
    use std::io::net::ip::{SocketAddr, Ipv6Addr};
    let ip6 = SocketAddr { ip: Ipv6Addr(0, 0, 0, 0, 0, 0, 0, 1), port: 4824 };
    socket_addr_as_sockaddr(ip6, |addr| {
        assert_eq!(ip6, sockaddr_to_socket_addr(addr));
    })
}

enum SocketNameKind {
    TcpPeer,
    Tcp,
    Udp
}

fn socket_name(sk: SocketNameKind, handle: *c_void) -> Result<SocketAddr, IoError> {
    unsafe {
        let getsockname = match sk {
            TcpPeer => uvll::uv_tcp_getpeername,
            Tcp     => uvll::uv_tcp_getsockname,
            Udp     => uvll::uv_udp_getsockname,
        };

        // Allocate a sockaddr_storage
        // since we don't know if it's ipv4 or ipv6
        let size = uvll::rust_sockaddr_size();
        let name = libc::malloc(size as size_t);
        assert!(!name.is_null());
        let mut namelen = size;

        let ret = match getsockname(handle, name, &mut namelen) {
            0 => Ok(sockaddr_to_socket_addr(name)),
            n => Err(uv_error_to_io_error(UvError(n)))
        };
        libc::free(name);
        ret
    }
}

////////////////////////////////////////////////////////////////////////////////
/// TCP implementation
////////////////////////////////////////////////////////////////////////////////

pub struct TcpWatcher {
    handle: *uvll::uv_tcp_t,
    stream: StreamWatcher,
    home: HomeHandle,
}

pub struct TcpListener {
    home: HomeHandle,
    handle: *uvll::uv_pipe_t,
    priv closing_task: Option<BlockedTask>,
    priv outgoing: Chan<Result<~rtio::RtioTcpStream, IoError>>,
    priv incoming: Port<Result<~rtio::RtioTcpStream, IoError>>,
}

pub struct TcpAcceptor {
    listener: ~TcpListener,
}

// TCP watchers (clients/streams)

impl TcpWatcher {
    pub fn new(io: &mut UvIoFactory) -> TcpWatcher {
        let handle = io.make_handle();
        TcpWatcher::new_home(&io.loop_, handle)
    }

    fn new_home(loop_: &Loop, home: HomeHandle) -> TcpWatcher {
        let handle = unsafe { uvll::malloc_handle(uvll::UV_TCP) };
        assert_eq!(unsafe {
            uvll::uv_tcp_init(loop_.handle, handle)
        }, 0);
        TcpWatcher {
            home: home,
            handle: handle,
            stream: StreamWatcher::new(handle),
        }
    }

    pub fn connect(io: &mut UvIoFactory, address: SocketAddr)
        -> Result<TcpWatcher, UvError>
    {
        struct Ctx { status: c_int, task: Option<BlockedTask> }

        let tcp = TcpWatcher::new(io);
        let ret = socket_addr_as_sockaddr(address, |addr| {
            let mut req = Request::new(uvll::UV_CONNECT);
            let result = unsafe {
                uvll::uv_tcp_connect(req.handle, tcp.handle, addr,
                                     connect_cb)
            };
            match result {
                0 => {
                    req.defuse(); // uv callback now owns this request
                    let mut cx = Ctx { status: 0, task: None };
                    wait_until_woken_after(&mut cx.task, || {
                        req.set_data(&cx);
                    });
                    match cx.status {
                        0 => Ok(()),
                        n => Err(UvError(n)),
                    }
                }
                n => Err(UvError(n))
            }
        });

        return match ret {
            Ok(()) => Ok(tcp),
            Err(e) => Err(e),
        };

        extern fn connect_cb(req: *uvll::uv_connect_t, status: c_int) {
            let req = Request::wrap(req);
            assert!(status != uvll::ECANCELED);
            let cx: &mut Ctx = unsafe { req.get_data() };
            cx.status = status;
            wakeup(&mut cx.task);
        }
    }
}

impl HomingIO for TcpWatcher {
    fn home<'r>(&'r mut self) -> &'r mut HomeHandle { &mut self.home }
}

impl rtio::RtioSocket for TcpWatcher {
    fn socket_name(&mut self) -> Result<SocketAddr, IoError> {
        let _m = self.fire_homing_missile();
        socket_name(Tcp, self.handle)
    }
}

impl rtio::RtioTcpStream for TcpWatcher {
    fn read(&mut self, buf: &mut [u8]) -> Result<uint, IoError> {
        let _m = self.fire_homing_missile();
        self.stream.read(buf).map_err(uv_error_to_io_error)
    }

    fn write(&mut self, buf: &[u8]) -> Result<(), IoError> {
        let _m = self.fire_homing_missile();
        self.stream.write(buf).map_err(uv_error_to_io_error)
    }

    fn peer_name(&mut self) -> Result<SocketAddr, IoError> {
        let _m = self.fire_homing_missile();
        socket_name(TcpPeer, self.handle)
    }

    fn control_congestion(&mut self) -> Result<(), IoError> {
        let _m = self.fire_homing_missile();
        status_to_io_result(unsafe {
            uvll::uv_tcp_nodelay(self.handle, 0 as c_int)
        })
    }

    fn nodelay(&mut self) -> Result<(), IoError> {
        let _m = self.fire_homing_missile();
        status_to_io_result(unsafe {
            uvll::uv_tcp_nodelay(self.handle, 1 as c_int)
        })
    }

    fn keepalive(&mut self, delay_in_seconds: uint) -> Result<(), IoError> {
        let _m = self.fire_homing_missile();
        status_to_io_result(unsafe {
            uvll::uv_tcp_keepalive(self.handle, 1 as c_int,
                                   delay_in_seconds as c_uint)
        })
    }

    fn letdie(&mut self) -> Result<(), IoError> {
        let _m = self.fire_homing_missile();
        status_to_io_result(unsafe {
            uvll::uv_tcp_keepalive(self.handle, 0 as c_int, 0 as c_uint)
        })
    }
}

impl UvHandle<uvll::uv_tcp_t> for TcpWatcher {
    fn uv_handle(&self) -> *uvll::uv_tcp_t { self.stream.handle }
}

impl Drop for TcpWatcher {
    fn drop(&mut self) {
        let _m = self.fire_homing_missile();
        self.close();
    }
}

// TCP listeners (unbound servers)

impl TcpListener {
    pub fn bind(io: &mut UvIoFactory, address: SocketAddr)
                -> Result<~TcpListener, UvError> {
        let handle = unsafe { uvll::malloc_handle(uvll::UV_TCP) };
        assert_eq!(unsafe {
            uvll::uv_tcp_init(io.uv_loop(), handle)
        }, 0);
        let (port, chan) = Chan::new();
        let l = ~TcpListener {
            home: io.make_handle(),
            handle: handle,
            closing_task: None,
            outgoing: chan,
            incoming: port,
        };
        let res = socket_addr_as_sockaddr(address, |addr| unsafe {
            uvll::uv_tcp_bind(l.handle, addr)
        });
        return match res {
            0 => Ok(l.install()),
            n => Err(UvError(n))
        };
    }
}

impl HomingIO for TcpListener {
    fn home<'r>(&'r mut self) -> &'r mut HomeHandle { &mut self.home }
}

impl UvHandle<uvll::uv_tcp_t> for TcpListener {
    fn uv_handle(&self) -> *uvll::uv_tcp_t { self.handle }
}

impl rtio::RtioSocket for TcpListener {
    fn socket_name(&mut self) -> Result<SocketAddr, IoError> {
        let _m = self.fire_homing_missile();
        socket_name(Tcp, self.handle)
    }
}

impl rtio::RtioTcpListener for TcpListener {
    fn listen(mut ~self) -> Result<~rtio::RtioTcpAcceptor, IoError> {
        // create the acceptor object from ourselves
        let mut acceptor = ~TcpAcceptor { listener: self };

        let _m = acceptor.fire_homing_missile();
        // XXX: the 128 backlog should be configurable
        match unsafe { uvll::uv_listen(acceptor.listener.handle, 128, listen_cb) } {
            0 => Ok(acceptor as ~rtio::RtioTcpAcceptor),
            n => Err(uv_error_to_io_error(UvError(n))),
        }
    }
}

extern fn listen_cb(server: *uvll::uv_stream_t, status: c_int) {
    assert!(status != uvll::ECANCELED);
    let tcp: &mut TcpListener = unsafe { UvHandle::from_uv_handle(&server) };
    let msg = match status {
        0 => {
            let loop_ = Loop::wrap(unsafe {
                uvll::get_loop_for_uv_handle(server)
            });
            let client = TcpWatcher::new_home(&loop_, tcp.home().clone());
            assert_eq!(unsafe { uvll::uv_accept(server, client.handle) }, 0);
            Ok(~client as ~rtio::RtioTcpStream)
        }
        n => Err(uv_error_to_io_error(UvError(n)))
    };
    tcp.outgoing.send(msg);
}

impl Drop for TcpListener {
    fn drop(&mut self) {
        let _m = self.fire_homing_missile();
        self.close();
    }
}

// TCP acceptors (bound servers)

impl HomingIO for TcpAcceptor {
    fn home<'r>(&'r mut self) -> &'r mut HomeHandle { self.listener.home() }
}

impl rtio::RtioSocket for TcpAcceptor {
    fn socket_name(&mut self) -> Result<SocketAddr, IoError> {
        let _m = self.fire_homing_missile();
        socket_name(Tcp, self.listener.handle)
    }
}

impl rtio::RtioTcpAcceptor for TcpAcceptor {
    fn accept(&mut self) -> Result<~rtio::RtioTcpStream, IoError> {
        self.listener.incoming.recv()
    }

    fn accept_simultaneously(&mut self) -> Result<(), IoError> {
        let _m = self.fire_homing_missile();
        status_to_io_result(unsafe {
            uvll::uv_tcp_simultaneous_accepts(self.listener.handle, 1)
        })
    }

    fn dont_accept_simultaneously(&mut self) -> Result<(), IoError> {
        let _m = self.fire_homing_missile();
        status_to_io_result(unsafe {
            uvll::uv_tcp_simultaneous_accepts(self.listener.handle, 0)
        })
    }
}

////////////////////////////////////////////////////////////////////////////////
/// UDP implementation
////////////////////////////////////////////////////////////////////////////////

pub struct UdpWatcher {
    handle: *uvll::uv_udp_t,
    home: HomeHandle,
}

impl UdpWatcher {
    pub fn bind(io: &mut UvIoFactory, address: SocketAddr)
                -> Result<UdpWatcher, UvError> {
        let udp = UdpWatcher {
            handle: unsafe { uvll::malloc_handle(uvll::UV_UDP) },
            home: io.make_handle(),
        };
        assert_eq!(unsafe {
            uvll::uv_udp_init(io.uv_loop(), udp.handle)
        }, 0);
        let result = socket_addr_as_sockaddr(address, |addr| unsafe {
            uvll::uv_udp_bind(udp.handle, addr, 0u32)
        });
        return match result {
            0 => Ok(udp),
            n => Err(UvError(n)),
        };
    }
}

impl UvHandle<uvll::uv_udp_t> for UdpWatcher {
    fn uv_handle(&self) -> *uvll::uv_udp_t { self.handle }
}

impl HomingIO for UdpWatcher {
    fn home<'r>(&'r mut self) -> &'r mut HomeHandle { &mut self.home }
}

impl rtio::RtioSocket for UdpWatcher {
    fn socket_name(&mut self) -> Result<SocketAddr, IoError> {
        let _m = self.fire_homing_missile();
        socket_name(Udp, self.handle)
    }
}

impl rtio::RtioUdpSocket for UdpWatcher {
    fn recvfrom(&mut self, buf: &mut [u8])
        -> Result<(uint, SocketAddr), IoError>
    {
        struct Ctx {
            task: Option<BlockedTask>,
            buf: Option<Buf>,
            result: Option<(ssize_t, Option<SocketAddr>)>,
        }
        let _m = self.fire_homing_missile();

        let a = match unsafe {
            uvll::uv_udp_recv_start(self.handle, alloc_cb, recv_cb)
        } {
            0 => {
                let mut cx = Ctx {
                    task: None,
                    buf: Some(slice_to_uv_buf(buf)),
                    result: None,
                };
                wait_until_woken_after(&mut cx.task, || {
                    unsafe { uvll::set_data_for_uv_handle(self.handle, &cx) }
                });
                match cx.result.take_unwrap() {
                    (n, _) if n < 0 =>
                        Err(uv_error_to_io_error(UvError(n as c_int))),
                    (n, addr) => Ok((n as uint, addr.unwrap()))
                }
            }
            n => Err(uv_error_to_io_error(UvError(n)))
        };
        return a;

        extern fn alloc_cb(handle: *uvll::uv_udp_t,
                           _suggested_size: size_t,
                           buf: *mut Buf) {
            unsafe {
                let cx: &mut Ctx =
                    cast::transmute(uvll::get_data_for_uv_handle(handle));
                *buf = cx.buf.take().expect("recv alloc_cb called more than once")
            }
        }

        extern fn recv_cb(handle: *uvll::uv_udp_t, nread: ssize_t, buf: *Buf,
                          addr: *uvll::sockaddr, _flags: c_uint) {
            assert!(nread != uvll::ECANCELED as ssize_t);
            let cx: &mut Ctx = unsafe {
                cast::transmute(uvll::get_data_for_uv_handle(handle))
            };

            // When there's no data to read the recv callback can be a no-op.
            // This can happen if read returns EAGAIN/EWOULDBLOCK. By ignoring
            // this we just drop back to kqueue and wait for the next callback.
            if nread == 0 {
                cx.buf = Some(unsafe { *buf });
                return
            }

            unsafe {
                assert_eq!(uvll::uv_udp_recv_stop(handle), 0)
            }

            let cx: &mut Ctx = unsafe {
                cast::transmute(uvll::get_data_for_uv_handle(handle))
            };
            let addr = if addr == ptr::null() {
                None
            } else {
                Some(sockaddr_to_socket_addr(addr))
            };
            cx.result = Some((nread, addr));
            wakeup(&mut cx.task);
        }
    }

    fn sendto(&mut self, buf: &[u8], dst: SocketAddr) -> Result<(), IoError> {
        struct Ctx { task: Option<BlockedTask>, result: c_int }

        let _m = self.fire_homing_missile();

        let mut req = Request::new(uvll::UV_UDP_SEND);
        let buf = slice_to_uv_buf(buf);
        let result = socket_addr_as_sockaddr(dst, |dst| unsafe {
            uvll::uv_udp_send(req.handle, self.handle, [buf], dst, send_cb)
        });

        return match result {
            0 => {
                req.defuse(); // uv callback now owns this request
                let mut cx = Ctx { task: None, result: 0 };
                wait_until_woken_after(&mut cx.task, || {
                    req.set_data(&cx);
                });
                match cx.result {
                    0 => Ok(()),
                    n => Err(uv_error_to_io_error(UvError(n)))
                }
            }
            n => Err(uv_error_to_io_error(UvError(n)))
        };

        extern fn send_cb(req: *uvll::uv_udp_send_t, status: c_int) {
            let req = Request::wrap(req);
            assert!(status != uvll::ECANCELED);
            let cx: &mut Ctx = unsafe { req.get_data() };
            cx.result = status;
            wakeup(&mut cx.task);
        }
    }

    fn join_multicast(&mut self, multi: IpAddr) -> Result<(), IoError> {
        let _m = self.fire_homing_missile();
        status_to_io_result(unsafe {
            multi.to_str().with_c_str(|m_addr| {
                uvll::uv_udp_set_membership(self.handle,
                                            m_addr, ptr::null(),
                                            uvll::UV_JOIN_GROUP)
            })
        })
    }

    fn leave_multicast(&mut self, multi: IpAddr) -> Result<(), IoError> {
        let _m = self.fire_homing_missile();
        status_to_io_result(unsafe {
            multi.to_str().with_c_str(|m_addr| {
                uvll::uv_udp_set_membership(self.handle,
                                            m_addr, ptr::null(),
                                            uvll::UV_LEAVE_GROUP)
            })
        })
    }

    fn loop_multicast_locally(&mut self) -> Result<(), IoError> {
        let _m = self.fire_homing_missile();
        status_to_io_result(unsafe {
            uvll::uv_udp_set_multicast_loop(self.handle,
                                            1 as c_int)
        })
    }

    fn dont_loop_multicast_locally(&mut self) -> Result<(), IoError> {
        let _m = self.fire_homing_missile();
        status_to_io_result(unsafe {
            uvll::uv_udp_set_multicast_loop(self.handle,
                                            0 as c_int)
        })
    }

    fn multicast_time_to_live(&mut self, ttl: int) -> Result<(), IoError> {
        let _m = self.fire_homing_missile();
        status_to_io_result(unsafe {
            uvll::uv_udp_set_multicast_ttl(self.handle,
                                           ttl as c_int)
        })
    }

    fn time_to_live(&mut self, ttl: int) -> Result<(), IoError> {
        let _m = self.fire_homing_missile();
        status_to_io_result(unsafe {
            uvll::uv_udp_set_ttl(self.handle, ttl as c_int)
        })
    }

    fn hear_broadcasts(&mut self) -> Result<(), IoError> {
        let _m = self.fire_homing_missile();
        status_to_io_result(unsafe {
            uvll::uv_udp_set_broadcast(self.handle,
                                       1 as c_int)
        })
    }

    fn ignore_broadcasts(&mut self) -> Result<(), IoError> {
        let _m = self.fire_homing_missile();
        status_to_io_result(unsafe {
            uvll::uv_udp_set_broadcast(self.handle,
                                       0 as c_int)
        })
    }
}

impl Drop for UdpWatcher {
    fn drop(&mut self) {
        // Send ourselves home to close this handle (blocking while doing so).
        let _m = self.fire_homing_missile();
        self.close();
    }
}

#[cfg(test)]
mod test {
    use std::rt::rtio::{RtioTcpStream, RtioTcpListener, RtioTcpAcceptor,
                        RtioUdpSocket};
    use std::io::test::{next_test_ip4, next_test_ip6};

    use super::{UdpWatcher, TcpWatcher, TcpListener};
    use super::super::local_loop;

    #[test]
    fn connect_close_ip4() {
        match TcpWatcher::connect(local_loop(), next_test_ip4()) {
            Ok(..) => fail!(),
            Err(e) => assert_eq!(e.name(), ~"ECONNREFUSED"),
        }
    }

    #[test]
    fn connect_close_ip6() {
        match TcpWatcher::connect(local_loop(), next_test_ip6()) {
            Ok(..) => fail!(),
            Err(e) => assert_eq!(e.name(), ~"ECONNREFUSED"),
        }
    }

    #[test]
    fn udp_bind_close_ip4() {
        match UdpWatcher::bind(local_loop(), next_test_ip4()) {
            Ok(..) => {}
            Err(..) => fail!()
        }
    }

    #[test]
    fn udp_bind_close_ip6() {
        match UdpWatcher::bind(local_loop(), next_test_ip6()) {
            Ok(..) => {}
            Err(..) => fail!()
        }
    }

    #[test]
    fn listen_ip4() {
        let (port, chan) = Chan::new();
        let addr = next_test_ip4();

        do spawn {
            let w = match TcpListener::bind(local_loop(), addr) {
                Ok(w) => w, Err(e) => fail!("{:?}", e)
            };
            let mut w = match w.listen() {
                Ok(w) => w, Err(e) => fail!("{:?}", e),
            };
            chan.send(());
            match w.accept() {
                Ok(mut stream) => {
                    let mut buf = [0u8, ..10];
                    match stream.read(buf) {
                        Ok(10) => {} e => fail!("{:?}", e),
                    }
                    for i in range(0, 10u8) {
                        assert_eq!(buf[i], i + 1);
                    }
                }
                Err(e) => fail!("{:?}", e)
            }
        }

        port.recv();
        let mut w = match TcpWatcher::connect(local_loop(), addr) {
            Ok(w) => w, Err(e) => fail!("{:?}", e)
        };
        match w.write([1, 2, 3, 4, 5, 6, 7, 8, 9, 10]) {
            Ok(()) => {}, Err(e) => fail!("{:?}", e)
        }
    }

    #[test]
    fn listen_ip6() {
        let (port, chan) = Chan::new();
        let addr = next_test_ip6();

        do spawn {
            let w = match TcpListener::bind(local_loop(), addr) {
                Ok(w) => w, Err(e) => fail!("{:?}", e)
            };
            let mut w = match w.listen() {
                Ok(w) => w, Err(e) => fail!("{:?}", e),
            };
            chan.send(());
            match w.accept() {
                Ok(mut stream) => {
                    let mut buf = [0u8, ..10];
                    match stream.read(buf) {
                        Ok(10) => {} e => fail!("{:?}", e),
                    }
                    for i in range(0, 10u8) {
                        assert_eq!(buf[i], i + 1);
                    }
                }
                Err(e) => fail!("{:?}", e)
            }
        }

        port.recv();
        let mut w = match TcpWatcher::connect(local_loop(), addr) {
            Ok(w) => w, Err(e) => fail!("{:?}", e)
        };
        match w.write([1, 2, 3, 4, 5, 6, 7, 8, 9, 10]) {
            Ok(()) => {}, Err(e) => fail!("{:?}", e)
        }
    }

    #[test]
    fn udp_recv_ip4() {
        let (port, chan) = Chan::new();
        let client = next_test_ip4();
        let server = next_test_ip4();

        do spawn {
            match UdpWatcher::bind(local_loop(), server) {
                Ok(mut w) => {
                    chan.send(());
                    let mut buf = [0u8, ..10];
                    match w.recvfrom(buf) {
                        Ok((10, addr)) => assert_eq!(addr, client),
                        e => fail!("{:?}", e),
                    }
                    for i in range(0, 10u8) {
                        assert_eq!(buf[i], i + 1);
                    }
                }
                Err(e) => fail!("{:?}", e)
            }
        }

        port.recv();
        let mut w = match UdpWatcher::bind(local_loop(), client) {
            Ok(w) => w, Err(e) => fail!("{:?}", e)
        };
        match w.sendto([1, 2, 3, 4, 5, 6, 7, 8, 9, 10], server) {
            Ok(()) => {}, Err(e) => fail!("{:?}", e)
        }
    }

    #[test]
    fn udp_recv_ip6() {
        let (port, chan) = Chan::new();
        let client = next_test_ip6();
        let server = next_test_ip6();

        do spawn {
            match UdpWatcher::bind(local_loop(), server) {
                Ok(mut w) => {
                    chan.send(());
                    let mut buf = [0u8, ..10];
                    match w.recvfrom(buf) {
                        Ok((10, addr)) => assert_eq!(addr, client),
                        e => fail!("{:?}", e),
                    }
                    for i in range(0, 10u8) {
                        assert_eq!(buf[i], i + 1);
                    }
                }
                Err(e) => fail!("{:?}", e)
            }
        }

        port.recv();
        let mut w = match UdpWatcher::bind(local_loop(), client) {
            Ok(w) => w, Err(e) => fail!("{:?}", e)
        };
        match w.sendto([1, 2, 3, 4, 5, 6, 7, 8, 9, 10], server) {
            Ok(()) => {}, Err(e) => fail!("{:?}", e)
        }
    }

    #[test]
    fn test_read_read_read() {
        let addr = next_test_ip4();
        static MAX: uint = 5000;
        let (port, chan) = Chan::new();

        do spawn {
            let listener = TcpListener::bind(local_loop(), addr).unwrap();
            let mut acceptor = listener.listen().unwrap();
            chan.send(());
            let mut stream = acceptor.accept().unwrap();
            let buf = [1, .. 2048];
            let mut total_bytes_written = 0;
            while total_bytes_written < MAX {
                assert!(stream.write(buf).is_ok());
                uvdebug!("wrote bytes");
                total_bytes_written += buf.len();
            }
        }

        port.recv();
        let mut stream = TcpWatcher::connect(local_loop(), addr).unwrap();
        let mut buf = [0, .. 2048];
        let mut total_bytes_read = 0;
        while total_bytes_read < MAX {
            let nread = stream.read(buf).unwrap();
            total_bytes_read += nread;
            for i in range(0u, nread) {
                assert_eq!(buf[i], 1);
            }
        }
        uvdebug!("read {} bytes total", total_bytes_read);
    }

    #[test]
    #[ignore(cfg(windows))] // FIXME(#10102) server never sees second packet
    fn test_udp_twice() {
        let server_addr = next_test_ip4();
        let client_addr = next_test_ip4();
        let (port, chan) = Chan::new();

        do spawn {
            let mut client = UdpWatcher::bind(local_loop(), client_addr).unwrap();
            port.recv();
            assert!(client.sendto([1], server_addr).is_ok());
            assert!(client.sendto([2], server_addr).is_ok());
        }

        let mut server = UdpWatcher::bind(local_loop(), server_addr).unwrap();
        chan.send(());
        let mut buf1 = [0];
        let mut buf2 = [0];
        let (nread1, src1) = server.recvfrom(buf1).unwrap();
        let (nread2, src2) = server.recvfrom(buf2).unwrap();
        assert_eq!(nread1, 1);
        assert_eq!(nread2, 1);
        assert_eq!(src1, client_addr);
        assert_eq!(src2, client_addr);
        assert_eq!(buf1[0], 1);
        assert_eq!(buf2[0], 2);
    }

    #[test]
    fn test_udp_many_read() {
        let server_out_addr = next_test_ip4();
        let server_in_addr = next_test_ip4();
        let client_out_addr = next_test_ip4();
        let client_in_addr = next_test_ip4();
        static MAX: uint = 500_000;

        let (p1, c1) = Chan::new();
        let (p2, c2) = Chan::new();

        do spawn {
            let l = local_loop();
            let mut server_out = UdpWatcher::bind(l, server_out_addr).unwrap();
            let mut server_in = UdpWatcher::bind(l, server_in_addr).unwrap();
            let (port, chan) = (p1, c2);
            chan.send(());
            port.recv();
            let msg = [1, .. 2048];
            let mut total_bytes_sent = 0;
            let mut buf = [1];
            while buf[0] == 1 {
                // send more data
                assert!(server_out.sendto(msg, client_in_addr).is_ok());
                total_bytes_sent += msg.len();
                // check if the client has received enough
                let res = server_in.recvfrom(buf);
                assert!(res.is_ok());
                let (nread, src) = res.unwrap();
                assert_eq!(nread, 1);
                assert_eq!(src, client_out_addr);
            }
            assert!(total_bytes_sent >= MAX);
        }

        let l = local_loop();
        let mut client_out = UdpWatcher::bind(l, client_out_addr).unwrap();
        let mut client_in = UdpWatcher::bind(l, client_in_addr).unwrap();
        let (port, chan) = (p2, c1);
        port.recv();
        chan.send(());
        let mut total_bytes_recv = 0;
        let mut buf = [0, .. 2048];
        while total_bytes_recv < MAX {
            // ask for more
            assert!(client_out.sendto([1], server_in_addr).is_ok());
            // wait for data
            let res = client_in.recvfrom(buf);
            assert!(res.is_ok());
            let (nread, src) = res.unwrap();
            assert_eq!(src, server_out_addr);
            total_bytes_recv += nread;
            for i in range(0u, nread) {
                assert_eq!(buf[i], 1);
            }
        }
        // tell the server we're done
        assert!(client_out.sendto([0], server_in_addr).is_ok());
    }

    #[test]
    fn test_read_and_block() {
        let addr = next_test_ip4();
        let (port, chan) = Chan::<Port<()>>::new();

        do spawn {
            let port2 = port.recv();
            let mut stream = TcpWatcher::connect(local_loop(), addr).unwrap();
            stream.write([0, 1, 2, 3, 4, 5, 6, 7]);
            stream.write([0, 1, 2, 3, 4, 5, 6, 7]);
            port2.recv();
            stream.write([0, 1, 2, 3, 4, 5, 6, 7]);
            stream.write([0, 1, 2, 3, 4, 5, 6, 7]);
            port2.recv();
        }

        let listener = TcpListener::bind(local_loop(), addr).unwrap();
        let mut acceptor = listener.listen().unwrap();
        let (port2, chan2) = Chan::new();
        chan.send(port2);
        let mut stream = acceptor.accept().unwrap();
        let mut buf = [0, .. 2048];

        let expected = 32;
        let mut current = 0;
        let mut reads = 0;

        while current < expected {
            let nread = stream.read(buf).unwrap();
            for i in range(0u, nread) {
                let val = buf[i] as uint;
                assert_eq!(val, current % 8);
                current += 1;
            }
            reads += 1;

            chan2.try_send(());
        }

        // Make sure we had multiple reads
        assert!(reads > 1);
    }

    #[test]
    fn test_simple_tcp_server_and_client_on_diff_threads() {
        let addr = next_test_ip4();

        do spawn {
            let listener = TcpListener::bind(local_loop(), addr).unwrap();
            let mut acceptor = listener.listen().unwrap();
            let mut stream = acceptor.accept().unwrap();
            let mut buf = [0, .. 2048];
            let nread = stream.read(buf).unwrap();
            assert_eq!(nread, 8);
            for i in range(0u, nread) {
                assert_eq!(buf[i], i as u8);
            }
        }

        let mut stream = TcpWatcher::connect(local_loop(), addr);
        while stream.is_err() {
            stream = TcpWatcher::connect(local_loop(), addr);
        }
        stream.unwrap().write([0, 1, 2, 3, 4, 5, 6, 7]);
    }

    #[should_fail] #[test]
    fn tcp_listener_fail_cleanup() {
        let addr = next_test_ip4();
        let w = TcpListener::bind(local_loop(), addr).unwrap();
        let _w = w.listen().unwrap();
        fail!();
    }

    #[should_fail] #[test]
    fn tcp_stream_fail_cleanup() {
        let (port, chan) = Chan::new();
        let addr = next_test_ip4();

        do spawn {
            let w = TcpListener::bind(local_loop(), addr).unwrap();
            let mut w = w.listen().unwrap();
            chan.send(());
            w.accept();
        }
        port.recv();
        let _w = TcpWatcher::connect(local_loop(), addr).unwrap();
        fail!();
    }

    #[should_fail] #[test]
    fn udp_listener_fail_cleanup() {
        let addr = next_test_ip4();
        let _w = UdpWatcher::bind(local_loop(), addr).unwrap();
        fail!();
    }

    #[should_fail] #[test]
    fn udp_fail_other_task() {
        let addr = next_test_ip4();
        let (port, chan) = Chan::new();

        // force the handle to be created on a different scheduler, failure in
        // the original task will force a homing operation back to this
        // scheduler.
        do spawn {
            let w = UdpWatcher::bind(local_loop(), addr).unwrap();
            chan.send(w);
        }

        let _w = port.recv();
        fail!();
    }
}
