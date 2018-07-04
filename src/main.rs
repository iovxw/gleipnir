#![feature(rust_2018_preview)]
#![feature(rust_2018_idioms)]
#![feature(nll)]
#![feature(const_fn)]

use libc;
use nfqueue;

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

    println!(
        "XML\n{}",
        msg.as_xml_str(&[nfqueue::XMLFormatFlags::XmlAll]).unwrap()
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
