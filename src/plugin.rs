/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

use std::collections::LinkedList;
use std::mem::transmute_copy;

use crate::config::*;
use crate::module::*;
use crate::error::{ Code, Code::*, CoreError };

pub type ActionResult = Result<Code, CoreError>;

pub trait Plugin {

    type ModuleType: ModuleType + 'static;

    fn configure(&mut self) -> ActionResult {
        Ok(DECLINED)
    }

    fn activate(&mut self) -> ActionResult {
        Ok(DECLINED)
    }

    fn deactivate(&mut self) -> ActionResult {
        Ok(DECLINED)
    }

    fn add_block<T: Value + 'static>(
        path: &str,
        cmd: &str,
        handler: CommandCallbackBlock<T>
    )
        -> ActionResult where Self: Sized
    {
        Config::add_block::<Self::ModuleType, T>(path, cmd, handler)
    }

    fn add_command<Context: 'static, T: Value + 'static>(
        path: &str,
        cmd: &str,
        handler: CommandCallback<T, Context>
    )
        -> ActionResult where Self: Sized
    {
        Config::add_command::<Self::ModuleType, Context, T>(path, cmd, handler)
    }

    fn name() -> &'static str where Self: Sized {
        unimplemented!()
    }

    fn wait(&mut self) {}
}

#[derive(Clone, Copy)]
pub enum PluginState {
    Configured,
    Activated,
    Deactivated,
    Failed
}

impl std::fmt::Display for PluginState {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            PluginState::Configured => write!(f, "Configured"),
            PluginState::Activated => write!(f, "Activated"),
            PluginState::Deactivated => write!(f, "Deactivated"),
            PluginState::Failed => write!(f, "Failed")
        }
    }
}

struct PluginData<T: ModuleType> {
    plugin: Box<dyn Plugin<ModuleType=T>>,
    state: PluginState,
    name: String
}

pub struct Plugins<T: ModuleType + 'static> {
    plugins: LinkedList<PluginData<T>>
}

impl<T: ModuleType> Plugins<T> {
    pub fn new() -> Plugins<T> {
        Plugins {
            plugins: LinkedList::new()
        }
    }

    pub fn register<P: Plugin<ModuleType=T> + 'static>(&mut self, plugin: P, name: &str) -> ActionResult {
        self.plugins.push_back(PluginData {
            plugin: Box::new(plugin),
            state: PluginState::Failed,
            name: name.to_string()
        });
        Ok(OK)
    }

    pub fn configure(&mut self) {
        let plugins = &mut self.plugins;
        for data in plugins.into_iter() {
            match data.plugin.configure() {
                Ok(_) => {
                    data.state = PluginState::Configured;
                    log_error!("debug", "{} has configured", data.name);
                },
                Err(err) => {
                    log_error!("error", "Failed to configure plugin '{}': {}", data.name, err);
                }
            }
        }
    }

    pub fn activate(&mut self) {
        let plugins = &mut self.plugins;
        for data in plugins.into_iter() {
            if let PluginState::Configured = data.state {
                if let Err(err) = data.plugin.activate() {
                    data.state = PluginState::Failed;
                    log_error!("error", "Failed to activate plugin '{}': {}", data.name, err)
                } else {
                    data.state = PluginState::Activated;
                    log_error!("debug", "{} has activated", data.name);
                }
            }
        }
    }

    pub fn deactivate(&mut self) {
        let plugins = &mut self.plugins;
        for data in plugins.into_iter() {
            if let PluginState::Activated = data.state {
                if let Err(err) = data.plugin.deactivate() {
                    data.state = PluginState::Failed;
                    log_error!("error", "Failed to deactivate plugin '{}': {}", data.name, err);
                } else {
                    data.state = PluginState::Deactivated;
                    log_error!("debug", "{} has deactivated", data.name);
                }
            }
        }
    }

    pub fn deactivate_plugin(&mut self, name: &str) -> ActionResult {
        let plugins = &mut self.plugins;
        for data in plugins.into_iter() {
            if data.name == name {
                return match data.state {
                    PluginState::Deactivated | PluginState::Configured => Ok(DECLINED),
                    PluginState::Activated => match data.plugin.deactivate() {
                        Ok(code) => {
                            data.state = PluginState::Deactivated;
                            log_error!("debug", "{} has deactivated", data.name);
                            Ok(code)
                        },
                        Err(err) => {
                            log_error!("error", "Failed to deactivate plugin '{}': {}", data.name, err);
                            Err(err)
                        }
                    },
                    PluginState::Failed => CoreError::throw("Plugin is in fail state")
                }
            }
        }
        Ok(DECLINED)
    }    

    pub fn activate_plugin(&mut self, name: &str) -> ActionResult {
        let plugins = &mut self.plugins;
        for data in plugins.into_iter() {
            if data.name == name {
                return match data.state {
                    PluginState::Activated => Ok(DECLINED),
                    PluginState::Deactivated | PluginState::Configured => match data.plugin.activate() {
                        Ok(code) => {
                            data.state = PluginState::Activated;
                            log_error!("debug", "{} has activated", data.name);
                            Ok(code)
                        },
                        Err(err) => {
                            log_error!("error", "Failed to activate plugin '{}': {}", data.name, err);
                            Err(err)
                        }
                    },
                    PluginState::Failed => CoreError::throw("Plugin is in fail state")
                }
            }
        }
        Ok(DECLINED)
    }    

    pub fn plugin_state(&mut self, name: &str) -> PluginState {
        let plugins = &mut self.plugins;
        for data in plugins.into_iter() {
            if data.name == name {
                return data.state;
            }
        }
        PluginState::Failed
    }    

    pub fn get<P: Plugin>(&mut self) -> Result<&'static mut P, CoreError> {
        let plugins = &mut self.plugins;
        let name = format!("{}::{}", T::name(), P::name());
        for data in plugins.into_iter() {
            if data.name == name {
                return Ok(unsafe {
                    // plugins have 'static lifetime
                    transmute_copy(&data.plugin)
                })
            }
        }
        throw!("Plugin '{}' not found", name)
    }    

    pub fn wait(&mut self) {
        let plugins = &mut self.plugins;
        for data in plugins.into_iter() {
            data.plugin.wait()
        }
    }
}

macro_rules! register_plugin {
    ($module:ident, $name:ident) => {
        #[allow(non_snake_case)]
        extern "C" fn $module() {
            let plugin = $name::new();
            $module::register(plugin, &format!("{}::{}", $module::name(), stringify!($name))).unwrap();
        }
        #[allow(dead_code)]
        #[allow(non_upper_case_globals)]
        #[used]
        #[link_section = ".init_array"]
        static $name: extern "C" fn() = $module;
    }
}