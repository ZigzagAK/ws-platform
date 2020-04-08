#[macro_use]
extern crate lazy_static;

#[macro_export]
macro_rules! log_error {
    ($level:tt, $fmt:expr, $($arg:tt)*) => {
        $crate::log_error!($level, format_args!($fmt, $($arg)*))
    };
    ("info", $text:expr) => {
        crate::core::ErrorLog::log("core", "info", &None, format_args!("{}", $text))
    };
    ("warn", $text:expr) => {
        crate::core::ErrorLog::log("core", "warn", &None, format_args!("{}", $text))
    };
    ("error", $text:expr) => {
        crate::core::ErrorLog::log("core", "error", &None, format_args!("{}", $text))
    };
    ("debug", $text:expr) => {
        crate::core::ErrorLog::log("core", "debug", &None, format_args!("{}", $text))
    };
}

pub mod keyval;
#[macro_use]
pub mod error;
#[macro_use]
pub mod plugin;
#[macro_use]
pub mod config;
#[macro_use]
pub mod core;
#[macro_use]
pub mod variable;
pub mod tcp_socket;
pub mod buffer;
#[macro_use]
pub mod client_context;
pub mod module;
pub mod handler;
#[macro_use]
pub mod http;
pub mod tcp;
pub mod connection_pool;
pub mod upstream;
pub mod fgac;