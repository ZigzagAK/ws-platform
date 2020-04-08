/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

use crate::connection_pool::*;

#[allow(non_camel_case_types)]
pub enum Flush {
    // Completed
    OK(Option<Peer>),
    // Closed
    DECLINED,
    // Again
    AGAIN,
    // Need read
    READ_MORE(Peer),
    // Need write
    WRITE_MORE(Peer),
    // Need read and write
    READ_WRITE_MORE(Peer)
}

#[allow(non_camel_case_types)]
#[derive(Clone, PartialEq, Debug)]
pub enum Code {
    OK,
    AGAIN,
    DECLINED
}

use Code::*;

impl std::fmt::Display for Code {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            OK => write!(f, "OK"),
            AGAIN => write!(f, "AGAIN"),
            DECLINED => write!(f, "DECLINED")
        }
    }
}

#[derive(Debug)]
pub struct CoreError {
    text: String
}

impl CoreError {
    pub fn what(&self) -> &str {
        &self.text
    }
}

impl CoreError {
    pub fn throw<T>(text: &str) -> Result<T, CoreError> {
        Err(CoreError::from(text))
    }    
}

impl std::fmt::Display for CoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", &self.text)
    }
}

impl From<&str> for CoreError {
    fn from(text: &str) -> CoreError {
        CoreError {
            text: String::from(text)
        }
    }
}

pub type CoreResult = Result<Code, CoreError>;
pub type FlushResult = Result<Flush, CoreError>;

#[macro_export]
macro_rules! throw {
    ($fmt:tt, $($arg:tt)*) => ($crate::error::CoreError::throw(&format!($fmt, $($arg)*)));
    ($arg:expr) => ($crate::error::CoreError::throw(&format!("{}",$arg)));
}
