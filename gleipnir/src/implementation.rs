use std::cell::RefCell;
use std::iter::FromIterator;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::ops::RangeInclusive;

use futures::{
    compat::{Compat, Executor01CompatExt},
    future::FutureExt,
};
use gleipnir_interface::{daemon, unixtransport, Device, Proto, Rule, RuleTarget};
use qmetaobject::*;
use tarpc;
use tokio::runtime::current_thread::Runtime;

use crate::listmodel::{MutListItem, MutListModel};

#[derive(QGadget, SimpleListItem, Default)]
pub struct QRule {
    pub device: qt_property!(usize),
    pub proto: qt_property!(usize),
    pub exe: qt_property!(QString),
    pub port_begin: qt_property!(u16),
    pub port_end: qt_property!(u16),
    pub addr: qt_property!(QString),
    pub mask: qt_property!(u8),
    pub target: qt_property!(usize),
}

impl From<&QRule> for Rule {
    fn from(qrule: &QRule) -> Self {
        let device = match qrule.device {
            0 => None,
            1 => Some(Device::Input),
            2 => Some(Device::Output),
            _ => unreachable!(),
        };
        let proto = match qrule.proto {
            0 => None,
            1 => Some(Proto::Tcp),
            2 => Some(Proto::Udp),
            3 => Some(Proto::UdpLite),
            _ => unreachable!(),
        };
        let exe = if !qrule.exe.to_slice().is_empty() {
            Some(String::from_utf16(qrule.exe.to_slice()).unwrap())
        } else {
            None
        };
        let port = match (qrule.port_begin, qrule.port_end) {
            (0, 0) => None,
            (port, 0) => Some(RangeInclusive::new(port, port)),
            (port_begin, port_end) => Some(RangeInclusive::new(port_begin, port_end)),
        };
        // TODO
        let addr = IpAddr::V4(Ipv4Addr::UNSPECIFIED);
        let subnet = (addr, qrule.mask);
        let target = match qrule.target {
            0 => RuleTarget::Accept,
            1 => RuleTarget::Drop,
            n => RuleTarget::RateLimit(n - 2),
        };
        Self {
            device,
            proto,
            exe,
            port,
            subnet,
            target,
        }
    }
}

impl MutListItem for QRule {
    fn get(&self, idx: i32) -> QVariant {
        match idx {
            0 => QMetaType::to_qvariant(&self.device),
            1 => QMetaType::to_qvariant(&self.proto),
            2 => QMetaType::to_qvariant(&self.exe),
            3 => QMetaType::to_qvariant(&self.port_begin),
            4 => QMetaType::to_qvariant(&self.port_end),
            5 => QMetaType::to_qvariant(&self.addr),
            6 => QMetaType::to_qvariant(&self.mask),
            7 => QMetaType::to_qvariant(&self.target),
            _ => QVariant::default(),
        }
    }
    fn set(&mut self, value: &QVariant, idx: i32) -> bool {
        match idx {
            0 => <_>::from_qvariant(value.clone()).map(|v| self.device = v),
            1 => <_>::from_qvariant(value.clone()).map(|v| self.proto = v),
            2 => <_>::from_qvariant(value.clone()).map(|v| self.exe = v),
            3 => <_>::from_qvariant(value.clone()).map(|v| self.port_begin = v),
            4 => <_>::from_qvariant(value.clone()).map(|v| self.port_end = v),
            5 => <_>::from_qvariant(value.clone()).map(|v| self.addr = v),
            6 => <_>::from_qvariant(value.clone()).map(|v| self.mask = v),
            7 => <_>::from_qvariant(value.clone()).map(|v| self.target = v),
            _ => None,
        }
        .is_some()
    }
    fn names() -> Vec<QByteArray> {
        vec![
            QByteArray::from("device"),
            QByteArray::from("proto"),
            QByteArray::from("exe"),
            QByteArray::from("portBegin"),
            QByteArray::from("portEnd"),
            QByteArray::from("addr"),
            QByteArray::from("mask"),
            QByteArray::from("target"),
        ]
    }
}

#[derive(QObject)]
pub struct Backend {
    base: qt_base_class!(trait QObject),
    pub rules: qt_property!(RefCell<MutListModel<QRule>>; CONST),
    pub targets: qt_property!(QVariantList; NOTIFY targets_changed),
    pub targets_changed: qt_signal!(),
    pub default_target: qt_property!(usize),
    pub apply_rules: qt_method!(fn(&mut self)),
    pub rate_rules: qt_property!(RefCell<MutListModel<RateLimitRule>>; CONST),
    pub daemon_connected: qt_property!(bool; NOTIFY daemon_connected_changed),
    pub daemon_connected_changed: qt_signal!(),
    runtime: Runtime,
    client: Option<daemon::Client>,
}

impl Backend {
    pub fn new() -> Self {
        let rules = MutListModel::from_iter(vec![QRule {
            device: 1,
            proto: 1,
            exe: "".to_string().into(),
            port_begin: 0,
            port_end: 0,
            addr: "8.8.8.8".to_string().into(),
            ..Default::default()
        },
        QRule {
            device: 1,
            proto: 1,
            exe: "".to_string().into(),
            port_begin: 0,
            port_end: 0,
            addr: "8.8.8.8".to_string().into(),
            ..Default::default()
        },
        QRule {
            device: 1,
            proto: 1,
            exe: "".to_string().into(),
            port_begin: 0,
            port_end: 0,
            addr: "8.8.8.8".to_string().into(),
            ..Default::default()
        }]);
        let targets = QVariantList::from_iter(vec![
            QString::from("Rate Limit Rule 1".to_string()),
            QString::from("Rate Limit Rule 2".to_string()),
        ]);
        let default_target = 0;

        let mut runtime = Runtime::new().unwrap();

        tarpc::init(tokio::executor::DefaultExecutor::current().compat());
        let client = runtime
            .block_on(Compat::new(
                async {
                    let transport = unixtransport::connect("/tmp/gleipnir").await?;
                    daemon::new_stub(tarpc::client::Config::default(), transport).await
                }
                    .boxed(),
            ))
            .ok();

        // TODO
        let rate_rules = MutListModel::from_iter(vec![]);

        Backend {
            base: Default::default(),
            rules: RefCell::new(rules),
            targets: targets,
            targets_changed: Default::default(),
            default_target,
            apply_rules: Default::default(),
            rate_rules: RefCell::new(rate_rules),
            daemon_connected: client.is_some(),
            daemon_connected_changed: Default::default(),
            runtime,
            client,
        }
    }

    pub fn apply_rules(&mut self) {
        let authed = self
            .runtime
            .block_on(Compat::new(
                self.client
                    .as_mut()
                    .expect("")
                    .register(tarpc::context::current())
                    .boxed(),
            ))
            .unwrap();
        dbg!(authed);

        let rules: Vec<Rule> = self.rules.borrow().iter().map(Into::into).collect();
        let rate_rules: Vec<usize> = self.rate_rules.borrow().iter().map(|v| v.limit).collect();

        let default_target = match self.default_target {
            0 => RuleTarget::Accept,
            1 => RuleTarget::Drop,
            n => RuleTarget::RateLimit(n - 2),
        };

        self.runtime
            .block_on(Compat::new(
                self.client
                    .as_mut()
                    .expect("")
                    .set_rules(tarpc::context::current(), default_target, rules, rate_rules)
                    .boxed(),
            ))
            .unwrap();
    }
}

pub struct RateLimitRule {
    name: QString, // TODO: String
    limit: usize,
}

impl MutListItem for RateLimitRule {
    fn get(&self, idx: i32) -> QVariant {
        match idx {
            0 => QMetaType::to_qvariant(&self.name),
            1 => QMetaType::to_qvariant(&self.limit),
            _ => QVariant::default(),
        }
    }
    fn set(&mut self, value: &QVariant, idx: i32) -> bool {
        match idx {
            0 => <_>::from_qvariant(value.clone()).map(|v| self.name = v),
            1 => <_>::from_qvariant(value.clone()).map(|v| self.limit = v),
            _ => None,
        }
        .is_some()
    }
    fn names() -> Vec<QByteArray> {
        vec![QByteArray::from("name"), QByteArray::from("limit")]
    }
}
