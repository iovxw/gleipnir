#![feature(nll)]
#![feature(const_fn)]

use std::net::{IpAddr, SocketAddr};

use libc;
use nfqueue;
use pnet::packet::{
    ip::IpNextHeaderProtocols, ipv4::Ipv4Packet, ipv6::Ipv6Packet, tcp::TcpPacket, udp::UdpPacket,
};

mod netlink;
mod proc;

const QUEUE_ID: u16 = 786;
const MAX_IP_PKG_LEN: u32 = 0xFFFF;

struct State {
    count: u32,
}

impl State {
    pub fn new() -> State {
        State { count: 0 }
    }
}

fn queue_callback(msg: &nfqueue::Message, state: &mut State) {
    println!("Packet received [id: 0x{:x}]\n", msg.get_id());

    println!(" -> msg: {}", msg);

    if msg.get_indev() != 0 {
        println!("INPUT");
    } else if msg.get_outdev() != 0 {
        println!("OUTPUT");
    } else {
        unreachable!("package is from neither INPUT nor OUTPUT");
    }

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
                &payload[Ipv6Packet::minimum_packet_size()..],
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
            (netlink::Proto::Tcp, pkt.get_source(), pkt.get_destination())
        }
        IpNextHeaderProtocols::Udp => {
            let pkt = UdpPacket::new(ip_payload).expect("UdpPacket");
            (netlink::Proto::Udp, pkt.get_source(), pkt.get_destination())
        }
        _ => {
            // ignore other protocol
            msg.set_verdict(nfqueue::Verdict::Accept);
            return;
        }
    };
    let src = SocketAddr::new(saddr, sport);
    let dst = SocketAddr::new(daddr, dport);
    println!(
        "SRC: {:?}, DST: {:?}, PROTOCOL: {:?}, LEN: {}",
        src,
        dst,
        protocol,
        payload.len()
    );
    state.count += 1;
    println!("count: {}", state.count);

    msg.set_verdict(nfqueue::Verdict::Accept);
}

// 两个 queue 接受包，一个 NEW，一个 RELATED,ESTABLISHED
// 分别管防火墙和限速
// diag 获取对应的 inode uid
// 监听所有 /proc/<PID>/fd/<N> -> socket[507218], 获得 pid 和 inode 的对应，需要注意会有多个 pid

fn main() {
    let mut q = nfqueue::Queue::new(State::new());

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
