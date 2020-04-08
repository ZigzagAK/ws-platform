/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

register_http_plugin!(ModArgs);

use crate::plugin::*;
use crate::http::*;
use crate::error::Code;

pub struct ModArgs
{}

impl Plugin for ModArgs {
    type ModuleType = HTTP;

    fn configure(&mut self) -> ActionResult {

        fn add_args(args: &HttpMap, r: &mut HttpRequest) -> Code {
            args.iter().for_each(|(key, values)| {
                values.iter().for_each(|value| {
                    let value = r.expand(&value);
                    r.args_mut().add(&key, value);
                });
            });
            Code::DECLINED
        }

        fn clear_args(args: &HttpList, r: &mut HttpRequest) -> Code {
            args.iter().for_each(|key| {
                let key = r.expand(&key);
                r.args_mut().remove(&key);
            });
            Code::DECLINED
        }

        // Server

        add_command!(Context::SERVER, "add_args", |server: &mut ServerContext, args: HttpMap| {
            server.rewrite.push_back(RewriteHandler::new(move |r| {
                add_args(&args, r)
            }));

            Ok(None)
        })?;

        add_command!(Context::SERVER, "clear_args", |server: &mut ServerContext, args: HttpList| {
            server.rewrite.push_back(RewriteHandler::new(move |r| {
                clear_args(&args, r)
            }));

            Ok(None)
        })?;

        // Route

        add_command!(Context::ROUTE, "add_args", |route: &mut RouteContext, args: HttpMap| {
            route.rewrite.push_back(RewriteHandler::new(move |r| {
                add_args(&args, r)
            }));

            Ok(None)
        })?;

        add_command!(Context::ROUTE, "clear_args", |route: &mut RouteContext, args: HttpList| {
            route.rewrite.push_back(RewriteHandler::new(move |r| {
                clear_args(&args, r)
            }));

            Ok(None)
        })?;

        Ok(OK)
    }
}

impl ModArgs {
    pub fn new() -> ModArgs {
        ModArgs {}
    }
}