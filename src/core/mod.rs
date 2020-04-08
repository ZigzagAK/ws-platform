/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

use std::ops::Deref;
use std::time::Duration;
use uuid::Uuid;

use crate::module::*;
use crate::client_context::ClientContext;
use crate::error::{ CoreResult, FlushResult };

pub struct NoRequest;

impl Request for NoRequest {

    fn new(_: ClientContext) -> Self {
        unimplemented!();
    }

    fn parse(&mut self) -> CoreResult {
        unimplemented!()
    }

    fn context(&mut self) -> &mut ClientContext {
        unimplemented!();
    }

    fn const_context(&self) -> &ClientContext {
        unimplemented!();
    }

    fn close(self) -> ClientContext {
        unimplemented!();
    }
}

pub struct NoResponse;

impl Response for NoResponse {

    type Request = NoRequest;

    fn new(_: Self::Request) -> Self {
        unimplemented!()
    }

    fn flush(&mut self) -> FlushResult {
        unimplemented!()
    }

    fn get_request(&mut self) -> &mut Self::Request {
        unimplemented!()
    }

    fn close(self) -> ClientContext {
        unimplemented!()
    }
}

#[derive(Clone, Default)]
pub struct MainContext {
    error_log: Option<String>
}

pub struct Core {}

impl ModuleType for Core {
    type Request = NoRequest;
    type Response = NoResponse;
    fn name() -> &'static str {
        "core"
    }
}

pub type CoreModule = GenericModule<Core>;

pub enum Context {
    MAIN
}

impl Deref for Context {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        match self {
            Context::MAIN => "root"
        }
    }
}

macro_rules! register_core_plugin {
    ($name:ident) => {
        register_plugin!(CoreModule, $name);
    }
}

#[derive(Clone)]
pub (crate) struct Options {
    pub request_timeout: Option<Duration>,
    pub response_timeout: Option<Duration>,
    pub keepalive_timeout: Option<Duration>,
    pub keepalive_requests: u64
}

impl Default for Options {
    fn default() -> Options {
        Options {
            request_timeout: None,
            response_timeout: None,
            keepalive_timeout: None,
            keepalive_requests: std::u64::MAX
        }
    }
}

pub (crate) struct State {
    opts: Options,
    requests: u64,
    request_id: Uuid
}

pub mod plugins;
mod io;
mod worker;
pub (crate) mod server;

pub type ErrorLog = plugins::error_log::ErrorLog;