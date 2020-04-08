/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

register_http_plugin!(ErrorLog);

use crate::plugin::*;
use crate::http::*;
use crate::error::Code;
use crate::core::plugins::error_log;

type CoreErrorLog = error_log::ErrorLog;

pub struct ErrorLog {
}

impl Plugin for ErrorLog {
    type ModuleType = HTTP;

    fn name() -> &'static str {
        "ErrorLog"
    }

    fn configure(&mut self) -> ActionResult {

        add_command!(Context::HTTP, "error_log", move |http: &mut HttpContext, filename: String| {
            CoreErrorLog::open(&filename)?;
            http.error_log = Some(filename);
            Ok(None)
        })?;

        add_command!(Context::SERVER, "error_log", move |server: &mut ServerContext, filename: String| {
            CoreErrorLog::open(&filename)?;
            server.error_log = Some(filename);
            Ok(None)
        })?;

        add_command!(Context::ROUTE, "error_log", move |route: &mut RouteContext, filename: String| {
            CoreErrorLog::open(&filename)?;
            route.error_log = Some(filename);
            Ok(None)
        })?;

        Ok(Code::OK)
    }
}

impl ErrorLog {
    pub fn new() -> ErrorLog {
        ErrorLog {}
    }

    pub fn log(level: &str, filename: &Option<String>, args: std::fmt::Arguments) {
        CoreErrorLog::log("http", level, filename, args)
    }

    pub fn info(filename: &Option<String>, text: String) {
        CoreErrorLog::info("http", filename, text)
    }

    pub fn warn(filename: &Option<String>, text: String) {
        CoreErrorLog::warn("http", filename, text)
    }

    pub fn error(filename: &Option<String>, text: String) {
        CoreErrorLog::error("http", filename, text)
    }

    pub fn debug(filename: &Option<String>, text: String) {
        CoreErrorLog::debug("http", filename, text)
    }
}

#[macro_export]
macro_rules! log_http_error {
    ($r:expr, $level:tt, $fmt:expr, $($arg:tt)*) => {
        $crate::log_http_error!($r, $level, format_args!($fmt, $($arg)*))
    };
    ($r:expr, "info", $text:expr) => {
        crate::http::plugins::error_log::ErrorLog::log("info", $r.get_error_log(), format_args!("{}", $text))
    };
    ($r:expr, "warn", $text:expr) => {
        crate::http::plugins::error_log::ErrorLog::log("warn", $r.get_error_log(), format_args!("{}", $text))
    };
    ($r:expr, "error", $text:expr) => {
        crate::http::plugins::error_log::ErrorLog::log("error", $r.get_error_log(), format_args!("{}", $text))
    };
    ($r:expr, "debug", $text:expr) => {
        crate::http::plugins::error_log::ErrorLog::log("debug", $r.get_error_log(), format_args!("{}", $text))
    };
}