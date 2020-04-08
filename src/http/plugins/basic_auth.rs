/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

register_http_plugin!(BasicAuth);

use crate::plugin::*;
use crate::http::*;
use crate::error::Code;

pub struct BasicAuth
{}

impl Plugin for BasicAuth {
    type ModuleType = HTTP;

    fn configure(&mut self) -> ActionResult {
        add_command!(Context::SERVER, "basic", |server: &mut ServerContext, basic: String| {
            server.access.push_back(AccessHandler::new(|r| -> Code {
                Code::DECLINED
            }));

            Ok(None)
        })?;

        add_command!(Context::ROUTE, "basic", |route: &mut RouteContext, basic: String| {
            route.access.push_back(AccessHandler::new(|resp| -> Code {
                Code::DECLINED
            }));

            Ok(None)
        })?;

        Ok(Code::OK)
    }
}

impl BasicAuth {
    pub fn new() -> BasicAuth {
        BasicAuth {}
    }
}