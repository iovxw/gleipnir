use std::fs;
use std::io;
use std::net::SocketAddr;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

use crossbeam_channel;
use futures::{
    compat::{Compat, Executor01CompatExt, Future01CompatExt},
    executor::block_on,
    future::{self, poll_fn, Ready},
    prelude::*,
    FutureExt,
};
use futures_locks::Mutex;
use gleipnir_interface::{daemon, monitor, unixtransport, PackageReport, Rule, RuleTarget, Rules};
use rpc::context;
use rpc::server::Server;
use slab::Slab;

use crate::ablock::AbSetter;
use crate::config;
use crate::rules::IndexedRules;

#[derive(Clone)]
struct Daemon {
    pid: u32,
    authenticated: Arc<AtomicBool>,
    rules_setter: Arc<Mutex<AbSetter<IndexedRules>>>,
    rules: Arc<Mutex<Rules>>,
    clients: Arc<Mutex<Slab<monitor::Client>>>,
    client_id: Arc<Mutex<Option<usize>>>,
}

impl Drop for Daemon {
    fn drop(&mut self) {
        // TODO: a better way, don't let client_id cloned outside Daemon
        if Arc::strong_count(&self.client_id) > 1 {
            return;
        }
        let mut client_id = match block_on(self.client_id.lock().compat()) {
            Ok(r) => r,
            _ => return,
        };
        if let Some(client_id) = client_id.take() {
            dbg!("dropped");
            block_on(self.clients.lock().compat())
                .unwrap()
                .remove(client_id);
        }
    }
}

impl daemon::Service for Daemon {
    existential type SetRulesFut: Future<Output = ()>;
    existential type UnlockFut: Future<Output = bool>;
    existential type InitMonitorFut: Future<Output = ()>;

    fn set_rules(self, _: context::Context, rules: Rules) -> Self::SetRulesFut {
        async move {
            if self.authenticated.load(Ordering::Relaxed) {
                let indexed_rules = IndexedRules::from(rules.clone());
                self.rules_setter
                    .lock()
                    .compat()
                    .await
                    .unwrap()
                    .set(indexed_rules);
                config::save_rules(&rules);
                *self.rules.lock().compat().await.unwrap() = rules.clone();
                let boardcast = async move {
                    let self_id = self.client_id.lock().compat().await.unwrap();
                    if let Some(self_id) = &*self_id {
                        let mut clients = self.clients.lock().compat().await.unwrap();
                        for (id, client) in clients.iter_mut() {
                            if id == *self_id {
                                continue;
                            }
                            if let Err(e) = client
                                .on_rules_updated(tarpc::context::current(), rules.clone())
                                .await
                            {
                                // TODO: remove client from clients?
                                // dbg!(e);
                            }
                        }
                    }
                    Ok(())
                };
                tokio::spawn(Compat::new(boardcast.boxed()));
            }
        }
    }
    fn unlock(self, _: context::Context) -> Self::UnlockFut {
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
    fn init_monitor(self, _: context::Context, socket_path: String) -> Self::InitMonitorFut {
        async move {
            let r: Result<(), failure::Error> = try {
                let mut clients = self.clients.lock().compat().await.unwrap();
                let mut client_id = self.client_id.lock().compat().await.unwrap();
                if client_id.is_some() {
                    // TODO: return a error, can not initialize multiple times
                    return;
                }

                let transport = unixtransport::connect(&socket_path).await?;
                let mut client =
                    monitor::new_stub(tarpc::client::Config::default(), transport).await?;
                let rules = self.rules.lock().compat().await.unwrap().clone();
                client
                    .on_rules_updated(tarpc::context::current(), rules)
                    .await?;
                *client_id = Some(clients.insert(client));
            };
            if let Err(e) = r {
                dbg!(e);
            }
        }
    }
}

pub fn run(
    rules: Rules,
    rules_setter: AbSetter<IndexedRules>,
    pkt_logs: crossbeam_channel::Receiver<PackageReport>,
) -> Result<(), std::io::Error> {
    let addr = std::path::PathBuf::from("/tmp/gleipnird");
    if addr.exists() {
        if UnixStream::connect(&addr).is_ok() {
            return Err(std::io::ErrorKind::AddrInUse.into());
        } else {
            fs::remove_file(&addr)?;
        }
    }

    let rules_setter = Arc::new(Mutex::new(rules_setter));
    let rules = Arc::new(Mutex::new(rules));

    let transport = unixtransport::listen(&addr)?;

    let permissions = fs::Permissions::from_mode(755);
    fs::set_permissions(&addr, permissions)?;

    let clients = Arc::new(Mutex::new(Slab::new()));
    let clients2 = clients.clone();

    let server = Server::default()
        .incoming(transport)
        .map_ok(move |channel| {
            // This is a hack, see unixtransport module
            let pid: u32 = if let SocketAddr::V4(addr) = channel.client_addr() {
                (*addr.ip()).into()
            } else {
                unreachable!()
            };
            dbg!(pid);

            let clients = clients.clone();
            let rules_setter = rules_setter.clone();
            let rules = rules.clone();
            tokio::executor::spawn(Compat::new(
                async move {
                    channel
                        .respond_with(daemon::serve(Daemon {
                            pid,
                            authenticated: Arc::new(AtomicBool::new(false)),
                            rules_setter,
                            rules,
                            clients,
                            client_id: Arc::new(Mutex::new(None)),
                        }))
                        .await;
                    Ok(())
                }
                    .map_err(|e: io::Error| eprintln!("Connecting to client: {}", e))
                    .boxed(),
            ));
        })
        .map_err(|e: io::Error| eprintln!("RPC Server: {}", e))
        .for_each(|_| futures::future::ready(()));

    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    let executor = runtime.executor();

    thread::spawn(move || loop {
        let mut logs = Vec::new();
        logs.push(pkt_logs.recv().expect("pkg_logs disconnected"));
        logs.extend(pkt_logs.try_iter());
        let clients = clients2.clone();
        let fut = async move {
            for (_id, client) in clients.lock().compat().await.unwrap().iter_mut() {
                let r = client
                    .on_packages(tarpc::context::current(), logs.clone())
                    .await;
                if let Err(e) = r {
                    // dbg!(e);
                }
            }
            Ok(())
        };
        executor.spawn(Compat::new(fut.boxed()));
    });

    rpc::init(runtime.executor().compat());
    runtime
        .block_on_all(server.unit_error().boxed().compat())
        .expect("run tokio runtime");

    Ok(())
}
