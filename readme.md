# Web server platform

Written in Rust.

Now is in development.

Will be dynamically configurable web server developer platform. 

**Main features:**

- YAML for configuration.
- Simple extendable with plugins.
- Script support (LUA, Python).
- Thread pools support.
- Event loop for IO operations for requests, responses and proxying.
- Upstreams support.
- Routing by:
    * Prefix tree
    * Method
    * Regexp
    * Host

And other features.

# Configuration

```
---
http:
  workgroups:
    - workgroup:
        name: default
        thread_pool_size: 10
        socket_pool_size: 4096
    - workgroup:
        name: proxy
        event_pool_size: 4
        thread_pool_size: 10
        socket_pool_size: 512
    - workgroup:
        name: app
        event_pool_size: 4
        thread_pool_size: 10
        socket_pool_size: 512
    - workgroup:
        name: group1
        thread_pool_size: 10
        socket_pool_size: 1024
    - workgroup:
        name: group2
        thread_pool_size: 10
        socket_pool_size: 1024
  upstreams:
    - upstream:
        name: u1
        max_active: 100
        keep_alive: 100
        servers:
          - server:
              address: 127.0.0.1:8081
              max_active: 100
              keep_alive: 100
          - server:
              address: 127.0.0.2:8081
              max_active: 100
              keep_alive: 100
          - server:
              address: 127.0.0.3:8081
              max_active: 100
              keep_alive: 100
              backup: true
    - upstream:
        name: nginx
        least_conn: true
        max_active: 500
        keep_alive: 500
        servers:
          - server:
              address: 127.0.0.1:6000
              max_active: 100
              keep_alive: 100
          - server:
              address: 127.0.0.2:6000
              max_active: 100
              keep_alive: 100
          - server:
              address: 127.0.0.3:6000
              max_active: 100
              keep_alive: 100
              backup: true
  servers:
    - server:
        bind: 0.0.0.0:9091
        group: proxy
        routes:
          - route:
              match: /*
              proxy: nginx
    - server:
        bind: 0.0.0.0:8000
        group: group1
        routes:
          - route:
              match: /hello
              echo: Hello from 8000/*
    - server:
        bind: 0.0.0.0:8000
        group: group1
        virtual_host: server1
        access_log: server1_access.log
        routes:
          - route:
              match: /hello
              access_log: server1_hello_access.log
              echo: Hello from 8000/server1
    - server:
        bind: 0.0.0.0:8000
        group: group1
        virtual_host: server2
        access_log: server1_access.log
        routes:
          - route:
              match: /hello
              echo: Hello from 8000/server2
    - server:
        bind: 0.0.0.0:8080
        group: app
        access_log: server8080_access.log
        routes:
          - route:
              match: /upstream/status
              upstream_status: get upstream status
          - route:
              match: '@internal'
              echo: Hello from internal!
          - route:
              match: /to_internal
              rewrite: '@internal'
          - route:
              match: ~ ^/to_internal/re
              rewrite: '@internal'
          - route:
              match: '@unauthorized'
              echo:
                text: Unauthorized
                status: 401
          - route:
              match: /unauthorized
              basic: '@unauthorized'
          - route:
              match: /unauthorized2
              basic: /unauthorized2
          - route:
              match: ~ ^/re/
              method: GET
              echo: re:GET
          - route:
              match: ~ ^/re/
              method: POST
              echo: re:POST
          - route:
              match: /ping
              rewrite: xxx
              method: PUT
              echo: echo:PUT
          - route:
              match: /ping
              access_log: ping_access.log
              method: GET
              echo: echo:GET
          - route:
              match: /ping
              method: POST
              echo: echo:POST
          - route:
              match: /lua
              lua: |
                return 'Hello from LUA!'
          - route:
              match: /api/*
              index: site
          - route:
              match: /demo/*
              index: site
    - server:
        bind: 0.0.0.0:8081
        group: group2
        routes:
          - route:
              match: /ping
              echo: echo
          - route:
              match: /python
              python: |
                import datetime, sys as s, os as o
                import math as m,time
                import numbers
                response.text = 'Hello from Python! Now is: {}'.format(datetime.datetime.now())
          - route:
              match: /python2
              python: |
                response.text = 'Hello from Python!'
          - route:
              match: /api/*
              index: site
          - route:
              match: /demo/*
              index: site
    - server:
        bind: 0.0.0.0:9090
        routes:
          - route:
              match: /*
              proxy:
                pass: 127.0.0.1:6000
                keep_alive: 500
                limit_conns: 500
    - server:
        bind: 0.0.0.0:9093
        routes:
          - route:
              match: /*
              proxy: u1
```

# Plugins examples

## Index

```rust
/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

register_http_plugin!(Index);

use chrono::Local;
use std::thread;

use crate::plugin::*;
use crate::config::*;
use crate::http::http::*;
use crate::http::http_server_core::{ RouteContentHandler, RouteContext };
use crate::http::response::HttpResponse;

pub struct Index {
}

impl Plugin for Index {
    type ModuleType = HTTP;

    fn configure(&mut self) -> ActionResult {
        self.add_command(&Context::ROUTE, "index", CommandHandler::new(
            |context: CommandContextType, root: Option<&ConfigBlock>| -> CommandResult {
                match context.borrow_mut().get_mut::<RouteContext>() {
                    Some(route) => {
                        let root = Config::get_text(root)?;
                        route.content = Some(RouteContentHandler::new(move |r| -> HttpResponse {
                            let mut resp = HttpResponse::new(r);

                            let now = Local::now();

                            let uri = format!("{}{}", root, resp.r.uri.trim_end_matches("/"));
                            let uri = match std::fs::metadata(&uri) {
                                Ok(m) => {
                                    if m.is_dir() {
                                        String::from(format!("{}/index.html", &uri))
                                    } else {
                                        uri
                                    }
                                },
                                Err(_) => uri
                            };

                            println!("{}: [{:?}] {} -> {}", now.format("%Y-%m-%d %H:%M:%S"), thread::current().id(), resp.r.uri, &uri);
                    
                            resp.send_file(&uri);
                            resp
                        }));
                
                        Ok(None)
                    },
                    None => invalid_context!()
                }
            }
        ))
    }
}

impl Index {
    pub fn new() -> Index {
        Index {}
    }
}
```

## Echo

```rust
/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

register_http_plugin!(Echo);

use crate::plugin::*;
use crate::config::*;
use crate::http::http::*;
use crate::http::http_server_core::{ RouteContentHandler, RouteContext };
use crate::http::response::{ HttpResponse, HttpStatus };
use crate::error::Code::*;

#[derive(Default, Clone)]
pub struct EchoContext {
    status: Option<HttpStatus>,
    text: Option<String>
}

pub struct Echo {
}

impl Plugin for Echo {
    type ModuleType = HTTP;

    fn configure(&mut self) -> ActionResult {

        self.add_command(&Context::ROUTE, "echo.text", CommandHandler::new(
            |context: CommandContextType, text: Option<&ConfigBlock>| -> CommandResult {
                match context.borrow_mut().get_mut::<EchoContext>() {
                    Some(echo) => {
                        let text = Config::get_text(text)?;
                        echo.text = Some(text);
                        Ok(None)
                    },
                    None => invalid_context!()
                }
            }
        ))?;

        self.add_command(&Context::ROUTE, "echo.status", CommandHandler::new(
            |context: CommandContextType, status: Option<&ConfigBlock>| -> CommandResult {
                match context.borrow_mut().get_mut::<EchoContext>() {
                    Some(echo) => {
                        let status = Config::get_i64(status)?;
                        echo.status = Some(HttpStatus::from(status));
                        Ok(None)
                    },
                    None => invalid_context!()
                }
            }
        ))?;

        self.add_command(&Context::ROUTE, "echo", CommandHandler::new(
            |context: CommandContextType, text: Option<&ConfigBlock>| -> CommandResult {
                let mut context_ = context.borrow_mut();
                match context_.get_mut::<EchoContext>() {
                    Some(echo) => {
                        // exit
                        let echo = std::mem::take(echo);
                        context_.parent().unwrap()
                                .get_mut::<RouteContext>().unwrap()
                                .content = Some(RouteContentHandler::new(move |r| -> HttpResponse {
                                    let mut resp = HttpResponse::new(r);
                                    resp.send(echo.status.unwrap_or(HttpStatus::OK), "text/plain", Some(echo.text.as_ref().unwrap().as_bytes()));
                                    resp
                                }));
                        Ok(None)
                    },
                    None => {
                        // enter
                        let mut echo = EchoContext::default();
                        echo.text = Some(Config::get_text(text).unwrap_or(String::new()));
                        Ok(Some(CommandContext::new(echo, &context)))
                    }
                }
            }
        ))?;

        Ok(OK)
    }
}

impl Echo {
    pub fn new() -> Echo {
        Echo {}
    }
}
```

## Simple least connection balancer

```rust
/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

register_http_plugin!(LeastConn);

use std::collections::hash_map::Iter;
use std::net::SocketAddr;

use crate::plugin::*;
use crate::config::*;
use crate::http::http::*;
use crate::error::Code::*;
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

        self.add_command(&Context::UPSTREAM, "least_conn", CommandHandler::new(
            |context: CommandContextType, flag: Option<&ConfigBlock>| -> CommandResult {
                match context.borrow_mut().get_mut::<UpstreamContext>() {
                    Some(upstream) => {
                        if Config::get_bool(flag)? {
                            upstream.balancer = Box::new(BalanceLeastConn::default());
                        }
                        Ok(None)
                    },
                    None => invalid_context!()
                }
            }
        ))?;

        Ok(OK)
    }
}

impl LeastConn {
    pub fn new() -> LeastConn {
        LeastConn {}
    }
}
```

## Async JOB

```rust
/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

register_http_plugin!(AsyncTask);

use std::{ thread, thread::JoinHandle };
use std::time::Duration;

use crate::plugin::*;
use crate::http::http::*;
use crate::error::Code::*;

pub struct AsyncTask {
    thr: Option<JoinHandle<()>>
}

impl Plugin for AsyncTask {
    type ModuleType = HTTP;

    fn configure(&mut self) -> ActionResult {
        Ok(OK)
    }

    fn activate(&mut self) -> ActionResult {
        self.thr = Some(thread::spawn(|| {
            for i in 0..10 {
                println!("{}", i);
                thread::sleep(Duration::from_millis(100));
            }
        }));
        Ok(OK)
    }

    fn deactivate(&mut self) -> ActionResult {
        Ok(DECLINED)
    }

    fn wait(&mut self) {
        if let Some(thr) = self.thr.take() {
            thr.join().unwrap();
        }
    }
}

impl AsyncTask {
    pub fn new() -> AsyncTask {
        AsyncTask {
            thr: None
        }
    }
}
```