use std::marker::PhantomData;
use std::ops::Deref;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::panicking;

pub struct AbLock<T> {
    a: T,
    b: Option<T>,
    state: AbState,
}

impl<T> AbLock<T> {
    pub fn new(v: T) -> (AbReader<T>, AbSetter<T>) {
        let inner = AbLock {
            a: v,
            b: None,
            state: AbState::new(true),
        };
        let inner = Arc::new(inner);
        (
            AbReader(inner.clone(), PhantomData),
            AbSetter(inner, PhantomData),
        )
    }
}

pub struct AbReader<T>(Arc<AbLock<T>>, PhantomData<*const ()>);
unsafe impl<T> Send for AbReader<T> {}
// unsafe impl<T> !Sync for AbReader<T> {}

pub struct AbSetter<T>(Arc<AbLock<T>>, PhantomData<*const ()>);
unsafe impl<T> Send for AbSetter<T> {}
// unsafe impl<T> !Sync for AbSetter<T> {}

impl<T> AbReader<T> {
    pub fn read(&self) -> ReadGuard<T> {
        let side = unsafe { self.0.state.set_read() };

        ReadGuard {
            value: if side {
                &self.0.a
            } else {
                self.0.b.as_ref().expect("unreachable AbLock state")
            },
            state: &self.0.state,
        }
    }
}

impl<T> AbSetter<T> {
    pub fn set(&self, value: T) {
        unsafe {
            self.0.state.swap_side(|current_side_a| {
                let ptr = (&*self.0) as *const AbLock<T> as *mut AbLock<T>;
                if current_side_a {
                    (*ptr).b = Some(value);
                } else {
                    (*ptr).a = value;
                };
            });
        }
    }
}

pub struct ReadGuard<'a, T> {
    value: &'a T,
    state: &'a AbState,
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

struct AbState {
    state: AtomicUsize,
}

impl AbState {
    fn new(side: bool) -> Self {
        AbState {
            state: AtomicUsize::new(if side { 0b000 } else { 0b100 }),
        }
    }
    unsafe fn set_read(&self) -> bool {
        match self.state.fetch_add(1, Ordering::AcqRel) {
            0b000 => true,
            0b100 => false,
            0b001 | 0b101 => panic!("AbLock can only have one reader"),
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
    fn ablock() {
        let (r, s) = AbLock::new(0);
        assert_eq!(*r.read(), 0);
        s.set(1);
        assert_eq!(*r.read(), 1);
        s.set(2);
        assert_eq!(*r.read(), 2);
    }
    #[test]
    #[should_panic(expected = "AbLock can only have one reader")]
    fn two_reader() {
        let (r, _s) = AbLock::new(0);
        let _a = r.read();
        r.read();
    }
}
