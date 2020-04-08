/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

use crate::error::Code;

#[derive(Debug)]
pub struct HttpError {
    text: String,
    fatal: bool
}

pub type HttpResult = Result<Code, HttpError>;

impl HttpError {
    pub fn throw<T>(what: &str) -> Result<T, HttpError> {
        Err(HttpError {
            text: String::from(what),
            fatal: false
        })
    }

    pub fn throw_fatal<T>(what: &str) -> Result<T, HttpError> {
        Err(HttpError {
            text: String::from(what),
            fatal: true
        })
    }

    pub fn what(&self) -> &str {
        &self.text
    }

    pub fn is_fatal(&self) -> bool {
        self.fatal
    }
}

#[macro_export]
macro_rules! http_throw {
    ($fmt:tt, $($arg:tt)*) => ($crate::http::error::HttpError::throw(&format!($fmt, $($arg)*)));
    ($arg:expr) => ($crate::http::error::HttpError::throw(&format!("{}",$arg)));
}

#[macro_export]
macro_rules! http_fatal {
    ($fmt:tt, $($arg:tt)*) => ($crate::http::error::HttpError::throw_fatal(&format!($fmt, $($arg)*)));
    ($arg:expr) => ($crate::http::error::HttpError::throw_fatal(&format!("{}",$arg)));
}
