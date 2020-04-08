/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

use std::time::{ SystemTime, Duration };
use std::collections::HashMap;
use std::sync::Once;
use std::mem::transmute_copy;

use crate::error::CoreResult;
use crate::client_context::ClientContext;
use crate::plugin::*;
use crate::config::*;
use crate::error::{ Code::*, CoreError, FlushResult };

pub trait Request: Sized + Send {

    fn new(ctx: ClientContext) -> Self;

    fn set_timeout(&mut self, timeout: Option<Duration>) -> Option<SystemTime> {
        self.context().set_timeout(timeout)
    }

    fn parse(&mut self) -> CoreResult;

    fn context(&mut self) -> &mut ClientContext;

    fn const_context(&self) -> &ClientContext;

    fn timedout(&mut self) -> bool {
        self.context().timedout()
    }

    fn on_timedout(&mut self) {}

    fn close(self) -> ClientContext;
}

pub trait Response: Sized + Send {

    type Request: Request;

    fn new(r: Self::Request) -> Self;

    fn flush(&mut self) -> FlushResult;

    fn get_request(&mut self) -> &mut Self::Request;

    fn context(&mut self) -> &mut ClientContext {
        self.get_request().context()
    }

    fn set_timeout(&mut self, timeout: Option<Duration>) -> Option<SystemTime> {
        self.get_request().set_timeout(timeout)
    }

    fn timedout(&mut self) -> bool {
        self.get_request().timedout()
    }

    fn on_timedout(&mut self) {
        self.get_request().on_timedout()
    }

    fn close(self) -> ClientContext;
}

pub trait ModuleType {
    type Request: Request;
    type Response: Response;

    fn name() -> &'static str {
        panic!("Module name is undefined")
    }

    fn root_context() -> Option<CommandContextType> {
        None
    }
}

pub struct ModuleConfig {
    pub commands: HashMap<String, Command>
}

impl Default for ModuleConfig {
    fn default() -> ModuleConfig {
        ModuleConfig {
            commands: HashMap::new()
        }
    }
}

pub type ModuleConfigResult = Result<ModuleConfig, CoreError>;

pub trait ModuleBase {}

pub trait Module: ModuleBase {

    type ModuleType: ModuleType + 'static;

    fn configure(&mut self) -> ModuleConfigResult {
        Ok(ModuleConfig::default())
    }

    fn activate(&mut self) -> ActionResult {
        Ok(DECLINED)
    }

    fn deactivate(&mut self) -> ActionResult {
        Ok(DECLINED)
    }

    fn activate_plugin(&mut self, _name: &str) -> ActionResult {
        Ok(DECLINED)
    }

    fn deactivate_plugin(&mut self, _name: &str) -> ActionResult {
        Ok(DECLINED)
    }

    fn plugin_state(&mut self, _name: &str) -> PluginState {
        PluginState::Failed
    }

    fn wait(&mut self);
}

pub struct GenericModule<T: ModuleType + 'static> {
    plugins: Plugins<T>,
    config: ModuleConfig
}

impl<T: ModuleType> ModuleBase for GenericModule<T> {
}

impl<T: ModuleType> Module for GenericModule<T> {
    type ModuleType = T;

    fn configure(&mut self) -> ModuleConfigResult {
        let config = ModuleConfig::default();
        self.plugins.configure();
        Ok(config)
    }

    fn activate(&mut self) -> ActionResult {
        self.plugins.activate();
        Ok(OK)
    }

    fn deactivate(&mut self) -> ActionResult {
        self.plugins.deactivate();
        Ok(OK)
    }

    fn activate_plugin(&mut self, name: &str) -> ActionResult {
        self.plugins.activate_plugin(name)
    }

    fn deactivate_plugin(&mut self, name: &str) -> ActionResult {
        self.plugins.deactivate_plugin(name)
    }

    fn plugin_state(&mut self, name: &str) -> PluginState {
        self.plugins.plugin_state(name)
    }

    fn wait(&mut self) {
        self.plugins.wait()
    }
}

impl<T: ModuleType + 'static> GenericModule<T> {
    pub fn new() -> GenericModule<T> {
        GenericModule {
            plugins: Plugins::new(),
            config: ModuleConfig::default()
        }
    }

    pub fn register<P: Plugin<ModuleType=T> + 'static>(plugin: P, name: &str) -> ActionResult {
        GenericModule::<T>::instance().plugins.register(plugin, name)
    }

    pub fn configure() {
        GenericModule::<T>::add_command("root", T::name(), CommandHandler::new(|_,_| -> CommandResult {
            Ok(T::root_context())
        })).unwrap();
        GenericModule::<T>::instance().plugins.configure()
    }

    pub fn activate() {
        GenericModule::<T>::instance().plugins.activate()
    }

    pub fn deactivate() {
        GenericModule::<T>::instance().plugins.deactivate()
    }

    pub fn get_plugin<P: Plugin>() -> &'static mut P {
        GenericModule::<T>::instance().plugins.get::<P>().unwrap()
    }

    pub fn get_plugin_ex<P: Plugin>() -> Option<&'static mut P> {
        match GenericModule::<T>::instance().plugins.get::<P>() {
            Ok(p) => Some(p),
            _ => None
        }
    }

    pub fn add_command(path: &str, cmd: &str, handler: CommandHandler) -> ActionResult {
        if GenericModule::<T>::instance().config.commands.insert(format!("{}.{}", path, cmd), Command {
            handler: handler
        }).is_some() {
            panic!("Command {}.{} has conflicts", path, cmd);
        }
        log_error!("debug", "Command {}.{} registered", path, cmd);
        Ok(OK)
    }

    pub fn wait() {
        GenericModule::<T>::instance().wait()
    }

    pub fn handle_command(path: &str, cmd: &str, context: CommandContextType, block: &mut ConfigBlock)
        -> Result<Option<CommandContextType>, CoreError>
    {
        match GenericModule::<T>::instance().config.commands.get(&format!("{}.{}", path, cmd)) {
            Some(command) => {
                match command.handler.handle(context, block) {
                    Ok(new_context) => match new_context {
                        None => Ok(None),
                        Some(new_context) => {
                            let mut block = block.clone();
                            new_context.borrow_mut().flush = Some(Box::new(move |new_context| {
                                (*command.handler)(new_context, &mut block).unwrap();
                            }));
                            Ok(Some(new_context))
                        }
                    },
                    Err(err) => throw!(format!("Failed to handle command '{}.{}': {}", path, cmd, err.what()))
                }
            },
            None => throw!("Unknown command: '{}.{}'", path, cmd)
        }
    }

    pub fn name() -> &'static str {
        T::name()
    }

    pub fn config_parse(s: &str) -> ActionResult {
        Config::parse::<T>(s)
    }

    fn instance() -> &'static mut GenericModule<T> {
        static mut MODULES: Option<HashMap<String, Box<dyn ModuleBase>>> = None;
        static INIT: Once = Once::new();

        unsafe {
            let modules = {
                INIT.call_once(|| {
                    MODULES = Some(HashMap::new());
                });
                MODULES.as_mut().unwrap()
            };

            if !modules.contains_key(T::name()) {
                modules.insert(String::from(T::name()), Box::new(GenericModule::<T>::new()));
            }

            // modules have 'static lifetime
            transmute_copy(&modules[T::name()])
        }
    }
}