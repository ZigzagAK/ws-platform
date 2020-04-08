/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

use std::collections::LinkedList;

use crate::module::Request;
use crate::tcp::response::TcpResponse;
use crate::handler::sync::RefHandler;
use crate::error::CoreResult;
use crate::client_context::ClientContext;

pub struct TcpRequestContext {
    pub client: ClientContext,
    pub cleanup: LinkedList<RefHandler<TcpResponse, ()>>
}

pub struct TcpRequest {
    pub ctx: TcpRequestContext
}

impl Request for TcpRequest {

    fn new(ctx: ClientContext) -> Self {
        TcpRequest::new(ctx)
    }

    fn parse(&mut self) -> CoreResult {
        unimplemented!()
    }

    fn context(&mut self) -> &mut ClientContext {
        &mut self.ctx.client
    }

    fn const_context(&self) -> &ClientContext {
        &self.ctx.client
    }

    fn close(self) -> ClientContext {
        self.ctx.client
    }
}

impl TcpRequest {
    fn new(client: ClientContext) -> TcpRequest {
        TcpRequest {
            ctx: TcpRequestContext {
                client: client,
                cleanup: LinkedList::new()
            }
        }
    }
}