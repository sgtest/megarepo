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
use std::io::net::ip;
use std::libc::{size_t, ssize_t, c_int, c_void, c_uint};
use std::libc;
use std::mem;
use std::ptr;
use std::rt::rtio;
use std::rt::task::BlockedTask;

use access::Access;
use homing::{HomingIO, HomeHandle};
use rc::Refcount;
use stream::StreamWatcher;
use super::{Loop, Request, UvError, Buf, status_to_io_result,
            uv_error_to_io_error, UvHandle, slice_to_uv_buf,
            wait_until_woken_after, wakeup};
use uvio::UvIoFactory;
use uvll;

////////////////////////////////////////////////////////////////////////////////
/// Generic functions related to dealing with sockaddr things
////////////////////////////////////////////////////////////////////////////////

pub fn htons(u: u16) -> u16 { mem::to_be16(u as i16) as u16 }
pub fn ntohs(u: u16) -> u16 { mem::from_be16(u as i16) as u16 }

pub fn sockaddr_to_addr(storage: &libc::sockaddr_storage,
                        len: uint) -> ip::SocketAddr {
    match storage.ss_family as c_int {
        libc::AF_INET => {
            assert!(len as uint >= mem::size_of::<libc::sockaddr_in>());
            let storage: &libc::sockaddr_in = unsafe {
                cast::transmute(storage)
            };
            let addr = storage.sin_addr.s_addr as u32;
            let a = (addr >>  0) as u8;
            let b = (addr >>  8) as u8;
            let c = (addr >> 16) as u8;
            let d = (addr >> 24) as u8;
            ip::SocketAddr {
                ip: ip::Ipv4Addr(a, b, c, d),
                port: ntohs(storage.sin_port),
            }
        }
        libc::AF_INET6 => {
            assert!(len as uint >= mem::size_of::<libc::sockaddr_in6>());
            let storage: &libc::sockaddr_in6 = unsafe {
                cast::transmute(storage)
            };
            let a = ntohs(storage.sin6_addr.s6_addr[0]);
            let b = ntohs(storage.sin6_addr.s6_addr[1]);
            let c = ntohs(storage.sin6_addr.s6_addr[2]);
            let d = ntohs(storage.sin6_addr.s6_addr[3]);
            let e = ntohs(storage.sin6_addr.s6_addr[4]);
            let f = ntohs(storage.sin6_addr.s6_addr[5]);
            let g = ntohs(storage.sin6_addr.s6_addr[6]);
            let h = ntohs(storage.sin6_addr.s6_addr[7]);
            ip::SocketAddr {
                ip: ip::Ipv6Addr(a, b, c, d, e, f, g, h),
                port: ntohs(storage.sin6_port),
            }
        }
        n => {
            fail!("unknown family {}", n);
        }
    }
}

fn addr_to_sockaddr(addr: ip::SocketAddr) -> (libc::sockaddr_storage, uint) {
    unsafe {
        let mut storage: libc::sockaddr_storage = mem::init();
        let len = match addr.ip {
            ip::Ipv4Addr(a, b, c, d) => {
                let storage: &mut libc::sockaddr_in =
                    cast::transmute(&mut storage);
                (*storage).sin_family = libc::AF_INET as libc::sa_family_t;
                (*storage).sin_port = htons(addr.port);
                (*storage).sin_addr = libc::in_addr {
                    s_addr: (d as u32 << 24) |
                            (c as u32 << 16) |
                            (b as u32 <<  8) |
                            (a as u32 <<  0)
                };
                mem::size_of::<libc::sockaddr_in>()
            }
            ip::Ipv6Addr(a, b, c, d, e, f, g, h) => {
                let storage: &mut libc::sockaddr_in6 =
                    cast::transmute(&mut storage);
                storage.sin6_family = libc::AF_INET6 as libc::sa_family_t;
                storage.sin6_port = htons(addr.port);
                storage.sin6_addr = libc::in6_addr {
                    s6_addr: [
                        htons(a),
                        htons(b),
                        htons(c),
                        htons(d),
                        htons(e),
                        htons(f),
                        htons(g),
                        htons(h),
                    ]
                };
                mem::size_of::<libc::sockaddr_in6>()
            }
        };
        return (storage, len);
    }
}

enum SocketNameKind {
    TcpPeer,
    Tcp,
    Udp
}

fn socket_name(sk: SocketNameKind,
               handle: *c_void) -> Result<ip::SocketAddr, IoError> {
    let getsockname = match sk {
        TcpPeer => uvll::uv_tcp_getpeername,
        Tcp     => uvll::uv_tcp_getsockname,
        Udp     => uvll::uv_udp_getsockname,
    };

    // Allocate a sockaddr_storage since we don't know if it's ipv4 or ipv6
    let mut sockaddr: libc::sockaddr_storage = unsafe { mem::init() };
    let mut namelen = mem::size_of::<libc::sockaddr_storage>() as c_int;

    let sockaddr_p = &mut sockaddr as *mut libc::sockaddr_storage;
    match unsafe {
        getsockname(handle, sockaddr_p as *mut libc::sockaddr, &mut namelen)
    } {
        0 => Ok(sockaddr_to_addr(&sockaddr, namelen as uint)),
        n => Err(uv_error_to_io_error(UvError(n)))
    }
}

////////////////////////////////////////////////////////////////////////////////
/// TCP implementation
////////////////////////////////////////////////////////////////////////////////

pub struct TcpWatcher {
    handle: *uvll::uv_tcp_t,
    stream: StreamWatcher,
    home: HomeHandle,
    priv refcount: Refcount,

    // libuv can't support concurrent reads and concurrent writes of the same
    // stream object, so we use these access guards in order to arbitrate among
    // multiple concurrent reads and writes. Note that libuv *can* read and
    // write simultaneously, it just can't read and read simultaneously.
    priv read_access: Access,
    priv write_access: Access,
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
            refcount: Refcount::new(),
            read_access: Access::new(),
            write_access: Access::new(),
        }
    }

    pub fn connect(io: &mut UvIoFactory, address: ip::SocketAddr)
        -> Result<TcpWatcher, UvError>
    {
        struct Ctx { status: c_int, task: Option<BlockedTask> }

        let tcp = TcpWatcher::new(io);
        let (addr, _len) = addr_to_sockaddr(address);
        let mut req = Request::new(uvll::UV_CONNECT);
        let result = unsafe {
            let addr_p = &addr as *libc::sockaddr_storage;
            uvll::uv_tcp_connect(req.handle, tcp.handle,
                                 addr_p as *libc::sockaddr,
                                 connect_cb)
        };
        return match result {
            0 => {
                req.defuse(); // uv callback now owns this request
                let mut cx = Ctx { status: 0, task: None };
                wait_until_woken_after(&mut cx.task, &io.loop_, || {
                    req.set_data(&cx);
                });
                match cx.status {
                    0 => Ok(tcp),
                    n => Err(UvError(n)),
                }
            }
            n => Err(UvError(n))
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
    fn socket_name(&mut self) -> Result<ip::SocketAddr, IoError> {
        let _m = self.fire_homing_missile();
        socket_name(Tcp, self.handle)
    }
}

impl rtio::RtioTcpStream for TcpWatcher {
    fn read(&mut self, buf: &mut [u8]) -> Result<uint, IoError> {
        let m = self.fire_homing_missile();
        let _g = self.read_access.grant(m);
        self.stream.read(buf).map_err(uv_error_to_io_error)
    }

    fn write(&mut self, buf: &[u8]) -> Result<(), IoError> {
        let m = self.fire_homing_missile();
        let _g = self.write_access.grant(m);
        self.stream.write(buf).map_err(uv_error_to_io_error)
    }

    fn peer_name(&mut self) -> Result<ip::SocketAddr, IoError> {
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

    fn clone(&self) -> ~rtio::RtioTcpStream {
        ~TcpWatcher {
            handle: self.handle,
            stream: StreamWatcher::new(self.handle),
            home: self.home.clone(),
            refcount: self.refcount.clone(),
            write_access: self.write_access.clone(),
            read_access: self.read_access.clone(),
        } as ~rtio::RtioTcpStream
    }
}

impl UvHandle<uvll::uv_tcp_t> for TcpWatcher {
    fn uv_handle(&self) -> *uvll::uv_tcp_t { self.stream.handle }
}

impl Drop for TcpWatcher {
    fn drop(&mut self) {
        let _m = self.fire_homing_missile();
        if self.refcount.decrement() {
            self.close();
        }
    }
}

// TCP listeners (unbound servers)

impl TcpListener {
    pub fn bind(io: &mut UvIoFactory, address: ip::SocketAddr)
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
        let (addr, _len) = addr_to_sockaddr(address);
        let res = unsafe {
            let addr_p = &addr as *libc::sockaddr_storage;
            uvll::uv_tcp_bind(l.handle, addr_p as *libc::sockaddr)
        };
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
    fn socket_name(&mut self) -> Result<ip::SocketAddr, IoError> {
        let _m = self.fire_homing_missile();
        socket_name(Tcp, self.handle)
    }
}

impl rtio::RtioTcpListener for TcpListener {
    fn listen(~self) -> Result<~rtio::RtioTcpAcceptor, IoError> {
        // create the acceptor object from ourselves
        let mut acceptor = ~TcpAcceptor { listener: self };

        let _m = acceptor.fire_homing_missile();
        // FIXME: the 128 backlog should be configurable
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
    fn socket_name(&mut self) -> Result<ip::SocketAddr, IoError> {
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

    // See above for what these fields are
    priv refcount: Refcount,
    priv read_access: Access,
    priv write_access: Access,
}

impl UdpWatcher {
    pub fn bind(io: &mut UvIoFactory, address: ip::SocketAddr)
                -> Result<UdpWatcher, UvError> {
        let udp = UdpWatcher {
            handle: unsafe { uvll::malloc_handle(uvll::UV_UDP) },
            home: io.make_handle(),
            refcount: Refcount::new(),
            read_access: Access::new(),
            write_access: Access::new(),
        };
        assert_eq!(unsafe {
            uvll::uv_udp_init(io.uv_loop(), udp.handle)
        }, 0);
        let (addr, _len) = addr_to_sockaddr(address);
        let result = unsafe {
            let addr_p = &addr as *libc::sockaddr_storage;
            uvll::uv_udp_bind(udp.handle, addr_p as *libc::sockaddr, 0u32)
        };
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
    fn socket_name(&mut self) -> Result<ip::SocketAddr, IoError> {
        let _m = self.fire_homing_missile();
        socket_name(Udp, self.handle)
    }
}

impl rtio::RtioUdpSocket for UdpWatcher {
    fn recvfrom(&mut self, buf: &mut [u8])
        -> Result<(uint, ip::SocketAddr), IoError>
    {
        struct Ctx {
            task: Option<BlockedTask>,
            buf: Option<Buf>,
            result: Option<(ssize_t, Option<ip::SocketAddr>)>,
        }
        let loop_ = self.uv_loop();
        let m = self.fire_homing_missile();
        let _g = self.read_access.grant(m);

        let a = match unsafe {
            uvll::uv_udp_recv_start(self.handle, alloc_cb, recv_cb)
        } {
            0 => {
                let mut cx = Ctx {
                    task: None,
                    buf: Some(slice_to_uv_buf(buf)),
                    result: None,
                };
                let handle = self.handle;
                wait_until_woken_after(&mut cx.task, &loop_, || {
                    unsafe { uvll::set_data_for_uv_handle(handle, &cx) }
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
                          addr: *libc::sockaddr, _flags: c_uint) {
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
                let len = mem::size_of::<libc::sockaddr_storage>();
                Some(sockaddr_to_addr(unsafe { cast::transmute(addr) }, len))
            };
            cx.result = Some((nread, addr));
            wakeup(&mut cx.task);
        }
    }

    fn sendto(&mut self, buf: &[u8], dst: ip::SocketAddr) -> Result<(), IoError> {
        struct Ctx { task: Option<BlockedTask>, result: c_int }

        let m = self.fire_homing_missile();
        let loop_ = self.uv_loop();
        let _g = self.write_access.grant(m);

        let mut req = Request::new(uvll::UV_UDP_SEND);
        let buf = slice_to_uv_buf(buf);
        let (addr, _len) = addr_to_sockaddr(dst);
        let result = unsafe {
            let addr_p = &addr as *libc::sockaddr_storage;
            uvll::uv_udp_send(req.handle, self.handle, [buf],
                              addr_p as *libc::sockaddr, send_cb)
        };

        return match result {
            0 => {
                req.defuse(); // uv callback now owns this request
                let mut cx = Ctx { task: None, result: 0 };
                wait_until_woken_after(&mut cx.task, &loop_, || {
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

    fn join_multicast(&mut self, multi: ip::IpAddr) -> Result<(), IoError> {
        let _m = self.fire_homing_missile();
        status_to_io_result(unsafe {
            multi.to_str().with_c_str(|m_addr| {
                uvll::uv_udp_set_membership(self.handle,
                                            m_addr, ptr::null(),
                                            uvll::UV_JOIN_GROUP)
            })
        })
    }

    fn leave_multicast(&mut self, multi: ip::IpAddr) -> Result<(), IoError> {
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

    fn clone(&self) -> ~rtio::RtioUdpSocket {
        ~UdpWatcher {
            handle: self.handle,
            home: self.home.clone(),
            refcount: self.refcount.clone(),
            write_access: self.write_access.clone(),
            read_access: self.read_access.clone(),
        } as ~rtio::RtioUdpSocket
    }
}

impl Drop for UdpWatcher {
    fn drop(&mut self) {
        // Send ourselves home to close this handle (blocking while doing so).
        let _m = self.fire_homing_missile();
        if self.refcount.decrement() {
            self.close();
        }
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

        spawn(proc() {
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
        });

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

        spawn(proc() {
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
        });

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

        spawn(proc() {
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
        });

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

        spawn(proc() {
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
        });

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

        spawn(proc() {
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
        });

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

        spawn(proc() {
            let mut client = UdpWatcher::bind(local_loop(), client_addr).unwrap();
            port.recv();
            assert!(client.sendto([1], server_addr).is_ok());
            assert!(client.sendto([2], server_addr).is_ok());
        });

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

        spawn(proc() {
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
        });

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

        spawn(proc() {
            let port2 = port.recv();
            let mut stream = TcpWatcher::connect(local_loop(), addr).unwrap();
            stream.write([0, 1, 2, 3, 4, 5, 6, 7]).unwrap();
            stream.write([0, 1, 2, 3, 4, 5, 6, 7]).unwrap();
            port2.recv();
            stream.write([0, 1, 2, 3, 4, 5, 6, 7]).unwrap();
            stream.write([0, 1, 2, 3, 4, 5, 6, 7]).unwrap();
            port2.recv();
        });

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

        spawn(proc() {
            let listener = TcpListener::bind(local_loop(), addr).unwrap();
            let mut acceptor = listener.listen().unwrap();
            let mut stream = acceptor.accept().unwrap();
            let mut buf = [0, .. 2048];
            let nread = stream.read(buf).unwrap();
            assert_eq!(nread, 8);
            for i in range(0u, nread) {
                assert_eq!(buf[i], i as u8);
            }
        });

        let mut stream = TcpWatcher::connect(local_loop(), addr);
        while stream.is_err() {
            stream = TcpWatcher::connect(local_loop(), addr);
        }
        stream.unwrap().write([0, 1, 2, 3, 4, 5, 6, 7]).unwrap();
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

        spawn(proc() {
            let w = TcpListener::bind(local_loop(), addr).unwrap();
            let mut w = w.listen().unwrap();
            chan.send(());
            drop(w.accept().unwrap());
        });
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
        spawn(proc() {
            let w = UdpWatcher::bind(local_loop(), addr).unwrap();
            chan.send(w);
        });

        let _w = port.recv();
        fail!();
    }
}
