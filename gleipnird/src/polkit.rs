use std::collections::HashMap;
use std::fs;
use std::time::Duration;

use dbus::arg::{RefArg, Variant};
use dbus::blocking::Connection;

include!(concat!(env!("OUT_DIR"), "/dbus_interfaces.rs"));

pub fn check_authorization(pid: u32) -> bool {
    let stat = fs::read_to_string(format!("/proc/{}/stat", pid)).expect("invalid pid");
    let start_time: u64 = stat.split(' ').skip(21).next().unwrap().parse().unwrap();

    let conn = Connection::new_system().expect("connect to dbus");

    let authority = conn.with_proxy(
        "org.freedesktop.PolicyKit1",
        "/org/freedesktop/PolicyKit1/Authority",
        Duration::from_secs(1),
    );

    let mut subject: HashMap<String, Variant<Box<dyn RefArg>>> = HashMap::new();
    subject.insert("pid".into(), Variant(Box::new(pid)));
    subject.insert("start-time".into(), Variant(Box::new(start_time)));
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
