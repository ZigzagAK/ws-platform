/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

register_core_plugin!(ErrorLog);

use std::fs::File;
use std::collections::HashMap;
use std::sync::{ Arc, Mutex };
use std::fs::OpenOptions;
use std::io::prelude::*;
use chrono::Utc;

use crate::core::*;
use crate::plugin::*;
use crate::error::Code;
use crate::config::CommandResult;

pub struct ErrorLog {
    filename: Option<String>,
    files: Arc<Mutex<HashMap<String, File>>>
}

impl Plugin for ErrorLog {
    type ModuleType = Core;

    fn name() -> &'static str {
        "ErrorLog"
    }

    fn configure(&mut self) -> ActionResult {

        add_command!(Context::MAIN, "error_log", |_: &mut MainContext, filename: String| {
            ErrorLog::open(&filename)?;
            CoreModule::get_plugin::<ErrorLog>().filename = Some(filename);
            Ok(None)
        })?;

        Ok(Code::OK)
    }
}

impl ErrorLog {
    pub fn new() -> ErrorLog {
        ErrorLog {
            filename: None,
            files: Arc::new(Mutex::new(HashMap::new()))
        }
    }

    pub fn open(filename: &String) -> CommandResult {
        let error_log = CoreModule::get_plugin::<ErrorLog>();

        let mut files = error_log.files.lock().unwrap();

        if files.get(filename).is_none() {
            return match OpenOptions::new().append(true)
                                           .create(true)
                                           .open(&filename) {
                Ok(f) => {
                    files.insert(filename.clone(), f);
                    Ok(None)
                },
                Err(err) => throw!("Failed to open error_log file '{}': {}", filename, err)
            }
        }

        Ok(None)
    }

    pub fn log(tp: &str, level: &str, filename: &Option<String>, args: std::fmt::Arguments) {
        match CoreModule::get_plugin_ex::<ErrorLog>() {
            Some(error_log) => {
                if let Some(filename) = filename.as_ref().or(error_log.filename.as_ref()) {
                    if let Some(f) = error_log.files.lock().unwrap().get_mut(filename) {
                        let _ = f.write_fmt(format_args!("{} [{}] [{}] {}\n", Utc::now().format("%Y/%m/%d-%H:%M:%S"), tp, level, args));
                        return;
                    }
                }
            },
            None => eprintln!("{} [{}] [{}] {}", Utc::now().format("%Y/%m/%d-%H:%M:%S"), tp, level, args)
        }
    }

    pub fn info(tp: &str, filename: &Option<String>, text: String) {
        ErrorLog::log(tp, "info", filename, format_args!("{}", text))
    }

    pub fn warn(tp: &str, filename: &Option<String>, text: String) {
        ErrorLog::log(tp, "warn", filename, format_args!("{}", text))
    }

    pub fn error(tp: &str, filename: &Option<String>, text: String) {
        ErrorLog::log(tp, "error", filename, format_args!("{}", text))
    }

    pub fn debug(tp: &str, filename: &Option<String>, text: String) {
        ErrorLog::log(tp, "debug", filename, format_args!("{}", text))
    }
}