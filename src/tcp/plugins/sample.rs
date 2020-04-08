/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

register_tcp_plugin!(Sample);

use crate::plugin::*;
use crate::tcp::tcp::*;
use crate::error::Code::*;

pub struct Sample {
}

impl Plugin for Sample {
    type ModuleType = TCP;

    fn configure(&mut self) -> ActionResult {
        Ok(OK)
    }

    fn activate(&mut self) -> ActionResult {
        Ok(OK)
    }

    fn deactivate(&mut self) -> ActionResult {
        Ok(OK)
    }
}

impl Sample {
    pub fn new() -> Sample {
        Sample {}
    }
}