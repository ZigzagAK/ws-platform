/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

use std::cmp::Ordering;
use std::ops::{ Deref, DerefMut };
use std::collections::{ LinkedList, HashMap };
use std::hash::{ Hash, Hasher };
use unicase::Ascii;

pub enum Value<'a, T: Clone> {
    Single(&'a T),
    Multi(&'a LinkedList<T>)
}

#[derive(Clone)]
pub struct Key(Ascii<String>);

#[derive(Default)]
pub struct KeyVal<T>(HashMap<Key, LinkedList<T>>);

impl From<&str> for Key {
    fn from(key: &str) -> Key {
        Key(Ascii::new(String::from(key)))
    }
}

impl From<String> for Key {
    fn from(key: String) -> Key {
        Key(Ascii::new(key))
    }
}

impl From<&String> for Key {
    fn from(key: &String) -> Key {
        Key(Ascii::new(key.clone()))
    }
}

impl Deref for Key {
    type Target = String;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Hash for Key {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state)
    }
}

impl Eq for Key {}

impl PartialOrd for Key {
    fn partial_cmp(&self, other: &Key) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Key {
    fn cmp(&self, other: &Key) -> Ordering {
        self.0.cmp(&other.0)
    }
}

impl PartialEq for Key {
    fn eq(&self, other: &Key) -> bool {
        self.0.eq(&other.0)
    }
}

impl std::fmt::Display for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", &self.0)
    }
}

impl<T: Clone> KeyVal<T> {
    fn get_arg(&self, name: &str, exact: bool) -> Option<Value<'_, T>> {
        match self.0.get(&Key::from(name)) {
            Some(ll) => {
                match ll.front() {
                    Some(s) => {
                        if ll.len() == 1 || exact {
                            return Some(Value::Single(s));
                        }
                        Some(Value::Multi(ll))
                    }
                    None => None
                }
            },
            None => None
        }
    }

    pub fn get(&self, name: &str) -> Option<Value<'_, T>> {
        return self.get_arg(name, false);
    }

    pub fn exact(&self, name: &str) -> Option<&T> {
        match self.get_arg(name, true) {
            Some(a) => match a {
                Value::Single(a) => Some(a),
                Value::Multi(_) => unreachable!()
            },
            None => None
        }
    }

    pub fn replace(&mut self, name: &str, value: Option<T>) {
        let key = Key::from(name);

        match value {
            Some(value) => {
                let ll = self.0.entry(key).or_default();
                ll.clear();
                ll.push_back(value);
            },
            None => {
                self.0.remove(&key);
            }
        };
    }

    pub fn set(&mut self, name: &str, value: T) {
        self.replace(name, Some(value))
    }

    pub fn add(&mut self, name: &str, value: T) {
        let ll = self.0.entry(Key::from(name)).or_default();
        ll.push_back(value);
    }

    pub fn batch_add(&mut self, args: &KeyVal<T>) {
        args.iter().for_each(|(key, values)| {
            let ll = self.0.entry(key.clone()).or_default();
            values.iter().for_each(|value| {
                ll.push_back(value.clone());
            });
        });
    }

    pub fn batch_replace(&mut self, args: &KeyVal<T>) {
        args.iter().for_each(|(key, values)| {
            self.0.insert(key.clone(), values.clone());
        });
    }

    pub fn remove(&mut self, name: &str) {
        self.0.remove(&Key::from(name));
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
}

impl<T: Clone> Deref for KeyVal<T> {
    type Target = HashMap<Key, LinkedList<T>>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: Clone> DerefMut for KeyVal<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}