/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

use std::collections::{ HashMap, LinkedList };
use std::net::SocketAddr;
use std::sync::{ Arc, RwLock };

use crate::http::server::HttpServer;
use crate::http::routers::{ trie::TrieRouter, re::RegexRouter, named::NamedRouter };
use crate::error::{ Code, CoreResult, CoreError };
use crate::handler::sync::RefHandler;
use crate::http::*;

impl RouteContext {
    pub fn copy(&mut self, src: &RouteContext) -> &'_ mut RouteContext {
        self.error_log = src.error_log.clone();
        self.host = src.host.clone();
        self.pattern = src.pattern.clone();
        self.method = src.method.clone();
        self.setvar = src.setvar.clone();
        self.rewrite = src.rewrite.clone();
        self.access = src.access.clone();
        self.content = src.content.clone();
        self.flush = src.flush.clone();
        self.header_filter = src.header_filter.clone();
        self.body_filter = src.body_filter.clone();
        self.log = src.log.clone();
        self
    }
}

type HttpNamedRouter = NamedRouter<RouteContext>;
type HttpTrieRouter = TrieRouter<RouteContext>;
type HttpRegexRouter = RegexRouter<RouteContext>;

#[derive(Default)]
struct Routers {
    trie: HttpTrieRouter,
    regex: HttpRegexRouter,
    named: HttpNamedRouter
}

pub struct HttpServerCore {
    server: HttpServer,
    routes: Arc<RwLock<HashMap<(SocketAddr, String), Routers>>>,
    phase_handlers: Arc<RwLock<HashMap<(SocketAddr, String), ServerContext>>>
}

impl HttpServerCore {
    pub fn new(
        worker_pool_size: usize,
        socket_poll_size: usize,
    ) -> Result<HttpServerCore, CoreError> {
        let server = match HttpServer::new(worker_pool_size,
            socket_poll_size,
            ContentHandler::new(|r| -> HttpResponse {
                let mut resp = HttpResponse::new(r);
                resp.send(HttpStatus::NO_CONTENT, "text/plain", None);
                resp
            })
        ) {
            Ok(server) => server,
            Err(err) => return Err(err)
        };

        Ok(HttpServerCore {
            server: server,
            routes: Arc::new(RwLock::new(HashMap::new())),
            phase_handlers: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    fn phase_handler(phase_handlers: &LinkedList<RefHandler<HttpRequest, Code>>, r: &mut HttpRequest) -> Code {
        for handler in phase_handlers.iter() {
            match handler.handle(r) {
                OK => return OK,
                AGAIN => return AGAIN,
                DECLINED => { /* void */ }
            }
        }
        DECLINED
    }

    fn unauthorized() -> ContentHandler {
        ContentHandler::new(|r| -> HttpResponse {
            let mut resp = HttpResponse::new(r);
            resp.send(HttpStatus::UNAUTHORIZED, "text/plain", Some(b"Unauthorized"));
            resp
        })
    }

    pub fn add_server(
        &mut self,
        server: &ServerContext,
        handler: Option<ContentHandler>
    ) -> CoreResult {
        let addr = get_addr(&server.bind)?;
        let routes = Arc::clone(&self.routes);
        let phase_handlers = Arc::clone(&self.phase_handlers);
        let key_default = (addr, "*".to_string());
        let server_ = server.clone();

        let code = self.server.add_server_handler(addr, ContentHandler::new(move |mut r| -> HttpResponse {
            let guard = (
                &* routes.read().unwrap(),
                &* phase_handlers.read().unwrap()
            );

            let key = (addr, r.host().clone());

            let routes = match guard.0.get(&key) {
                None => match guard.0.get(&key_default) {
                    Some(routes) => Some(routes),
                    None => None
                },
                Some(routes) => Some(routes)
            };
    
            let phase_handlers = match guard.1.get(&key) {
                None => match guard.1.get(&key_default) {
                    Some(phase_handlers) => Some(phase_handlers),
                    None => None
                },
                Some(phase_handlers) => Some(phase_handlers)
            };

            loop {
                let mut found = (None, None, None);

                if let Some(routes) = routes {
                    if r.uri().starts_with("@") {
                        if let Some(route) = routes.named.get(&r) {
                            found.2 = Some(route);
                        }
                    } else if let Some(route) = routes.trie.get(&mut r) {
                        match route {
                            (route, true) => {
                                // exact
                                found.0 = Some(route);
                            },
                            (route, false) => {
                                // partial
                                match routes.regex.get(&mut r) {
                                    Some(route) => found.1 = Some(route),
                                    None => found.0 = Some(route)
                                }
                            }
                        }
                    } else {
                        if let Some(route) = routes.regex.get(&mut r) {
                            found.1 = Some(route);
                        }
                    }
                }

                let mut content_handler = None;

                match found {
                    /* (trie, regex, named) */
                    (None, Some(route), None) | (Some(route), None, None) | (None, None, Some(route)) => {
                        // phase handlers
                        let mut rc = DECLINED;
                        // rewrite
                        if let Some(phase_handlers) = phase_handlers {
                            HttpServerCore::phase_handler(&phase_handlers.setvar, &mut r);
                            rc = HttpServerCore::phase_handler(&phase_handlers.rewrite, &mut r);
                            if rc == AGAIN {
                                continue;
                            }
                        }
                        if rc == DECLINED {
                            if HttpServerCore::phase_handler(&route.context.rewrite, &mut r) == AGAIN {
                                continue;
                            }
                        }
                        // access
                        let uri = r.uri().clone();
                        if let Some(phase_handlers) = phase_handlers {
                            rc = HttpServerCore::phase_handler(&phase_handlers.access, &mut r);
                        }
                        if rc == DECLINED {
                            rc = HttpServerCore::phase_handler(&route.context.access, &mut r);
                        }
                        if rc == AGAIN {
                            if uri != *r.uri() {
                                // redirect to another route
                                continue;
                            }
                            content_handler = Some(HttpServerCore::unauthorized());
                        } else if let Some(content) = &route.content {
                            content_handler = Some(content.clone());
                        }
                        // server handlers
                        phase_handlers.map(|phase_handlers| {
                            phase_handlers.header_filter.iter().for_each(|h| r.add_header_filter(h.clone()));
                            phase_handlers.body_filter.iter().for_each(|h| r.add_body_filter(h.clone()));
                            phase_handlers.log.iter().for_each(|h| r.add_log(h.clone()));
                        });
                        // header filter handlers
                        route.header_filter.iter().for_each(|h| r.add_header_filter(h.clone()));
                        // body filter handlers
                        route.body_filter.iter().for_each(|h| r.add_body_filter(h.clone()));
                        // flush handlers
                        route.flush.iter().for_each(|h| r.add_flush(h.clone()));
                        // log handlers
                        route.context.log.iter().for_each(|h| r.add_log(h.clone()));
                        // error_log
                        match &route.error_log {
                            Some(error_log) => r.set_error_log(error_log),
                            None => if let Some(error_log) = &server_.error_log {
                                r.set_error_log(error_log)
                            }
                        }
                    },
                    (None, None, None) => {
                        if let Some(phase_handlers) = phase_handlers {
                            HttpServerCore::phase_handler(&phase_handlers.setvar, &mut r);
                            if HttpServerCore::phase_handler(&phase_handlers.rewrite, &mut r) == AGAIN {
                                continue;
                            }
                            if HttpServerCore::phase_handler(&phase_handlers.access, &mut r) == AGAIN {
                                content_handler = Some(HttpServerCore::unauthorized());
                            }
                            // server handlers
                            phase_handlers.header_filter.iter().for_each(|h| r.add_header_filter(h.clone()));
                            phase_handlers.body_filter.iter().for_each(|h| r.add_body_filter(h.clone()));
                            phase_handlers.log.iter().for_each(|h| r.add_log(h.clone()));
                            // error log
                            if let Some(error_log) = &server_.error_log {
                                r.set_error_log(error_log)
                            }
                        }
                    },
                    _ => unreachable!()
                }

                return match content_handler {
                    Some(content_handler) => {
                        drop(guard);
                        content_handler.handle(r)
                    },
                    None => match &handler {
                        Some(content_handler) => {
                            let content_handler = content_handler.clone();
                            drop(guard);
                            content_handler.handle(r)
                        },
                        None => {
                            let mut resp = HttpResponse::new(r);
                            resp.send(HttpStatus::NOT_FOUND, "text/plain", Some(b"Not found"));
                            resp
                        }
                    }
                }
            }
        }),
        server.request_timeout,
        server.response_timeout,
        server.keepalive_timeout,
        server.keepalive_requests)?;

        server.setvar.iter().for_each(|handler| {
            self.add_setvar_handler(&server.bind, server.virtual_host.clone(), handler.clone()).unwrap();
        });

        server.rewrite.iter().for_each(|handler| {
            self.add_rewrite_handler(&server.bind, server.virtual_host.clone(), handler.clone()).unwrap();
        });

        server.access.iter().for_each(|handler| {
            self.add_access_handler(&server.bind, server.virtual_host.clone(), handler.clone()).unwrap();
        });

        server.log.iter().for_each(|handler| {
            self.add_log_handler(&server.bind, server.virtual_host.clone(), handler.clone()).unwrap();
        });

        server.header_filter.iter().for_each(|handler| {
            self.add_header_filter_handler(&server.bind, server.virtual_host.clone(), handler.clone()).unwrap();
        });

        server.body_filter.iter().for_each(|handler| {
            self.add_body_filter_handler(&server.bind, server.virtual_host.clone(), handler.clone()).unwrap();
        });

        if let Some(routes) = server.routes.clone() {
            for mut route in routes {
                route.host = server.virtual_host.clone();
                self.add_route(&server.bind, &route)?;
            }
        }

        Ok(code)
    }

    pub fn add_setvar_handler(&mut self, bind: &str, host: Option<String>, handler: SetVarHandler) -> CoreResult {
        let key = (get_addr(bind)?, host.unwrap_or("*".to_string()));
        self.phase_handlers.write().unwrap().entry(key).or_default().setvar.push_back(handler);
        Ok(OK)
    }

    pub fn add_rewrite_handler(&mut self, bind: &str, host: Option<String>, handler: RewriteHandler) -> CoreResult {
        let key = (get_addr(bind)?, host.unwrap_or("*".to_string()));
        self.phase_handlers.write().unwrap().entry(key).or_default().rewrite.push_back(handler);
        Ok(OK)
    }

    pub fn add_access_handler(&mut self, bind: &str, host: Option<String>, handler: AccessHandler) -> CoreResult {
        let key = (get_addr(bind)?, host.unwrap_or("*".to_string()));
        self.phase_handlers.write().unwrap().entry(key).or_default().access.push_back(handler);
        Ok(OK)
    }

    pub fn add_log_handler(&mut self, bind: &str, host: Option<String>, handler: LogHandler) -> CoreResult {
        let key = (get_addr(bind)?, host.unwrap_or("*".to_string()));
        self.phase_handlers.write().unwrap().entry(key).or_default().log.push_back(handler);
        Ok(OK)
    }

    pub fn add_header_filter_handler(&mut self, bind: &str, host: Option<String>, handler: HeaderFilterHandler) -> CoreResult {
        let key = (get_addr(bind)?, host.unwrap_or("*".to_string()));
        self.phase_handlers.write().unwrap().entry(key).or_default().header_filter.push_back(handler);
        Ok(OK)
    }

    pub fn add_body_filter_handler(&mut self, bind: &str, host: Option<String>, handler: BodyFilterHandler) -> CoreResult {
        let key = (get_addr(bind)?, host.unwrap_or("*".to_string()));
        self.phase_handlers.write().unwrap().entry(key).or_default().body_filter.push_back(handler);
        Ok(OK)
    }

    pub fn remove_server(&mut self, bind: &str) -> CoreResult {
        let addr = get_addr(bind)?;
        self.server.remove_listener(addr);
        self.server.remove_server_handler(addr);
        Ok(OK)
    }

    pub fn remove_server_with_routes(&mut self, bind: &str, host: Option<String>) -> CoreResult {
        let key = (get_addr(bind)?, host.unwrap_or("*".to_string()));
        self.remove_server(bind)?;
        self.routes.write().unwrap().remove(&key);
        Ok(OK)
    }

    pub fn add_route(
        &mut self,
        bind: &str,
        route: &RouteContext
    ) -> CoreResult {
        let key = (get_addr(bind)?, route.host.clone().unwrap_or("*".to_string()));
        let method = get_method(route.method);
        let path = &route.pattern;
        if let Ok(ref mut routes) = self.routes.write() {
            if path.starts_with("~") {
                routes.entry(key).or_default().regex.upsert(path.trim_start_matches("~ "), method, move |context, _| {
                    context.copy(&route);
                })?;
            } else if path.starts_with("@") {
                routes.entry(key).or_default().named.upsert(&path, method, move |context, _| {
                    context.copy(&route);
                })?;
            } else if !path.is_empty() {
                routes.entry(key).or_default().trie.upsert(&path, method, move |context, _| {
                    context.copy(&route);
                })?;
            } else {
                return throw!("Pattern required");
            }
            return Ok(OK);
        }
        unreachable!()
    }

    pub fn remove_route(&mut self, bind: &str, host: Option<String>, path: &str, method: Option<HttpMethod>)
        -> Result<(), CoreError>
    {
        let method = get_method(method);
        let key = (get_addr(bind)?, host.unwrap_or("*".to_string()));
        if let Some(ref mut routes) = self.routes.write().unwrap().get_mut(&key) {
            if path.starts_with("~") {
                routes.regex.remove(path.trim_start_matches("~ "), method);
            } else if path.starts_with("@") {
                routes.named.remove(path, method);
            } else {
                routes.trie.remove(path, method);
            }
        }
        Ok(())
    }

    pub fn stop(&mut self) {
        self.server.stop();
    }

    pub fn wait(&mut self) {
        self.server.wait();
    }

}

fn get_addr(bind: &str) -> Result<SocketAddr, CoreError> {
    match bind.parse() {
        Ok(addr) => Ok(addr),
        Err(err) => {
            throw!("Failed to parse bind address: {}", err)
        }
    }
}

fn get_method(method: Option<HttpMethod>) -> Option<String> {
    match method {
        Some(method) => Some(format!("{}", method)),
        None => None
    }
}