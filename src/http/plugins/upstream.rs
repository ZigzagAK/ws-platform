/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

register_http_plugin!(Upstream);

use std::sync::{ Arc, RwLock };
use std::net::SocketAddr;
use std::collections::{ HashMap, LinkedList };
use std::time::Duration;

use crate::plugin::*;
use crate::config::*;
use crate::http::*;
use crate::error::CoreError;
use crate::upstream;
use crate::connection_pool::Peer;

#[derive(Clone)]
pub struct ServerContext {
    keepalive: usize,
    max_active: usize,
    address: Option<SocketAddr>,
    backup: bool
}

pub struct UpstreamContext {
    name: String,
    keepalive: usize,
    max_active: usize,
    keepalive_timeout: Option<Duration>,
    keepalive_requests: Option<u64>,
    servers: LinkedList<ServerContext>,
    pub balancer: Box<dyn upstream::UpstreamBalance>
}

impl Default for ServerContext {
    fn default() -> ServerContext {
        ServerContext {
            keepalive: 0,
            max_active: std::usize::MAX,
            address: None,
            backup: false
        }
    }
}

impl Default for UpstreamContext {
    fn default() -> UpstreamContext {
        UpstreamContext {
            name: String::new(),
            keepalive: 0,
            max_active: std::usize::MAX,
            keepalive_timeout: None,
            keepalive_requests: None,
            servers: LinkedList::new(),
            balancer: Box::new(upstream::RoundRobin::new())
        }
    }
}

pub struct Upstream {
    upstreams: Arc<RwLock<HashMap<String, upstream::Upstream>>>
}

impl Plugin for Upstream {
    type ModuleType = HTTP;

    fn name() -> &'static str {
        "Upstream"
    }

    fn configure(&mut self) -> ActionResult {
        add_command!(Context::UPSTREAM, "servers.server.address", |server: &mut ServerContext, address: String| {
            server.address = Some(get_addr(&address)?);
            Ok(None)
        })?;

        add_command!(Context::UPSTREAM, "servers.server.max_active", |server: &mut ServerContext, max_active: usize| {
            server.max_active = max_active;
            Ok(None)
        })?;

        add_command!(Context::UPSTREAM, "servers.server.keepalive", |server: &mut ServerContext, keepalive: usize| {
            server.keepalive = keepalive;
            Ok(None)
        })?;

        add_command!(Context::UPSTREAM, "servers.server.backup", |server: &mut ServerContext, backup: bool| {
            server.backup = backup;
            Ok(None)
        })?;

        add_block!(Context::UPSTREAM, "servers.server", |context| {
            match context.get_mut::<ServerContext>() {
                Some(server) => {
                    // exit
                    let server = server.clone();
                    context.parent().unwrap()
                           .get_mut::<UpstreamContext>().unwrap()
                           .servers.push_back(server);
                    Ok(None)
                },
                None =>
                    // enter
                    Ok(Some(CommandContext::new_default::<ServerContext>()))
            }
        })?;

        add_empty_block!(Context::UPSTREAM, "servers")?;

        add_command!(Context::UPSTREAM, "max_active", |upstream: &mut UpstreamContext, max_active: usize| {
            upstream.max_active = max_active;
            Ok(None)
        })?;

        add_command!(Context::UPSTREAM, "keepalive", |upstream: &mut UpstreamContext, keepalive: usize| {
            upstream.keepalive = keepalive;
            Ok(None)
        })?;

        add_command!(Context::UPSTREAM, "keepalive_timeout", |upstream: &mut UpstreamContext, keepalive_timeout: Duration| {
            upstream.keepalive_timeout = Some(keepalive_timeout);
            Ok(None)
        })?;

        add_command!(Context::UPSTREAM, "keepalive_requests", |upstream: &mut UpstreamContext, keepalive_requests: u64| {
            upstream.keepalive_requests = Some(keepalive_requests);
            Ok(None)
        })?;

        add_command!(Context::UPSTREAM, "name", |upstream: &mut UpstreamContext, name: String| {
            upstream.name = name;
            Ok(None)
        })?;

        let upstreams_ = self.upstreams.clone();

        add_block!(Context::HTTP, "upstreams.upstream", move |context| {
            match context.get_mut::<UpstreamContext>() {
                Some(upstream) => {
                    // exit
                    let upstream = std::mem::take(upstream);
                    let mut u = upstream::Upstream::new(upstream.balancer,
                                                        &upstream.name,
                                                        upstream.keepalive,
                                                        upstream.max_active,
                                                        None,
                                                        upstream.keepalive_timeout,
                                                        upstream.keepalive_requests);
                    for server in upstream.servers.iter() {
                        if let Some(address) = server.address {
                            if server.backup {
                                u.add_backup(address, server.keepalive, server.max_active);
                            } else {
                                u.add_primary(address, server.keepalive, server.max_active);
                            }
                        }
                    }
                    upstreams_.write().unwrap()
                              .insert(upstream.name.clone(), u);
                    Ok(None)
                },
                None =>
                    // enter
                    Ok(Some(CommandContext::new_default::<UpstreamContext>()))
            }
        })?;

        add_empty_block!(Context::HTTP, "upstreams")?;

        let upstreams_ = self.upstreams.clone();

        add_command!(Context::ROUTE, "upstream_status", move |route: &mut RouteContext| {
            let upstreams_ = upstreams_.clone();
            route.content = Some(ContentHandler::new(move |mut r| -> HttpResponse {
                match r.args_mut().exact("upstream") {
                    Some(upstream) => match upstreams_.read().unwrap().get(upstream) {
                        Some(upstream) => {
                            let mut resp = HttpResponse::new(r);
                            resp.send(HttpStatus::OK, "text/plain",
                                      Some(format!("active: {}\nidle: {}\n", upstream.active(), upstream.idle()).as_bytes()));
                            resp
                        },
                        None => {
                            let mut resp = HttpResponse::new(r);
                            resp.send(HttpStatus::NOT_FOUND, "text/plain", Some(b"upstream not found"));
                            resp
                        }
                    },
                    None => {
                        let mut resp = HttpResponse::new(r);
                        resp.send(HttpStatus::BAD_REQUEST, "text/plain", Some(b"upstream parameter requered"));
                        resp
                    }
                }
            }));

            Ok(None)
        })?;

        Ok(OK)
    }
}

impl Upstream {
    pub fn new() -> Upstream {
        Upstream {
            upstreams: Arc::new(RwLock::new(HashMap::new()))
        }
    }

    pub fn connect(&self, name: &str, timeout: Option<Duration>) -> Result<Peer, CoreError> {
        if let Some(upstream) = self.upstreams.read().unwrap().get(name) {
            return upstream.connect(timeout);
        }
        throw!("Upstream '{}' not found", name)
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
