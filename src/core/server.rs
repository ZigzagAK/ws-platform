/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{ Arc, RwLock };

use crate::error::{ Code::*, CoreResult, CoreError };
use crate::core::{ Options, io::IO };
use crate::module::{ ModuleType, Request };
use crate::handler::sync::Handler;

pub (crate) struct Server<T: ModuleType + 'static> {
    io: IO,
    handlers: Arc<RwLock<HashMap<SocketAddr, Handler<T::Request, T::Response>>>>
}

impl<T: ModuleType> Server<T> {
    pub fn new(
        worker_pool_size: usize,
        socket_poll_size: usize,
        default_handler: Handler<T::Request, T::Response>
    )
        -> Result<Server<T>, CoreError>
    {
        let handlers = Arc::new(RwLock::new(HashMap::new()));
        let handlers_ = Arc::clone(&handlers);

        match IO::new::<T, _>(
            worker_pool_size,
            socket_poll_size,
            move |r: T::Request| -> T::Response {
                Server::<T>::handler(&handlers.read().unwrap(), &default_handler, r)
            }
        ) {
            Ok(core) => Ok(Server {
                io: core,
                handlers: handlers_
            }),
            Err(err) => Err(err)
        }
    }

    fn handler(
        handlers: &HashMap<SocketAddr, Handler<T::Request, T::Response>>,
        default: &Handler<T::Request, T::Response>,
        mut r: T::Request
    ) -> T::Response {
        match handlers.get(&r.context().server_addr) {
            Some(handler) => handler.handle(r),
            None => default.handle(r)
        }
    }

    pub fn add_listener(
        &mut self,
        addr: SocketAddr,
        opts: Option<Options>
    ) -> CoreResult {
        self.io.add_listener(addr, opts)
    }

    pub fn remove_listener(&mut self, addr: SocketAddr) {
        self.io.remove_listener(addr)
    }

    pub fn add_server_handler(
        &mut self,
        addr: SocketAddr, 
        handler: Handler<T::Request, T::Response>,
        opts: Option<Options>
    ) -> CoreResult {
        self.add_listener(addr, opts)?;
        self.handlers.write().unwrap().insert(addr, handler);
        Ok(OK)
    }

    pub fn remove_server_handler(&mut self, addr: SocketAddr) {
        self.handlers.write().unwrap().remove(&addr);
    }

    pub fn stop(&mut self) {
        self.io.stop();
    }

    pub fn wait(&mut self) {
        self.io.wait();
    }
}