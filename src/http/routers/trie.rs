/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

use std::collections::HashMap;
use std::sync::RwLock;

use crate::error::{ Code::*, CoreError, CoreResult };
use crate::http::routers::result::*;
use crate::http::HttpRequest;
use crate::variable::Variable;

type TrieResult<'a, Context> = RouteResult<'a, Context>;
type TrieResultMut<'a, Context> = RouteResultMut<'a, Context>;

#[derive(Default, Clone)]
struct Data<Context> {
    context: Context,
    uri_parts: Vec<Option<String>>
}

struct TrieNode<Context: Default> {
    words: HashMap<String, TrieNode<Context>>,
    context: HashMap<String, Data<Context>>
}

pub struct TrieRouter<Context: Default> {
    lock: RwLock<()>,
    root: TrieNode<Context>
}

impl<Context: Default> TrieNode<Context> {
    pub fn new() -> TrieNode<Context> {
        TrieNode {
            words: HashMap::new(),
            context: HashMap::new()
        }
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }
}

impl<Context: Default> Default for TrieNode<Context> {
    fn default() -> TrieNode<Context> {
        TrieNode {
            words: HashMap::new(),
            context: HashMap::new()
        }
    }
}

impl<Context: Default> Default for TrieRouter<Context> {
    fn default() -> TrieRouter<Context> {
        TrieRouter {
            lock: RwLock::new(()),
            root: TrieNode::new()
        }
    }
}

impl<Context: Default> TrieRouter<Context> {
    pub fn new() -> TrieRouter<Context> {
        TrieRouter {
            lock: RwLock::new(()),
            root: TrieNode::new()
        }
    }

    pub fn add(
        &mut self,
        path: &str,
        method: Option<String>,
        context: Context
    ) -> Result<(TrieResultMut<'_, Context>, bool), CoreError> {
        self.insert(path, method, context, false)
    }

    pub fn replace(
        &mut self,
        path: &str,
        method: Option<String>,
        context: Context
    ) -> Result<(TrieResultMut<'_, Context>, bool), CoreError> {
        self.insert(path, method, context, true)
    }

    fn insert(
        &mut self,
        path: &str,
        method: Option<String>,
        context: Context,
        replace: bool
    ) -> Result<(TrieResultMut<'_, Context>, bool), CoreError> {
        let guard = self.lock.write().unwrap();
        let mut node = &mut self.root;
        let method = method.unwrap_or(String::from("*"));
        let mut uri_parts = vec![];

        for word in path.split("/") {
            let var = word.trim_start_matches("{").trim_end_matches("}");
            if var.len() == word.len() {
                uri_parts.push(None);
                node = node.words.entry(String::from(word)).or_default();
            } else {
                uri_parts.push(Some(var.to_string()));
                node = node.words.entry("*".to_string()).or_default();
            }
        }

        if node.context.contains_key(&method) {
            return match replace {
                true => {
                    node.context.insert(method.clone(), Data { context, uri_parts });
                    Ok((TrieResultMut::new(guard, &mut node.context.get_mut(&method).unwrap().context), false))
                },
                false =>  {
                    Ok((TrieResultMut::new(guard, &mut node.context.get_mut(&method).unwrap().context), true))
                }
            }
        }

        node.context.insert(method.clone(), Data { context, uri_parts });
        Ok((TrieResultMut::new(guard, &mut node.context.get_mut(&method).unwrap().context), true))
    }

    pub fn remove(&mut self, path: &str, method: Option<String>) -> bool {
        let _guard = self.lock.write().unwrap();
        let mut node = &mut self.root;

        for word in path.split("/") {
            match node.words.get_mut(word) {
                Some(n) => {
                    node = n;
                },
                None => return false
            }
        }

        node.context.remove(&method.unwrap_or(String::from("*"))).is_some()
    }

    pub fn get(&self, r: &mut HttpRequest) -> Option<(TrieResult<'_, Context>, bool)> {
        let guard = self.lock.read().unwrap();
        let root = &self.root;

        if root.is_empty() {
            return None;
        }

        struct Traverser<'a, 'b, Context: Default> {
            parts: Vec<&'a str>,
            method: &'a String,
            star: Option<&'b Data<Context>>
        }

        impl<'a, 'b, Context: Default> Traverser<'a, 'b, Context> {
            fn new(path: &'a str, method: &'a String) -> Traverser<'a, 'b, Context> {
                Traverser {
                    parts: path.split("/").collect(),
                    method: method,
                    star: None
                }
            }

            fn traverse(
                &mut self,
                i: usize,
                node: &'b TrieNode<Context>,
                data: Option<&'b Data<Context>>
            ) -> Option<(&'b Data<Context>, bool)> {
                if let Some(data) = data {
                    self.star = Some(data)
                }

                if i == self.parts.len() {
                    // leaf
                    return match node.context.get(self.method) {
                        Some(h) => Some((h, true)),
                        None => match node.context.get("*") {
                            Some(h) => Some((h, true)),
                            None => None
                        }
                    }
                }

                let lp = node.words.get(self.parts[i]);
                let la = node.words.get("*");

                if la.is_none() && lp.is_none() {
                    return match node.context.get(self.method) {
                        Some(h) => Some((h, false)),
                        None => match node.context.get("*") {
                            Some(h) => Some((h, false)),
                            None => None
                        }
                    }
                }

                let mut f = None;

                if let Some(lp) = lp {
                    f = self.traverse(i + 1, lp, None);
                }

                if f.is_none() {
                    if let Some(la) = la {
                        f = self.traverse(i + 1, la, match node.context.get(self.method) {
                            Some(h) => Some(h),
                            None => match node.context.get("*") {
                                Some(h) => Some(h),
                                None => None
                            }
                        });
                    }
                }

                match f {
                    Some(f) => Some(f),
                    None => match self.star {
                        None => None,
                        Some(data) => Some((data, false))
                    }
                }
            }
        }

        let method = format!("{}", r.method());
        let uri = r.uri().to_string();
        let mut traverser = Traverser::new(&uri, &method);

        match traverser.traverse(0, &root, None) {
            Some((data, exact)) => {
                let vars = r.vars_mut();
                let mut index = 0;
                data.uri_parts.iter().for_each(|var| {
                    if let Some(var) = var {
                        vars.set(&var, Variable::simple(traverser.parts[index]))
                    }
                    index += 1;
                });
                Some((TrieResult::new(guard, &data.context), exact))
            },
            None => None
        }
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
//     use crate::handler::sync::Handler;

//     #[test]
//     fn test() {
//         let mut t = TrieRouter::new();
//         let get = String::from("GET");
//         let post = String::from("POST");
//         assert!(t.add("/a/b/c/d", Some(get.clone()), Some(Handler::new(|r| -> u64 {
//             r * 2
//         }))).is_ok());
//         assert!(t.add("/a/b/c/d", Some(post.clone()), Some(Handler::new(|r| -> u64 {
//             r * 3
//         }))).is_ok());
//         assert!(t.add("/a/b", None, Some(Handler::new(|r| -> u64 {
//             r * 4
//         }))).is_ok());
//         assert!(t.add("/a/b/*/d", None, Some(Handler::new(|r| -> u64 {
//             r * 10
//         }))).is_ok());
//         assert!(t.add("/a/b/*", None, Some(Handler::new(|r| -> u64 {
//             r * 5
//         }))).is_ok());
//         assert!(t.add("/b/*", None, Some(Handler::new(|r| -> u64 {
//             r
//         }))).is_ok());
//         assert!(t.add("/a/b/c/d/e/f/g", None, Some(Handler::new(|r| -> u64 {
//             r * 100
//         }))).is_ok());
//         assert_eq!(t.get("/a/b/c/d", &get).unwrap().0.as_ref().unwrap().handle(2), 4);
//         assert_eq!(t.get("/a/b/c/d", &post).unwrap().0.as_ref().unwrap().handle(2), 6);
//         assert_eq!(t.get("/a/b", &get).unwrap().0.as_ref().unwrap().handle(2), 8);
//         assert_eq!(t.get("/a/b/1/d", &get).unwrap().0.as_ref().unwrap().handle(2), 20);
//         assert_eq!(t.get("/a/b/1/2", &get).unwrap().0.as_ref().unwrap().handle(2), 10);
//         assert_eq!(t.get("/a/b/1", &get).unwrap().0.as_ref().unwrap().handle(4), 20);
//         assert_eq!(t.get("/b/1", &get).unwrap().0.as_ref().unwrap().handle(4), 4);
//         assert_eq!(t.get("/b/1/2", &get).unwrap().0.as_ref().unwrap().handle(4), 4);
//         assert_eq!(t.get("/a/b/1/1", &get).unwrap().0.as_ref().unwrap().handle(4), 20);
//         assert!(t.get("/a/c", &get).is_none());
//         assert_eq!(t.get("/a/b/c/d/e/f/g", &get).unwrap().0.as_ref().unwrap().handle(4), 400);
//         assert!(t.get("/1", &get).is_none());
//         assert!(t.remove("/a/b", None));
//         assert!(t.get("/a/b", &get).is_none());
//         assert!(!t.remove("/a/b", None));
//         assert!(t.remove("/a/b/*", None));
//         assert!(t.get("/a/b/1/2", &get).is_none());
//         assert!(t.get("/xxx/yyy", &get).is_none());
//     }
// }