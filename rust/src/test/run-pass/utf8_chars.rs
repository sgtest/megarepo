extern mod std;

fn main() {
    // Chars of 1, 2, 3, and 4 bytes
    let chs: ~[char] = ~['e', 'é', '€', 0x10000 as char];
    let s: ~str = str::from_chars(chs);

    assert (str::len(s) == 10u);
    assert (str::char_len(s) == 4u);
    assert (vec::len(str::chars(s)) == 4u);
    assert (str::from_chars(str::chars(s)) == s);
    assert (str::char_at(s, 0u) == 'e');
    assert (str::char_at(s, 1u) == 'é');

    assert (str::is_utf8(str::to_bytes(s)));
    assert (!str::is_utf8(~[0x80_u8]));
    assert (!str::is_utf8(~[0xc0_u8]));
    assert (!str::is_utf8(~[0xc0_u8, 0x10_u8]));

    let mut stack = ~"a×c€";
    assert (str::pop_char(stack) == '€');
    assert (str::pop_char(stack) == 'c');
    str::push_char(stack, 'u');
    assert (stack == ~"a×u");
    assert (str::shift_char(stack) == 'a');
    assert (str::shift_char(stack) == '×');
    str::unshift_char(stack, 'ß');
    assert (stack == ~"ßu");
}
