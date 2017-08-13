// Copyright 2016 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::io::Write;

use rustc_serialize::json::as_json;

use rls_data::{self, Analysis, Import, Def, DefKind, Ref, RefKind, MacroRef,
               Relation, CratePreludeData};
use rls_data::config::Config;
use rls_span::{Column, Row};

pub struct JsonDumper<O: DumpOutput> {
    result: Analysis,
    config: Config,
    output: O,
}

pub trait DumpOutput {
    fn dump(&mut self, result: &Analysis);
}

pub struct WriteOutput<'b, W: Write + 'b> {
    output: &'b mut W,
}

impl<'b, W: Write> DumpOutput for WriteOutput<'b, W> {
    fn dump(&mut self, result: &Analysis) {
        if let Err(_) = write!(self.output, "{}", as_json(&result)) {
            error!("Error writing output");
        }
    }
}

pub struct CallbackOutput<'b> {
    callback: &'b mut FnMut(&Analysis),
}

impl<'b> DumpOutput for CallbackOutput<'b> {
    fn dump(&mut self, result: &Analysis) {
        (self.callback)(result)
    }
}

impl<'b, W: Write> JsonDumper<WriteOutput<'b, W>> {
    pub fn new(writer: &'b mut W, config: Config) -> JsonDumper<WriteOutput<'b, W>> {
        JsonDumper {
            output: WriteOutput { output: writer },
            config: config.clone(),
            result: Analysis::new(config)
        }
    }
}

impl<'b> JsonDumper<CallbackOutput<'b>> {
    pub fn with_callback(callback: &'b mut FnMut(&Analysis),
                         config: Config)
                         -> JsonDumper<CallbackOutput<'b>> {
        JsonDumper {
            output: CallbackOutput { callback: callback },
            config: config.clone(),
            result: Analysis::new(config),
        }
    }
}

impl<O: DumpOutput> Drop for JsonDumper<O> {
    fn drop(&mut self) {
        self.output.dump(&self.result);
    }
}

impl<'b, O: DumpOutput + 'b> JsonDumper<O> {
    pub fn crate_prelude(&mut self, data: CratePreludeData) {
        self.result.prelude = Some(data)
    }

    pub fn macro_use(&mut self, data: MacroRef) {
        if self.config.pub_only {
            return;
        }
        self.result.macro_refs.push(data);
    }

    pub fn import(&mut self, public: bool, import: Import) {
        if !public && self.config.pub_only {
            return;
        }
        self.result.imports.push(import);
    }

    pub fn dump_ref(&mut self, data: Ref) {
        if self.config.pub_only {
            return;
        }
        self.result.refs.push(data);
    }

    pub fn dump_def(&mut self, public: bool, mut data: Def) {
        if !public && self.config.pub_only {
            return;
        }
        if data.kind == DefKind::Mod && data.span.file_name.to_str().unwrap() != data.value {
            // If the module is an out-of-line defintion, then we'll make the
            // definition the first character in the module's file and turn the
            // the declaration into a reference to it.
            let rf = Ref {
                kind: RefKind::Mod,
                span: data.span,
                ref_id: data.id,
            };
            self.result.refs.push(rf);
            data.span = rls_data::SpanData {
                file_name: data.value.clone().into(),
                byte_start: 0,
                byte_end: 0,
                line_start: Row::new_one_indexed(1),
                line_end: Row::new_one_indexed(1),
                column_start: Column::new_one_indexed(1),
                column_end: Column::new_one_indexed(1),
            }
        }
        self.result.defs.push(data);
    }

    pub fn dump_relation(&mut self, data: Relation) {
        self.result.relations.push(data);
    }
}
