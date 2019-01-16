use std::collections::HashMap;
use std::fs;
use std::net::SocketAddr;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::io::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};

use futures::{
    future::{self, Ready},
    prelude::*,
    Future,
};
use nix::sys::socket::{getsockopt, sockopt::PeerCredentials};
use rpc::context;
use rpc::server::{self, Handler, Server};

use crate::netlink::Proto;
use crate::rules::{Rule, RuleTarget};
use crate::Device;

mod daemon {
    use crate::rules::{Rule, RuleTarget};
    tarpc::service! {
        rpc set_rules(default_target: RuleTarget, rules: Vec<Rule>, qos_rules: Vec<usize>);
        rpc register();
        rpc unregister();
    }
}

mod monitor {
    use super::*;
    tarpc::service! {
        rpc on_packages(logs: Vec<PackageReport>);
        rpc on_traffic(logs: Vec<ProcTraffic>);
    }
}

#[derive(Debug)]
struct ProcTraffic {
    exe: String,
    receiving: usize,
    sending: usize,
}

#[derive(Debug)]
struct PackageReport {
    device: Device,
    protocol: Proto,
    addr: SocketAddr,
    len: usize,
    exe: String,
    dropped: bool,
    matched_rule: Option<usize>,
}

#[derive(Clone)]
struct Daemon;

impl daemon::Service for Daemon {
    type SetRulesFut = Ready<()>;
    type RegisterFut = Ready<()>;
    type UnregisterFut = Ready<()>;

    fn set_rules(
        self,
        _: context::Context,
        default_target: RuleTarget,
        rules: Vec<Rule>,
        qos_rules: Vec<usize>,
    ) -> Self::SetRulesFut {
        future::ready(())
    }
    fn register(self, _: context::Context) -> Self::RegisterFut {
        future::ready(())
    }
    fn unregister(self, _: context::Context) -> Self::UnregisterFut {
        future::ready(())
    }
}

pub fn run() -> Result<(), std::io::Error> {
    let addr = std::path::PathBuf::from("/tmp/gleipnir");
    if addr.exists() {
        if UnixStream::connect(&addr).is_ok() {
            return Err(std::io::ErrorKind::AddrInUse.into());
        } else {
            fs::remove_file(&addr)?;
        }
    }
    let listener = UnixListener::bind(&addr)?;
    let permissions = fs::Permissions::from_mode(755);
    fs::set_permissions(&addr, permissions)?;
    for stream in listener.incoming() {
        let stream = stream?;
        let pid = getsockopt(stream.as_raw_fd(), PeerCredentials)
            .unwrap()
            .pid();
        crate::polkit::check_authorization(pid as u32);
    }
    Ok(())
}
