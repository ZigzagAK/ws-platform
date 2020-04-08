/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

register_http_plugin!(LuaAPI);

use std::collections::hash_map::DefaultHasher;
use std::hash::{ Hash, Hasher };
use rlua::{ Function, Lua };

use crate::plugin::*;
use crate::http::*;

pub struct LuaAPI {}

fn get_hash<T: Hash>(t: &T) -> String {
    let mut s = DefaultHasher::new();
    t.hash(&mut s);
    format!("closure_{}", s.finish())
}

impl Plugin for LuaAPI {
    type ModuleType = HTTP;

    fn configure(&mut self) -> ActionResult {
        add_command!(Context::ROUTE, "lua", |route: &mut RouteContext, code: String| {
            let closure_name = get_hash(&code);
            thread_local!(static LUA_STATE: Lua = Lua::new());
            route.content = Some(ContentHandler::new(move |r| -> HttpResponse {
                let mut resp = HttpResponse::new(r);
                LUA_STATE.with(|lua| {
                    let closure_name_ = closure_name.clone();
                    lua.context(|ctx| {
                        let globals = ctx.globals();
                        let closure = match globals.get::<_, Function>(closure_name_.clone()) {
                            Ok(closure) => closure,
                            _ => {
                                ctx.load(&format!("function {}() {} end", &closure_name_, code)).exec().unwrap();
                                globals.get::<_, Function>(closure_name_).unwrap()
                            }
                        };
                        let text = closure.call::<_, String>(()).unwrap();
                        resp.send(HttpStatus::OK, "text/plain", Some(text.as_bytes()));
                    })
                });
                resp
            }));

            Ok(None)
        })
    }
}

impl LuaAPI {
    pub fn new() -> LuaAPI {
        LuaAPI {}
    }
}