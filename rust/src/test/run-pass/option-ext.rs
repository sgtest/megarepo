fn main() {
    let thing = ~"{{ f }}";
    let f = str::find_str(thing, ~"{{");

    if f.is_none() {
        io::println(~"None!");
    }
}
