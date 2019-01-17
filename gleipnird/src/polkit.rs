use std::collections::HashMap;
use std::fs;

use dbus::arg::{RefArg, Variant};
use dbus::{BusType, Connection};

include!(concat!(env!("OUT_DIR"), "/dbus_interfaces.rs"));

pub fn check_authorization(pid: u32) -> bool {
    let stat = fs::read_to_string(format!("/proc/{}/stat", pid)).expect("invalid pid");
    let start_time: u64 = stat.split(' ').skip(21).next().unwrap().parse().unwrap();

    let conn = Connection::get_private(BusType::System).expect("connect to dbus");

    let authority = conn.with_path(
        "org.freedesktop.PolicyKit1",
        "/org/freedesktop/PolicyKit1/Authority",
        100000,
    );
    let mut subject: HashMap<&str, Variant<Box<dyn RefArg>>> = HashMap::new();
    subject.insert("pid", Variant(Box::new(pid)));
    subject.insert("start-time", Variant(Box::new(start_time)));
    let details = HashMap::new();
    let (is_authorized, is_challenge, details) = authority
        .check_authorization(
            ("unix-process", subject),
            "org.freedesktop.policykit.exec",
            details,
            1,
            "",
        )
        .unwrap();
    dbg!(is_authorized, is_challenge, details);
    is_authorized
}
