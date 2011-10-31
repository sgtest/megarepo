# Generics

## Generic functions

Throughout this tutorial, I've been defining functions like `map` and
`for_rev` to take vectors of integers. It is 2011, and we no longer
expect to be defining such functions again and again for every type
they apply to. Thus, Rust allows functions and datatypes to have type
parameters.

    fn for_rev<T>(v: [T], act: block(T)) {
        let i = std::vec::len(v);
        while i > 0u {
            i -= 1u;
            act(v[i]);
        }
    }
    
    fn map<T, U>(f: block(T) -> U, v: [T]) -> [U] {
        let acc = [];
        for elt in v { acc += [f(elt)]; }
        ret acc;
    }

When defined in this way, these functions can be applied to any type
of vector, as long as the type of the block's argument and the type of
the vector's content agree with each other.

Inside a parameterized (generic) function, the names of the type
parameters (capitalized by convention) stand for opaque types. You
can't look inside them, but you can pass them around.

## Generic datatypes

Generic `type` and `tag` declarations follow the same pattern:

    type circular_buf<T> = {start: uint,
                            end: uint,
                            buf: [mutable T]};
    
    tag option<T> { some(T); none; }

You can then declare a function to take a `circular_buf<u8>` or return
an `option<str>`, or even an `option<T>` if the function itself is
generic.

The `option` type given above exists in the standard library as
`std::option::t`, and is the way Rust programs express the thing that
in C would be a nullable pointer. The nice part is that you have to
explicitly unpack an `option` type, so accidental null pointer
dereferences become impossible.

## Type-inference and generics

Rust's type inferrer works very well with generics, but there are
programs that just can't be typed.

    let n = none;

If you never do anything else with none, the compiler will not be able
to assign a type to it. (The same goes for `[]`, in fact.) If you
really want to have such a statement, you'll have to write it like
this:

    let n = none::<int>;

Note that, in a value expression, `<` already has a meaning as a
comparison operator, so you'll have to write `::<T>` to explicitly
give a type to a name that denotes a generic value. Fortunately, this
is rarely necessary.

## Polymorphic built-ins

There are two built-in operations that, perhaps surprisingly, act on
values of any type. It was already mentioned earlier that `log` can
take any type of value and output it as a string.

More interesting is that Rust also defines an ordering for all
datatypes, and allows you to meaningfully apply comparison operators
(`<`, `>`, `<=`, `>=`, `==`, `!=`) to them. For structural types, the
comparison happens left to right, so `"abc" < "bac"` (but note that
`"bac" < "ác"`, because the ordering acts on UTF-8 sequences without
any sophistication).

## Generic functions and argument-passing

If you try this program:

    fn plus1(x: int) -> int { x + 1 }
    map(plus1, [1, 2, 3]);

You will get an error message about argument passing styles
disagreeing. The reason is that generic types are always passed by
pointer, so `map` expects a function that takes its argument by
pointer. The `plus1` you defined, however, uses the default, efficient
way to pass integers, which is by value. To get around this issue, you
have to explicitly mark the arguments to a function that you want to
pass to a generic higher-order function as being passed by pointer:

    fn plus1(&&x: int) -> int { x + 1 }
    map(plus1, [1, 2, 3]);

NOTE: This is inconvenient, and we are hoping to get rid of this
restriction in the future.
