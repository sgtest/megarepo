# Functions

Functions (like all other static declarations, such as `type`) can be
declared both at the top level and inside other functions (or modules,
which we'll come back to in moment).

The `ret` keyword immediately returns from a function. It is
optionally followed by an expression to return. In functions that
return `()`, the returned expression can be left off. A function can
also return a value by having its top level block produce an
expression (by omitting the final semicolon).

Some functions (such as the C function `exit`) never return normally.
In Rust, these are annotated with return type `!`:

    fn dead_end() -> ! { fail; }

This helps the compiler avoid spurious error messages. For example,
the following code would be a type error if `dead_end` would be
expected to return.

    # fn can_go_left() -> bool { true }
    # fn can_go_right() -> bool { true }
    # tag dir { left; right; }
    # fn dead_end() -> ! { fail; }
    let dir = if can_go_left() { left }
              else if can_go_right() { right }
              else { dead_end(); };

## Closures

Normal Rust functions (declared with `fn`) do not close over their
environment. A `lambda` expression can be used to create a closure.

    fn make_plus_function(x: int) -> lambda(int) -> int {
        lambda(y: int) -> int { x + y }
    }
    let plus_two = make_plus_function(2);
    assert plus_two(3) == 5;

A `lambda` function *copies* its environment (in this case, the
binding for `x`). It can not mutate the closed-over bindings, and will
not see changes made to these variables after the `lambda` was
evaluated. `lambda`s can be put in data structures and passed around
without limitation.

The type of a closure is `lambda(args) -> type`, as opposed to
`fn(args) -> type`. The `fn` type stands for 'bare' functions, with no
closure attached. Keep this in mind when writing higher-order
functions.

A different form of closure is the block. Blocks are written like they
are in Ruby: `{|x| x + y}`, the formal parameters between pipes,
followed by the function body. They are stack-allocated and properly
close over their environment (they see updates to closed over
variables, for example). But blocks can only be used in a limited set
of circumstances. They can be passed to other functions, but not
stored in data structures or returned.

    fn map_int(f: block(int) -> int, vec: [int]) -> [int] {
        let result = [];
        for i in vec { result += [f(i)]; }
        ret result;
    }
    map_int({|x| x + 1 }, [1, 2, 3]);

The type of blocks is spelled `block(args) -> type`. Both closures and
bare functions are automatically convert to `block`s when appropriate.
Most higher-order functions should take their function arguments as
`block`s.

A block with no arguments is written `{|| body(); }`—you can not leave
off the pipes.

## Binding

Partial application is done using the `bind` keyword in Rust.

    let daynum = bind std::vec::position(_, ["mo", "tu", "we", "do",
                                             "fr", "sa", "su"]);

Binding a function produces a closure (`lambda` type) in which some of
the arguments to the bound function have already been provided.
`daynum` will be a function taking a single string argument, and
returning the day of the week that string corresponds to (if any).

## Iteration

Functions taking blocks provide a good way to define non-trivial
iteration constructs. For example, this one iterates over a vector
of integers backwards:

    fn for_rev(v: [int], act: block(int)) {
        let i = std::vec::len(v);
        while (i > 0u) {
            i -= 1u;
            act(v[i]);
        }
    }

To run such an iteration, you could do this:

    # fn for_rev(v: [int], act: block(int)) {}
    for_rev([1, 2, 3], {|n| log n; });

But Rust allows a more pleasant syntax for this situation, with the
loop block moved out of the parenthesis and the final semicolon
omitted:

    # fn for_rev(v: [int], act: block(int)) {}
    for_rev([1, 2, 3]) {|n|
        log n;
    }
