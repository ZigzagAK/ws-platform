/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

use std::time::Duration;
use std::rc::Rc;
use std::cell::{ RefCell, RefMut };
use std::any::Any;
use yaml_rust::{ yaml, yaml::Yaml };
use std::ops::Deref;
use std::convert::TryInto;
use std::net::SocketAddr;
use std::mem::take;
use std::collections::LinkedList;

use crate::keyval::*;
use crate::plugin::ActionResult;
use crate::module::*;
use crate::error::{ Code::*, CoreError };
use crate::variable::Variable;
use crate::core::MainContext;

pub type ConfigBlock = Yaml;

pub type CommandData = Box<dyn Any>;
pub type CommandContextType = Rc<RefCell<CommandContext>>;
pub type CommandResult = Result<Option<CommandContextType>, CoreError>;

pub struct CommandContext {
    parent: Option<CommandContextType>,
    pub data: Option<CommandData>,
    pub flush: Option<Box<dyn FnMut(CommandContextType)>>
}

impl Drop for CommandContext {
    fn drop(&mut self) {
        let flush = self.flush.take();
        if let Some(mut flush) = flush {
            flush(Rc::new(RefCell::new(CommandContext {
                parent: self.parent.take(),
                data: self.data.take(),
                flush: None
            })));
        }
    }
}

impl CommandContext {

    pub fn new_empty<T: 'static>() -> CommandContextType {
        Rc::new(RefCell::new(CommandContext {
            parent: None,
            data: None,
            flush: None
        }))
    }

    pub fn new_default<T: Default + 'static>() -> CommandContextType {
        CommandContext::new(T::default())
    }

    pub fn new<T: 'static>(data: T) -> CommandContextType {
        Rc::new(RefCell::new(CommandContext {
            parent: None,
            data: Some(Box::new(data)),
            flush: None
        }))
    }

    pub fn get<T: 'static>(&self) -> Option<&T> {
        match &self.data {
            None => None,
            Some(ref data) => data.downcast_ref::<T>()
        }
    }

    pub fn get_mut<T: 'static>(&mut self) -> Option<&mut T> {
        match &mut self.data {
            None => None,
            Some(ref mut data) => data.downcast_mut::<T>()
        }
    }

    pub fn parent(&self) -> Option<RefMut<CommandContext>> {
        match self.parent {
            None => None,
            Some(ref parent) => Some(parent.borrow_mut())
        }
    }
}

pub struct CommandHandler(Rc<Box<dyn Fn(CommandContextType, &mut ConfigBlock) -> CommandResult>>);

impl Deref for CommandHandler {
    type Target = dyn Fn(CommandContextType, &mut ConfigBlock) -> CommandResult;
    fn deref(&self) -> &Self::Target {
        & *self.0
    }
}

impl CommandHandler {
    pub fn new<F: 'static>(f: F)
        -> CommandHandler
    where
      F: Fn(CommandContextType, &mut ConfigBlock) -> CommandResult
    {
        CommandHandler(Rc::new(Box::new(f)))
    }

    pub fn handle(&self, context: CommandContextType, block: &mut ConfigBlock) -> CommandResult {
        (*self)(context, block)
    }
}

pub struct Command {
    pub handler: CommandHandler
}

pub struct Config;

pub trait Value {
    type Type;
    fn get(v: &mut ConfigBlock) -> Result<Self::Type, CoreError>;
}

pub type NoValue = ();
pub type Map<T> = KeyVal<Variable<T>>;
pub type List<T> = LinkedList<Variable<T>>;

impl Value for NoValue {
    type Type = NoValue;
    fn get(_: &mut ConfigBlock) -> Result<Self::Type, CoreError> {
        Ok(NoValue::from(()))
    }
}

impl Value for i64 {
    type Type = i64;
    fn get(v: &mut ConfigBlock) -> Result<Self::Type, CoreError> {
        match v {
            Yaml::Integer(i) => Ok((*i as i64).try_into().or_else(|_| throw!("type mismatch"))?),
            _ => throw!("type mismatch")
        }
    }
}

impl Value for u64 {
    type Type = u64;
    fn get(v: &mut ConfigBlock) -> Result<Self::Type, CoreError> {
        match v {
            Yaml::Integer(i) => Ok((*i as u64).try_into().or_else(|_| throw!("type mismatch"))?),
            _ => throw!("type mismatch")
        }
    }
}

impl Value for usize {
    type Type = usize;
    fn get(v: &mut ConfigBlock) -> Result<Self::Type, CoreError> {
        match v {
            Yaml::Integer(i) => Ok((*i as usize).try_into().or_else(|_| throw!("type mismatch"))?),
            _ => throw!("type mismatch")
        }
    }
}

impl Value for bool {
    type Type = bool;
    fn get(v: &mut ConfigBlock) -> Result<Self::Type, CoreError> {
        match v {
            Yaml::Boolean(b) => Ok(*b),
            _ => throw!("type mismatch")
        }
    }
}

impl Value for String {
    type Type = String;
    fn get(v: &mut ConfigBlock) -> Result<Self::Type, CoreError> {
        match v {
            Yaml::String(s) => Ok(s.clone()),
            Yaml::Null | Yaml::Hash(_) => Ok(String::from("")),
            _ => throw!("type mismatch")
        }
    }
}

impl<T: Request> Value for Variable<T> {
    type Type = Variable<T>;
    fn get(v: &mut ConfigBlock) -> Result<Self::Type, CoreError> {
        match v {
            Yaml::String(s) => Ok(Variable::complex(s)),
            Yaml::Null | Yaml::Hash(_) => Ok(Variable::complex("")),
            _ => throw!("type mismatch")
        }
    }
}

fn val_to_cv<T: Request>(y: ConfigBlock) -> Result<Variable<T>, CoreError> {
    match y {
        Yaml::String(s)
            => Ok(Variable::from(s)),
        Yaml::Boolean(b)
            => Ok(Variable::from(b)),
        Yaml::Integer(i)
            => Ok(Variable::from(i)),
        Yaml::Real(r)
            => Ok(Variable::from(r)),
        Yaml::Null
            => Ok(Variable::from("null")),
        _ =>
            throw!("map/set value type mismatch")
    }
}

impl<T: Request> Value for Map<T> {
    type Type = Map<T>;
    fn get(v: &mut ConfigBlock) -> Result<Self::Type, CoreError> {
        match v {
            Yaml::Hash(h) => {
                let mut map = KeyVal::default();
                for (k, v) in take(h) {
                    let key = Key::from(match k {
                        Yaml::String(s) => s,
                        _ => return throw!("map key type mismatch")
                    });
                    let entry = map.entry(key).or_default();
                    match v {
                        Yaml::Array(a) => {
                            for v in a {
                                entry.push_back(val_to_cv(v)?)
                            }
                        },
                        _ => entry.push_back(val_to_cv(v)?)
                    }
                }
                Ok(map)
            },
            _ => throw!("type mismatch")
        }
    }
}

impl<T: Request> Value for List<T> {
    type Type = List<T>;
    fn get(v: &mut ConfigBlock) -> Result<Self::Type, CoreError> {
        match v {
            Yaml::Array(a) => {
                let mut set = List::default();
                for v in take(a) {
                    set.push_back(val_to_cv(v)?);
                };
                Ok(set)
            },
            _ => throw!("set value type mismatch")
        }
    }
}

impl Value for SocketAddr {
    type Type = SocketAddr;
    fn get(v: &mut ConfigBlock) -> Result<Self::Type, CoreError> {
        match v {
            Yaml::String(s) => Ok(s.parse().or_else(|_| throw!("failed to parse address"))?),
            _ => throw!("type mismatch")
        }
    }
}

impl Value for Duration {
    type Type = Duration;
    fn get(v: &mut ConfigBlock) -> Result<Self::Type, CoreError> {
        match v {
            Yaml::Integer(i) => Ok(Duration::from_millis((*i as u64).try_into().or_else(|_| throw!("type mismatch"))?)),
            _ => throw!("type mismatch")
        }
    }
}

pub type CommandCallback<T, Context> = Box<dyn Fn(&mut Context, <T as Value>::Type) -> CommandResult>;
pub type CommandCallbackBlock<T> = Box<dyn Fn(&mut CommandContext, <T as Value>::Type) -> CommandResult>;

impl Config {

    pub fn add_block<M: ModuleType + 'static, T: Value + 'static>(
        path: &str,
        cmd: &str,
        handler: CommandCallbackBlock<T>
    )
        -> ActionResult
    {
        GenericModule::<M>::add_command(path, cmd, CommandHandler::new(
            move |context: CommandContextType, v: &mut ConfigBlock| -> CommandResult {
                let block = handler(&mut * context.borrow_mut(), T::get(v)?)?;
                if let Some(block) = &block {
                    let mut block_context = block.borrow_mut();
                    if block_context.parent.is_none() {
                        block_context.parent = Some(context.clone());
                    }
                }
                Ok(block)
            }
        ))
    }

    pub fn add_command<M: ModuleType + 'static, Context: 'static, T: Value + 'static>(
        path: &str,
        cmd: &str,
        handler: CommandCallback<T, Context>
    )
        -> ActionResult
    {
        GenericModule::<M>::add_command(path, cmd, CommandHandler::new(
            move |context: CommandContextType, v: &mut ConfigBlock| -> CommandResult {
                match context.borrow_mut().get_mut::<Context>() {
                    Some(mut ctx) => {
                        handler(&mut ctx, T::get(v)?)
                    },
                    None => throw!("invalid context")
                }
            }
        ))
    }

    pub fn parse<T: ModuleType + 'static>(s: &str) -> ActionResult {

        fn parse_node<T: ModuleType + 'static>(path: &str, context: &mut CommandContextType, doc: &mut Yaml)-> ActionResult {
            match *doc {
                Yaml::Array(ref mut v) => {
                    for x in v {
                        parse_node::<T>(path, context, x)?;
                    }
                }
                Yaml::Hash(ref mut h) => {
                    for (k, v) in h {
                        let key = k.as_str().unwrap();
                        if let Some(ref mut new_context) = GenericModule::<T>::handle_command(path, key, context.clone(), v)? {
                            parse_node::<T>(&format!("{}.{}", path, key), new_context, v)?;
                        } else {
                            parse_node::<T>(&format!("{}.{}", path, key), context, v)?;
                        }
                    }
                }
                _ => {}
            }
            Ok(OK)
        }

        match yaml::YamlLoader::load_from_str(&s) {
            Ok(mut docs) => {
                for doc in &mut docs {
                    parse_node::<T>("root", &mut CommandContext::new_default::<MainContext>(), doc)?;
                }
                return Ok(OK);    
            },
            Err(err) => {
                eprintln!("{}", err);
                throw!("Failed to parse config")
            }
        }    
    }
}

#[macro_export]
macro_rules! invalid_context {
    () => ($crate::error::throw!("invalid context"));
}

#[macro_export]
macro_rules! add_command {
    ($base:path, $name:tt, |_: &mut $ctx_t:ty, $data:ident: $data_t:ty| $body:expr) => {
        Self::add_command::<$ctx_t,$data_t>(&$base, $name, Box::new(|_: &mut $ctx_t, $data: $data_t| $body))
    };
    ($base:path, $name:tt, |$ctx:ident: &mut $ctx_t:ty, $data:ident: $data_t:ty| $body:expr) => {
        Self::add_command::<$ctx_t,$data_t>(&$base, $name, Box::new(|$ctx: &mut $ctx_t, $data: $data_t| $body))
    };
    ($base:path, $name:tt, |$ctx:ident: &mut $ctx_t:ty| $body:expr) => {
        Self::add_command::<$ctx_t,crate::config::NoValue>(&$base, $name, Box::new(|$ctx: &mut $ctx_t, _| $body))
    };
    ($base:path, $name:tt, move |_: &mut $ctx_t:ty, $data:ident: $data_t:ty| $body:expr) => {
        Self::add_command::<$ctx_t,$data_t>(&$base, $name, Box::new(move |_: &mut $ctx_t, $data: $data_t| $body))
    };
    ($base:path, $name:tt, move |$ctx:ident: &mut $ctx_t:ty, $data:ident: $data_t:ty| $body:expr) => {
        Self::add_command::<$ctx_t,$data_t>(&$base, $name, Box::new(move |$ctx: &mut $ctx_t, $data: $data_t| $body))
    };
    ($base:path, $name:tt, move |$ctx:ident: &mut $ctx_t:ty| $body:expr) => {
        Self::add_command::<$ctx_t,crate::config::NoValue>(&$base, $name, Box::new(move |$ctx: &mut $ctx_t, _| $body))
    }
}

#[macro_export]
macro_rules! add_block {
    ($base:path, $name:tt, |$ctx:ident, $data:ident: $data_t:ty| $body:expr) => {
        Self::add_block::<$data_t>(&$base, $name, Box::new(|$ctx, $data: $data_t| $body))
    };
    ($base:path, $name:tt, |$ctx:ident| $body:expr) => {
        Self::add_block::<crate::config::NoValue>(&$base, $name, Box::new(|$ctx, _| $body))
    };
    ($base:path, $name:tt, move |$ctx:ident, $data:ident: $data_t:ty| $body:expr) => {
        Self::add_block::<$data_t>(&$base, $name, Box::new(move |$ctx, $data: $data_t| $body))
    };
    ($base:path, $name:tt, move |$ctx:ident| $body:expr) => {
        Self::add_block::<crate::config::NoValue>(&$base, $name, Box::new(move |$ctx, _| $body))
    }
}

#[macro_export]
macro_rules! add_empty_block {
    ($base:path, $name:tt) => {
        Self::add_block::<crate::config::NoValue>(&$base, $name, Box::new(|_,_| { Ok(None) }))
    }
}
