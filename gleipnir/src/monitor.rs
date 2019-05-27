use std::fs;
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::implementation::Backend;
use futures::{
    compat::Executor01CompatExt,
    future::{self, Ready},
    prelude::*,
    FutureExt,
};
use gleipnir_interface::{monitor, unixtransport, PackageReport};
use qmetaobject::QPointer;
use rpc::context;
use rpc::server::Server;

pub static MONITOR_RUNNING: AtomicBool = AtomicBool::new(false);

#[derive(Clone)]
struct Monitor<F>
where
    F: Fn(Vec<PackageReport>) + Send + Sync + Clone + 'static,
{
    on_packages: F,
}

impl<F> monitor::Service for Monitor<F>
where
    F: Fn(Vec<PackageReport>) + Send + Sync + Clone + 'static,
{
    type OnPackagesFut = Ready<()>;
    fn on_packages(self, _: context::Context, logs: Vec<PackageReport>) -> Self::OnPackagesFut {
        (self.on_packages)(logs);
        future::ready(())
    }
}

pub fn run<F>(on_packages: F) -> Result<(), std::io::Error>
where
    F: Fn(Vec<PackageReport>) + Send + Sync + Clone + 'static,
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
            async move {
                channel
                    .respond_with(monitor::serve(Monitor { on_packages }))
                    .await;
                Ok(())
            }
                .boxed()
        })
        .map_err(|e| {
            dbg!(e);
        })
        .for_each(|_| futures::future::ready(()));

    rpc::init(tokio::executor::DefaultExecutor::current().compat());
    tokio::run(server.unit_error().boxed().compat());

    Ok(())
}
