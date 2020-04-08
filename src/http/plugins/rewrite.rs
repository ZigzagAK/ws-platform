/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

register_http_plugin!(Rewrite);

use crate::plugin::*;
use crate::http::*;
use crate::error::Code;

pub struct Rewrite {
}

impl Plugin for Rewrite {
    type ModuleType = HTTP;

    fn configure(&mut self) -> ActionResult {
        add_command!(Context::SERVER, "rewrite", |server: &mut ServerContext, rewrite: HttpComplexValue| {
            server.rewrite.push_back(RewriteHandler::new(move |r| -> Code {
                r.rewrite(&r.expand(&rewrite));
                Code::AGAIN
            }));

            Ok(None)
        })?;

        add_command!(Context::ROUTE, "rewrite", |route: &mut RouteContext, rewrite: HttpComplexValue| {
            route.rewrite.push_back(RewriteHandler::new(move |r| -> Code {
                r.rewrite(&r.expand(&rewrite));
                Code::AGAIN
            }));

            Ok(None)
        })?;

        add_command!(Context::ROUTE, "break", |route: &mut RouteContext| {
            route.rewrite.push_back(RewriteHandler::new(move |_| -> Code {
                Code::OK
            }));

            Ok(None)
        })?;

        Ok(Code::OK)
    }
}

impl Rewrite {
    pub fn new() -> Rewrite {
        Rewrite {}
    }
}