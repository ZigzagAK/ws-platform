/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

use std::net::SocketAddr;
use std::sync::{ Arc, RwLock, atomic::{ AtomicUsize, Ordering } };
use std::collections::{ HashMap, hash_map::Iter };
use std::time::Duration;
use std::cmp::min;

use crate::connection_pool::*;
use crate::error::CoreError;

pub trait UpstreamBalance: Send + Sync {
    fn balance(&self, iter: Iter<SocketAddr, ConnectionPool>) -> Option<SocketAddr>;
}

pub struct RoundRobin {
    index: AtomicUsize
}

impl RoundRobin {
    pub fn new() -> RoundRobin {
        RoundRobin {
            index: AtomicUsize::new(0)
        }
    }
}

impl UpstreamBalance for RoundRobin {
    fn balance(&self, mut iter: Iter<SocketAddr, ConnectionPool>) -> Option<SocketAddr> {
        match iter.nth(self.index.fetch_add(1, Ordering::SeqCst) % iter.len()) {
            Some((addr, _)) => Some(*addr),
            None => unreachable!()
        }
    }
}

pub struct Upstream {
    name: String,
    max_keepalive: usize,
    max_active: usize,
    timeout: Option<Duration>,
    keepalive_timeout: Option<Duration>,
    keepalive_requests: Option<u64>,
    active: Arc<usize>,
    servers: RwLock<[HashMap<SocketAddr, ConnectionPool>; 2]>,
    balancer: Box<dyn UpstreamBalance>
}

impl Upstream {
    pub fn new(
        balancer: Box<dyn UpstreamBalance>,
        name: &str,
        max_keepalive: usize,
        max_active: usize,
        timeout: Option<Duration>,
        keepalive_timeout: Option<Duration>,
        keepalive_requests: Option<u64>
    ) -> Upstream {
        Upstream {
            max_keepalive: max_keepalive,
            max_active: max_active,
            timeout: timeout,
            keepalive_timeout: keepalive_timeout,
            keepalive_requests: keepalive_requests,
            name: name.to_string(),
            servers: RwLock::new([HashMap::new(), HashMap::new()]),
            active: Arc::new(0),
            balancer: balancer
        }
    }

    pub fn add_primary(&mut self, addr: SocketAddr, max_keepalive: usize, max_active: usize) {
        self.servers.write().unwrap()[0]
            .insert(addr,
                    ConnectionPool::with_timeouts(
                        &self.name,
                        min(max_keepalive, self.max_keepalive),
                        min(max_active, self.max_active),
                        self.timeout,
                        self.keepalive_timeout,
                        self.keepalive_requests
                    ));
    }

    pub fn add_backup(&mut self, addr: SocketAddr, max_keepalive: usize, max_active: usize) {
        self.servers.write().unwrap()[1]
            .insert(addr,
                    ConnectionPool::with_timeouts(
                        &self.name,
                        min(max_keepalive, self.max_keepalive),
                        min(max_active, self.max_active),
                        self.timeout,
                        self.keepalive_timeout,
                        self.keepalive_requests
                    ));
    }

    pub fn connect(&self, timeout: Option<Duration>) -> Result<Peer, CoreError> {
        let userdata = Box::new(Arc::clone(&self.active));

        if self.active() == self.max_active {
            return throw!("Bad gateway");
        }

        let servers = self.servers.read().unwrap();

        for i in 0..1 {
            for _ in 0..servers[i].len() {
                match self.balancer.balance(servers[i].iter()) {
                    Some(addr) => {
                        match servers[i].get(&addr) {
                            Some(pool) => {
                                if let Ok(mut peer) = pool.connect(&addr, timeout) {
                                    peer.attach_userdata(userdata);
                                    return Ok(peer);
                                }
                            },
                            None => {
                                log_error!("error", "Can't find '{}' in upstream '{}'", addr, self.name);
                                break;
                            }
                        }
                    },
                    None => break
                }
            }
        }

        throw!("Bad gateway")
    }

    pub fn active(&self) -> usize {
        min(self.max_active, Arc::strong_count(&self.active) - 1)
    }

    pub fn idle(&self) -> usize {
        let servers = self.servers.read().unwrap();
        let mut count = 0;
        for i in 0..1 {
            for server in servers[i].values() {
                count += server.idle()
            }
        }
        count
    }
}