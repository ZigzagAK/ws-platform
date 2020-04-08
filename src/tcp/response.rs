/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

use crate::module::Response;
use crate::error::FlushResult;
use crate::client_context::ClientContext;
use crate::tcp::request::TcpRequest;

pub struct TcpResponse {
    r: TcpRequest
}

impl Response for TcpResponse {

    type Request = TcpRequest;

    fn new(r: Self::Request) -> Self {
        TcpResponse::new(r)
    }

    fn flush(&mut self) -> FlushResult {
        unimplemented!()
    }

    fn get_request(&mut self) -> &mut Self::Request {
        &mut self.r
    }

    fn close(mut self) -> ClientContext {
        std::mem::take(&mut self.r.ctx.cleanup).iter().for_each(|h| h.handle(&mut self));
        self.r.ctx.client
    }
}

impl TcpResponse {
    fn new(r: TcpRequest) -> TcpResponse {
        TcpResponse {
            r: r
        }
    }
}