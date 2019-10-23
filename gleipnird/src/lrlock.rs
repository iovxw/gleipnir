use std::marker::PhantomData;
use std::ops::Deref;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::panicking;

/// A wait-free read lock
pub struct LeftRightLock<T> {
    left: T,
    right: Option<T>,
    state: State,
}

impl<T> LeftRightLock<T> {
    pub fn new(v: T) -> (Reader<T>, Setter<T>) {
        let inner = LeftRightLock {
            left: v,
            right: None,
            state: State::new(true),
        };
        let inner = Arc::new(inner);
        (
            Reader(inner.clone(), PhantomData),
            Setter(inner, PhantomData),
        )
    }
}

pub struct Reader<T>(Arc<LeftRightLock<T>>, PhantomData<*const ()>);
unsafe impl<T: Send> Send for Reader<T> {}

pub struct Setter<T>(Arc<LeftRightLock<T>>, PhantomData<*const ()>);
unsafe impl<T: Send> Send for Setter<T> {}

impl<T> Reader<T> {
    pub fn read(&self) -> ReadGuard<T> {
        let side = unsafe { self.0.state.set_read() };

        ReadGuard {
            value: if side {
                &self.0.left
            } else {
                self.0.right.as_ref().expect("unreachable LeftRightLock state")
            },
            state: &self.0.state,
        }
    }
}

impl<T> Setter<T> {
    pub fn set(&self, value: T) {
        unsafe {
            self.0.state.swap_side(|current_side_a| {
                let ptr = (&*self.0) as *const LeftRightLock<T> as *mut LeftRightLock<T>;
                if current_side_a {
                    (*ptr).right = Some(value);
                } else {
                    (*ptr).left = value;
                };
            });
        }
    }
}

pub struct ReadGuard<'a, T> {
    value: &'a T,
    state: &'a State,
}

impl<'a, T> Deref for ReadGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<'a, T> Drop for ReadGuard<'a, T> {
    fn drop(&mut self) {
        if !panicking() {
            unsafe { self.state.unset_read() }
        }
    }
}

struct State {
    state: AtomicUsize,
}

impl State {
    fn new(side: bool) -> Self {
        State {
            state: AtomicUsize::new(if side { 0b000 } else { 0b100 }),
        }
    }
    unsafe fn set_read(&self) -> bool {
        match self.state.fetch_add(1, Ordering::AcqRel) {
            0b000 => true,
            0b100 => false,
            0b001 | 0b101 => panic!("LeftRightLock can only have one reader"),
            _ => unreachable!(),
        }
    }
    unsafe fn unset_read(&self) {
        match self.state.fetch_sub(1, Ordering::AcqRel) {
            0b001 | 0b101 => (),
            _ => unreachable!(),
        }
    }
    unsafe fn swap_side<F: FnOnce(bool)>(&self, f: F) {
        // Apply the mask to get a "not reading" value
        let current_side = self.state.load(Ordering::Acquire) & 0b100;
        let next_side = current_side ^ 0b100;

        f(current_side == 0);

        // swap to next side when current side is not reading
        while self
            .state
            .compare_and_swap(current_side, next_side, Ordering::AcqRel)
            != current_side
        {}
    }
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn lrlock() {
        let (r, s) = LeftRightLock::new(0);
        assert_eq!(*r.read(), 0);
        s.set(1);
        assert_eq!(*r.read(), 1);
        s.set(2);
        assert_eq!(*r.read(), 2);
    }
    #[test]
    #[should_panic(expected = "LeftRightLock can only have one reader")]
    fn two_reader() {
        let (r, _s) = LeftRightLock::new(0);
        let _a = r.read();
        r.read();
    }
}
