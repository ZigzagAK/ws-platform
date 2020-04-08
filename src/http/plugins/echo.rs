/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

register_http_plugin!(Echo);

use crate::plugin::*;
use crate::config::*;
use crate::http::*;

#[derive(Default, Clone)]
pub struct EchoContext {
    status: Option<HttpStatus>,
    text: Option<HttpComplexValue>
}

pub struct Echo
{}

impl Plugin for Echo {
    type ModuleType = HTTP;

    fn configure(&mut self) -> ActionResult {

        add_command!(Context::ROUTE, "echo.text", |echo: &mut EchoContext, cv: HttpComplexValue| {
            echo.text = Some(cv);
            Ok(None)
        })?;

        add_command!(Context::ROUTE, "echo.status", |echo: &mut EchoContext, status: i64| {
            echo.status = Some(HttpStatus::from(status));
            Ok(None)
        })?;

        add_block!(Context::ROUTE, "echo", move |context, cv: HttpComplexValue| {
            match context.get_mut::<EchoContext>() {
                Some(echo) => {
                    // exit
                    let echo = std::mem::take(echo);
                    context.parent().unwrap()
                           .get_mut::<RouteContext>().unwrap()
                           .content = Some(ContentHandler::new(move |r| -> HttpResponse {
                               let text = r.expand(&echo.text.as_ref().unwrap());
                               let mut resp = HttpResponse::new(r);
                               resp.send(echo.status.unwrap_or(HttpStatus::OK),
                                         "text/plain",
                                         Some(text.as_bytes()));
                               resp
                           }));
                    Ok(None)
                },
                None => {
                    // enter
                    let mut echo = EchoContext::default();
                    echo.text = Some(cv);
                    Ok(Some(CommandContext::new(echo)))
                }
            }
        })?;

        Ok(OK)
    }
}

impl Echo {
    pub fn new() -> Echo {
        Echo {}
    }
}