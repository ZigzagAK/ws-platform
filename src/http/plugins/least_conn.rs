/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

register_http_plugin!(LeastConn);

use std::collections::hash_map::Iter;
use std::net::SocketAddr;

use crate::plugin::*;
use crate::http::*;
use crate::http::plugins::upstream::UpstreamContext;
use crate::connection_pool::ConnectionPool;
use crate::upstream::UpstreamBalance;

#[derive(Default)]
pub struct BalanceLeastConn {}

impl UpstreamBalance for BalanceLeastConn {
    fn balance(&self, iter: Iter<SocketAddr, ConnectionPool>) -> Option<SocketAddr> {
        let mut best = (std::usize::MAX, None);
        for (addr, pool) in iter {
            let active = pool.active();
            if active < best.0 {
                best.0 = active;
                best.1 = Some(*addr);
            }
        }
        best.1
    }
}

pub struct LeastConn {
}

impl Plugin for LeastConn {
    type ModuleType = HTTP;

    fn name() -> &'static str {
        "LeastConn"
    }

    fn configure(&mut self) -> ActionResult {

        add_command!(Context::UPSTREAM, "least_conn", |upstream: &mut UpstreamContext, enabled: bool| {
            if enabled {
                upstream.balancer = Box::new(BalanceLeastConn::default());
            }

            Ok(None)
        })
    }
}

impl LeastConn {
    pub fn new() -> LeastConn {
        LeastConn {}
    }
}