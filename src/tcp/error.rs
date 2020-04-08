/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

use crate::error::Code;

#[derive(Debug)]
pub struct TcpError {
    text: &'static str,
    fatal: bool
}

pub type TcpResult = Result<Code, TcpError>;

impl TcpError {
    pub fn throw<T>(what: &'static str) -> Result<T, TcpError> {
        Err(TcpError {
            text: what,
            fatal: false
        })
    }

    pub fn throw_fatal<T>(what: &'static str) -> Result<T, TcpError> {
        Err(TcpError {
            text: what,
            fatal: true
        })
    }

    pub fn what(&self) -> &'static str {
        self.text
    }

    pub fn is_fatal(&self) -> bool {
        self.fatal
    }
}