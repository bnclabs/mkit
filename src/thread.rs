//! Module `thread` implement a generic multi-threading pattern.
//!
//! It is inspired from gen-server model from Erlang, where by
//! every thread is expected hold onto its own state, and handle all
//! inter-thread communication via channels and message queues.

use log::debug;

#[allow(unused_imports)]
use std::{
    mem,
    sync::{mpsc, Arc},
    thread,
};

use crate::{Error, Result};

/// IPC type, that enumerates as either [std::sync::mpsc::Sender] or,
/// [std::sync::mpsc::SyncSender] channel.
///
/// The clone behavior is similar to [std::sync::mpsc::Sender] or,
/// [std::sync::mpsc::Sender].
pub enum Tx<Q, R> {
    N(mpsc::Sender<(Q, Option<mpsc::Sender<R>>)>),
    S(mpsc::SyncSender<(Q, Option<mpsc::Sender<R>>)>),
}

impl<Q, R> Clone for Tx<Q, R> {
    fn clone(&self) -> Self {
        match self {
            Tx::N(tx) => Tx::N(tx.clone()),
            Tx::S(tx) => Tx::S(tx.clone()),
        }
    }
}

impl<Q, R> Tx<Q, R> {
    /// Post a message to thread and don't wait for response.
    pub fn post(&self, msg: Q) -> Result<()> {
        match self {
            Tx::N(tx) => err_at!(IPCFail, tx.send((msg, None)))?,
            Tx::S(tx) => err_at!(IPCFail, tx.send((msg, None)))?,
        };
        Ok(())
    }

    /// Send a request message to thread and wait for a response.
    pub fn request(&self, request: Q) -> Result<R> {
        let (stx, srx) = mpsc::channel();
        match self {
            Tx::N(tx) => err_at!(IPCFail, tx.send((request, Some(stx))))?,
            Tx::S(tx) => err_at!(IPCFail, tx.send((request, Some(stx))))?,
        }
        Ok(err_at!(IPCFail, srx.recv())?)
    }
}

/// IPC type, that shall be passed to the thread's main loop.
///
/// Refer to [Thread::new] for details.
pub type Rx<Q, R> = mpsc::Receiver<(Q, Option<mpsc::Sender<R>>)>;

/// Thread type, providing gen-server pattern to do multi-threading.
///
/// When a thread value is dropped, it is made sure that there are
/// no dangling thread routines. To acheive this following requirements
/// need to be satisfied:
///
/// * All [Tx] clones on this thread should be dropped.
/// * The thread's main loop should handle _disconnect_ signal on its
///   [Rx] channel.
pub struct Thread<Q, R, T> {
    name: String,
    inner: Option<Inner<Q, R, T>>,
}

struct Inner<Q, R, T> {
    tx: Tx<Q, R>,
    handle: thread::JoinHandle<T>,
}

impl<Q, R, T> Inner<Q, R, T> {
    fn close_wait(self) -> Result<T> {
        mem::drop(self.tx); // drop input channel to thread.
        match self.handle.join() {
            Ok(val) => Ok(val),
            Err(err) => err_at!(ThreadFail, msg: "fail {:?}", err),
        }
    }
}

impl<Q, R, T> Drop for Thread<Q, R, T> {
    fn drop(&mut self) {
        match self.inner.take() {
            Some(inner) => {
                inner.close_wait().ok();
            }
            None => (),
        }
        debug!(target: "thread", "dropped thread `{}`", self.name);
    }
}

impl<Q, R, T> Thread<Q, R, T> {
    /// Create a new Thread instance, using asynchronous channel with
    /// infinite buffer. `main_loop` shall be called with the rx side
    /// of the channel and shall return a function that can be spawned
    /// using thread::spawn.
    pub fn new<F, N>(name: &str, main_loop: F) -> Thread<Q, R, T>
    where
        F: 'static + FnOnce(Rx<Q, R>) -> N + Send,
        N: 'static + Send + FnOnce() -> T,
        T: 'static + Send,
    {
        let (tx, rx) = mpsc::channel();
        let handle = thread::spawn(main_loop(rx));

        debug!(target: "thread", "{} spawned in async mode", name);

        Thread {
            name: name.to_string(),
            inner: Some(Inner {
                tx: Tx::N(tx),
                handle,
            }),
        }
    }

    /// Create a new Thread instance, using synchronous channel with
    /// finite buffer.
    pub fn new_sync<F, N>(name: &str, channel_size: usize, main_loop: F) -> Thread<Q, R, T>
    where
        F: 'static + FnOnce(Rx<Q, R>) -> N + Send,
        N: 'static + Send + FnOnce() -> T,
        T: 'static + Send,
    {
        let (tx, rx) = mpsc::sync_channel(channel_size);
        let handle = thread::spawn(main_loop(rx));

        debug!(target: "thread", "{} spawned in sync mode", name);

        Thread {
            name: name.to_string(),
            inner: Some(Inner {
                tx: Tx::S(tx),
                handle,
            }),
        }
    }

    /// Clone the a IPC sender to communicate with this thread.
    pub fn clone_tx(&self) -> Tx<Q, R> {
        self.inner.as_ref().map(|x| x.tx.clone()).unwrap()
    }

    /// Recommended way to exit/shutdown the thread. Note that all [Tx]
    /// clones of this thread must also be dropped for this call to return.
    ///
    /// Even otherwise, when Thread value goes out of scope its drop
    /// implementation shall call this method to exit the thread, except
    /// that any errors are ignored.
    pub fn close_wait(mut self) -> Result<T> {
        self.inner.take().unwrap().close_wait()
    }
}
