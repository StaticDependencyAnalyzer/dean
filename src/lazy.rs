use std::cell::UnsafeCell;
use std::sync::Once;

pub struct Lazy<T> {
    init: Once,
    hold: UnsafeCell<Option<T>>,
}

unsafe impl<T: Send> Send for Lazy<T> {}
unsafe impl<T: Sync> Sync for Lazy<T> {}

impl<T> Lazy<T> {
    pub fn new() -> Self {
        Self {
            init: Once::new(),
            hold: UnsafeCell::new(None),
        }
    }

    pub fn get<F>(&self, f: F) -> &T
    where
        F: FnOnce() -> T,
    {
        self.init.call_once(|| unsafe {
            // SAFETY: This is safe because only one thread can access the `hold` because of the `Once`.
            *self.hold.get() = Some(f());
        });

        // SAFETY: This is safe because the value has already been initialized in the `Once` block.
        let hold = unsafe { &*self.hold.get() };
        hold.as_ref().unwrap()
    }
}

#[cfg(test)]
mod tests {

    use std::sync::atomic::{AtomicU8, Ordering};
    use std::sync::Arc;

    use super::*;

    #[test]
    fn it_build_once_and_only_once_the_value() {
        let counter = Arc::new(AtomicU8::new(0));

        let lazy = Lazy::<u8>::new();

        let _value = lazy.get(|| {
            counter.fetch_add(1, Ordering::Relaxed);
            5_u8
        });
        let value = lazy.get(|| {
            counter.fetch_add(1, Ordering::Relaxed);
            5_u8
        });
        lazy.get(|| {
            counter.fetch_add(1, Ordering::Relaxed);
            5_u8
        });

        assert_eq!(value, &5_u8);
        assert_eq!(counter.load(Ordering::Relaxed), 1_u8);
    }
}
