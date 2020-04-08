/*
 * Copyright (C) 2020 Aleksei Konovkin (alkon2000@mail.ru)
 */

register_http_plugin!(PythonAPI);

use pyo3::{ prelude::*, PyClassShell, types::{ PyDict } };
use regex::Regex;
use std::ops::Deref;

use crate::plugin::*;
use crate::http::*;
use crate::error::CoreError;
use crate::http::HttpStatus;

macro_rules! python_throw {
    ($py:ident,$err:ident,$msg:literal) => {
        $err.print_and_set_sys_last_vars($py);
        return throw!($msg);
    }
}

pub struct PythonAPI {}

#[derive(Default)]
struct PythonResponse {
    pub text: String
}

#[pyclass]
struct PythonResponseWrapper {
    pub response: Option<PythonResponse>
}

#[pymethods]
impl PythonResponseWrapper {
    #[setter(text)]
    fn set_text(&mut self, text: &str) -> PyResult<()> {
        match &mut self.response {
            None => {
                self.response = Some(PythonResponse { text: String::from(text) })
            },
            Some(response) => {
                response.text = String::from(text)
            }
        };
        Ok(())
     }
}

fn import(py: &Python, dict: &PyDict, modules: &[(String, String)]) -> PyResult<()> {
    for (name, module) in modules.iter() {
        dict.set_item(name, py.import(&module)?)?;
    }
    Ok(())
}

fn exec(modules: &[(String, String)], code: Option<&str>) -> Result<Option<PythonResponse>, CoreError> {
    let gil = Python::acquire_gil();
    let py = gil.python();
    let dict = PyDict::new(py);
    import(&py, dict, &modules).or_else(|err| {
        python_throw!(py, err, "import failed");
    })?;
    if let Some(code) = code {
        let wrap = PyClassShell::new_mut(py, PythonResponseWrapper {
            response: None
        }).or_else(|err| {
            python_throw!(py, err, "import failed");
        })?;
        dict.set_item("response", &wrap).or_else(|err| {
            python_throw!(py, err, "python failed");
        })?;
        py.run(code, None, Some(dict)).or_else(|err| {
            python_throw!(py, err, "exec failed");
        })?;
        return Ok(wrap.response.take());
    }
    Ok(None)
}

fn find_imports(code: &str) -> (String, Vec<(String, String)>) {
    let mut modules = vec![];
    let re_import = Regex::new("[^\r\n]*import[ \t]+(.+)").unwrap();
    let re_import_coma = Regex::new("([^,]+)").unwrap();
    let re_module = Regex::new("^([^ \t]+)[ \t]+as[ \t]+([^ \t]+)").unwrap();
    for cap in re_import.captures_iter(code) {
        for cap in re_import_coma.captures_iter(&cap[1]) {
            let module = String::from(cap[1].trim());
            match re_module.captures(&module) {
                None => {
                    modules.push((module.clone(), module))
                },
                Some(module) => {
                    modules.push((String::from(&module[2]), String::from(&module[1])))
                }
            }
        }
    }
    (String::from(re_import.replace_all(code, "").deref()), modules)
}

impl Plugin for PythonAPI {
    type ModuleType = HTTP;

    fn configure(&mut self) -> ActionResult {
        add_command!(Context::ROUTE, "python", |route: &mut RouteContext, code: String| {
            let (code, modules) = find_imports(&code);
            if exec(&modules, None).is_err() {
                return throw!("invalid code");
            }
            route.content = Some(ContentHandler::new(move |r| -> HttpResponse {
                let mut resp = HttpResponse::new(r);
                match exec(&modules, Some(&code)) {
                    Ok(Some(response)) => resp.send(HttpStatus::OK, "text/plain", Some(response.text.as_bytes())),
                    Err(err) => resp.send(HttpStatus::INTERNAL_SERVER_ERROR, "text/plain", Some(err.what().as_bytes())),
                    Ok(None) => unreachable!()
                };
                resp
            }));
            Ok(None)
        })
    }
}

impl PythonAPI {
    pub fn new() -> PythonAPI {
        PythonAPI {}
    }
}