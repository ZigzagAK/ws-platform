/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

register_http_plugin!(BodyLogger);

use crate::plugin::*;
use crate::http::*;

pub struct BodyLogger
{}

impl Plugin for BodyLogger {
    type ModuleType = HTTP;

    fn configure(&mut self) -> ActionResult {

        // Server

        add_command!(Context::SERVER, "body_log", |server: &mut ServerContext| {
            server.body_filter.push_back(BodyFilterHandler::new(|body| {
                if let Some(body) = &body {
                    println!("{:?}", body)
                }
                body
            }));

            Ok(None)
        })?;

        // Route

        add_command!(Context::ROUTE, "body_log", |route: &mut RouteContext| {
            route.body_filter.push_back(BodyFilterHandler::new(|body| {
                if let Some(body) = &body {
                    println!("{:?}", body)
                }
                body
            }));

            Ok(None)
        })?;

        Ok(OK)
    }
}

impl BodyLogger {
    pub fn new() -> BodyLogger {
        BodyLogger {}
    }
}