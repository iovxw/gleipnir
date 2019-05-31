use std::cell::RefCell;
use std::collections::HashMap;
use std::env;
use std::io;
use std::iter::FromIterator;
use std::mem;
use std::ops::AddAssign;
use std::ops::RangeInclusive;
use std::os::unix::net::UnixStream;
use std::process::Command;
use std::sync::atomic::Ordering;
use std::thread;

use futures::{
    compat::{Compat, Executor01CompatExt},
    future::FutureExt,
};
use gleipnir_interface::{
    daemon, unixtransport, Device, PackageReport, Proto, RateLimitRule, Rule, RuleTarget, Rules,
};
use qmetaobject::*;
use tarpc;
use tokio::runtime::current_thread::Runtime;

use crate::implementation;
use crate::listmodel::{MutListItem, MutListModel};
use crate::monitor;

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
        let addr = String::from_utf16_lossy(qrule.addr.to_slice())
            .parse()
            .expect("Failed to parse IpAddr");
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
    pub new_rule: qt_method!(fn(&mut self)),
    pub swap_rule: qt_method!(fn(&mut self, a: usize, b: usize)),
    pub remove_rule: qt_method!(fn(&mut self, i: usize)),
    pub start_daemon: qt_method!(fn(&mut self)),
    pub start_daemon_error: qt_signal!(e: QString),
    pub connect_to_daemon: qt_method!(fn(&mut self)),
    pub connect_to_daemon_error: qt_signal!(e: QString),
    pub daemon_exists: qt_method!(fn(&self) -> bool),
    pub refresh_monitor: qt_method!(fn(&mut self)),
    pub logs: qt_property!(RefCell<SimpleListModel<QPackageLog>>; CONST),
    pub traffic: qt_property!(RefCell<SimpleListModel<ProgramStatus>>; CONST),
    pub charts: qt_property!(QVariantList; NOTIFY charts_changed),
    pub charts_changed: qt_signal!(),
    pub chart_x_size: qt_property!(usize),
    current_traffic: HashMap<String, ProgramStatus>,
    traffic_history: HashMap<String, Vec<u32>>,
    prev_proc_on_chart: Vec<String>,
    runtime: Runtime,
    client: Option<daemon::Client>,
}

impl Backend {
    pub fn new() -> Self {
        let rules = MutListModel::from_iter(vec![
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
        ]);
        let targets = QVariantList::from_iter(vec![
            QString::from("Rate Limit Rule 1".to_string()),
            QString::from("Rate Limit Rule 2".to_string()),
        ]);
        let default_target = 0;

        let runtime = Runtime::new().unwrap();

        tarpc::init(tokio::executor::DefaultExecutor::current().compat());

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
            daemon_connected: false,
            daemon_connected_changed: Default::default(),
            new_rule: Default::default(),
            swap_rule: Default::default(),
            remove_rule: Default::default(),
            start_daemon: Default::default(),
            start_daemon_error: Default::default(),
            connect_to_daemon: Default::default(),
            connect_to_daemon_error: Default::default(),
            daemon_exists: Default::default(),
            refresh_monitor: Default::default(),
            logs: Default::default(),
            traffic: Default::default(),
            charts: Default::default(),
            charts_changed: Default::default(),
            chart_x_size: 80,
            current_traffic: Default::default(),
            traffic_history: Default::default(),
            prev_proc_on_chart: vec![String::default(); 5],
            runtime,
            client: None,
        }
    }

    pub fn apply_rules(&mut self) {
        let authed = self
            .runtime
            .block_on(Compat::new(
                self.client
                    .as_mut()
                    .expect("")
                    .unlock(tarpc::context::current())
                    .boxed(),
            ))
            .unwrap();
        dbg!(authed);

        let rules: Vec<Rule> = self.rules.borrow().iter().map(Into::into).collect();
        let rate_rules = (&**self.rate_rules.borrow()).to_vec();

        let default_target = match self.default_target {
            0 => RuleTarget::Accept,
            1 => RuleTarget::Drop,
            n => RuleTarget::RateLimit(n - 2),
        };

        let rules = Rules {
            rules,
            rate_rules,
            default_target,
        };

        self.runtime
            .block_on(Compat::new(
                self.client
                    .as_mut()
                    .expect("")
                    .set_rules(tarpc::context::current(), rules)
                    .boxed(),
            ))
            .unwrap();
    }

    pub fn new_rule(&mut self) {
        self.rules.borrow_mut().push(QRule::default());
    }
    pub fn swap_rule(&mut self, a: usize, b: usize) {
        self.rules.borrow_mut().swap(a, b);
    }
    pub fn remove_rule(&mut self, i: usize) {
        self.rules.borrow_mut().remove(i);
    }
    pub fn start_daemon(&mut self) {
        // To Packager: set a START_GLEIPNIRD_CMD env at compile time to override the default
        // command, when you are not using systemd
        let cmd = env::var("START_GLEIPNIRD_CMD")
            .ok()
            .or_else(|| option_env!("START_GLEIPNIRD_CMD").map(String::from))
            .unwrap_or_else(|| "systemctl start gleipnird".to_string());
        let r: io::Result<()> = try {
            let output = Command::new("/bin/sh").args(&["-c", &cmd]).output()?;
            if !output.status.success() {
                self.start_daemon_error((&*String::from_utf8_lossy(&output.stderr)).into())
            }
            self.connect_to_daemon_impl()?;
        };
        if let Err(e) = r {
            self.start_daemon_error(e.to_string().into());
        }
    }
    pub fn connect_to_daemon(&mut self) {
        if let Err(e) = self.connect_to_daemon_impl() {
            self.connect_to_daemon_error(e.to_string().into());
        }
    }
    fn connect_to_daemon_impl(&mut self) -> Result<(), io::Error> {
        if !monitor::MONITOR_RUNNING.load(Ordering::Acquire) {
            let ptr = QPointer::from(&*self);
            let on_packages_callback = queued_callback(move |logs| {
                ptr.as_ref()
                    .map(|p| {
                        let mutp = unsafe { &mut *(p as *const _ as *mut implementation::Backend) };
                        mutp.on_packages(logs);
                    })
                    .expect("QObject doesn't exist");
            });

            thread::spawn(|| {
                monitor::run(on_packages_callback).expect("Failed to start monitor");
            });
            while !monitor::MONITOR_RUNNING.load(Ordering::Acquire) {}
        }
        let client: Result<daemon::Client, io::Error> = self.runtime.block_on(Compat::new(
            async {
                let transport = unixtransport::connect("/tmp/gleipnird").await?;
                let mut client =
                    daemon::new_stub(tarpc::client::Config::default(), transport).await?;
                client
                    .init_monitor(tarpc::context::current(), "/tmp/gleipnir".to_string())
                    .await;
                Ok(client)
            }
                .boxed(),
        ));
        let client = client?;
        self.client = Some(client);
        self.daemon_connected = true;
        self.daemon_connected_changed();
        Ok(())
    }
    pub fn daemon_exists(&self) -> bool {
        let addr = std::path::PathBuf::from("/tmp/gleipnird");
        addr.exists() && UnixStream::connect(&addr).is_ok()
    }
    pub fn refresh_monitor(&mut self) {
        let empty_traffic: HashMap<_, _> = self
            .current_traffic
            .iter()
            .map(|(k, v)| (k.clone(), v.clone_new()))
            .collect();
        let traffic = mem::replace(&mut self.current_traffic, empty_traffic);
        for (name, traffic) in &traffic {
            let history = self
                .traffic_history
                .entry(name.to_owned())
                .or_insert_with(Vec::new);
            history.push((traffic.sending + traffic.receiving) as u32);
        }
        let mut traffic: Vec<_> = traffic.into_iter().map(|(_k, v)| v).collect();
        traffic.sort();
        self.traffic.borrow_mut().reset_data(traffic);

        let mut charts: Vec<(&String, u32)> = self
            .traffic_history
            .iter()
            .map(|(name, history)| {
                (
                    name,
                    history
                        .iter()
                        .skip(history.len().saturating_sub(self.chart_x_size))
                        .take(self.chart_x_size)
                        .sum(),
                )
            })
            .collect();
        charts.sort_by(|(_, a), (_, b)| a.cmp(&b).reverse());
        let charts: Vec<_> = charts
            .into_iter()
            .map(|(k, _)| k)
            .take(5)
            .map(|proc| HistoryChart {
                name: proc.as_str().into(),
                model: QVariantList::from_iter(self.traffic_history[proc].clone()),
            })
            .map(|v| v.to_qvariant())
            .collect();
        self.charts = QVariantList::from_iter(charts);
        self.charts_changed();
    }
    pub fn on_packages(&mut self, logs: Vec<PackageReport>) {
        let mut self_logs = self.logs.borrow_mut();
        // TODO: impl extend_from_slice for SimpleListModel
        for log in &logs {
            self_logs.push(log.into());
            let status = self
                .current_traffic
                .entry(log.exe.clone())
                .or_insert_with(|| ProgramStatus::new(&log.exe));
            *status += log;
        }
    }
}

#[derive(QGadget, Default, Clone)]
pub struct HistoryChart {
    pub name: qt_property!(QString),
    pub model: qt_property!(QVariantList),
}

#[derive(SimpleListItem, Default)]
pub struct QPackageLog {
    pub dropped: bool,
    pub input: bool,
    pub exe: QString,
    pub protocol: QString,
    pub addr: QString,
    pub len: usize,
    pub matched_rule: usize,
}

impl From<&'_ PackageReport> for QPackageLog {
    fn from(v: &PackageReport) -> Self {
        Self {
            dropped: v.dropped,
            input: v.device.is_input(),
            exe: (&*v.exe).into(),
            protocol: v.protocol.to_string().into(),
            addr: v.addr.to_string().into(),
            len: v.len,
            matched_rule: v.matched_rule.map(|x| x + 1).unwrap_or(0),
        }
    }
}

#[derive(SimpleListItem, Default, Clone, Eq, PartialEq, Debug)]
pub struct ProgramStatus {
    pub exe: QString,
    pub sending: usize,
    pub receiving: usize,
}

impl ProgramStatus {
    fn new(program: &str) -> Self {
        Self {
            exe: program.into(),
            ..Default::default()
        }
    }
    fn clone_new(&self) -> Self {
        Self {
            exe: self.exe.clone(),
            ..Default::default()
        }
    }
}

impl Ord for ProgramStatus {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.sending + self.receiving)
            .cmp(&(other.sending + other.receiving))
            .reverse()
            .then_with(|| self.exe.to_slice().cmp(&other.exe.to_slice()))
    }
}

impl PartialOrd for ProgramStatus {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl AddAssign<&'_ PackageReport> for ProgramStatus {
    fn add_assign(&mut self, other: &PackageReport) {
        if other.dropped {
            return;
        }
        if other.device.is_input() {
            self.receiving += other.len
        } else {
            self.sending += other.len
        }
    }
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
