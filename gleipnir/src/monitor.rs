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
    daemon, monitor, unixtransport, Device, PackageReport, Proto, Rule, RuleTarget,
};
use rpc::context;
use rpc::server::{self, Handler, Server};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
struct Monitor {}

impl monitor::Service for Monitor {
    type OnPackagesFut = Ready<()>;
    fn on_packages(self, _: context::Context, logs: Vec<PackageReport>) -> Self::OnPackagesFut {
        dbg!(logs);
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
            tokio::executor::spawn(Compat::new(
                async move {
                    dbg!("ahhj");
                    channel.respond_with(monitor::serve(Monitor {})).await;
                    Ok(())
                }
                    .boxed(),
            ))
        })
        .for_each(|_| futures::future::ready(()));

    rpc::init(tokio::executor::DefaultExecutor::current().compat());
    tokio::run(server.unit_error().boxed().compat());

    Ok(())
}
