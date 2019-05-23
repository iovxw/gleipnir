use std::cell::RefCell;
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
use gleipnir_interface::{daemon, monitor, unixtransport, PackageReport, Rule, RuleTarget};
use rpc::context;
use rpc::server::Server;
use slab::Slab;

use crate::ablock::AbSetter;
use crate::rules::Rules;

thread_local! {
    static RULES_SETTER: RefCell<Option<AbSetter<Rules>>> = RefCell::new(None);
}
#[derive(Clone, Copy)]
struct LocalRulesSetter {
    _private: (),
}

impl LocalRulesSetter {
    fn init(v: AbSetter<Rules>) -> Self {
        RULES_SETTER.with(|rules| {
            if rules.borrow().is_some() {
                panic!("LocalRulesSetter already initialized")
            }
            *rules.borrow_mut() = Some(v);
        });
        Self { _private: () }
    }

    fn borrow<'a>(&'a self) -> &'a AbSetter<Rules> {
        // This is safe since self is never 'static
        unsafe {
            &*RULES_SETTER.with(|x| {
                x.borrow()
                    .as_ref()
                    .expect("LocalRulesSetter is not thread safe!")
                    as *const AbSetter<_>
            })
        }
    }
}

#[derive(Clone)]
struct Daemon {
    pid: u32,
    authenticated: Arc<AtomicBool>,
    rules_setter: LocalRulesSetter,
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
        rate_rules: Vec<usize>,
    ) -> Self::SetRulesFut {
        dbg!(default_target, &rules, &rate_rules);
        if self.authenticated.load(Ordering::Relaxed) {
            let rules = Rules::new(default_target, rules, rate_rules);
            self.rules_setter.borrow().set(rules);
        }
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

pub fn run(
    rules_setter: AbSetter<Rules>,
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

    let rules_setter = LocalRulesSetter::init(rules_setter);

    let transport = unixtransport::listen(&addr)?;

    let permissions = fs::Permissions::from_mode(755);
    fs::set_permissions(&addr, permissions)?;

    let clients = Arc::new(Mutex::new(Slab::new()));
    let clients2 = clients.clone();
    let clients3 = clients.clone();

    let server = Server::default()
        .incoming(transport)
        .and_then(move |channel| {
            let clients2 = clients2.clone();
            async move {
                let mut clients = clients2.lock().compat().await.unwrap();
                if !clients.is_empty() {
                    // TODO: log
                    return Err(io::ErrorKind::Other.into());
                }

                let transport = unixtransport::connect("/tmp/gleipnir").await?;
                let client = monitor::new_stub(tarpc::client::Config::default(), transport).await?;
                let client_id = clients.insert(client);

                Ok((channel, client_id))
            }
        })
        .map_ok(move |(channel, client_id)| {
            struct ClientGuard {
                clients: Arc<Mutex<Slab<monitor::Client>>>,
                client_id: usize,
            }
            impl Drop for ClientGuard {
                fn drop(&mut self) {
                    block_on(self.clients.lock().compat())
                        .unwrap()
                        .remove(self.client_id);
                }
            }

            // This is a hack, see unixtransport module
            let pid: u32 = if let SocketAddr::V4(addr) = channel.client_addr() {
                (*addr.ip()).into()
            } else {
                unreachable!()
            };
            dbg!(pid);

            let clients = clients.clone();
            tokio::executor::spawn(Compat::new(
                async move {
                    let _guard = ClientGuard { clients, client_id };
                    channel
                        .respond_with(daemon::serve(Daemon {
                            pid,
                            authenticated: Arc::new(AtomicBool::new(false)),
                            rules_setter,
                        }))
                        .await;
                    Ok(())
                }
                    .map_err(|e: io::Error| eprintln!("Connecting to client: {}", e))
                    .boxed(),
            ));
        })
        .for_each(|_| futures::future::ready(()));

    let mut runtime = tokio::runtime::current_thread::Runtime::new().expect("tokio runtime");
    let handle = runtime.handle();

    thread::spawn(move || {
        while let Ok(log) = pkt_logs.recv() {
            let clients = clients3.clone();
            let fut = async move {
                for (_id, client) in clients.lock().compat().await.unwrap().iter_mut() {
                    let r = client
                        .on_packages(tarpc::context::current(), vec![log.clone()])
                        .await;
                    if let Err(e) = r {
                        dbg!(e);
                    }
                }
                Ok(())
            };
            handle
                .spawn(Compat::new(fut.boxed()))
                .expect("spawn future");
        }
    });

    rpc::init(tokio::executor::DefaultExecutor::current().compat());
    runtime.spawn(server.unit_error().boxed().compat());
    runtime.run().expect("run tokio runtime");

    Ok(())
}
