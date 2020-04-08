/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

register_http_plugin!(HttpServer);

use chrono::prelude::*;
use std::sync::{ Arc, Mutex };
use std::rc::Rc;
use std::cell::RefCell;
use std::collections::{ HashMap, LinkedList };
use std::mem::take;
use std::time::Duration;

use crate::plugin::*;
use crate::config::*;
use crate::http::*;
use crate::http::http_server_core::*;
use crate::http::HttpMethod;
use crate::variable::*;

type ServerType = Rc<RefCell<HttpServerCore>>;

struct WorkgroupContext {
    name: String,
    event_pool_size: usize,
    thread_pool_size: usize,
    socket_pool_size: usize
}

impl Default for WorkgroupContext {
    fn default() -> WorkgroupContext {
        WorkgroupContext {
            name: "default".to_string(),
            event_pool_size: 1,
            thread_pool_size: 10,
            socket_pool_size: 1024
        }
    }
}

pub struct HttpServer {
    groups: Arc<Mutex<HashMap<String, Vec<ServerType>>>>
}

impl Plugin for HttpServer {
    type ModuleType = HTTP;

    fn configure(&mut self) -> ActionResult {

        let groups_ = self.groups.clone();

        // Workgroup

        add_empty_block!(Context::HTTP, "workgroups")?;

        add_block!(Context::HTTP, "workgroups.workgroup", move |context| {
            match context.get_mut::<WorkgroupContext>() {
                Some(context) => {
                    // exit
                    let mut groups = groups_.lock().unwrap();
                    let e = groups.entry(context.name.clone()).or_default();
                    for _ in 0..context.event_pool_size {
                        e.push(Rc::new(RefCell::new(HttpServerCore::new(context.thread_pool_size, context.socket_pool_size)?)))
                    }
                    Ok(None)
                },
                None =>
                    // enter
                    Ok(Some(CommandContext::new_default::<WorkgroupContext>()))
            }
        })?;

        add_command!(Context::WORKGROUP, "name", |workgroup: &mut WorkgroupContext, name: String| {
            workgroup.name = name;
            Ok(None)
        })?;

        add_command!(Context::WORKGROUP, "event_pool_size", |workgroup: &mut WorkgroupContext, event_pool_size: usize| {
            workgroup.event_pool_size = event_pool_size;
            Ok(None)
        })?;

        add_command!(Context::WORKGROUP, "thread_pool_size", |workgroup: &mut WorkgroupContext, thread_pool_size: usize| {
            workgroup.thread_pool_size = thread_pool_size;
            Ok(None)
        })?;

        add_command!(Context::WORKGROUP, "socket_pool_size", |workgroup: &mut WorkgroupContext, socket_pool_size: usize| {
            workgroup.socket_pool_size = socket_pool_size;
            Ok(None)
        })?;

        // Routes

        add_block!(Context::SERVER, "routes", |context| {
            match context.get_mut::<LinkedList<RouteContext>>() {
                Some(routes) => {
                    // exit
                    context.parent().unwrap()
                           .get_mut::<ServerContext>().unwrap()
                           .routes = Some(take(routes));
                    Ok(None)
                },
                None =>
                    // enter
                    Ok(Some(CommandContext::new_default::<LinkedList<RouteContext>>()))
            }
        })?;

        add_block!(Context::SERVER, "routes.route", |context| {
            match context.get_mut::<RouteContext>() {
                Some(route) => {
                    // exit
                    let route = take(route);
                    context.parent().unwrap()
                           .get_mut::<LinkedList<RouteContext>>().unwrap()
                           .push_back(route);
                    Ok(None)
                },
                None =>
                    // enter
                    Ok(Some(CommandContext::new_default::<RouteContext>()))
            }
        })?;

        add_command!(Context::ROUTE, "match", |route: &mut RouteContext, pattern: String| {
            route.pattern = pattern;
            Ok(None)
        })?;

        add_command!(Context::ROUTE, "method", |route: &mut RouteContext, method: String| {
            route.method = match HttpMethod::from(method) {
                HttpMethod::UNSUPPORTED => return throw!("invalid value"),
                m => Some(m)
            };
            Ok(None)
        })?;

        // Server

        add_empty_block!(Context::HTTP, "servers")?;

        add_command!(Context::SERVER, "bind", |server: &mut ServerContext, bind: String| {
            server.bind = bind;
            Ok(None)
        })?;

        add_command!(Context::SERVER, "request_timeout", |server: &mut ServerContext, request_timeout: Duration| {
            server.request_timeout = Some(request_timeout);
            Ok(None)
        })?;

        add_command!(Context::SERVER, "response_timeout", |server: &mut ServerContext, response_timeout: Duration| {
            server.response_timeout = Some(response_timeout);
            Ok(None)
        })?;

        add_command!(Context::SERVER, "keepalive_timeout", |server: &mut ServerContext, keepalive_timeout: Duration| {
            server.keepalive_timeout = Some(keepalive_timeout);
            Ok(None)
        })?;

        add_command!(Context::SERVER, "keepalive_requests", |server: &mut ServerContext, keepalive_requests: u64| {
            server.keepalive_requests = keepalive_requests;
            Ok(None)
        })?;

        add_command!(Context::SERVER, "group", |server: &mut ServerContext, workgroup: String| {
            server.workgroup = workgroup;
            Ok(None)
        })?;

        add_command!(Context::SERVER, "virtual_host", |server: &mut ServerContext, virtual_host: String| {
            server.virtual_host = Some(virtual_host);
            Ok(None)
        })?;

        let groups_ = self.groups.clone();

        add_block!(Context::HTTP, "servers.server", move |context| {
            match context.get_mut::<ServerContext>() {
                Some(context) => {
                    // exit
                    if context.bind.len() != 0 {
                        let mut guard = groups_.lock().unwrap();
                        let groups = guard.entry(context.workgroup.clone()).or_insert_with(||
                            vec![Rc::new(RefCell::new(HttpServerCore::new(10, 1024).unwrap()))]
                        );
                        for group in groups.iter() {
                            let mut group = group.borrow_mut();
                            group.add_server(&context, None)?;
                        }
                        Ok(None)
                    } else {
                        return throw!("'bind' is not defined");
                    }
                },
                None => {
                    // enter
                    let mut context = ServerContext::default();

                    context.workgroup = "default".to_string();
                    context.keepalive_requests = std::u64::MAX;
    
                    context.setvar.push_back(SetVarHandler::new(move |r| {
                        add_var_lazy!(r, "uri", |r: &HttpRequest| {
                            r.uri()
                        });
                        add_var_lazy!(r, "request_uri", |r: &HttpRequest| {
                            r.request_uri()
                        });
                        add_var_lazy!(r, "request_method", |r: &HttpRequest| {
                            r.method()
                        });
                        add_var_lazy!(r, "query_string", |r: &HttpRequest| {
                            r.query_string()
                        });
                        add_var_lazy!(r, "protocol", |r: &HttpRequest| {
                            r.protocol()
                        });
                        add_var_lazy!(r, "content-length", |r: &HttpRequest| {
                            r.content_length().unwrap_or(0)
                        });
                        add_var_lazy!(r, "local_time", |_| {
                            format!("{}", Local::now().format("%Y/%m/%d-%H:%M:%S"))
                        });
                        add_var_lazy!(r, "remote_addr", |r: &HttpRequest| {
                            r.const_context().remote_addr()
                        });
                        add_var_lazy!(r, "request_start", |r: &HttpRequest| {
                            format!("{}", r.request_start().format("%Y/%m/%d-%H:%M:%S"))
                        });
                        add_var_lazy!(r, "request_time", |r: &HttpRequest| {
                            r.request_time()
                        });
                        Code::DECLINED
                    }));
        
                    Ok(Some(CommandContext::new::<ServerContext>(context)))
                }
            }
        })?;

        Ok(OK)
    }

    fn activate(&mut self) -> ActionResult {
        Ok(DECLINED)
    }

    fn deactivate(&mut self) -> ActionResult {
        Ok(DECLINED)
    }

    fn wait(&mut self) {
        if let Ok(groups) = self.groups.lock() {
            let groups = & *groups;
            groups.iter().for_each(|(_, group)| {
                for group in group.iter() {
                    group.borrow_mut().wait()
                }
            });
        }
    }
}

impl HttpServer {
    pub fn new() -> HttpServer {
        HttpServer {
            groups: Arc::new(Mutex::new(HashMap::new()))
        }
    }
}