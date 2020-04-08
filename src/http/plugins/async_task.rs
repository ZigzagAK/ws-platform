/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

register_http_plugin!(AsyncTask);

use std::{ thread, thread::JoinHandle };
use std::time::Duration;

use crate::plugin::*;
use crate::http::*;

pub struct AsyncTask {
    thr: Option<JoinHandle<()>>
}

impl Plugin for AsyncTask {
    type ModuleType = HTTP;

    fn configure(&mut self) -> ActionResult {
        Ok(OK)
    }

    fn activate(&mut self) -> ActionResult {
        self.thr = Some(thread::spawn(|| {
            for i in 0..10 {
                println!("{}", i);
                thread::sleep(Duration::from_millis(100));
            }
        }));
        Ok(OK)
    }

    fn deactivate(&mut self) -> ActionResult {
        Ok(DECLINED)
    }

    fn wait(&mut self) {
        if let Some(thr) = self.thr.take() {
            thr.join().unwrap();
        }
    }
}

impl AsyncTask {
    pub fn new() -> AsyncTask {
        AsyncTask {
            thr: None
        }
    }
}