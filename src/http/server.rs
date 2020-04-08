/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

use std::net::SocketAddr;
use std::time::Duration;

use crate::core::Options;
use crate::core::server::Server;
use crate::module::*;
use crate::http::*;
use crate::error::{ CoreResult, CoreError };
use crate::http::{ HttpStatus, ContentHandler };

pub struct HttpServer {
    server: Server::<HttpServer>
}

impl ModuleType for HttpServer {
    type Request = HttpRequest;
    type Response = HttpResponse;
}

impl HttpServer {
    pub fn new(
        worker_pool_size: usize,
        socket_poll_size: usize,
        default_handler: ContentHandler
    )
        -> Result<HttpServer, CoreError>
    {
        match Server::<HttpServer>::new(
            worker_pool_size,
            socket_poll_size,
            ContentHandler::new(move |request| -> HttpResponse {
                if !request.is_mailformed() {
                    return default_handler.handle(request);
                };
                let mut bad_request = HttpResponse::new(request);
                bad_request.send(HttpStatus::BAD_REQUEST, "text/plain", Some(b"Bad request"));
                bad_request
            })
        ) {
            Ok(server) => {
                Ok(HttpServer {
                    server: server
                })
            },
            Err(err) => Err(err)
        }
    }

    pub fn add_listener(
        &mut self,
        addr: SocketAddr,
        request_timeout: Option<Duration>,
        response_timeout: Option<Duration>,
        keepalive_timeout: Option<Duration>,
        keepalive_requests: u64
    ) -> CoreResult {
        self.server.add_listener(addr, Some(Options {
            request_timeout: request_timeout,
            response_timeout: response_timeout,
            keepalive_timeout: keepalive_timeout,
            keepalive_requests: keepalive_requests
        }))
    }

    pub fn remove_listener(&mut self, addr: SocketAddr) {
        self.server.remove_listener(addr)
    }

    pub fn add_server_handler(
        &mut self,
        addr: SocketAddr,
        handler: ContentHandler,
        request_timeout: Option<Duration>,
        response_timeout: Option<Duration>,
        keepalive_timeout: Option<Duration>,
        keepalive_requests: u64
    ) -> CoreResult {
        self.server.add_server_handler(addr, ContentHandler::new(move |request| -> HttpResponse {
            if !request.is_mailformed() {
                return handler.handle(request);
            };
            let mut bad_request = HttpResponse::new(request);
            bad_request.send(HttpStatus::BAD_REQUEST, "text/plain", Some(b"Bad request"));
            bad_request
        }), Some(Options {
            request_timeout: request_timeout,
            response_timeout: response_timeout,
            keepalive_timeout: keepalive_timeout,
            keepalive_requests: keepalive_requests
        }))
    }

    pub fn remove_server_handler(&mut self, addr: SocketAddr) {
        self.server.remove_server_handler(addr)
    }

    pub fn stop(&mut self) {
        self.server.stop();
    }

    pub fn wait(&mut self) {
        self.server.wait();
    }
}