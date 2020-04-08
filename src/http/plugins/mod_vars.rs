/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

register_http_plugin!(ModVars);

use crate::plugin::*;
use crate::http::*;
use crate::error::Code;

pub struct ModVars
{}

impl Plugin for ModVars {
    type ModuleType = HTTP;

    fn configure(&mut self) -> ActionResult {

        fn add_vars(vars: &HttpMap, r: &mut HttpRequest) -> Code {
            r.vars_mut().batch_replace(vars);
            Code::DECLINED
        }

        // Server

        add_command!(Context::SERVER, "vars", |server: &mut ServerContext, vars: HttpMap| {
            server.setvar.push_back(SetVarHandler::new(move |r| {
                add_vars(&vars, r)
            }));

            Ok(None)
        })?;

        // Route

        add_command!(Context::ROUTE, "vars", |route: &mut RouteContext, vars: HttpMap| {
            route.setvar.push_back(SetVarHandler::new(move |r| {
                add_vars(&vars, r)
            }));

            Ok(None)
        })?;

        Ok(OK)
    }
}

impl ModVars {
    pub fn new() -> ModVars {
        ModVars {}
    }
}