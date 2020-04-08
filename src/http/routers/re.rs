/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

use regex::Regex;
use std::sync::RwLock;
use std::collections::HashMap;

use crate::http::HttpRequest;
use crate::error::{ Code::*, CoreError, CoreResult };
use crate::http::routers::result::*;
use crate::variable::Variable;

type RegexResult<'a, Context> = RouteResult<'a, Context>;
type RegexResultMut<'a, Context> = RouteResultMut<'a, Context>;

struct RegexRoute<Context: Default> {
    pattern: String,
    re: Regex,
    context: HashMap<String, Context>
}

pub struct RegexRouter<Context: Default> {
    lock: RwLock<()>,
    routes: Vec<RegexRoute<Context>>
}

impl<Context: Default> Default for RegexRouter<Context> {
    fn default() -> RegexRouter<Context> {
        RegexRouter {
            lock: RwLock::new(()),
            routes: Vec::new()
        }
    }
}

impl<Context: Default> RegexRoute<Context> {
    fn new(
        pattern: &str
    ) -> Result<RegexRoute<Context>, CoreError> {
        let route = match Regex::new(pattern) {
            Ok(re) => RegexRoute {
                pattern: String::from(pattern),
                re: re,
                context: HashMap::new()
            },
            Err(err) => {
                return throw!("Invalid pattern: {}", err);
            }
        };
        Ok(route)
    }

    fn matches<'a, 'b>(&'a self, path: &'b str) -> (bool, Vec<(&'a str, &'b str)>) {
        match self.re.captures(path) {
            None => (false, vec![]),
            Some(captures) => {
                let mut vars = vec![];
                self.re.capture_names().for_each(|name| {
                    if let Some(name) = name {
                        if let Some(val) = captures.name(name) {
                            vars.push((name, val.as_str()))
                        }
                    }
                });
                (true, vars)
            }
        }
    }
}

impl<Context: Default> RegexRouter<Context> {
    pub fn new() -> RegexRouter<Context> {
        RegexRouter {
            lock: RwLock::new(()),
            routes: Vec::with_capacity(10)
        }
    }

    pub fn add(
        &mut self,
        pattern: &str,
        method: Option<String>,
        context: Context
    ) -> Result<(RegexResultMut<'_, Context>, bool), CoreError> {
        let guard = self.lock.write().unwrap();
        let routes = &mut self.routes;
        let method = method.unwrap_or(String::from("*"));

        for i in 0..routes.len() {
            if routes[i].pattern == pattern {
                let mut added = false;
                let context = routes[i].context.entry(method).or_insert_with(|| {
                    added = true;
                    context
                });
                return Ok((RegexResultMut::new(guard, context), added));
            }
            if pattern.len() > routes[i].pattern.len() {
                routes.insert(i, RegexRoute::new(pattern)?);
                let route = routes.get_mut(i).unwrap();
                return Ok((RegexResultMut::new(guard, route.context.entry(method).or_insert(context)), true));
            }
        }

        routes.push(RegexRoute::new(pattern)?);
        let route = routes.last_mut().unwrap();
        Ok((RegexResultMut::new(guard, route.context.entry(method).or_insert(context)), true))
    }

    pub fn replace(&mut self,
        pattern: &str,
        method: Option<String>,
        context: Context
    ) -> Result<RegexResultMut<'_, Context>, CoreError> {
        let guard = self.lock.write().unwrap();
        let routes = &mut self.routes;
        let method = method.unwrap_or(String::from("*"));

        for i in 0..routes.len() {
            if routes[i].pattern == pattern {
                routes.remove(i);
                let context = routes[i].context.entry(method).or_insert(context);
                return Ok(RegexResultMut::new(guard, context));
            }
            if pattern.len() > routes[i].pattern.len() {
                routes.insert(i, RegexRoute::new(pattern)?);
                let route = routes.get_mut(i).unwrap();
                return Ok(RegexResultMut::new(guard, route.context.entry(method).or_insert(context)));
            }
        }

        routes.push(RegexRoute::new(pattern)?);
        let route = routes.last_mut().unwrap();
        Ok(RegexResultMut::new(guard, route.context.entry(method).or_insert(context)))
    }

    pub fn remove(&mut self, pattern: &str, method: Option<String>) -> bool {
        let _guard = self.lock.write();
        let routes = &mut self.routes;

        let method = method.unwrap_or(String::from("*"));

        for i in 0..routes.len() {
            let route = &mut routes[i];
            if route.pattern == pattern {
                route.context.remove(&method);
                if route.context.is_empty() {
                    routes.remove(i);
                }
                return true
            }
        }

        false
    }

    pub fn get(&self, r: &mut HttpRequest) -> Option<RegexResult<'_, Context>> {
        let guard = self.lock.read().unwrap();
        let routes = &self.routes;

        let path = r.uri().clone();
        let method = format!("{}", r.method());

        for p in routes.iter() {
            let (matched, vars) = p.matches(&path);
            if matched {
                return match p.context.get(&method) {
                    Some(context) => {
                        vars.iter().for_each(|(name, val)| r.vars_mut().set(name, Variable::simple(val)));
                        Some(RegexResult::new(guard, context))
                    },
                    None => match p.context.get("*") {
                        Some(context) => {
                            vars.iter().for_each(|(name, val)| r.vars_mut().set(name, Variable::simple(val)));
                            Some(RegexResult::new(guard, context))
                        },
                        None => None
                    }
                }
            }
        }

        None
    }

    pub fn upsert<F>(&mut self, path: &str, method: Option<String>, f: F) -> CoreResult
    where
        F: Fn(&mut Context, bool)
    {
        return match self.add(path, method, Context::default()) {
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
//         let mut t = RegexRouter::new();
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