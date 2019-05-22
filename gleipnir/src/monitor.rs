use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};

use futures::{
    compat::{Compat, Executor01CompatExt},
    future::{self, poll_fn, Ready},
    prelude::*,
    FutureExt,
};
use gleipnir_interface::{
    daemon, monitor, unixtransport, Device, PackageReport, ProcTraffic, Proto, Rule, RuleTarget,
};
use rpc::context;
use rpc::server::{self, Handler, Server};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
struct Monitor {}

impl monitor::Service for Monitor {
    type OnPackagesFut = Ready<()>;
    type OnTrafficFut = Ready<()>;
    fn on_packages(self, _: context::Context, logs: Vec<PackageReport>) -> Self::OnPackagesFut {
        future::ready(())
    }
    fn on_traffic(self, _: context::Context, logs: Vec<ProcTraffic>) -> Self::OnTrafficFut {
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

    let transport = unixtransport::listen(&addr)?;

    let server = Server::default()
        .incoming(transport)
        .map_ok(move |channel| {
            async move {
                channel.respond_with(monitor::serve(Monitor {})).await;
            }
                .boxed()
        })
        .for_each(|_| futures::future::ready(()));

    rpc::init(tokio::executor::DefaultExecutor::current().compat());
    tokio::run(server.unit_error().boxed().compat());

    Ok(())
}
