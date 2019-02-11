use std::cell::RefCell;
use std::iter::FromIterator;

use qmetaobject::*;

use crate::listmodel::{MutListItem, MutListModel};

#[derive(QGadget, SimpleListItem, Default)]
pub struct QRule {
    pub is_input: qt_property!(bool),
    pub proto: qt_property!(usize),
    pub exe: qt_property!(QString),
    pub port_begin: qt_property!(u16),
    pub port_end: qt_property!(u16),
    pub addr: qt_property!(QString),
    pub mask: qt_property!(u8),
    pub target: qt_property!(usize),
}

impl MutListItem for QRule {
    fn get(&self, idx: i32) -> QVariant {
        match idx {
            0 => QMetaType::to_qvariant(&self.is_input),
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
            0 => <_>::from_qvariant(value.clone()).map(|v| self.is_input = v),
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
            QByteArray::from("isInput"),
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
}

impl Backend {
    pub fn new() -> Self {
        let rules = MutListModel::from_iter(vec![QRule {
            is_input: true,
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

        Backend {
            base: Default::default(),
            rules: RefCell::new(rules),
            targets: targets,
            targets_changed: Default::default(),
        }
    }
}
