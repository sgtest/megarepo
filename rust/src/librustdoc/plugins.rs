// Copyright 2012-2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![allow(deprecated)]

use clean;

use std::mem;
use std::string::String;
use std::path::PathBuf;

use rustc_back::dynamic_lib as dl;

pub type PluginResult = clean::Crate;
pub type PluginCallback = fn (clean::Crate) -> PluginResult;

/// Manages loading and running of plugins
pub struct PluginManager {
    dylibs: Vec<dl::DynamicLibrary> ,
    callbacks: Vec<PluginCallback> ,
    /// The directory plugins will be loaded from
    pub prefix: PathBuf,
}

impl PluginManager {
    /// Create a new plugin manager
    pub fn new(prefix: PathBuf) -> PluginManager {
        PluginManager {
            dylibs: Vec::new(),
            callbacks: Vec::new(),
            prefix: prefix,
        }
    }

    /// Load a plugin with the given name.
    ///
    /// Turns `name` into the proper dynamic library filename for the given
    /// platform. On windows, it turns into name.dll, on macOS, name.dylib, and
    /// elsewhere, libname.so.
    pub fn load_plugin(&mut self, name: String) {
        let x = self.prefix.join(libname(name));
        let lib_result = dl::DynamicLibrary::open(Some(&x));
        let lib = lib_result.unwrap();
        unsafe {
            let plugin = lib.symbol("rustdoc_plugin_entrypoint").unwrap();
            self.callbacks.push(mem::transmute::<*mut u8,PluginCallback>(plugin));
        }
        self.dylibs.push(lib);
    }

    /// Load a normal Rust function as a plugin.
    ///
    /// This is to run passes over the cleaned crate. Plugins run this way
    /// correspond to the A-aux tag on Github.
    pub fn add_plugin(&mut self, plugin: PluginCallback) {
        self.callbacks.push(plugin);
    }
    /// Run all the loaded plugins over the crate, returning their results
    pub fn run_plugins(&self, mut krate: clean::Crate) -> clean::Crate {
        for &callback in &self.callbacks {
            krate = callback(krate);
        }
        krate
    }
}

#[cfg(target_os = "windows")]
fn libname(mut n: String) -> String {
    n.push_str(".dll");
    n
}

#[cfg(target_os="macos")]
fn libname(mut n: String) -> String {
    n.push_str(".dylib");
    n
}

#[cfg(all(not(target_os="windows"), not(target_os="macos")))]
fn libname(n: String) -> String {
    let mut i = String::from("lib");
    i.push_str(&n);
    i.push_str(".so");
    i
}
