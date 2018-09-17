use std::{fmt, io, mem, net};

use pnet_macros_support::packet::Packet;
use pnetlink::{
    packet::netlink::{NetlinkIterable, NLMSG_DONE, NLMSG_ERROR},
    socket::{NetlinkProtocol, NetlinkSocket},
};

pub struct SockDiag {
    socket: NetlinkSocket,
    buf: Vec<u8>,
}

impl SockDiag {
    pub fn new() -> io::Result<SockDiag> {
        let socket = NetlinkSocket::bind(NetlinkProtocol::Inet_diag, 0)?;
        let buf = vec![0; 128];
        Ok(SockDiag { socket, buf })
    }

    pub fn find_one<'a>(
        &'a mut self,
        protocol: Proto,
        src: net::SocketAddr,
        dst: net::SocketAddr,
    ) -> Result<&'a InetDiagMsg, io::Error> {
        const NLMSG_ALIGNTO: usize = 4;
        const fn nlmsg_align(len: usize) -> usize {
            (len + NLMSG_ALIGNTO - 1) & !(NLMSG_ALIGNTO - 1)
        }
        const NLMSG_HDRLEN: usize = nlmsg_align(mem::size_of::<libc::nlmsghdr>());
        const fn nlmsg_length(len: usize) -> usize {
            len + NLMSG_HDRLEN
        }
        const SOCK_DIAG_BY_FAMILY: u16 = 20;
        const INET_DIAG_NOCOOKIE: u32 = !0;

        assert_eq!(src.is_ipv4(), dst.is_ipv4(),);

        let nlh = libc::nlmsghdr {
            nlmsg_len: nlmsg_length(mem::size_of::<InetDiagReqV2>()) as u32,
            nlmsg_type: SOCK_DIAG_BY_FAMILY,
            nlmsg_flags: (libc::NLM_F_REQUEST) as u16,
            nlmsg_seq: 0,
            nlmsg_pid: 0,
        };
        let req = InetDiagReqV2 {
            sdiag_family: if src.is_ipv4() {
                libc::AF_INET
            } else {
                libc::AF_INET6
            } as u8,
            sdiag_protocol: protocol as u8,
            idiag_ext: 0,
            pad: 0,
            idiag_states: !0, // any state
            id: InetDiagSockId {
                idiag_sport: src.port().into(),
                idiag_dport: dst.port().into(),
                idiag_src: src.ip().into(),
                idiag_dst: dst.ip().into(),
                idiag_if: 0,
                idiag_cookie: [INET_DIAG_NOCOOKIE; 2],
            },
        };
        // let mut iov = [
        //     libc::iovec {
        //         iov_base: &mut nlh as *mut _ as *mut libc::c_void,
        //         iov_len: mem::size_of::<libc::nlmsghdr>(),
        //     },
        //     libc::iovec {
        //         iov_base: &mut req as *mut _ as *mut libc::c_void,
        //         iov_len: mem::size_of::<InetDiagReqV2>(),
        //     },
        // ];
        // let mut sa = unsafe { mem::zeroed::<libc::sockaddr_nl>() };
        // sa.nl_family = libc::AF_NETLINK as u16;
        // let mut msg = libc::msghdr {
        //     msg_name: &mut sa as *mut _ as *mut libc::c_void,
        //     msg_namelen: mem::size_of::<libc::sockaddr_nl>() as u32,
        //     msg_iov: &mut iov as *mut _ as *mut libc::iovec,
        //     msg_iovlen: 2,
        //     msg_control: libc::NL0 as *mut _,
        //     msg_controllen: 0,
        //     msg_flags: 0,
        // };
        // let res = unsafe { libc::sendmsg(socket.as_raw_fd(), &mut msg, 0) };
        // if res == -1 {
        //     return Err(io::Error::last_os_error());
        // }
        #[repr(C)]
        struct Msg(libc::nlmsghdr, InetDiagReqV2);
        let msg = Msg(nlh, req);
        let msg: [u8; mem::size_of::<Msg>()] = unsafe { mem::transmute(msg) };
        self.socket.send(&msg)?;

        let n = self.socket.recv(&mut self.buf)?;
        if let Some(msg) = NetlinkIterable::new(&self.buf[..n]).next() {
            if msg.get_kind() == NLMSG_ERROR || msg.get_kind() == NLMSG_DONE {
                return Err(io::Error::from(io::ErrorKind::NotFound));
            }
            let diag_msg = msg.payload() as *const _ as *const InetDiagMsg;
            let diag_msg = unsafe { &(*diag_msg) };
            // make sure socket is empty
            match self.socket.recv(&mut [0u8; 1]) {
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => (),
                Err(e) => return Err(e),
                Ok(_) => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "SockDiag::find_one got more than one response",
                    ))
                }
            }
            Ok(diag_msg)
        } else {
            Err(io::Error::from(io::ErrorKind::NotFound))
        }
    }
}

#[repr(C)]
#[derive(Debug)]
struct InetDiagReqV2 {
    sdiag_family: u8,
    sdiag_protocol: u8,
    idiag_ext: u8,
    pad: u8,
    idiag_states: u32,
    id: InetDiagSockId,
}

#[repr(C)]
#[derive(Debug)]
pub struct InetDiagMsg {
    pub idiag_family: u8,
    pub idiag_state: u8,
    pub idiag_timer: u8,
    pub idiag_retrans: u8,
    pub id: InetDiagSockId,
    pub idiag_expires: u32,
    pub idiag_rqueue: u32,
    pub idiag_wqueue: u32,
    pub idiag_uid: u32,
    pub idiag_inode: u32,
}

#[repr(C)]
#[derive(Debug)]
pub struct InetDiagSockId {
    pub idiag_sport: Port,
    pub idiag_dport: Port,
    pub idiag_src: Ipv4or6,
    pub idiag_dst: Ipv4or6,
    pub idiag_if: u32,
    pub idiag_cookie: [u32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct Port([u8; 2]); // u16be

impl From<u16> for Port {
    fn from(port: u16) -> Self {
        Port([(port >> 8) as u8, port as u8])
    }
}

impl From<Port> for u16 {
    fn from(port: Port) -> Self {
        ((port.0[0] as u16) << 8) | (port.0[1] as u16)
    }
}

impl fmt::Debug for Port {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        u16::from(*self).fmt(f)
    }
}

// [u32be; 4]
#[repr(C)]
#[derive(Clone, Copy)]
pub union Ipv4or6 {
    v4: [u8; 4],
    v6: [u8; 16],
}

// TODO: zero copy
impl From<net::Ipv4Addr> for Ipv4or6 {
    fn from(addr: net::Ipv4Addr) -> Self {
        Ipv4or6 { v4: addr.octets() }
    }
}

impl From<Ipv4or6> for net::Ipv4Addr {
    fn from(addr: Ipv4or6) -> Self {
        unsafe { addr.v4.into() }
    }
}

// TODO: zero copy
impl From<net::Ipv6Addr> for Ipv4or6 {
    fn from(addr: net::Ipv6Addr) -> Self {
        #[inline]
        fn l(v: u16) -> u8 {
            (v >> 8) as u8
        }
        #[inline]
        fn r(v: u16) -> u8 {
            v as u8
        }
        let v6: [u16; 8] = addr.segments();
        let v6: [u8; 16] = [
            l(v6[0]),
            r(v6[0]),
            l(v6[1]),
            r(v6[1]),
            l(v6[2]),
            r(v6[2]),
            l(v6[3]),
            r(v6[3]),
            l(v6[4]),
            r(v6[4]),
            l(v6[5]),
            r(v6[5]),
            l(v6[6]),
            r(v6[6]),
            l(v6[7]),
            r(v6[7]),
        ];
        Ipv4or6 { v6 }
    }
}

impl From<Ipv4or6> for net::Ipv6Addr {
    fn from(addr: Ipv4or6) -> Self {
        unsafe { addr.v6.into() }
    }
}

impl From<net::IpAddr> for Ipv4or6 {
    fn from(addr: net::IpAddr) -> Self {
        use std::net::IpAddr::*;
        match addr {
            V4(addr) => addr.into(),
            V6(addr) => addr.into(),
        }
    }
}

impl fmt::Debug for Ipv4or6 {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Ipv4or6")
            .field("v4", &net::Ipv4Addr::from(*self))
            .field("v6", &net::Ipv6Addr::from(*self))
            .finish()
    }
}

#[repr(C)]
#[derive(Debug)]
pub enum Proto {
    Tcp = libc::IPPROTO_TCP as isize,
    Udp = libc::IPPROTO_UDP as isize,
}

#[test]
fn ipv4or6_convert_stdipaddr() {
    let v4: net::Ipv4Addr = "127.0.0.1".parse().unwrap();
    let ipv4or6: Ipv4or6 = v4.into();
    assert_eq!(net::Ipv4Addr::from(ipv4or6), v4);

    let v6: net::Ipv6Addr = "2001:0db8:0000:0000:0000:ff00:0042:8329".parse().unwrap();
    let ipv4or6: Ipv4or6 = v6.into();
    assert_eq!(net::Ipv6Addr::from(ipv4or6), v6);
}

#[test]
fn port_convert_u16() {
    let port = Port::from(1234);
    assert_eq!(u16::from(port), 1234);
}
