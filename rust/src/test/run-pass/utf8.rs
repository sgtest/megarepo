// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

pub fn main() {
    let yen: char = '¥'; // 0xa5
    let c_cedilla: char = 'ç'; // 0xe7
    let thorn: char = 'þ'; // 0xfe
    let y_diaeresis: char = 'ÿ'; // 0xff
    let pi: char = 'Π'; // 0x3a0

    fail_unless!((yen as int == 0xa5));
    fail_unless!((c_cedilla as int == 0xe7));
    fail_unless!((thorn as int == 0xfe));
    fail_unless!((y_diaeresis as int == 0xff));
    fail_unless!((pi as int == 0x3a0));

    fail_unless!((pi as int == '\u03a0' as int));
    fail_unless!(('\x0a' as int == '\n' as int));

    let bhutan: ~str = ~"འབྲུག་ཡུལ།";
    let japan: ~str = ~"日本";
    let uzbekistan: ~str = ~"Ўзбекистон";
    let austria: ~str = ~"Österreich";

    let bhutan_e: ~str =
        ~"\u0f60\u0f56\u0fb2\u0f74\u0f42\u0f0b\u0f61\u0f74\u0f63\u0f0d";
    let japan_e: ~str = ~"\u65e5\u672c";
    let uzbekistan_e: ~str =
        ~"\u040e\u0437\u0431\u0435\u043a\u0438\u0441\u0442\u043e\u043d";
    let austria_e: ~str = ~"\u00d6sterreich";

    let oo: char = 'Ö';
    fail_unless!((oo as int == 0xd6));

    fn check_str_eq(a: ~str, b: ~str) {
        let mut i: int = 0;
        for str::each(a) |ab| {
            log(debug, i);
            log(debug, ab);
            let bb: u8 = b[i];
            log(debug, bb);
            fail_unless!((ab == bb));
            i += 1;
        }
    }

    check_str_eq(bhutan, bhutan_e);
    check_str_eq(japan, japan_e);
    check_str_eq(uzbekistan, uzbekistan_e);
    check_str_eq(austria, austria_e);
}
