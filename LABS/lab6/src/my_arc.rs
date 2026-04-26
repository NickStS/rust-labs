//! Custom atomic reference counted pointer.

use std::ops::Deref;
use std::ptr::NonNull;
use std::sync::atomic::{fence, AtomicUsize, Ordering};

struct Inner<T> {
    refcount: AtomicUsize,
    value: T,
}

pub struct MyArc<T> {
    ptr: NonNull<Inner<T>>,
}

unsafe impl<T: Send + Sync> Send for MyArc<T> {}
unsafe impl<T: Send + Sync> Sync for MyArc<T> {}

impl<T> MyArc<T> {
    pub fn new(value: T) -> Self {
        let boxed = Box::new(Inner {
            refcount: AtomicUsize::new(1),
            value,
        });
        let ptr = NonNull::new(Box::into_raw(boxed)).expect("Box::into_raw returned null");
        MyArc { ptr }
    }

    pub fn strong_count(this: &Self) -> usize {
        unsafe { this.ptr.as_ref() }
            .refcount
            .load(Ordering::Acquire)
    }
}

impl<T> Clone for MyArc<T> {
    fn clone(&self) -> Self {
        let inner = unsafe { self.ptr.as_ref() };
        let old = inner.refcount.fetch_add(1, Ordering::Relaxed);
        if old > isize::MAX as usize {
            std::process::abort();
        }
        MyArc { ptr: self.ptr }
    }
}

impl<T> Deref for MyArc<T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &self.ptr.as_ref().value }
    }
}

impl<T> Drop for MyArc<T> {
    fn drop(&mut self) {
        let inner = unsafe { self.ptr.as_ref() };
        if inner.refcount.fetch_sub(1, Ordering::Release) == 1 {
            fence(Ordering::Acquire);
            unsafe {
                drop(Box::from_raw(self.ptr.as_ptr()));
            }
        }
    }
}
