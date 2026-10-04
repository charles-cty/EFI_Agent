//! Allocation-free completion queue for single-processor firmware notifications.
use alloc::boxed::Box;
use core::{
    ptr::{self, NonNull},
    sync::atomic::{AtomicBool, AtomicPtr, Ordering},
};

/// An intrusive queue node. Its owner keeps it pinned until CloseEvent and drain.
pub struct Notification {
    completed: AtomicBool,
    ready: AtomicBool,
    next: AtomicPtr<Notification>,
}

impl Notification {
    pub fn new() -> Box<Self> {
        Box::new(Self {
            completed: AtomicBool::new(false),
            ready: AtomicBool::new(false),
            next: AtomicPtr::new(ptr::null_mut()),
        })
    }

    /// Callback-only path: no allocation, firmware wait, UI, or protocol work.
    ///
    /// # Safety
    /// The node must stay at its address until its event is closed and the queue
    /// is drained. Each node belongs to one queue and one operation. The consumer
    /// runs on the same processor, outside the callback's higher TPL.
    pub unsafe fn enqueue(context: NonNull<Self>, queue: &CompletionQueue) {
        // SAFETY: the event owner keeps this node alive through CloseEvent.
        let node = unsafe { context.as_ref() };
        if node.completed.swap(true, Ordering::AcqRel) {
            return;
        }
        let mut head = queue.pending.load(Ordering::Acquire);
        loop {
            node.next.store(head, Ordering::Relaxed);
            match queue.pending.compare_exchange_weak(
                head,
                context.as_ptr(),
                Ordering::Release,
                Ordering::Acquire,
            ) {
                Ok(_) => return,
                Err(current) => head = current,
            }
        }
    }

    pub fn ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }
}

pub struct CompletionQueue {
    pending: AtomicPtr<Notification>,
}

impl Default for CompletionQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl CompletionQueue {
    pub const fn new() -> Self {
        Self {
            pending: AtomicPtr::new(ptr::null_mut()),
        }
    }

    /// Process queued notifications only in the application dispatcher.
    ///
    /// # Safety
    /// All enqueued nodes must remain allocated. Only one consumer may drain,
    /// and nodes may be freed only after their callback event is closed and
    /// this queue is drained. This is not a general-purpose SMP queue.
    pub unsafe fn drain(&self) {
        let mut next = self.pending.swap(ptr::null_mut(), Ordering::AcqRel);
        while let Some(node) = NonNull::new(next) {
            // SAFETY: owners only retire nodes at APPLICATION after draining.
            let node = unsafe { node.as_ref() };
            next = node.next.load(Ordering::Acquire);
            node.ready.store(true, Ordering::Release);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    #[test]
    fn callbacks_only_publish_and_dispatch_retires_every_node_once() {
        let mut seed = 731u32;
        for count in [1, 2, 31, 64, 257] {
            let queue = CompletionQueue::new();
            let mut nodes: Vec<_> = (0..count).map(|_| Notification::new()).collect();
            let mut completed = alloc::vec![false; count];
            let mut dispatched = alloc::vec![false; count];
            for step in 0..count * 8 {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                let index = seed as usize % count;
                // SAFETY: all nodes remain boxed through all dispatches.
                unsafe { Notification::enqueue(NonNull::from(nodes[index].as_mut()), &queue) };
                completed[index] = true;
                if step % 7 == 0 {
                    unsafe { queue.drain() };
                    dispatched.clone_from(&completed);
                }
                for (node, expected) in nodes.iter().zip(&dispatched) {
                    assert_eq!(node.ready(), *expected);
                }
            }
            for node in &mut nodes {
                unsafe { Notification::enqueue(NonNull::from(node.as_mut()), &queue) };
            }
            unsafe { queue.drain() };
            assert!(nodes.iter().all(|node| node.ready()));
            unsafe { queue.drain() };
        }
    }
}
