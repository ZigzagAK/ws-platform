/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

register_http_plugin!(Index);

use std::thread;

use crate::module::*;
use crate::plugin::*;
use crate::http::*;

pub struct Index
{}

impl Plugin for Index {
    type ModuleType = HTTP;

    fn configure(&mut self) -> ActionResult {
        add_command!(Context::ROUTE, "index", |route: &mut RouteContext, root: String| {
            route.content = Some(ContentHandler::new(move |r| -> HttpResponse {

                let uri = format!("{}{}", root, r.uri().trim_end_matches("/"));
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

                log_http_error!(r, "debug", "[{:?}] {} -> {}", thread::current().id(), r.uri(), &uri);
        
                let mut resp = HttpResponse::new(r);
                let _ = resp.send_file(&uri);

                resp
            }));

            Ok(None)
        })
    }
}

impl Index {
    pub fn new() -> Index {
        Index {}
    }
}