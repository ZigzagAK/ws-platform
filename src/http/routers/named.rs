/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

use std::sync::RwLock;
use std::collections::HashMap;

use crate::error::{ Code::*, CoreError, CoreResult };
use crate::http::routers::result::*;
use crate::http::HttpRequest;

type NamedResult<'a, Context> = RouteResult<'a, Context>;
type NamedResultMut<'a, Context> = RouteResultMut<'a, Context>;

struct NamedRoute<Context: Default> {
    name: String,
    context: HashMap<String, Context>
}

pub struct NamedRouter<Context: Default> {
    lock: RwLock<()>,
    routes: Vec<NamedRoute<Context>>
}

impl<Context: Default> Default for NamedRouter<Context> {
    fn default() -> NamedRouter<Context> {
        NamedRouter {
            lock: RwLock::new(()),
            routes: Vec::new()
        }
    }
}

impl<Context: Default> NamedRoute<Context> {
    fn new(
        name: &str
    ) -> NamedRoute<Context> {
        NamedRoute {
            name: String::from(name),
            context: HashMap::new()
        }
    }

    fn matched(&self, path: &str) -> bool {
        self.name == path
    }
}

impl<Context: Default> NamedRouter<Context> {
    pub fn new() -> NamedRouter<Context> {
        NamedRouter {
            lock: RwLock::new(()),
            routes: Vec::with_capacity(10)
        }
    }

    pub fn add(
        &mut self,
        name: &str,
        method: Option<String>,
        context: Context
    ) -> Result<(NamedResultMut<'_, Context>, bool), CoreError> {
        let guard = self.lock.write().unwrap();
        let routes = &mut self.routes;
        let method = method.unwrap_or(String::from("*"));

        for i in 0..routes.len() {
            if routes[i].name == name {
                let mut added = false;
                let context = routes[i].context.entry(method).or_insert_with(|| {
                    added = true;
                    context
                });
                return Ok((NamedResultMut::new(guard, context), added));
            }
        }

        routes.push(NamedRoute::new(name));
        let route = routes.last_mut().unwrap();
        Ok((NamedResultMut::new(guard, route.context.entry(method).or_insert(context)), true))
    }

    pub fn replace(&mut self,
        name: &str,
        method: Option<String>,
        context: Context
    ) -> Result<NamedResultMut<'_, Context>, CoreError> {
        let guard = self.lock.write().unwrap();
        let routes = &mut self.routes;
        let method = method.unwrap_or(String::from("*"));

        for i in 0..routes.len() {
            if routes[i].name == name {
                routes.remove(i);
                let context = routes[i].context.entry(method).or_insert(context);
                return Ok(NamedResultMut::new(guard, context));
            }
        }

        routes.push(NamedRoute::new(name));
        let route = routes.last_mut().unwrap();
        Ok(NamedResultMut::new(guard, route.context.entry(method).or_insert(context)))
    }

    pub fn remove(&mut self, name: &str, method: Option<String>) -> bool {
        let _guard = self.lock.write();
        let routes = &mut self.routes;

        let method = method.unwrap_or(String::from("*"));

        for i in 0..routes.len() {
            let route = &mut routes[i];
            if route.name == name {
                route.context.remove(&method);
                if route.context.is_empty() {
                    routes.remove(i);
                }
                return true
            }
        }

        false
    }

    pub fn get(&self, r: &HttpRequest) -> Option<NamedResult<'_, Context>> {
        let guard = self.lock.read().unwrap();
        let routes = &self.routes;

        let name = r.uri();
        let method = format!("{}", r.method());

        for p in routes.iter() {
            if p.matched(name) {
                return match p.context.get(&method) {
                    Some(context) => Some(NamedResult::new(guard, context)),
                    None => match p.context.get("*") {
                        Some(context) => Some(NamedResult::new(guard, context)),
                        None => None
                    }
                }
            }
        }

        None
    }

    pub fn upsert<F>(&mut self, name: &str, method: Option<String>, f: F) -> CoreResult
    where
        F: Fn(&mut Context, bool)
    {
        return match self.add(name, method, Context::default()) {
            Err(err) => Err(err),
            Ok((route, added)) => {
                (f)(route.context, added);
                Ok(OK)
            }
        }
    }
}

// #[cfg(test)]
// mod test {
//     use super::*;
//     use crate::handler::Handler;

//     #[test]
//     fn test() {
//         let mut t = NamedRouter::new();
//         let get = String::from("GET");
//         let post = String::from("POST");
//         assert!(t.add("^/[ab]/", Some(get.clone()), Some(Handler::new(|r| -> i32 {
//             r * 2
//         }))).is_ok());
//         assert!(t.add("^/[ab]/", Some(post.clone()), Some(Handler::new(|r| -> i32 {
//             r * 3
//         }))).is_ok());
//         assert!(t.add("^/x/[ab]+/.+", None, Some(Handler::new(|r| -> i32 {
//             r * 4
//         }))).is_ok());
//         assert!(t.add("\\.(jpg|gif)$", None, Some(Handler::new(|r| -> i32 {
//             r * 4
//         }))).is_ok());
//         assert!(t.add("/xxx/yyy", None, Some(Handler::new(|r| -> i32 {
//             r * r
//         }))).is_ok());
//         assert!(t.add("/xxx/yyy/aaa", None, Some(Handler::new(|r| -> i32 {
//             r * r * r
//         }))).is_ok());
//         assert_eq!(t.get("/xxx/yyy", &get).unwrap().context.as_ref().unwrap().handle(8), 64);
//         assert_eq!(t.get("/xxx/yyy/zzz", &get).unwrap().context.as_ref().unwrap().handle(8), 64);
//         assert_eq!(t.get("/a/1", &get).unwrap().context.as_ref().unwrap().handle(2), 4);
//         assert_eq!(t.get("/b/1", &get).unwrap().context.as_ref().unwrap().handle(2), 4);
//         assert_eq!(t.get("/a/1", &post).unwrap().context.as_ref().unwrap().handle(2), 6);
//         assert_eq!(t.get("/b/1", &post).unwrap().context.as_ref().unwrap().handle(2), 6);
//         assert!(t.get("/c/1", &get).is_none());
//         assert_eq!(t.get("/x/aaaabbb/1", &get).unwrap().context.as_ref().unwrap().handle(2), 8);
//         assert!(t.get("/x/aaaabbbc/1", &get).is_none());
//         assert_eq!(t.get("image.jpg", &get).unwrap().context.as_ref().unwrap().handle(2), 8);
//         assert_eq!(t.get("image.gif", &get).unwrap().context.as_ref().unwrap().handle(2), 8);
//         assert!(t.get("image.bmp", &get).is_none());
//         assert_eq!(t.get("/xxx/yyy/zzz", &get).unwrap().context.as_ref().unwrap().handle(8), 64);
//         assert_eq!(t.get("/aaa/xxx/yyy/zzz", &get).unwrap().context.as_ref().unwrap().handle(8), 64);
//         assert!(t.get("/xxxyyy/yyy", &get).is_none());
//         assert_eq!(t.get("/aaa/xxx/yyy/aaa/bbb", &get).unwrap().context.as_ref().unwrap().handle(10), 1000);
//     }
// }