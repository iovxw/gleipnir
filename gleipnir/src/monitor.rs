use std::fs;
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};

use defer::defer;
use futures::{
    compat::Executor01CompatExt,
    future::{self, Ready},
    prelude::*,
};
use gleipnir_interface::{unixtransport, Monitor, PackageReport, Rules};
use tarpc::rpc::context::Context;
use tarpc::server::Channel;
use tokio_serde::formats::Bincode;

pub static MONITOR_RUNNING: AtomicBool = AtomicBool::new(false);

#[derive(Clone)]
struct MyMonitor<F0, F1>
where
    F0: Fn(Vec<PackageReport>) + Send + Sync + Clone + 'static,
    F1: Fn(Rules) + Send + Sync + Clone + 'static,
{
    on_packages: F0,
    on_rules_updated: F1,
}

impl<F0, F1> Monitor for MyMonitor<F0, F1>
where
    F0: Fn(Vec<PackageReport>) + Send + Sync + Clone + 'static,
    F1: Fn(Rules) + Send + Sync + Clone + 'static,
{
    type OnPackagesFut = Ready<()>;
    type OnRulesUpdatedFut = Ready<()>;
    fn on_packages(self, _: Context, logs: Vec<PackageReport>) -> Self::OnPackagesFut {
        (self.on_packages)(logs);
        future::ready(())
    }
    fn on_rules_updated(self, _: Context, rules: Rules) -> Self::OnRulesUpdatedFut {
        (self.on_rules_updated)(rules);
        future::ready(())
    }
}

pub fn run<F0, F1>(on_packages: F0, on_rules_updated: F1) -> Result<(), std::io::Error>
where
    F0: Fn(Vec<PackageReport>) + Send + Sync + Clone + 'static,
    F1: Fn(Rules) + Send + Sync + Clone + 'static,
{
    let addr = std::path::PathBuf::from("/tmp/gleipnir");
    if addr.exists() {
        if UnixStream::connect(&addr).is_ok() {
            return Err(std::io::ErrorKind::AddrInUse.into());
        } else {
            fs::remove_file(&addr)?;
        }
    }

    let server = async {
        let incoming = unixtransport::listen(&addr, Bincode::default).await?;
        MONITOR_RUNNING.store(true, Ordering::Release);
        incoming
            .filter_map(|r| future::ready(r.ok()))
            .map(|(_peer_pid, transport)| tarpc::server::BaseChannel::with_defaults(transport))
            .for_each(move |channel| {
                let server = MyMonitor {
                    on_packages: on_packages.clone(),
                    on_rules_updated: on_rules_updated.clone(),
                };
                channel.respond_with(server.serve()).execute()
            })
            .await;
        Ok(())
    };

    tokio::runtime::Runtime::new().unwrap().block_on(server)
}
