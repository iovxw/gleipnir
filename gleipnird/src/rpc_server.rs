use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

use crossbeam_channel;
use futures::{compat::Future01CompatExt, executor::block_on, prelude::*};
use futures_locks::Mutex;
use gleipnir_interface::{self, unixtransport, Daemon, PackageReport, Rules};
use slab::Slab;
use tarpc::rpc::context::Context;
use tarpc::server::Channel;
use tokio::task::block_in_place;
use tokio_serde::formats::Bincode;

use crate::config;
use crate::lrlock::Setter;
use crate::rules::IndexedRules;

#[derive(Clone)]
struct MyDaemon {
    peer_pid: u32,
    authenticated: Arc<AtomicBool>,
    rules_setter: Arc<Mutex<Setter<IndexedRules>>>,
    rules: Arc<Mutex<Rules>>,
    clients: Arc<Mutex<Slab<gleipnir_interface::MonitorClient>>>,
    client_id: Arc<Mutex<Option<usize>>>,
}

impl Drop for MyDaemon {
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
            block_on(self.clients.lock().compat())
                .unwrap()
                .remove(client_id);
        }
    }
}

impl gleipnir_interface::Daemon for MyDaemon {
    type SetRulesFut = impl Future<Output = ()>;
    type UnlockFut = impl Future<Output = bool>;
    type InitMonitorFut = impl Future<Output = ()>;

    fn set_rules(self, _: Context, rules: Rules) -> Self::SetRulesFut {
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
                                dbg!(e);
                            }
                        }
                    }
                };
                tokio::spawn(boardcast);
            }
        }
    }
    fn unlock(self, _: Context) -> Self::UnlockFut {
        async move {
            let authenticated =
                block_in_place(|| crate::polkit::check_authorization(self.peer_pid));
            self.authenticated.store(authenticated, Ordering::Relaxed);
            authenticated
        }
    }
    fn init_monitor(self, _: Context, socket_path: String) -> Self::InitMonitorFut {
        async move {
            let r: Result<(), failure::Error> = try {
                let mut clients = self.clients.lock().compat().await.unwrap();
                let mut client_id = self.client_id.lock().compat().await.unwrap();
                if client_id.is_some() {
                    // TODO: return a error, can not initialize multiple times
                    return;
                }

                let (_, transport) =
                    unixtransport::connect(&socket_path, Bincode::default()).await?;
                let mut client = gleipnir_interface::MonitorClient::new(
                    tarpc::client::Config::default(),
                    transport,
                )
                .spawn()?;
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
    rules_setter: Setter<IndexedRules>,
    pkt_logs: crossbeam_channel::Receiver<PackageReport>,
) -> Result<(), std::io::Error> {
    let addr = std::path::PathBuf::from("/var/run/gleipnird");
    if addr.exists() {
        if UnixStream::connect(&addr).is_ok() {
            return Err(std::io::ErrorKind::AddrInUse.into());
        } else {
            fs::remove_file(&addr)?;
        }
    }

    let rules_setter = Arc::new(Mutex::new(rules_setter));
    let rules = Arc::new(Mutex::new(rules));

    let clients: Arc<Mutex<Slab<gleipnir_interface::MonitorClient>>> =
        Arc::new(Mutex::new(Slab::new()));
    let clients2 = clients.clone();

    let mut runtime = tokio::runtime::Runtime::new().expect("tokio runtime");

    let server = async move {
        let incoming = unixtransport::listen(&addr, Bincode::default).await?;
        fs::set_permissions(&addr, fs::Permissions::from_mode(0o755))?;

        incoming
            .filter_map(|r| future::ready(r.ok()))
            .map(|(peer_pid, transport)| {
                (
                    peer_pid,
                    tarpc::server::BaseChannel::with_defaults(transport),
                )
            })
            .for_each(move |(peer_pid, channel)| {
                let server = MyDaemon {
                    peer_pid,
                    authenticated: Arc::new(AtomicBool::new(false)),
                    rules_setter: rules_setter.clone(),
                    rules: rules.clone(),
                    clients: clients.clone(),
                    client_id: Arc::new(Mutex::new(None)),
                };
                channel.respond_with(server.serve()).execute()
            })
            .await;
        Ok(())
    };

    let handle = runtime.handle().clone();

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
                    dbg!(e);
                }
            }
        };
        handle.spawn(fut);
    });

    runtime.block_on(server)
}
