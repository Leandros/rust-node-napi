use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossbeam_channel::unbounded;
use napi::bindgen_prelude::*;
use napi_derive::napi;
use once_cell::sync::Lazy;

type BoxedFunc = Box<dyn FnMut() -> Result<()> + Send>;

#[derive(Debug)]
struct ThreadPool {
    tx: crossbeam_channel::Sender<BoxedFunc>,
    rx: crossbeam_channel::Receiver<BoxedFunc>,
    stop: Arc<AtomicBool>,
    threads: Vec<Option<JoinHandle<()>>>,
}

impl ThreadPool {
    fn new() -> Self {
        let (tx, rx) = unbounded::<BoxedFunc>();
        ThreadPool {
            tx,
            rx,
            stop: Arc::new(AtomicBool::new(false)),
            threads: Vec::new(),
        }
    }

    fn spawn(&mut self, n: u32) -> bool {
        for i in 0..n {
            // println!("starting thread #{}", i);
            self.threads.push(Some(thread::spawn(worker_thread)));
        }

        true
    }

    fn push_task(&self, f: BoxedFunc) {
        self.tx.send(f).unwrap();
    }

    fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    fn join(&mut self) {
        for handle in self.threads.iter_mut() {
            handle.take().map(JoinHandle::join);
        }
    }
}

static INSTANCE: Lazy<RwLock<ThreadPool>> = Lazy::new(|| {
    let pool = ThreadPool::new();
    RwLock::new(pool)
});

#[napi]
pub fn spawn_threads(n: u32) -> bool {
    let mut pool = INSTANCE.write().expect("acquiring writer access");
    pool.spawn(n)
}

#[napi]
pub fn stop() {
    let mut pool = INSTANCE.write().expect("acquiring writer access");
    pool.stop();
}

#[napi]
pub fn join() {
    let mut pool = INSTANCE.write().expect("acquiring writer access");
    pool.join();
}

#[napi]
pub fn push_task<F>(f: F)
where
    F: FnMut() -> Result<()>,
{
    let pool = INSTANCE.read().expect("acquiring reader access");
    unsafe {
        let boxed = Box::new(f);
        let boxed_send = std::mem::transmute::<
            Box<dyn FnMut() -> Result<()>>,
            Box<dyn FnMut() -> Result<()> + Send>,
        >(boxed);
        pool.push_task(boxed_send);
    }
}

fn worker_thread() {
    let (stop_arc, rx) = {
        let pool = INSTANCE.read().expect("acquiring reader");
        (pool.stop.clone(), pool.rx.clone())
    };

    loop {
        if stop_arc.load(Ordering::Relaxed) {
            // println!("stopping thread ...");
            return;
        }

        if let Ok(ref mut f) = rx.recv_timeout(Duration::from_millis(200)) {
            println!("got task");
            // let _ = f();
        }
    }
}
