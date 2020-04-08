/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

use std::ops::Deref;

use crate::module::*;
use crate::tcp::request::TcpRequest;
use crate::tcp::response::TcpResponse;

pub struct TCP {}

impl ModuleType for TCP {
    type Request = TcpRequest;
    type Response = TcpResponse;
    fn name() -> &'static str {
        "tcp"
    }
}

pub type TcpModule = GenericModule<TCP>;

pub enum Context {
    MAIN,
    TCP,
    WORKGROUP,
    SERVER,
    UPSTREAM
}

impl Deref for Context {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        match self {
            Context::MAIN => "root",
            Context::TCP => "root.tcp",
            Context::WORKGROUP => "root.tcp.workgroups.workgroup",
            Context::SERVER  => "root.tcp.servers.server",
            Context::UPSTREAM => "root.tcp.upstreams.upstream"
        }
    }
}

macro_rules! register_tcp_plugin {
    ($name:ident) => {
        register_plugin!(TcpModule, $name);
    }
}