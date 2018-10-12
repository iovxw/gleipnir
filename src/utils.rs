#[doc(hidden)]
pub mod inner {
    pub fn cast<T>(p: &T) -> *const T {
        p
    }
    pub struct Helper<T>(pub T);
}

#[macro_export]
macro_rules! let_tls {
    ($name: ident, $v: expr) => {
        let tmp: &'static _ = $v.with(|x| unsafe { &*$crate::utils::inner::cast(x) });
        let tmp = $crate::utils::inner::Helper(tmp);
        let $name = tmp.0;
    };
}
