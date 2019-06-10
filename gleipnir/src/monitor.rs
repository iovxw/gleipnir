use std::fs;
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};

use futures::{
    compat::Executor01CompatExt,
    future::{self, Ready},
    prelude::*,
    FutureExt,
};
use gleipnir_interface::{monitor, unixtransport, PackageReport, Rules};
use rpc::context;
use rpc::server::Server;

pub static MONITOR_RUNNING: AtomicBool = AtomicBool::new(false);

#[derive(Clone)]
struct Monitor<F0, F1>
where
    F0: Fn(Vec<PackageReport>) + Send + Sync + Clone + 'static,
    F1: Fn(Rules) + Send + Sync + Clone + 'static,
{
    on_packages: F0,
    on_rules_updated: F1,
}

impl<F0, F1> monitor::Service for Monitor<F0, F1>
where
    F0: Fn(Vec<PackageReport>) + Send + Sync + Clone + 'static,
    F1: Fn(Rules) + Send + Sync + Clone + 'static,
{
    type OnPackagesFut = Ready<()>;
    type OnRulesUpdatedFut = Ready<()>;
    fn on_packages(self, _: context::Context, logs: Vec<PackageReport>) -> Self::OnPackagesFut {
        (self.on_packages)(logs);
        future::ready(())
    }
    fn on_rules_updated(self, _: context::Context, rules: Rules) -> Self::OnRulesUpdatedFut {
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

    let transport = unixtransport::listen(&addr)?;
    MONITOR_RUNNING.store(true, Ordering::Release);

    let server = Server::default()
        .incoming(transport)
        .and_then(move |channel| {
            let on_packages = on_packages.clone();
            let on_rules_updated = on_rules_updated.clone();
            async move {
                channel
                    .respond_with(monitor::serve(Monitor {
                        on_packages,
                        on_rules_updated,
                    }))
                    .await;
                Ok(())
            }
                .boxed()
        })
        .map_err(|e| {
            dbg!(e);
        })
        .for_each(|_| futures::future::ready(()))
        .map(|()| MONITOR_RUNNING.store(false, Ordering::Release));

    rpc::init(tokio::executor::DefaultExecutor::current().compat());
    tokio::run(server.unit_error().boxed().compat());

    Ok(())
}
