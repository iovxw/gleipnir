#![feature(const_fn)]
#![feature(async_await)]
#![feature(existential_type)]
#![feature(proc_macro_hygiene)]

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::thread;

use libc;
use nfqueue;
use pnet::packet::{
    ip::IpNextHeaderProtocols, ipv4::Ipv4Packet, ipv6::Ipv6Packet, tcp::TcpPacket, udp::UdpPacket,
};
use serde::{Serialize, Deserialize};

#[macro_use]
mod utils;
mod ablock;
mod netlink;
mod polkit;
mod proc;
pub mod rpc_server;
mod rules;
mod unixtransport;

const QUEUE_ID: u16 = 786;
const MAX_IP_PKG_LEN: u32 = 0xFFFF;

struct State {
    diag: netlink::SockDiag,
    rules: ablock::AbReader<rules::Rules>,
}

#[derive(Debug, Eq, PartialEq, Hash, Copy, Clone, Ord, PartialOrd, Serialize, Deserialize)]
pub enum Device {
    Input,
    Ouput,
}

impl Device {
    fn is_input(&self) -> bool {
        match self {
            Device::Input => true,
            Device::Ouput => false,
        }
    }
}

fn queue_callback(msg: nfqueue::Message, state: &mut State) {
    let device = if msg.get_indev() != 0 {
        Device::Input
    } else if msg.get_outdev() != 0 {
        Device::Ouput
    } else {
        unreachable!("package is from neither INPUT nor OUTPUT");
    };

    let payload = msg.get_payload();
    let (saddr, daddr, protocol, ip_payload) = match payload[0] >> 4 {
        4 => {
            let pkt = Ipv4Packet::new(payload).expect("Ipv4Packet");
            let src: IpAddr = pkt.get_source().into();
            let dst: IpAddr = pkt.get_destination().into();
            (
                src,
                dst,
                pkt.get_next_level_protocol(),
                &payload[Ipv4Packet::minimum_packet_size()..],
            )
        }
        6 => {
            let pkt = Ipv6Packet::new(payload).expect("Ipv6Packet");
            let src: IpAddr = pkt.get_source().into();
            let dst: IpAddr = pkt.get_destination().into();
            (
                src,
                dst,
                pkt.get_next_header(),
                &payload[Ipv6Packet::minimum_packet_size()..],
            )
        }
        _ => unreachable!("package is neither IPv4 nor IPv6"),
    };
    let mut possible_sockets: [Option<(_, _)>; 3] = [None; 3];
    let (protocol, src, dst) = match protocol {
        IpNextHeaderProtocols::Tcp => {
            let pkt = TcpPacket::new(ip_payload).expect("TcpPacket");
            let (sport, dport) = (pkt.get_source(), pkt.get_destination());
            let (src, dst) = (SocketAddr::new(saddr, sport), SocketAddr::new(daddr, dport));
            if device.is_input() {
                // for INPUT, dst is loacal address, src is remote address
                possible_sockets[0] = Some((dst, src));
            } else {
                possible_sockets[0] = Some((src, dst));
            }
            (netlink::Proto::Tcp, src, dst)
        }
        IpNextHeaderProtocols::Udp | IpNextHeaderProtocols::UdpLite => {
            let pkt = UdpPacket::new(ip_payload).expect("UdpPacket");
            let (sport, dport) = (pkt.get_source(), pkt.get_destination());
            let (src, dst) = (SocketAddr::new(saddr, sport), SocketAddr::new(daddr, dport));

            // for UDP listener, the remote address is unspecified
            let unspecified_addr = if saddr.is_ipv4() {
                Ipv4Addr::UNSPECIFIED.into()
            } else {
                Ipv6Addr::UNSPECIFIED.into()
            };
            let unspecified_socket = SocketAddr::new(unspecified_addr, 0);
            if device.is_input() {
                possible_sockets[0] = Some((dst, src));
                possible_sockets[1] = Some((dst, unspecified_socket));
                possible_sockets[2] =
                    Some((SocketAddr::new(unspecified_addr, dport), unspecified_socket));
            } else {
                possible_sockets[0] = Some((src, dst));
                possible_sockets[1] = Some((src, unspecified_socket));
                possible_sockets[2] =
                    Some((SocketAddr::new(unspecified_addr, sport), unspecified_socket));
            };
            let p = if protocol == IpNextHeaderProtocols::Udp {
                netlink::Proto::Udp
            } else {
                netlink::Proto::UdpLite
            };
            (p, src, dst)
        }
        _ => {
            // ignore other protocol
            msg.set_verdict(nfqueue::Verdict::Accept);
            return;
        }
    };

    let mut diag_msg = None;
    for &(local_address, remote_address) in possible_sockets
        .iter()
        .take_while(|x| Option::is_some(x))
        .map(|x| x.as_ref().unwrap())
    {
        match state.diag.query(protocol, local_address, remote_address) {
            Ok(r) => diag_msg = Some(r),
            Err(ref e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => {
                eprintln!(
                    "ERROR: {}, DEV: {:?}, PROTOCOL: {:?}, LOACALADDR: {}, REMOTEADDR: {}",
                    e, device, protocol, local_address, remote_address,
                );
                msg.set_verdict(nfqueue::Verdict::Accept);
                return;
            }
        };
        break;
    }

    let diag_msg = match diag_msg {
        Some(r) => r,
        None => {
            eprintln!(
                "ERROR: not found, DEV: {:?}, PROTOCOL: {:?}, SRC: {}, DST: {}",
                device, protocol, src, dst
            );
            msg.set_verdict(nfqueue::Verdict::Accept);
            return;
        }
    };

    let proc = match proc::get_proc_by_inode(diag_msg.idiag_inode) {
        Some(r) => r,
        None => {
            eprintln!(
                "ERROR: failed to find process by inode {}, DEV: {:?}, PROTOCOL: {:?}, SRC: {}, DST: {}",
                diag_msg.idiag_inode, device, protocol, src, dst
            );
            msg.set_verdict(nfqueue::Verdict::Accept);
            return;
        }
    };

    println!(
        "DEV: {:?}, PROTOCOL: {:?}, SRC: {}, DST: {}, LEN: {}, EXE: {}",
        device,
        protocol,
        src,
        dst,
        payload.len(),
        proc.exe,
    );

    let rules = state.rules.read();
    let (rule_id, accept) = rules.is_acceptable(
        device,
        protocol,
        if device.is_input() { src } else { dst },
        payload.len(),
        &proc.exe,
    );
    dbg!(rule_id, accept);
    // TODO: write result to logs
    if accept {
        msg.set_verdict(nfqueue::Verdict::Accept);
    } else {
        msg.set_verdict(nfqueue::Verdict::Drop);
    }
}

fn main() {
    let (rules, rules_setter) = ablock::AbLock::new(unsafe { std::mem::zeroed() });
    let state = State {
        diag: netlink::SockDiag::new().expect(""),
        rules,
    };
    let mut q = nfqueue::Queue::new(state);

    thread::spawn(|| {
        // TODO: start a dbus server
        rpc_server::run();
        std::mem::drop(rules_setter);
    });

    q.open();
    q.unbind(libc::AF_INET); // ignore result, failure is not critical here
    q.unbind(libc::AF_INET6);
    assert_eq!(q.bind(libc::AF_INET), 0);
    assert_eq!(q.bind(libc::AF_INET6), 0);

    q.create_queue(QUEUE_ID, queue_callback);
    q.set_mode(nfqueue::CopyMode::CopyPacket, MAX_IP_PKG_LEN);

    q.run_loop();

    q.close();
}

#[allow(unused)]
/// debug function
fn dump_net(proto: &str) {
    let v = std::fs::read_to_string(format!("/proc/net/{}", proto)).unwrap();
    for line in v.lines().skip(1) {
        let mut iter = line.split_whitespace().skip(1);
        let (local_socket, remote_socket) = (iter.next().expect("1"), iter.next().expect("2"));
        let (local_addr, local_port) = local_socket.split_at(local_socket.rfind(':').expect("3"));
        let (local_addr, local_port) = (
            u32::from_be(u32::from_str_radix(local_addr, 16).expect("4")),
            u16::from_str_radix(&local_port[1..], 16).expect("5"),
        );
        let (remote_addr, remote_port) =
            remote_socket.split_at(remote_socket.rfind(':').expect("6"));
        let (remote_addr, remote_port) = (
            u32::from_be(u32::from_str_radix(remote_addr, 16).expect("7")),
            u16::from_str_radix(&remote_port[1..], 16).expect("8"),
        );
        let local_socket =
            std::net::SocketAddr::new(std::net::Ipv4Addr::from(local_addr).into(), local_port);
        let remote_socket =
            std::net::SocketAddr::new(std::net::Ipv4Addr::from(remote_addr).into(), remote_port);
        println!("LOCAL: {}, REMOTE: {}", local_socket, remote_socket);
    }
}
