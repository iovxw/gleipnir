#![feature(const_fn)]
#![feature(type_alias_impl_trait)]
#![feature(try_blocks)]

#[global_allocator]
static ALLOC: jemallocator::Jemalloc = jemallocator::Jemalloc;

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::thread;

use crossbeam_channel;
use gleipnir_interface::{Device, PackageReport, Proto};
use lru_time_cache::LruCache;
use nfq;
use nix::unistd::Uid;
use pnet::packet::{
    ip::IpNextHeaderProtocols, ipv4::Ipv4Packet, ipv6::Ipv6Packet, tcp::TcpPacket, udp::UdpPacket,
};

#[macro_use]
mod utils;
mod ablock;
mod config;
mod netfilter;
mod netlink;
mod polkit;
mod proc;
pub mod rpc_server;
mod rules;

use rules::IndexedRules;

const QUEUE_ID: u16 = 786;

struct State {
    diag: netlink::SockDiag,
    rules: ablock::AbReader<IndexedRules>,
    pkt_logs: crossbeam_channel::Sender<PackageReport>,
    cache: LruCache<u64, proc::Process>,
}

impl State {
    fn query_process_cached(
        &mut self,
        device: Device,
        protocol: Proto,
        src: SocketAddr,
        dst: SocketAddr,
    ) -> Result<proc::Process, io::Error> {
        let mut hasher = DefaultHasher::new();
        (device, protocol, src, dst).hash(&mut hasher);
        let lru_index = hasher.finish();

        self.cache
            .get(&lru_index)
            .cloned()
            .map(Ok)
            .unwrap_or_else(|| {
                let result = self.query_process(device, protocol, src, dst)?;
                self.cache.insert(lru_index, result.clone());
                Ok(result)
            })
    }
    fn query_process(
        &mut self,
        device: Device,
        protocol: Proto,
        src: SocketAddr,
        dst: SocketAddr,
    ) -> Result<proc::Process, io::Error> {
        let mut possible_sockets: [Option<(_, _)>; 3] = [None; 3];

        match protocol {
            Proto::Tcp => {
                if device.is_input() {
                    // for INPUT, dst is loacal address, src is remote address
                    possible_sockets[0] = Some((dst, src));
                } else {
                    possible_sockets[0] = Some((src, dst));
                }
            }
            Proto::Udp | Proto::UdpLite => {
                // for UDP listener, the remote address is unspecified
                let unspecified_addr = if src.is_ipv4() {
                    Ipv4Addr::UNSPECIFIED.into()
                } else {
                    Ipv6Addr::UNSPECIFIED.into()
                };
                let unspecified_socket = SocketAddr::new(unspecified_addr, 0);
                if device.is_input() {
                    possible_sockets[0] = Some((dst, src));
                    possible_sockets[1] = Some((dst, unspecified_socket));
                    possible_sockets[2] = Some((
                        SocketAddr::new(unspecified_addr, dst.port()),
                        unspecified_socket,
                    ));
                } else {
                    possible_sockets[0] = Some((src, dst));
                    possible_sockets[1] = Some((src, unspecified_socket));
                    possible_sockets[2] = Some((
                        SocketAddr::new(unspecified_addr, src.port()),
                        unspecified_socket,
                    ));
                };
            }
        }

        let mut diag_msg = None;
        for &(local_address, remote_address) in possible_sockets
            .iter()
            .take_while(|x| Option::is_some(x))
            .map(|x| x.as_ref().unwrap())
        {
            match self.diag.query(protocol, local_address, remote_address) {
                Ok(r) => diag_msg = Some(r),
                Err(ref e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e),
            };
            break;
        }

        let diag_msg = match diag_msg {
            Some(r) => r,
            None => return Err(io::ErrorKind::NotFound.into()),
        };

        proc::get_proc_by_inode(diag_msg.idiag_inode).ok_or_else(|| io::ErrorKind::NotFound.into())
    }
}

fn queue_callback(msg: &mut nfq::Message, state: &mut State) {
    let device = if msg.get_indev() != 0 {
        Device::Input
    } else if msg.get_outdev() != 0 {
        Device::Output
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

    let (protocol, sport, dport) = match protocol {
        IpNextHeaderProtocols::Tcp => {
            let pkt = TcpPacket::new(ip_payload).expect("TcpPacket");
            let (sport, dport) = (pkt.get_source(), pkt.get_destination());
            (Proto::Tcp, sport, dport)
        }
        IpNextHeaderProtocols::Udp | IpNextHeaderProtocols::UdpLite => {
            let pkt = UdpPacket::new(ip_payload).expect("UdpPacket");
            let (sport, dport) = (pkt.get_source(), pkt.get_destination());
            let p = if protocol == IpNextHeaderProtocols::Udp {
                Proto::Udp
            } else {
                Proto::UdpLite
            };
            (p, sport, dport)
        }
        _ => {
            // ignore other protocol
            msg.set_verdict(nfq::Verdict::Accept);
            return;
        }
    };
    let (src, dst) = (SocketAddr::new(saddr, sport), SocketAddr::new(daddr, dport));

    let proc = match state.query_process_cached(device, protocol, src, dst) {
        Ok(r) => r,
        Err(ref e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("NOT FOUND: {:?},\t{},\t{},\t{}", device, protocol, src, dst);
            msg.set_verdict(nfq::Verdict::Accept);
            return;
        }
        Err(e) => {
            eprintln!(
                "ERROR: {},\t{:?},\t{},\t{},\t{}",
                e, device, protocol, src, dst
            );
            msg.set_verdict(nfq::Verdict::Accept);
            return;
        }
    };

    let rule_addr = if device.is_input() { src } else { dst };
    let rules = state.rules.read();
    let (rule_id, accept) =
        rules.is_acceptable(device, protocol, rule_addr, payload.len(), &proc.exe);

    let log = PackageReport {
        device,
        protocol,
        addr: rule_addr,
        len: payload.len(),
        exe: proc.exe,
        dropped: !accept,
        matched_rule: rule_id,
    };

    if accept {
        msg.set_verdict(nfq::Verdict::Accept);
    } else {
        msg.set_verdict(nfq::Verdict::Drop);
    }

    state.pkt_logs.try_send(log).expect("logs service dead");
}

// TODO: expect messages
fn main() {
    let rules = config::load_rules().expect("Failed to load rules");

    let (rules_reader, rules_setter) = ablock::AbLock::new(IndexedRules::from(rules.clone()));
    let (sender, receiver) = crossbeam_channel::unbounded();
    let mut state = State {
        diag: netlink::SockDiag::new().expect(""),
        rules: rules_reader,
        pkt_logs: sender,
        cache: LruCache::with_capacity(2048),
    };
    let mut q = nfq::Queue::open().expect("");

    thread::spawn(|| {
        if let Err(e) = rpc_server::run(rules, rules_setter, receiver) {
            dbg!(e);
            std::process::exit(1);
        }
    });

    q.bind(QUEUE_ID).expect("");
    // The max size of IPv4 + TCP is (20 + 40 optional) + (20 + 40 optional) = 120
    q.set_copy_range(QUEUE_ID, 128).expect("");

    if Uid::current().is_root() {
        netfilter::register_nfqueue(QUEUE_ID);
    }

    loop {
        let mut msg = q.recv().expect("");
        queue_callback(&mut msg, &mut state);
        q.verdict(msg).expect("");
    }
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
