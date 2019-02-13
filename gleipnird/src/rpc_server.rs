use std::collections::HashMap;
use std::fs;
use std::net::SocketAddr;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::io::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use futures::{
    compat::{Compat, Executor01CompatExt},
    future::{self, poll_fn, Ready},
    prelude::*,
    FutureExt,
};
use gleipnir_interface::{daemon, unixtransport, Device, Proto, Rule, RuleTarget};
use nix::sys::socket::{getsockopt, sockopt::PeerCredentials};
use rpc::context;
use rpc::server::{self, Handler, Server};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
struct Daemon {
    pid: u32,
    authenticated: Arc<AtomicBool>,
}

impl daemon::Service for Daemon {
    type SetRulesFut = Ready<()>;
    existential type RegisterFut: Future<Output = bool>;
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
        use futures::task::Poll;
        use tokio::prelude::Async;
        async move {
            let authenticated = poll_fn(|_| {
                if let Async::Ready(r) =
                    tokio_threadpool::blocking(|| crate::polkit::check_authorization(self.pid))
                        .unwrap()
                {
                    Poll::Ready(r)
                } else {
                    Poll::Pending
                }
            })
            .await;
            self.authenticated.store(authenticated, Ordering::Relaxed);
            authenticated
        }
    }
    fn unregister(self, _: context::Context) -> Self::UnregisterFut {
        self.authenticated.store(false, Ordering::Relaxed);
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

    let permissions = fs::Permissions::from_mode(755);
    fs::set_permissions(&addr, permissions)?;

    let server = Server::default()
        .incoming(transport)
        .map_ok(|channel| {
            // This is a hack, see unixtransport module
            let pid: u32 = if let SocketAddr::V4(addr) = channel.client_addr() {
                (*addr.ip()).into()
            } else {
                unreachable!()
            };
            dbg!(pid);

            tokio::executor::spawn(Compat::new(
                async move {
                    channel
                        .respond_with(daemon::serve(Daemon {
                            pid,
                            authenticated: Arc::new(AtomicBool::new(false)),
                        }))
                        .await;
                    Ok(())
                }
                    .boxed(),
            ));
        })
        .for_each(|_| futures::future::ready(()));

    rpc::init(tokio::executor::DefaultExecutor::current().compat());
    tokio::run(server.unit_error().boxed().compat());

    Ok(())
}
