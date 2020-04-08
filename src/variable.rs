/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

use std::str::FromStr;
use regex::Regex;

use crate::handler::sync::ConstRefHandler;

pub type LazyHandler<T> = ConstRefHandler<T, String>;

#[derive(Clone)]
enum Part {
    Text(String),
    Var(String)
}

enum Inner<T> {
    Simple(String),
    CV(Vec<Part>),
    Lazy(LazyHandler<T>)
}

pub struct Variable<T> {
    inner: Inner<T>
}

impl<T> Default for Variable<T> {
    fn default() -> Variable<T> {
        Variable {
            inner: Inner::CV(vec![])
        }
    }
}

impl<T> Clone for Inner<T> {
    fn clone(&self) -> Inner<T> {
        match self {
            Inner::Simple(s) => Inner::Simple(s.clone()),
            Inner::CV(cv) => Inner::CV(cv.clone()),
            Inner::Lazy(h) => Inner::Lazy(h.clone())
        }
    }
}

impl<T> Clone for Variable<T> {
    fn clone(&self) -> Variable<T> {
        Variable {
            inner: self.inner.clone()
        }
    }
}

const EMPTY_STR: String = String::new();

impl<T> Variable<T> {
    pub fn simple(s: &str) -> Variable<T> {
        Variable {
            inner: Inner::Simple(s.to_string())
        }
    }

    pub fn complex(s: &str) -> Variable<T> {
        let mut parts = vec![];

        let re = Regex::new("(\\$\\{[^}]+})").unwrap();
        let mut start = 0;

        re.find_iter(s).for_each(|m| {
            let var = m.as_str().trim_start_matches("${").trim_end_matches("}");
            parts.push(Part::Text(s[start..m.start()].to_string()));
            parts.push(Part::Var(var.to_string()));
            start = m.end();
        });

        parts.push(Part::Text(s[start..].to_string()));

        Variable {
            inner: Inner::CV(parts)
        }
    }

    pub fn lazy(h: LazyHandler<T>) -> Variable<T> {
        Variable {
            inner: Inner::Lazy(h)
        }
    }

    pub fn expand_with<F>(&self, f: F, r: &T) -> String
    where
        F: Fn(&str) -> Option<String>
    {
        match &self.inner {
            Inner::CV(parts) => {
                let mut ll = Vec::with_capacity(parts.len());
                parts.iter().for_each(|p| {
                    ll.push(match p {
                        Part::Text(text) => text.clone(),
                        Part::Var(var) => match (f)(&var) {
                            Some(s) => s,
                            None => EMPTY_STR
                        }
                    })
                });
                ll.concat()
            },
            Inner::Simple(s) => s.clone(),
            Inner::Lazy(h) => h.handle(r)
        }
    }
}

impl<T> FromStr for Variable<T> {
    type Err = String;
    #[inline]
    fn from_str(s: &str) -> Result<Variable<T>, Self::Err> {
        Ok(Variable::complex(s))
    }
}

impl<V: ToString, T> From<V> for Variable<T> {
    fn from(v: V) -> Variable::<T> {
        Variable::complex(&v.to_string())
    }
}

#[macro_export]
macro_rules! add_var_simple {
    ($r:ident, $name:literal, $s:ident) => {
        $r.add_var($name, HttpComplexValue::simple($s));
    }
}

#[macro_export]
macro_rules! add_var_complex {
    ($r:ident, $name:literal, $s:ident) => {
        $r.add_var($name, HttpComplexValue::complex($s));
    }
}

#[macro_export]
macro_rules! add_var_lazy {
    ($r:ident, $name:literal, move |_| $body:expr) => {
        $r.add_var($name, HttpComplexValue::lazy(LazyHandler::new(move |_| $body.to_string())));
    };
    ($r:ident, $name:literal, |_| $body:expr) => {
        $r.add_var($name, HttpComplexValue::lazy(LazyHandler::new(move |_| $body.to_string())));
    };
    ($r:ident, $name:literal, move |$arg:ident| $body:expr) => {
        $r.add_var($name, HttpComplexValue::lazy(LazyHandler::new(move |$arg| $body.to_string())));
    };
    ($r:ident, $name:literal, |$arg:ident| $body:expr) => {
        $r.add_var($name, HttpComplexValue::lazy(LazyHandler::new(move |$arg| $body.to_string())));
    };
    ($r:ident, $name:literal, move |$arg:ident: &$tp:ty| $body:expr) => {
        $r.add_var($name, HttpComplexValue::lazy(LazyHandler::new(move |$arg: &$tp| $body.to_string())));
    };
    ($r:ident, $name:literal, |$arg:ident: &$tp:ty| $body:expr) => {
        $r.add_var($name, HttpComplexValue::lazy(LazyHandler::new(|$arg: &$tp| $body.to_string())));
    }
}
