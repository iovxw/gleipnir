use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::net::SocketAddr;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::io::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::LocalKey;

use futures::{
    compat::{Compat, Executor01CompatExt},
    future::{self, poll_fn, Ready},
    prelude::*,
    FutureExt,
};
use gleipnir_interface::{daemon, monitor, unixtransport, Device, Proto, Rule, RuleTarget};
use nix::sys::socket::{getsockopt, sockopt::PeerCredentials};
use rpc::context;
use rpc::server::{self, Handler, Server};
use serde::{Deserialize, Serialize};

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
    client: Arc<monitor::Client>,
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

pub fn run(rules_setter: AbSetter<Rules>) -> Result<(), std::io::Error> {
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

    let clients = Arc::new(AtomicUsize::new(0));

    struct ClientRc(Arc<AtomicUsize>);
    impl Drop for ClientRc {
        fn drop(&mut self) {
            dbg!("disconnected");
            self.0.fetch_sub(1, Ordering::AcqRel);
        }
    }

    let server = Server::default()
        .incoming(transport)
        .map_ok(move |channel| {
            if clients.fetch_add(1, Ordering::AcqRel) >= 1 {
                // TODO: log
                clients.fetch_sub(1, Ordering::AcqRel);
                return;
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
                    let counter = ClientRc(clients);
                    let transport = unixtransport::connect("/tmp/gleipnir").await?;
                    let client =
                        monitor::new_stub(tarpc::client::Config::default(), transport).await?;
                    channel
                        .respond_with(daemon::serve(Daemon {
                            pid,
                            authenticated: Arc::new(AtomicBool::new(false)),
                            rules_setter,
                            client: Arc::new(client),
                        }))
                        .await;
                    Ok(())
                }
                    .map_err(|e: io::Error| eprintln!("Connecting to client: {}", e))
                    .boxed(),
            ));
        })
        .for_each(|_| futures::future::ready(()));

    rpc::init(tokio::executor::DefaultExecutor::current().compat());
    tokio::run(server.unit_error().boxed().compat());

    Ok(())
}
