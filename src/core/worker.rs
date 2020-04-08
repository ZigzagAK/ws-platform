/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

use std::{ thread, thread::JoinHandle };
use std::sync::atomic::{ AtomicBool, Ordering };
use std::sync::{ mpsc, mpsc::Sender, Arc, Mutex };
use std::time::Duration;

use crate::module::*;
use crate::error::{ Code::*, CoreResult };

struct Worker {
    thr: Option<JoinHandle<()>>,
    stop: Arc<AtomicBool>
}

pub struct ThreadPool<T: ModuleType + 'static, F: 'static>
where
    F: Fn(T::Request) + Clone + Sync + Send
{
    tx: Sender<T::Request>,
    workers: Vec<Worker>,
    handler: Option<F>
}

impl Worker {
    pub fn new<F: 'static, T: 'static>(
        rx: Arc<Mutex<mpsc::Receiver<T::Request>>>,
        handler: F
    ) -> Worker
    where
        F: Fn(T::Request) + Sync + Send,
        T: ModuleType
    {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_flag = stop.clone();
        let thr = thread::Builder::new().name("ws: worker".to_string()).spawn(move || loop {
            let msg = rx.lock().unwrap().recv_timeout(Duration::from_secs(1));
            match msg {
                Ok(r) => {
                    handler(r);
                },
                Err(mpsc::RecvTimeoutError::Timeout) if stop_flag.load(Ordering::Relaxed) => {
                    break;
                },
                Err(mpsc::RecvTimeoutError::Timeout) => {},
                Err(err) => {
                    log_error!("error", "Failed to recv from channel: {:?}", err);
                }
            }
        }).unwrap();
        Worker {
            thr: Some(thr),
            stop: stop
        }
    }

    fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    fn wait(&mut self) {
        self.thr.take().map(|thr| thr.join());
    }
}

impl<T: ModuleType, F: 'static> ThreadPool<T, F>
where
    F: Fn(T::Request) + Clone + Sync + Send
{
    pub fn new(
        size: usize,
        handler: F
    ) -> ThreadPool<T, F> {
        let (tx, rx) = mpsc::channel();
        let rx = Arc::new(Mutex::new(rx));
        ThreadPool {
            tx: tx,
            handler: match size {
                0 => Some(handler.clone()),
                _ => None
            },
            workers: (0..size).map(|_| Worker::new::<_ ,T>(Arc::clone(&rx), handler.clone())).collect()
        }
    }

    pub fn post(&self, r: T::Request) -> CoreResult {
        match &self.handler {
            None => {
                match self.tx.send(r) {
                    Ok(()) => Ok(OK),
                    Err(_) => throw!("Failed to post task")
                }
            },
            Some(handler) => {
                (handler)(r);
                Ok(OK)
            }
        }
    }

    pub fn stop(&mut self) {
        (&mut self.workers).into_iter().for_each(|w| w.stop());
    }

    pub fn wait(&mut self) {
        (&mut self.workers).into_iter().for_each(|w| w.wait());
    }
}