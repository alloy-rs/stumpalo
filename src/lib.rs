#![cfg_attr(not(feature = "std"), no_std)]
#![doc = "Local evm2 fork of stumpalo with explicit stack-frame commit and rollback."]

extern crate alloc;

use alloc::vec::Vec;
use core::{fmt, marker::PhantomData};

/// Checkpoint token for [`FrameStack`].
///
/// Frames must be resolved in strict LIFO order with [`FrameStack::commit`] or
/// [`FrameStack::rollback`]. Dropping a frame token has no effect.
#[must_use = "stack frames must be committed or rolled back"]
#[derive(Eq, PartialEq)]
pub struct StackFrame<T> {
    id: usize,
    len: usize,
    _marker: PhantomData<fn() -> T>,
}

impl<T> Clone for StackFrame<T> {
    #[inline]
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for StackFrame<T> {}

impl<T> StackFrame<T> {
    /// Returns the stack length captured by this frame.
    #[inline]
    #[must_use]
    pub const fn len(self) -> usize {
        self.len
    }
}

impl<T> fmt::Debug for StackFrame<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StackFrame").field("id", &self.id).field("len", &self.len).finish()
    }
}

/// A LIFO stack with explicit checkpoint frames.
///
/// This is the small API surface evm2 needs from the stumpalo fork: append
/// rollback entries, checkpoint the current top, keep appended entries on
/// commit, and drain appended entries on rollback.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FrameStack<T> {
    entries: Vec<T>,
    frames: Vec<(usize, usize)>,
    next_frame_id: usize,
}

impl<T> Default for FrameStack<T> {
    #[inline]
    fn default() -> Self {
        Self { entries: Vec::new(), frames: Vec::new(), next_frame_id: 0 }
    }
}

impl<T> FrameStack<T> {
    /// Creates an empty stack.
    #[inline]
    #[must_use]
    pub const fn new() -> Self {
        Self { entries: Vec::new(), frames: Vec::new(), next_frame_id: 0 }
    }

    /// Returns the number of entries in the stack.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns whether the stack is empty.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Pushes an entry onto the stack.
    #[inline]
    pub fn push(&mut self, entry: T) {
        self.entries.push(entry);
    }

    /// Removes all entries and active frame bookkeeping.
    #[inline]
    pub fn clear(&mut self) {
        self.entries.clear();
        self.frames.clear();
        self.next_frame_id = 0;
    }

    /// Creates a new frame at the current stack top.
    #[inline]
    pub fn checkpoint(&mut self) -> StackFrame<T> {
        let len = self.entries.len();
        let id = self.next_frame_id;
        self.next_frame_id = self.next_frame_id.wrapping_add(1);
        self.frames.push((id, len));
        StackFrame { id, len, _marker: PhantomData }
    }

    /// Commits a frame, leaving entries appended after it in place.
    ///
    /// Panics if `frame` is not the most recent unresolved frame.
    #[inline]
    pub fn commit(&mut self, frame: StackFrame<T>) {
        self.assert_top(frame, "commit");
        self.frames.pop();
    }

    /// Rolls back a frame and returns entries appended after it in reverse order.
    ///
    /// Panics if `frame` is not the most recent unresolved frame.
    #[inline]
    pub fn rollback(&mut self, frame: StackFrame<T>) -> impl Iterator<Item = T> + '_ {
        self.assert_top(frame, "rollback");
        self.frames.pop();
        self.entries.drain(frame.len..).rev()
    }

    #[inline]
    fn assert_top(&self, frame: StackFrame<T>, op: &str) {
        let Some(&(id, len)) = self.frames.last() else {
            panic!("stack frame {op}: no active frame");
        };
        assert_eq!(
            (frame.id, frame.len),
            (id, len),
            "out-of-order stack frame {op} (expected top of stack)"
        );
        assert!(frame.len <= self.entries.len(), "stack frame {op}: frame is past stack length");
    }
}

#[cfg(test)]
mod tests {
    use super::FrameStack;

    #[test]
    fn commit_keeps_entries() {
        let mut stack = FrameStack::new();
        stack.push(1);
        let frame = stack.checkpoint();
        stack.push(2);
        stack.commit(frame);
        assert_eq!(stack.len(), 2);
    }

    #[test]
    fn rollback_drains_entries_in_reverse_order() {
        let mut stack = FrameStack::new();
        stack.push(1);
        let frame = stack.checkpoint();
        stack.push(2);
        stack.push(3);
        let drained: Vec<_> = stack.rollback(frame).collect();
        assert_eq!(drained, [3, 2]);
        assert_eq!(stack.len(), 1);
    }

    #[test]
    #[should_panic(expected = "out-of-order")]
    fn rejects_out_of_order_resolution() {
        let mut stack: FrameStack<u8> = FrameStack::new();
        let outer = stack.checkpoint();
        stack.push(1);
        let _inner = stack.checkpoint();
        stack.commit(outer);
    }
}
