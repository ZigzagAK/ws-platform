/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

register_http_plugin!(ModHeaders);

use crate::plugin::*;
use crate::http::*;

pub struct ModHeaders
{}

impl Plugin for ModHeaders {
    type ModuleType = HTTP;

    fn configure(&mut self) -> ActionResult {

        fn add_headers(headers: &HttpMap, resp: &mut HttpResponse) {
            headers.iter().for_each(|(key, values)| {
                values.iter().for_each(|value| {
                    resp.add_header(&key, &resp.expand(&value));
                })
            })
        }

        fn clear_headers(headers: &HttpList, resp: &mut HttpResponse) {
            headers.iter().for_each(|key| {
                resp.remove_header(&resp.expand(&key));
            })
        }

        fn set_request_headers(headers: &HttpMap, r: &mut HttpRequest) -> Code {
            headers.iter().for_each(|(key, values)| {
                values.iter().for_each(|value| {
                    let value = r.expand(&value);
                    r.headers_mut().set(&key, value);
                })
            });

            Code::DECLINED
        }

        fn clear_request_headers(headers: &HttpList, r: &mut HttpRequest) -> Code {
            headers.iter().for_each(|key| {
                let key = r.expand(&key);
                r.headers_mut().remove(&key);
            });

            Code::DECLINED
        }

        // Server

        add_command!(Context::SERVER, "add_headers", |server: &mut ServerContext, headers: HttpMap| {
            server.header_filter.push_back(HeaderFilterHandler::new(move |resp| {
                add_headers(&headers, resp);
            }));

            Ok(None)
        })?;

        add_command!(Context::SERVER, "clear_headers", |server: &mut ServerContext, headers: HttpList| {
            server.header_filter.push_back(HeaderFilterHandler::new(move |resp| {
                clear_headers(&headers, resp);
            }));

            Ok(None)
        })?;

        add_command!(Context::SERVER, "set_request_headers", |server: &mut ServerContext, headers: HttpMap| {
            server.rewrite.push_back(RewriteHandler::new(move |r| -> Code {
                set_request_headers(&headers, r)
            }));

            Ok(None)
        })?;

        add_command!(Context::SERVER, "clear_request_headers", |server: &mut ServerContext, headers: HttpList| {
            server.rewrite.push_back(RewriteHandler::new(move |r| -> Code {
                clear_request_headers(&headers, r)
            }));

            Ok(None)
        })?;

        // Route

        add_command!(Context::ROUTE, "add_headers", |route: &mut RouteContext, headers: HttpMap| {
            route.header_filter.push_back(HeaderFilterHandler::new(move |resp| {
                add_headers(&headers, resp);
            }));

            Ok(None)
        })?;

        add_command!(Context::ROUTE, "clear_headers", |server: &mut RouteContext, headers: HttpList| {
            server.header_filter.push_back(HeaderFilterHandler::new(move |resp| {
                clear_headers(&headers, resp);
            }));

            Ok(None)
        })?;

        add_command!(Context::ROUTE, "set_request_headers", |route: &mut RouteContext, headers: HttpMap| {
            route.rewrite.push_back(RewriteHandler::new(move |r| -> Code {
                set_request_headers(&headers, r)
            }));    

            Ok(None)
        })?;

        add_command!(Context::ROUTE, "clear_request_headers", |route: &mut RouteContext, headers: HttpList| {
            route.rewrite.push_back(RewriteHandler::new(move |r| -> Code {
                clear_request_headers(&headers, r)
            }));

            Ok(None)
        })?;

        Ok(OK)
    }
}

impl ModHeaders {
    pub fn new() -> ModHeaders {
        ModHeaders {}
    }
}