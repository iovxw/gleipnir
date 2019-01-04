use std::net::SocketAddr;

use crate::netlink::Proto;
use crate::rules::{Rule, RuleTarget};
use crate::Device;

trait GleiphierDaemon {
    fn set_rules(
        default_target: crate::rules::RuleTarget,
        rules: Vec<crate::rules::Rule>,
        qos_rules: Vec<usize>,
    ) {
    }
    fn register_monitor(dbus_path: String) {}
}

trait GleiphierMonitor {
    fn on_message(logs: Vec<MonitorMsg>) {}
}

enum MonitorMsg {
    ProcTraffic {
        exe: String,
        receiving: usize,
        sending: usize,
    },
    FirewallReport {
        device: Device,
        protocol: Proto,
        addr: SocketAddr,
        len: usize,
        exe: String,
        dropped: bool,
        matched_rule: Option<usize>,
    },
}
