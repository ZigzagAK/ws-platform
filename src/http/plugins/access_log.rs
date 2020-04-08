/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

register_http_plugin!(AccessLog);

use std::fs::File;
use std::collections::HashMap;
use std::sync::{ Arc, Mutex, RwLock };
use std::fs::OpenOptions;
use std::io::prelude::*;
use std::mem::take;

use crate::plugin::*;
use crate::http::*;
use crate::error::Code;

#[derive(Default, Clone)]
pub struct AccessLogFormatContext {
    name: Option<String>,
    format: Option<HttpComplexValue>,
}

#[derive(Default, Clone)]
pub struct AccessLogContext {
    filename: String,
    format: Option<HttpComplexValue>,
    buffer_size: usize
}

struct AccessFile {
    file: File,
    buffer: Vec<u8>
}

pub struct AccessLog {
    log_formats: Arc<RwLock<HashMap<String, HttpComplexValue>>>,
    files: Arc<Mutex<HashMap<String, AccessFile>>>
}

impl Plugin for AccessLog {
    type ModuleType = HTTP;

    fn name() -> &'static str {
        "AccessLog"
    }

    fn configure(&mut self) -> ActionResult {

        add_command!(Context::HTTP, "log_formats.log_format.name", |log_format: &mut AccessLogFormatContext, name: String| {
            log_format.name = Some(name);
            Ok(None)
        })?;

        add_command!(Context::HTTP, "log_formats.log_format.format", |log_format: &mut AccessLogFormatContext, format: HttpComplexValue| {
            log_format.format = Some(format);
            Ok(None)
        })?;

        let log_formats_ = Arc::clone(&self.log_formats);

        add_empty_block!(Context::HTTP, "log_formats")?;

        add_block!(Context::HTTP, "log_formats.log_format", move |context| {
            match context.get_mut::<AccessLogFormatContext>() {
                Some(log_format) => {
                    // exit
                    if let Some(name) = &log_format.name {
                        if let Some(format) = &log_format.format {
                            log_formats_.write().unwrap().insert(name.clone(), format.clone());
                            return Ok(None);
                        }
                    }
                    throw!("log_format: 'name' and 'format' required")
                },
                None =>
                    // enter
                    Ok(Some(CommandContext::new_default::<AccessLogFormatContext>()))
            }
        })?;

        // Server

        add_command!(Context::SERVER, "access_log.filename", |access_log: &mut AccessLogContext, filename: String| {
            access_log.filename = filename;
            Ok(None)
        })?;

        add_command!(Context::SERVER, "access_log.buffer_size", |access_log: &mut AccessLogContext, buffer_size: usize| {
            access_log.buffer_size = buffer_size;
            Ok(None)
        })?;

        let log_formats_ = Arc::clone(&self.log_formats);

        add_command!(Context::SERVER, "access_log.format", move |access_log: &mut AccessLogContext, format: String| {
            access_log.format = match log_formats_.read().unwrap().get(&format) {
                Some(format) => Some(format.clone()),
                None => return throw!("Format '{}' is not found", format)
            };
            Ok(None)
        })?;

        add_block!(Context::SERVER, "access_log", move |context| {
            match context.get_mut::<AccessLogContext>() {
                Some(access_log) => {
                    // exit
                    let access_log = take(access_log);
                    if access_log.filename.len() != 0 {
                        if let Some(format) = access_log.format.clone() {
                            context.parent().unwrap()
                                   .get_mut::<ServerContext>().unwrap()
                                   .log.push_back(LogHandler::new(move |resp| {
                                AccessLog::write(&access_log, resp.expand(&format));
                            }));
                            return Ok(None);
                        }
                    }
                    throw!("access_log: 'filename' and 'format' required")
                },
                None =>
                    // enter
                    Ok(Some(CommandContext::new_default::<AccessLogContext>()))
            }
        })?;

        // Route

        add_command!(Context::ROUTE, "access_log.filename", |access_log: &mut AccessLogContext, filename: String| {
            access_log.filename = filename;
            Ok(None)
        })?;

        let log_formats_ = Arc::clone(&self.log_formats);

        add_command!(Context::ROUTE, "access_log.format", move |access_log: &mut AccessLogContext, format: String| {
            access_log.format = match log_formats_.read().unwrap().get(&format) {
                Some(format) => Some(format.clone()),
                None => return throw!("Format '{}' is not found", format)
            };
            Ok(None)
        })?;

        add_command!(Context::ROUTE, "access_log.buffer_size", |access_log: &mut AccessLogContext, buffer_size: usize| {
            access_log.buffer_size = buffer_size;
            Ok(None)
        })?;

        add_block!(Context::ROUTE, "access_log", move |context| {
            match context.get_mut::<AccessLogContext>() {
                Some(access_log) => {
                    // exit
                    let access_log = take(access_log);
                    if access_log.filename.len() != 0 {
                        if let Some(format) = access_log.format.clone() {
                            context.parent().unwrap()
                                   .get_mut::<RouteContext>().unwrap()
                                   .log.push_back(LogHandler::new(move |resp| {
                                AccessLog::write(&access_log, resp.expand(&format));
                            }));
                            return Ok(None);
                        }
                    }
                    throw!("access_log: 'filename' and 'format' required")
                },
                None =>
                    // enter
                    Ok(Some(CommandContext::new_default::<AccessLogContext>()))
            }
        })?;

        Ok(Code::OK)
    }
}

impl AccessLog {
    pub fn new() -> AccessLog {
        AccessLog {
            log_formats: Arc::new(RwLock::new(HashMap::new())),
            files: Arc::new(Mutex::new(HashMap::new()))
        }
    }

    fn write(context: &AccessLogContext, text: String) {
        thread_local!(
            static ACCESS_LOG: &'static mut AccessLog = HttpModule::get_plugin::<AccessLog>()
        );

        ACCESS_LOG.with(|access_log| {
            let mut files = access_log.files.lock().unwrap();

            let access_log_file = match files.get_mut(&context.filename) {
                Some(file) => file,
                None => {
                    let file = match OpenOptions::new().append(true)
                                                       .create(true)
                                                       .open(&context.filename) {
                        Ok(file) => file,
                        Err(err) => {
                            log_error!("error", "Failed to open log file '{}': {}", context.filename, err);
                            return;
                        }
                    };
                    files.insert(context.filename.clone(), AccessFile {
                        file: file,
                        buffer: Vec::with_capacity(context.buffer_size + 1024)
                    });
                    files.get_mut(&context.filename).unwrap()
                }
            };

            access_log_file.buffer.extend_from_slice(text.as_bytes());
            access_log_file.buffer.extend_from_slice(b"\n");

            if access_log_file.buffer.len() < context.buffer_size {
                return;
            }

            if let Err(err) = access_log_file.file.write_all(&access_log_file.buffer) {
                log_error!("error", "failed to write '{}', {}", context.filename, err)
            }

            access_log_file.buffer.clear();
        })
    }
}