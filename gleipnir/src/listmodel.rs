use std::ops::{Deref, DerefMut};

use qmetaobject::*;

pub trait MutListItem {
    /// Get the item in for the given role.
    /// Note that the role is, in a way, an index in the names() array.
    fn get(&self, role: i32) -> QVariant;
    fn set(&mut self, value: &QVariant, role: i32) -> bool;
    /// Array of the role names.
    fn names() -> Vec<QByteArray>;
}

#[derive(QObject, Default)]
pub struct MutListModel<T: MutListItem + 'static> {
    // https://github.com/rust-lang/rust/issues/50676
    // base: qt_base_class!(trait QAbstractListModel),
    #[qt_base_class = "QAbstractListModel"]
    base: QObjectCppWrapper,
    values: Vec<T>,
}

impl<T> std::iter::FromIterator<T> for MutListModel<T>
where
    T: MutListItem,
{
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> MutListModel<T> {
        Self {
            base: Default::default(),
            values: Vec::from_iter(iter),
        }
    }
}

impl<T: MutListItem> Deref for MutListModel<T> {
    type Target = [T];
    fn deref(&self) -> &Self::Target {
        &self.values
    }
}

impl<T: MutListItem> DerefMut for MutListModel<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.values
    }
}

impl<T: MutListItem> MutListModel<T> {
    pub fn insert(&mut self, index: usize, element: T) {
        (self as &mut QAbstractListModel).begin_insert_rows(index as i32, index as i32);
        self.values.insert(index, element);
        (self as &mut QAbstractListModel).end_insert_rows();
    }
    pub fn push(&mut self, value: T) {
        let idx = self.values.len();
        self.insert(idx, value);
    }
    pub fn remove(&mut self, index: usize) {
        (self as &mut QAbstractListModel).begin_remove_rows(index as i32, index as i32);
        self.values.remove(index);
        (self as &mut QAbstractListModel).end_remove_rows();
    }
    pub fn change_line(&mut self, index: usize, value: T) {
        self.values[index] = value;
        let idx = (self as &mut QAbstractListModel).row_index(index as i32);
        (self as &mut QAbstractListModel).data_changed(idx, idx);
    }
    pub fn reset_data(&mut self, data: Vec<T>) {
        (self as &mut QAbstractListModel).begin_reset_model();
        self.values = data;
        (self as &mut QAbstractListModel).end_reset_model();
    }
}

impl<T> QAbstractListModel for MutListModel<T>
where
    T: MutListItem,
{
    fn row_count(&self) -> i32 {
        self.values.len() as i32
    }
    fn data(&self, index: QModelIndex, role: i32) -> QVariant {
        let idx = index.row();
        if idx >= 0 && (idx as usize) < self.values.len() {
            self.values[idx as usize].get(role - USER_ROLE).clone()
        } else {
            QVariant::default()
        }
    }
    fn role_names(&self) -> std::collections::HashMap<i32, QByteArray> {
        T::names()
            .iter()
            .enumerate()
            .map(|(i, x)| (i as i32 + USER_ROLE, x.clone()))
            .collect()
    }
    fn set_data(&mut self, index: QModelIndex, value: &QVariant, role: i32) -> bool {
        let idx = index.row();
        let success = idx >= 0
            && (idx as usize) < self.values.len()
            && self.values[idx as usize].set(value, role - USER_ROLE);
        if success {
            (self as &mut QAbstractListModel).data_changed(index, index);
        }
        success
    }
}
