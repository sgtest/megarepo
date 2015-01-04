% The Rust Guide

Hey there! Welcome to the Rust guide. This is the place to be if you'd like to
learn how to program in Rust. Rust is a systems programming language with a
focus on "high-level, bare-metal programming": the lowest level control a
programming language can give you, but with zero-cost, higher level
abstractions, because people aren't computers. We really think Rust is
something special, and we hope you do too.

To show you how to get going with Rust, we're going to write the traditional
"Hello, World!" program. Next, we'll introduce you to a tool that's useful for
writing real-world Rust programs and libraries: "Cargo." After that, we'll talk
about the basics of Rust, write a little program to try them out, and then learn
more advanced things.

Sound good? Let's go!

# Installing Rust

The first step to using Rust is to install it! There are a number of ways to
install Rust, but the easiest is to use the `rustup` script. If you're on
Linux or a Mac, all you need to do is this (note that you don't need to type
in the `$`s, they just indicate the start of each command):

```bash
curl -L https://static.rust-lang.org/rustup.sh | sudo sh
```

If you're concerned about the [potential insecurity](http://curlpipesh.tumblr.com/) of using `curl | sudo sh`,
please keep reading and see our disclaimer below. And feel free to use a two-step version of the installation and examine our installation script:

```bash
curl -L https://static.rust-lang.org/rustup.sh -O
sudo sh rustup.sh
```

If you're on Windows, please download either the [32-bit
installer](https://static.rust-lang.org/dist/rust-nightly-i686-pc-windows-gnu.exe)
or the [64-bit
installer](https://static.rust-lang.org/dist/rust-nightly-x86_64-pc-windows-gnu.exe)
and run it.

If you decide you don't want Rust anymore, we'll be a bit sad, but that's okay.
Not every programming language is great for everyone. Just pass an argument to
the script:

```bash
$ curl -s https://static.rust-lang.org/rustup.sh | sudo sh -s -- --uninstall
```

If you used the Windows installer, just re-run the `.exe` and it will give you
an uninstall option.

You can re-run this script any time you want to update Rust. Which, at this
point, is often. Rust is still pre-1.0, and so people assume that you're using
a very recent Rust.

This brings me to one other point: some people, and somewhat rightfully so, get
very upset when we tell you to `curl | sudo sh`. And they should be! Basically,
when you do this, you are trusting that the good people who maintain Rust
aren't going to hack your computer and do bad things. That's a good instinct!
If you're one of those people, please check out the documentation on [building
Rust from Source](https://github.com/rust-lang/rust#building-from-source), or
[the official binary downloads](http://www.rust-lang.org/install.html). And we
promise that this method will not be the way to install Rust forever: it's just
the easiest way to keep people updated while Rust is in its alpha state.

Oh, we should also mention the officially supported platforms:

* Windows (7, 8, Server 2008 R2)
* Linux (2.6.18 or later, various distributions), x86 and x86-64
* OSX 10.7 (Lion) or greater, x86 and x86-64

We extensively test Rust on these platforms, and a few others, too, like
Android. But these are the ones most likely to work, as they have the most
testing.

Finally, a comment about Windows. Rust considers Windows to be a first-class
platform upon release, but if we're honest, the Windows experience isn't as
integrated as the Linux/OS X experience is. We're working on it! If anything
does not work, it is a bug. Please let us know if that happens. Each and every
commit is tested against Windows just like any other platform.

If you've got Rust installed, you can open up a shell, and type this:

```bash
$ rustc --version
```

You should see some output that looks something like this:

```bash
rustc 0.12.0-nightly (b7aa03a3c 2014-09-28 11:38:01 +0000)
```

If you did, Rust has been installed successfully! Congrats!

If not, there are a number of places where you can get help. The easiest is
[the #rust IRC channel on irc.mozilla.org](irc://irc.mozilla.org/#rust), which
you can access through
[Mibbit](http://chat.mibbit.com/?server=irc.mozilla.org&channel=%23rust). Click
that link, and you'll be chatting with other Rustaceans (a silly nickname we
call ourselves), and we can help you out. Other great resources include [our
forum](http://discuss.rust-lang.org/), [the /r/rust
subreddit](http://www.reddit.com/r/rust), and [Stack
Overflow](http://stackoverflow.com/questions/tagged/rust).

# Hello, world!

Now that you have Rust installed, let's write your first Rust program. It's
traditional to make your first program in any new language one that prints the
text "Hello, world!" to the screen. The nice thing about starting with such a
simple program is that you can verify that your compiler isn't just installed,
but also working properly. And printing information to the screen is a pretty
common thing to do.

The first thing that we need to do is make a file to put our code in. I like
to make a `projects` directory in my home directory, and keep all my projects
there. Rust does not care where your code lives.

This actually leads to one other concern we should address: this guide will
assume that you have basic familiarity with the command line. Rust does not
require that you know a whole ton about the command line, but until the
language is in a more finished state, IDE support is spotty. Rust makes no
specific demands on your editing tooling, or where your code lives.

With that said, let's make a directory in our projects directory.

```{bash}
$ mkdir ~/projects
$ cd ~/projects
$ mkdir hello_world
$ cd hello_world
```

If you're on Windows and not using PowerShell, the `~` may not work. Consult
the documentation for your shell for more details.

Let's make a new source file next. I'm going to use the syntax `editor
filename` to represent editing a file in these examples, but you should use
whatever method you want. We'll call our file `main.rs`:

```{bash}
$ editor main.rs
```

Rust files always end in a `.rs` extension. If you're using more than one word
in your filename, use an underscore. `hello_world.rs` rather than
`helloworld.rs`.

Now that you've got your file open, type this in:

```{rust}
fn main() {
    println!("Hello, world!");
}
```

Save the file, and then type this into your terminal window:

```{bash}
$ rustc main.rs
$ ./main # or main.exe on Windows
Hello, world!
```

You can also run these examples on [play.rust-lang.org](http://play.rust-lang.org/) by clicking on the arrow that appears in the upper right of the example when you mouse over the code.

Success! Let's go over what just happened in detail.

```{rust}
fn main() {

}
```

These lines define a **function** in Rust. The `main` function is special:
it's the beginning of every Rust program. The first line says "I'm declaring a
function named `main`, which takes no arguments and returns nothing." If there
were arguments, they would go inside the parentheses (`(` and `)`), and because
we aren't returning anything from this function, we've dropped that notation
entirely.  We'll get to it later.

You'll also note that the function is wrapped in curly braces (`{` and `}`).
Rust requires these around all function bodies. It is also considered good
style to put the opening curly brace on the same line as the function
declaration, with one space in between.

Next up is this line:

```{rust}
    println!("Hello, world!");
```

This line does all of the work in our little program. There are a number of
details that are important here. The first is that it's indented with four
spaces, not tabs. Please configure your editor of choice to insert four spaces
with the tab key. We provide some [sample configurations for various
editors](https://github.com/rust-lang/rust/tree/master/src/etc).

The second point is the `println!()` part. This is calling a Rust **macro**,
which is how metaprogramming is done in Rust. If it were a function instead, it
would look like this: `println()`. For our purposes, we don't need to worry
about this difference. Just know that sometimes, you'll see a `!`, and that
means that you're calling a macro instead of a normal function. Rust implements
`println!` as a macro rather than a function for good reasons, but that's a
very advanced topic. You'll learn more when we talk about macros later. One
last thing to mention: Rust's macros are significantly different from C macros,
if you've used those. Don't be scared of using macros. We'll get to the details
eventually, you'll just have to trust us for now.

Next, `"Hello, world!"` is a **string**. Strings are a surprisingly complicated
topic in a systems programming language, and this is a **statically allocated**
string. We will talk more about different kinds of allocation later. We pass
this string as an argument to `println!`, which prints the string to the
screen. Easy enough!

Finally, the line ends with a semicolon (`;`). Rust is an **expression
oriented** language, which means that most things are expressions. The `;` is
used to indicate that this expression is over, and the next one is ready to
begin. Most lines of Rust code end with a `;`. We will cover this in-depth
later in the guide.

Finally, actually **compiling** and **running** our program. We can compile
with our compiler, `rustc`, by passing it the name of our source file:

```{bash}
$ rustc main.rs
```

This is similar to `gcc` or `clang`, if you come from a C or C++ background. Rust
will output a binary executable. You can see it with `ls`:

```{bash}
$ ls
main  main.rs
```

Or on Windows:

```{bash}
$ dir
main.exe  main.rs
```

There are now two files: our source code, with the `.rs` extension, and the
executable (`main.exe` on Windows, `main` everywhere else)

```{bash}
$ ./main  # or main.exe on Windows
```

This prints out our `Hello, world!` text to our terminal.

If you come from a dynamically typed language like Ruby, Python, or JavaScript,
you may not be used to these two steps being separate. Rust is an
**ahead-of-time compiled language**, which means that you can compile a
program, give it to someone else, and they don't need to have Rust installed.
If you give someone a `.rb` or `.py` or `.js` file, they need to have
Ruby/Python/JavaScript installed, but you just need one command to both compile
and run your program. Everything is a tradeoff in language design, and Rust has
made its choice.

Congratulations! You have officially written a Rust program. That makes you a
Rust programmer! Welcome.

Next, I'd like to introduce you to another tool, Cargo, which is used to write
real-world Rust programs. Just using `rustc` is nice for simple things, but as
your project grows, you'll want something to help you manage all of the options
that it has, and to make it easy to share your code with other people and
projects.

# Hello, Cargo!

[Cargo](http://crates.io) is a tool that Rustaceans use to help manage their
Rust projects. Cargo is currently in an alpha state, just like Rust, and so it
is still a work in progress. However, it is already good enough to use for many
Rust projects, and so it is assumed that Rust projects will use Cargo from the
beginning.

Cargo manages three things: building your code, downloading the dependencies
your code needs, and building the dependencies your code needs.  At first, your
program doesn't have any dependencies, so we'll only be using the first part of
its functionality. Eventually, we'll add more. Since we started off by using
Cargo, it'll be easy to add later.

Let's convert Hello World to Cargo. The first thing we need to do to begin
using Cargo is to install Cargo. Luckily for us, the script we ran to install
Rust includes Cargo by default. If you installed Rust some other way, you may
want to [check the Cargo
README](https://github.com/rust-lang/cargo#installing-cargo-from-nightlies)
for specific instructions about installing it.

To Cargo-ify our project, we need to do two things: Make a `Cargo.toml`
configuration file, and put our source file in the right place. Let's
do that part first:

```{bash}
$ mkdir src
$ mv main.rs src/main.rs
```

Cargo expects your source files to live inside a `src` directory. That leaves
the top level for other things, like READMEs, license information, and anything
not related to your code. Cargo helps us keep our projects nice and tidy. A
place for everything, and everything in its place.

Next, our configuration file:

```{bash}
$ editor Cargo.toml
```

Make sure to get this name right: you need the capital `C`!

Put this inside:

```toml
[package]

name = "hello_world"
version = "0.0.1"
authors = [ "Your name <you@example.com>" ]

[[bin]]

name = "hello_world"
```

This file is in the [TOML](https://github.com/toml-lang/toml) format. Let's let
it explain itself to you:

> TOML aims to be a minimal configuration file format that's easy to read due
> to obvious semantics. TOML is designed to map unambiguously to a hash table.
> TOML should be easy to parse into data structures in a wide variety of
> languages.

TOML is very similar to INI, but with some extra goodies.

Anyway, there are two **table**s in this file: `package` and `bin`. The first
tells Cargo metadata about your package. The second tells Cargo that we're
interested in building a binary, not a library (though we could do both!), as
well as what it is named.

Once you have this file in place, we should be ready to build! Try this:

```{bash}
$ cargo build
   Compiling hello_world v0.0.1 (file:///home/yourname/projects/hello_world)
$ ./target/hello_world
Hello, world!
```

Bam! We build our project with `cargo build`, and run it with
`./target/hello_world`. This hasn't bought us a whole lot over our simple use
of `rustc`, but think about the future: when our project has more than one
file, we would need to call `rustc` twice, and pass it a bunch of options to
tell it to build everything together. With Cargo, as our project grows, we can
just `cargo build` and it'll work the right way.

You'll also notice that Cargo has created a new file: `Cargo.lock`.

```toml
[root]
name = "hello_world"
version = "0.0.1"
```

This file is used by Cargo to keep track of dependencies in your application.
Right now, we don't have any, so it's a bit sparse. You won't ever need
to touch this file yourself, just let Cargo handle it.

That's it! We've successfully built `hello_world` with Cargo. Even though our
program is simple, it's using much of the real tooling that you'll use for the
rest of your Rust career.

Now that you've got the tools down, let's actually learn more about the Rust
language itself. These are the basics that will serve you well through the rest
of your time with Rust.

# Variable bindings

The first thing we'll learn about are 'variable bindings.' They look like this:

```{rust}
fn main() {
    let x = 5i;
}
```

Putting `fn main() {` in each example is a bit tedious, so we'll leave that out
in the future. If you're following along, make sure to edit your `main()`
function, rather than leaving it off. Otherwise, you'll get an error.

In many languages, this is called a 'variable.' But Rust's variable bindings
have a few tricks up their sleeves. Rust has a very powerful feature called
'pattern matching' that we'll get into detail with later, but the left
hand side of a `let` expression is a full pattern, not just a variable name.
This means we can do things like:

```{rust}
let (x, y) = (1i, 2i);
```

After this expression is evaluated, `x` will be one, and `y` will be two.
Patterns are really powerful, but this is about all we can do with them so far.
So let's just keep this in the back of our minds as we go forward.

By the way, in these examples, `i` indicates that the number is an integer.

Rust is a statically typed language, which means that we specify our types up
front. So why does our first example compile? Well, Rust has this thing called
"type inference." If it can figure out what the type of something is, Rust
doesn't require you to actually type it out.

We can add the type if we want to, though. Types come after a colon (`:`):

```{rust}
let x: int = 5;
```

If I asked you to read this out loud to the rest of the class, you'd say "`x`
is a binding with the type `int` and the value `five`."

In future examples, we may annotate the type in a comment. The examples will
look like this:

```{rust}
fn main() {
    let x = 5i; // x: int
}
```

Note the similarities between this annotation and the syntax you use with `let`.
Including these kinds of comments is not idiomatic Rust, but we'll occasionally
include them to help you understand what the types that Rust infers are.

By default, bindings are **immutable**. This code will not compile:

```{ignore}
let x = 5i;
x = 10i;
```

It will give you this error:

```text
error: re-assignment of immutable variable `x`
     x = 10i;
     ^~~~~~~
```

If you want a binding to be mutable, you can use `mut`:

```{rust}
let mut x = 5i; // mut x: int
x = 10i;
```

There is no single reason that bindings are immutable by default, but we can
think about it through one of Rust's primary focuses: safety. If you forget to
say `mut`, the compiler will catch it, and let you know that you have mutated
something you may not have intended to mutate. If bindings were mutable by
default, the compiler would not be able to tell you this. If you _did_ intend
mutation, then the solution is quite easy: add `mut`.

There are other good reasons to avoid mutable state when possible, but they're
out of the scope of this guide. In general, you can often avoid explicit
mutation, and so it is preferable in Rust. That said, sometimes, mutation is
what you need, so it's not verboten.

Let's get back to bindings. Rust variable bindings have one more aspect that
differs from other languages: bindings are required to be initialized with a
value before you're allowed to use them. If we try...

```{ignore}
let x;
```

...we'll get an error:

```text
src/main.rs:2:9: 2:10 error: cannot determine a type for this local variable: unconstrained type
src/main.rs:2     let x;
                      ^
```

Giving it a type will compile, though:

```{rust}
let x: int;
```

Let's try it out. Change your `src/main.rs` file to look like this:

```{rust}
fn main() {
    let x: int;

    println!("Hello world!");
}
```

You can use `cargo build` on the command line to build it. You'll get a warning,
but it will still print "Hello, world!":

```text
   Compiling hello_world v0.0.1 (file:///home/you/projects/hello_world)
src/main.rs:2:9: 2:10 warning: unused variable: `x`, #[warn(unused_variable)] on by default
src/main.rs:2     let x: int;
                      ^
```

Rust warns us that we never use the variable binding, but since we never use it,
no harm, no foul. Things change if we try to actually use this `x`, however. Let's
do that. Change your program to look like this:

```{rust,ignore}
fn main() {
    let x: int;

    println!("The value of x is: {}", x);
}
```

And try to build it. You'll get an error:

```{bash}
$ cargo build
   Compiling hello_world v0.0.1 (file:///home/you/projects/hello_world)
src/main.rs:4:39: 4:40 error: use of possibly uninitialized variable: `x`
src/main.rs:4     println!("The value of x is: {}", x);
                                                    ^
note: in expansion of format_args!
<std macros>:2:23: 2:77 note: expansion site
<std macros>:1:1: 3:2 note: in expansion of println!
src/main.rs:4:5: 4:42 note: expansion site
error: aborting due to previous error
Could not compile `hello_world`.
```

Rust will not let us use a value that has not been initialized. Next, let's
talk about this stuff we've added to `println!`.

If you include two curly braces (`{}`, some call them moustaches...) in your
string to print, Rust will interpret this as a request to interpolate some sort
of value. **String interpolation** is a computer science term that means "stick
in the middle of a string." We add a comma, and then `x`, to indicate that we
want `x` to be the value we're interpolating. The comma is used to separate
arguments we pass to functions and macros, if you're passing more than one.

When you just use the curly braces, Rust will attempt to display the
value in a meaningful way by checking out its type. If you want to specify the
format in a more detailed manner, there are a [wide number of options
available](std/fmt/index.html). For now, we'll just stick to the default:
integers aren't very complicated to print.

# `if`

Rust's take on `if` is not particularly complex, but it's much more like the
`if` you'll find in a dynamically typed language than in a more traditional
systems language. So let's talk about it, to make sure you grasp the nuances.

`if` is a specific form of a more general concept, the 'branch.' The name comes
from a branch in a tree: a decision point, where depending on a choice,
multiple paths can be taken.

In the case of `if`, there is one choice that leads down two paths:

```rust
let x = 5i;

if x == 5i {
    println!("x is five!");
}
```

If we changed the value of `x` to something else, this line would not print.
More specifically, if the expression after the `if` evaluates to `true`, then
the block is executed. If it's `false`, then it is not.

If you want something to happen in the `false` case, use an `else`:

```{rust}
let x = 5i;

if x == 5i {
    println!("x is five!");
} else {
    println!("x is not five :(");
}
```

This is all pretty standard. However, you can also do this:


```{rust}
let x = 5i;

let y = if x == 5i {
    10i
} else {
    15i
}; // y: int
```

Which we can (and probably should) write like this:

```{rust}
let x = 5i;

let y = if x == 5i { 10i } else { 15i }; // y: int
```

This reveals two interesting things about Rust: it is an expression-based
language, and semicolons are different from semicolons in other 'curly brace
and semicolon'-based languages. These two things are related.

## Expressions vs. Statements

Rust is primarily an expression based language. There are only two kinds of
statements, and everything else is an expression.

So what's the difference? Expressions return a value, and statements do not.
In many languages, `if` is a statement, and therefore, `let x = if ...` would
make no sense. But in Rust, `if` is an expression, which means that it returns
a value. We can then use this value to initialize the binding.

Speaking of which, bindings are a kind of the first of Rust's two statements.
The proper name is a **declaration statement**. So far, `let` is the only kind
of declaration statement we've seen. Let's talk about that some more.

In some languages, variable bindings can be written as expressions, not just
statements. Like Ruby:

```{ruby}
x = y = 5
```

In Rust, however, using `let` to introduce a binding is _not_ an expression. The
following will produce a compile-time error:

```{ignore}
let x = (let y = 5i); // expected identifier, found keyword `let`
```

The compiler is telling us here that it was expecting to see the beginning of
an expression, and a `let` can only begin a statement, not an expression.

Note that assigning to an already-bound variable (e.g. `y = 5i`) is still an
expression, although its value is not particularly useful. Unlike C, where an
assignment evaluates to the assigned value (e.g. `5i` in the previous example),
in Rust the value of an assignment is the unit type `()` (which we'll cover later).

The second kind of statement in Rust is the **expression statement**. Its
purpose is to turn any expression into a statement. In practical terms, Rust's
grammar expects statements to follow other statements. This means that you use
semicolons to separate expressions from each other. This means that Rust
looks a lot like most other languages that require you to use semicolons
at the end of every line, and you will see semicolons at the end of almost
every line of Rust code you see.

What is this exception that makes us say 'almost?' You saw it already, in this
code:

```{rust}
let x = 5i;

let y: int = if x == 5i { 10i } else { 15i };
```

Note that I've added the type annotation to `y`, to specify explicitly that I
want `y` to be an integer.

This is not the same as this, which won't compile:

```{ignore}
let x = 5i;

let y: int = if x == 5i { 10i; } else { 15i; };
```

Note the semicolons after the 10 and 15. Rust will give us the following error:

```text
error: mismatched types: expected `int` but found `()` (expected int but found ())
```

We expected an integer, but we got `()`. `()` is pronounced 'unit', and is a
special type in Rust's type system. In Rust, `()` is _not_ a valid value for a
variable of type `int`. It's only a valid value for variables of the type `()`,
which aren't very useful. Remember how we said statements don't return a value?
Well, that's the purpose of unit in this case. The semicolon turns any
expression into a statement by throwing away its value and returning unit
instead.

There's one more time in which you won't see a semicolon at the end of a line
of Rust code. For that, we'll need our next concept: functions.

# Functions

You've already seen one function so far, the `main` function:

```{rust}
fn main() {
}
```

This is the simplest possible function declaration. As we mentioned before,
`fn` says 'this is a function,' followed by the name, some parentheses because
this function takes no arguments, and then some curly braces to indicate the
body. Here's a function named `foo`:

```{rust}
fn foo() {
}
```

So, what about taking arguments? Here's a function that prints a number:

```{rust}
fn print_number(x: int) {
    println!("x is: {}", x);
}
```

Here's a complete program that uses `print_number`:

```{rust}
fn main() {
    print_number(5);
}

fn print_number(x: int) {
    println!("x is: {}", x);
}
```

As you can see, function arguments work very similar to `let` declarations:
you add a type to the argument name, after a colon.

Here's a complete program that adds two numbers together and prints them:

```{rust}
fn main() {
    print_sum(5, 6);
}

fn print_sum(x: int, y: int) {
    println!("sum is: {}", x + y);
}
```

You separate arguments with a comma, both when you call the function, as well
as when you declare it.

Unlike `let`, you _must_ declare the types of function arguments. This does
not work:

```{ignore}
fn print_number(x, y) {
    println!("x is: {}", x + y);
}
```

You get this error:

```text
hello.rs:5:18: 5:19 error: expected `:` but found `,`
hello.rs:5 fn print_number(x, y) {
```

This is a deliberate design decision. While full-program inference is possible,
languages which have it, like Haskell, often suggest that documenting your
types explicitly is a best-practice. We agree that forcing functions to declare
types while allowing for inference inside of function bodies is a wonderful
sweet spot between full inference and no inference.

What about returning a value? Here's a function that adds one to an integer:

```{rust}
fn add_one(x: int) -> int {
    x + 1
}
```

Rust functions return exactly one value, and you declare the type after an
'arrow', which is a dash (`-`) followed by a greater-than sign (`>`).

You'll note the lack of a semicolon here. If we added it in:

```{ignore}
fn add_one(x: int) -> int {
    x + 1;
}
```

We would get an error:

```text
error: not all control paths return a value
fn add_one(x: int) -> int {
     x + 1;
}

help: consider removing this semicolon:
     x + 1;
          ^
```

Remember our earlier discussions about semicolons and `()`? Our function claims
to return an `int`, but with a semicolon, it would return `()` instead. Rust
realizes this probably isn't what we want, and suggests removing the semicolon.

This is very much like our `if` statement before: the result of the block
(`{}`) is the value of the expression. Other expression-oriented languages,
such as Ruby, work like this, but it's a bit unusual in the systems programming
world. When people first learn about this, they usually assume that it
introduces bugs. But because Rust's type system is so strong, and because unit
is its own unique type, we have never seen an issue where adding or removing a
semicolon in a return position would cause a bug.

But what about early returns? Rust does have a keyword for that, `return`:

```{rust}
fn foo(x: int) -> int {
    if x < 5 { return x; }

    x + 1
}
```

Using a `return` as the last line of a function works, but is considered poor
style:

```{rust}
fn foo(x: int) -> int {
    if x < 5 { return x; }

    return x + 1;
}
```

There are some additional ways to define functions, but they involve features
that we haven't learned about yet, so let's just leave it at that for now.


# Comments

Now that we have some functions, it's a good idea to learn about comments.
Comments are notes that you leave to other programmers to help explain things
about your code. The compiler mostly ignores them.

Rust has two kinds of comments that you should care about: **line comment**s
and **doc comment**s.

```{rust}
// Line comments are anything after '//' and extend to the end of the line.

let x = 5i; // this is also a line comment.

// If you have a long explanation for something, you can put line comments next
// to each other. Put a space between the // and your comment so that it's
// more readable.
```

The other kind of comment is a doc comment. Doc comments use `///` instead of
`//`, and support Markdown notation inside:

```{rust}
/// `hello` is a function that prints a greeting that is personalized based on
/// the name given.
///
/// # Arguments
///
/// * `name` - The name of the person you'd like to greet.
///
/// # Example
///
/// ```rust
/// let name = "Steve";
/// hello(name); // prints "Hello, Steve!"
/// ```
fn hello(name: &str) {
    println!("Hello, {}!", name);
}
```

When writing doc comments, adding sections for any arguments, return values,
and providing some examples of usage is very, very helpful.

You can use the [`rustdoc`](rustdoc.html) tool to generate HTML documentation
from these doc comments.

# Compound Data Types

Rust, like many programming languages, has a number of different data types
that are built-in. You've already done some simple work with integers and
strings, but next, let's talk about some more complicated ways of storing data.

## Tuples

The first compound data type we're going to talk about are called **tuple**s.
Tuples are an ordered list of a fixed size. Like this:

```rust
let x = (1i, "hello");
```

The parentheses and commas form this two-length tuple. Here's the same code, but
with the type annotated:

```rust
let x: (int, &str) = (1, "hello");
```

As you can see, the type of a tuple looks just like the tuple, but with each
position having a type name rather than the value. Careful readers will also
note that tuples are heterogeneous: we have an `int` and a `&str` in this tuple.
You haven't seen `&str` as a type before, and we'll discuss the details of
strings later. In systems programming languages, strings are a bit more complex
than in other languages. For now, just read `&str` as "a string slice," and
we'll learn more soon.

You can access the fields in a tuple through a **destructuring let**. Here's
an example:

```rust
let (x, y, z) = (1i, 2i, 3i);

println!("x is {}", x);
```

Remember before when I said the left-hand side of a `let` statement was more
powerful than just assigning a binding? Here we are. We can put a pattern on
the left-hand side of the `let`, and if it matches up to the right-hand side,
we can assign multiple bindings at once. In this case, `let` 'destructures,'
or 'breaks up,' the tuple, and assigns the bits to three bindings.

This pattern is very powerful, and we'll see it repeated more later.

There are also a few things you can do with a tuple as a whole, without
destructuring. You can assign one tuple into another, if they have the same
arity and contained types.

```rust
let mut x = (1i, 2i); // x: (int, int)
let y = (2i, 3i);     // y: (int, int)

x = y;
```

You can also check for equality with `==`. Again, this will only compile if the
tuples have the same type.

```rust
let x = (1i, 2i, 3i);
let y = (2i, 2i, 4i);

if x == y {
    println!("yes");
} else {
    println!("no");
}
```

This will print `no`, because some of the values aren't equal.

One other use of tuples is to return multiple values from a function:

```rust
fn next_two(x: int) -> (int, int) { (x + 1i, x + 2i) }

fn main() {
    let (x, y) = next_two(5i);
    println!("x, y = {}, {}", x, y);
}
```

Even though Rust functions can only return one value, a tuple _is_ one value,
that happens to be made up of two. You can also see in this example how you
can destructure a pattern returned by a function, as well.

Tuples are a very simple data structure, and so are not often what you want.
Let's move on to their bigger sibling, structs.

## Structs

A struct is another form of a 'record type,' just like a tuple. There's a
difference: structs give each element that they contain a name, called a
'field' or a 'member.' Check it out:

```rust
struct Point {
    x: int,
    y: int,
}

fn main() {
    let origin = Point { x: 0i, y: 0i }; // origin: Point

    println!("The origin is at ({}, {})", origin.x, origin.y);
}
```

There's a lot going on here, so let's break it down. We declare a struct with
the `struct` keyword, and then with a name. By convention, structs begin with a
capital letter and are also camel cased: `PointInSpace`, not `Point_In_Space`.

We can create an instance of our struct via `let`, as usual, but we use a `key:
value` style syntax to set each field. The order doesn't need to be the same as
in the original declaration.

Finally, because fields have names, we can access the field through dot
notation: `origin.x`.

The values in structs are immutable by default, like other bindings in Rust.
Use `mut` to make them mutable:

```{rust}
struct Point {
    x: int,
    y: int,
}

fn main() {
    let mut point = Point { x: 0i, y: 0i };

    point.x = 5;

    println!("The point is at ({}, {})", point.x, point.y);
}
```

This will print `The point is at (5, 0)`.

## Tuple Structs and Newtypes

Rust has another data type that's like a hybrid between a tuple and a struct,
called a **tuple struct**. Tuple structs do have a name, but their fields
don't:


```{rust}
struct Color(int, int, int);
struct Point(int, int, int);
```

These two will not be equal, even if they have the same values:

```{rust}
# struct Color(int, int, int);
# struct Point(int, int, int);
let black  = Color(0, 0, 0);
let origin = Point(0, 0, 0);
```

It is almost always better to use a struct than a tuple struct. We would write
`Color` and `Point` like this instead:

```{rust}
struct Color {
    red: int,
    blue: int,
    green: int,
}

struct Point {
    x: int,
    y: int,
    z: int,
}
```

Now, we have actual names, rather than positions. Good names are important,
and with a struct, we have actual names.

There _is_ one case when a tuple struct is very useful, though, and that's a
tuple struct with only one element. We call this a 'newtype,' because it lets
you create a new type that's a synonym for another one:

```{rust}
struct Inches(int);

let length = Inches(10);

let Inches(integer_length) = length;
println!("length is {} inches", integer_length);
```

As you can see here, you can extract the inner integer type through a
destructuring `let`.

## Enums

Finally, Rust has a "sum type", an **enum**. Enums are an incredibly useful
feature of Rust, and are used throughout the standard library. This is an enum
that is provided by the Rust standard library:

```{rust}
enum Ordering {
    Less,
    Equal,
    Greater,
}
```

An `Ordering` can only be _one_ of `Less`, `Equal`, or `Greater` at any given
time.

Because `Ordering` is provided by the standard library, we can use the `use`
keyword to use it in our code. We'll learn more about `use` later, but it's
used to bring names into scope.

Here's an example of how to use `Ordering`:

```{rust}
use std::cmp::Ordering;

fn cmp(a: int, b: int) -> Ordering {
    if a < b { Ordering::Less }
    else if a > b { Ordering::Greater }
    else { Ordering::Equal }
}

fn main() {
    let x = 5i;
    let y = 10i;

    let ordering = cmp(x, y); // ordering: Ordering

    if ordering == Ordering::Less {
        println!("less");
    } else if ordering == Ordering::Greater {
        println!("greater");
    } else if ordering == Ordering::Equal {
        println!("equal");
    }
}
```

There's a symbol here we haven't seen before: the double colon (`::`).
This is used to indicate a namesapce. In this case, `Ordering` lives in
the `cmp` submodule of the `std` module. We'll talk more about modules
later in the guide. For now, all you need to know is that you can `use`
things from the standard library if you need them.

Okay, let's talk about the actual code in the example. `cmp` is a function that
compares two things, and returns an `Ordering`. We return either
`Ordering::Less`, `Ordering::Greater`, or `Ordering::Equal`, depending on if
the two values are greater, less, or equal. Note that each variant of the
`enum` is namespaced under the `enum` itself: it's `Ordering::Greater` not
`Greater`.

The `ordering` variable has the type `Ordering`, and so contains one of the
three values. We can then do a bunch of `if`/`else` comparisons to check which
one it is. However, repeated `if`/`else` comparisons get quite tedious. Rust
has a feature that not only makes them nicer to read, but also makes sure that
you never miss a case. Before we get to that, though, let's talk about another
kind of enum: one with values.

This enum has two variants, one of which has a value:

```{rust}
enum OptionalInt {
    Value(int),
    Missing,
}
```

This enum represents an `int` that we may or may not have. In the `Missing`
case, we have no value, but in the `Value` case, we do. This enum is specific
to `int`s, though. We can make it usable by any type, but we haven't quite
gotten there yet!

You can also have any number of values in an enum:

```{rust}
enum OptionalColor {
    Color(int, int, int),
    Missing,
}
```

And you can also have something like this:

```{rust}
enum StringResult {
    StringOK(String),
    ErrorReason(String),
}
```
Where a `StringResult` is either a `StringResult::StringOK`, with the result of
a computation, or an `StringResult::ErrorReason` with a `String` explaining
what caused the computation to fail. These kinds of `enum`s are actually very
useful and are even part of the standard library.

Here is an example of using our `StringResult`:

```rust
enum StringResult {
    StringOK(String),
    ErrorReason(String),
}

fn respond(greeting: &str) -> StringResult {
    if greeting == "Hello" {
        StringResult::StringOK("Good morning!".to_string())
    } else {
        StringResult::ErrorReason("I didn't understand you!".to_string())
    }
}
```

That's a lot of typing! We can use the `use` keyword to make it shorter:

```rust
use StringResult::StringOK;
use StringResult::ErrorReason;

enum StringResult {
    StringOK(String),
    ErrorReason(String),
}

# fn main() {}

fn respond(greeting: &str) -> StringResult {
    if greeting == "Hello" {
        StringOK("Good morning!".to_string())
    } else {
        ErrorReason("I didn't understand you!".to_string())
    }
}
```

`use` declarations must come before anything else, which looks a little strange in this example,
since we `use` the variants before we define them. Anyway, in the body of `respond`, we can just
say `StringOK` now, rather than the full `StringResult::StringOK`. Importing variants can be
convenient, but can also cause name conflicts, so do this with caution. It's considered good style
to rarely import variants for this reason.

As you can see, `enum`s with values are quite a powerful tool for data representation,
and can be even more useful when they're generic across types. Before we get to generics,
though, let's talk about how to use them with pattern matching, a tool that will
let us deconstruct this sum type (the type theory term for enums) in a very elegant
way and avoid all these messy `if`/`else`s.

# Match

Often, a simple `if`/`else` isn't enough, because you have more than two
possible options. Also, `else` conditions can get incredibly complicated, so
what's the solution?

Rust has a keyword, `match`, that allows you to replace complicated `if`/`else`
groupings with something more powerful. Check it out:

```{rust}
let x = 5i;

match x {
    1 => println!("one"),
    2 => println!("two"),
    3 => println!("three"),
    4 => println!("four"),
    5 => println!("five"),
    _ => println!("something else"),
}
```

`match` takes an expression and then branches based on its value. Each 'arm' of
the branch is of the form `val => expression`. When the value matches, that arm's
expression will be evaluated. It's called `match` because of the term 'pattern
matching', which `match` is an implementation of.

So what's the big advantage here? Well, there are a few. First of all, `match`
enforces 'exhaustiveness checking'. Do you see that last arm, the one with the
underscore (`_`)? If we remove that arm, Rust will give us an error:

```text
error: non-exhaustive patterns: `_` not covered
```

In other words, Rust is trying to tell us we forgot a value. Because `x` is an
integer, Rust knows that it can have a number of different values – for example,
`6i`. Without the `_`, however, there is no arm that could match, and so Rust refuses
to compile. `_` acts like a 'catch-all arm'. If none of the other arms match,
the arm with `_` will, and since we have this catch-all arm, we now have an arm
for every possible value of `x`, and so our program will compile successfully.

`match` statements also destructure enums, as well. Remember this code from the
section on enums?

```{rust}
use std::cmp::Ordering;

fn cmp(a: int, b: int) -> Ordering {
    if a < b { Ordering::Less }
    else if a > b { Ordering::Greater }
    else { Ordering::Equal }
}

fn main() {
    let x = 5i;
    let y = 10i;

    let ordering = cmp(x, y);

    if ordering == Ordering::Less {
        println!("less");
    } else if ordering == Ordering::Greater {
        println!("greater");
    } else if ordering == Ordering::Equal {
        println!("equal");
    }
}
```

We can re-write this as a `match`:

```{rust}
use std::cmp::Ordering;

fn cmp(a: int, b: int) -> Ordering {
    if a < b { Ordering::Less }
    else if a > b { Ordering::Greater }
    else { Ordering::Equal }
}

fn main() {
    let x = 5i;
    let y = 10i;

    match cmp(x, y) {
        Ordering::Less    => println!("less"),
        Ordering::Greater => println!("greater"),
        Ordering::Equal   => println!("equal"),
    }
}
```

This version has way less noise, and it also checks exhaustively to make sure
that we have covered all possible variants of `Ordering`. With our `if`/`else`
version, if we had forgotten the `Greater` case, for example, our program would
have happily compiled. If we forget in the `match`, it will not. Rust helps us
make sure to cover all of our bases.

`match` expressions also allow us to get the values contained in an `enum`
(also known as destructuring) as follows:

```{rust}
enum OptionalInt {
    Value(int),
    Missing,
}

fn main() {
    let x = OptionalInt::Value(5);
    let y = OptionalInt::Missing;

    match x {
        OptionalInt::Value(n) => println!("x is {}", n),
        OptionalInt::Missing  => println!("x is missing!"),
    }

    match y {
        OptionalInt::Value(n) => println!("y is {}", n),
        OptionalInt::Missing  => println!("y is missing!"),
    }
}
```

That is how you can get and use the values contained in `enum`s.
It can also allow us to handle errors or unexpected computations; for example, a
function that is not guaranteed to be able to compute a result (an `int` here)
could return an `OptionalInt`, and we would handle that value with a `match`.
As you can see, `enum` and `match` used together are quite useful!

`match` is also an expression, which means we can use it on the right-hand
side of a `let` binding or directly where an expression is used. We could
also implement the previous line like this:

```{rust}
use std::cmp::Ordering;

fn cmp(a: int, b: int) -> Ordering {
    if a < b { Ordering::Less }
    else if a > b { Ordering::Greater }
    else { Ordering::Equal }
}

fn main() {
    let x = 5i;
    let y = 10i;

    println!("{}", match cmp(x, y) {
        Ordering::Less    => "less",
        Ordering::Greater => "greater",
        Ordering::Equal   => "equal",
    });
}
```

Sometimes, it's a nice pattern.

# Looping

Looping is the last basic construct that we haven't learned yet in Rust. Rust has
two main looping constructs: `for` and `while`.

## `for`

The `for` loop is used to loop a particular number of times. Rust's `for` loops
work a bit differently than in other systems languages, however. Rust's `for`
loop doesn't look like this "C-style" `for` loop:

```{c}
for (x = 0; x < 10; x++) {
    printf( "%d\n", x );
}
```

Instead, it looks like this:

```{rust}
for x in range(0i, 10i) {
    println!("{}", x); // x: int
}
```

In slightly more abstract terms,

```{ignore}
for var in expression {
    code
}
```

The expression is an iterator, which we will discuss in more depth later in the
guide. The iterator gives back a series of elements. Each element is one
iteration of the loop. That value is then bound to the name `var`, which is
valid for the loop body. Once the body is over, the next value is fetched from
the iterator, and we loop another time. When there are no more values, the
`for` loop is over.

In our example, `range` is a function that takes a start and an end position,
and gives an iterator over those values. The upper bound is exclusive, though,
so our loop will print `0` through `9`, not `10`.

Rust does not have the "C-style" `for` loop on purpose. Manually controlling
each element of the loop is complicated and error prone, even for experienced C
developers.

We'll talk more about `for` when we cover **iterator**s, later in the Guide.

## `while`

The other kind of looping construct in Rust is the `while` loop. It looks like
this:

```{rust}
let mut x = 5u;       // mut x: uint
let mut done = false; // mut done: bool

while !done {
    x += x - 3;
    println!("{}", x);
    if x % 5 == 0 { done = true; }
}
```

`while` loops are the correct choice when you're not sure how many times
you need to loop.

If you need an infinite loop, you may be tempted to write this:

```{rust,ignore}
while true {
```

However, Rust has a dedicated keyword, `loop`, to handle this case:

```{rust,ignore}
loop {
```

Rust's control-flow analysis treats this construct differently than a
`while true`, since we know that it will always loop. The details of what
that _means_ aren't super important to understand at this stage, but in
general, the more information we can give to the compiler, the better it
can do with safety and code generation, so you should always prefer
`loop` when you plan to loop infinitely.

## Ending iteration early

Let's take a look at that `while` loop we had earlier:

```{rust}
let mut x = 5u;
let mut done = false;

while !done {
    x += x - 3;
    println!("{}", x);
    if x % 5 == 0 { done = true; }
}
```

We had to keep a dedicated `mut` boolean variable binding, `done`, to know
when we should exit out of the loop. Rust has two keywords to help us with
modifying iteration: `break` and `continue`.

In this case, we can write the loop in a better way with `break`:

```{rust}
let mut x = 5u;

loop {
    x += x - 3;
    println!("{}", x);
    if x % 5 == 0 { break; }
}
```

We now loop forever with `loop` and use `break` to break out early.

`continue` is similar, but instead of ending the loop, goes to the next
iteration. This will only print the odd numbers:

```{rust}
for x in range(0i, 10i) {
    if x % 2 == 0 { continue; }

    println!("{}", x);
}
```

Both `continue` and `break` are valid in both kinds of loops.

# Strings

Strings are an important concept for any programmer to master. Rust's string
handling system is a bit different from other languages, due to its systems
focus. Any time you have a data structure of variable size, things can get
tricky, and strings are a re-sizable data structure. That being said, Rust's
strings also work differently than in some other systems languages, such as C.

Let's dig into the details. A **string** is a sequence of Unicode scalar values
encoded as a stream of UTF-8 bytes. All strings are guaranteed to be
validly encoded UTF-8 sequences. Additionally, strings are not null-terminated
and can contain null bytes.

Rust has two main types of strings: `&str` and `String`.

The first kind is a `&str`. This is pronounced a 'string slice.' String literals
are of the type `&str`:

```{rust}
let string = "Hello there."; // string: &str
```

This string is statically allocated, meaning that it's saved inside our
compiled program, and exists for the entire duration it runs. The `string`
binding is a reference to this statically allocated string. String slices
have a fixed size, and cannot be mutated.

A `String`, on the other hand, is an in-memory string.  This string is
growable, and is also guaranteed to be UTF-8.

```{rust}
let mut s = "Hello".to_string(); // mut s: String
println!("{}", s);

s.push_str(", world.");
println!("{}", s);
```

You can get a `&str` view into a `String` with the `as_slice()` method:

```{rust}
fn takes_slice(slice: &str) {
    println!("Got: {}", slice);
}

fn main() {
    let s = "Hello".to_string();
    takes_slice(s.as_slice());
}
```

To compare a String to a constant string, prefer `as_slice()`...

```{rust}
fn compare(string: String) {
    if string.as_slice() == "Hello" {
        println!("yes");
    }
}
```

... over `to_string()`:

```{rust}
fn compare(string: String) {
    if string == "Hello".to_string() {
        println!("yes");
    }
}
```

Viewing a `String` as a `&str` is cheap, but converting the `&str` to a
`String` involves allocating memory. No reason to do that unless you have to!

That's the basics of strings in Rust! They're probably a bit more complicated
than you are used to, if you come from a scripting language, but when the
low-level details matter, they really matter. Just remember that `String`s
allocate memory and control their data, while `&str`s are a reference to
another string, and you'll be all set.

# Arrays, Vectors, and Slices

Like many programming languages, Rust has list types to represent a sequence of
things. The most basic is the **array**, a fixed-size list of elements of the
same type. By default, arrays are immutable.

```{rust}
let a = [1i, 2i, 3i];     // a: [int; 3]
let mut m = [1i, 2i, 3i]; // mut m: [int; 3]
```

There's a shorthand for initializing each element of an array to the same
value. In this example, each element of `a` will be initialized to `0i`:

```{rust}
let a = [0i; 20]; // a: [int; 20]
```

Arrays have type `[T; N]`. We'll talk about this `T` notation later, when we
cover generics.

You can get the number of elements in an array `a` with `a.len()`, and use
`a.iter()` to iterate over them with a for loop. This code will print each
number in order:

```{rust}
let a = [1i, 2, 3]; // Only the first item needs a type suffix

println!("a has {} elements", a.len());
for e in a.iter() {
    println!("{}", e);
}
```

You can access a particular element of an array with **subscript notation**:

```{rust}
let names = ["Graydon", "Brian", "Niko"]; // names: [&str, 3]

println!("The second name is: {}", names[1]);
```

Subscripts start at zero, like in most programming languages, so the first name
is `names[0]` and the second name is `names[1]`. The above example prints
`The second name is: Brian`. If you try to use a subscript that is not in the
array, you will get an error: array access is bounds-checked at run-time. Such
errant access is the source of many bugs in other systems programming
languages.

A **vector** is a dynamic or "growable" array, implemented as the standard
library type [`Vec<T>`](std/vec/) (we'll talk about what the `<T>` means
later). Vectors are to arrays what `String` is to `&str`. You can create them
with the `vec!` macro:

```{rust}
let v = vec![1i, 2, 3]; // v: Vec<int>
```

(Notice that unlike the `println!` macro we've used in the past, we use square
brackets `[]` with `vec!`. Rust allows you to use either in either situation,
this is just convention.)

You can get the length of, iterate over, and subscript vectors just like
arrays. In addition, (mutable) vectors can grow automatically:

```{rust}
let mut nums = vec![1i, 2, 3]; // mut nums: Vec<int>

nums.push(4);

println!("The length of nums is now {}", nums.len());   // Prints 4
```

Vectors have many more useful methods.

A **slice** is a reference to (or "view" into) an array. They are useful for
allowing safe, efficient access to a portion of an array without copying. For
example, you might want to reference just one line of a file read into memory.
By nature, a slice is not created directly, but from an existing variable.
Slices have a length, can be mutable or not, and in many ways behave like
arrays:

```{rust}
let a = [0i, 1, 2, 3, 4];
let middle = a.slice(1, 4);     // A slice of a: just the elements [1,2,3]

for e in middle.iter() {
    println!("{}", e);          // Prints 1, 2, 3
}
```

You can also take a slice of a vector, `String`, or `&str`, because they are
backed by arrays. Slices have type `&[T]`, which we'll talk about when we cover
generics.

We have now learned all of the most basic Rust concepts. We're ready to start
building our guessing game, we just need to know one last thing: how to get
input from the keyboard. You can't have a guessing game without the ability to
guess!

# Standard Input

Getting input from the keyboard is pretty easy, but uses some things
we haven't seen before. Here's a simple program that reads some input,
and then prints it back out:

```{rust,ignore}
fn main() {
    println!("Type something!");

    let input = std::io::stdin().read_line().ok().expect("Failed to read line");

    println!("{}", input);
}
```

Let's go over these chunks, one by one:

```{rust,ignore}
std::io::stdin();
```

This calls a function, `stdin()`, that lives inside the `std::io` module. As
you can imagine, everything in `std` is provided by Rust, the 'standard
library.' We'll talk more about the module system later.

Since writing the fully qualified name all the time is annoying, we can use
the `use` statement to import it in:

```{rust}
use std::io::stdin;

stdin();
```

However, it's considered better practice to not import individual functions, but
to import the module, and only use one level of qualification:

```{rust}
use std::io;

io::stdin();
```

Let's update our example to use this style:

```{rust,ignore}
use std::io;

fn main() {
    println!("Type something!");

    let input = io::stdin().read_line().ok().expect("Failed to read line");

    println!("{}", input);
}
```

Next up:

```{rust,ignore}
.read_line()
```

The `read_line()` method can be called on the result of `stdin()` to return
a full line of input. Nice and easy.

```{rust,ignore}
.ok().expect("Failed to read line");
```

Do you remember this code?

```{rust}
enum OptionalInt {
    Value(int),
    Missing,
}

fn main() {
    let x = OptionalInt::Value(5);
    let y = OptionalInt::Missing;

    match x {
        OptionalInt::Value(n) => println!("x is {}", n),
        OptionalInt::Missing  => println!("x is missing!"),
    }

    match y {
        OptionalInt::Value(n) => println!("y is {}", n),
        OptionalInt::Missing  => println!("y is missing!"),
    }
}
```

We had to match each time to see if we had a value or not. In this case,
though, we _know_ that `x` has a `Value`, but `match` forces us to handle
the `missing` case. This is what we want 99% of the time, but sometimes, we
know better than the compiler.

Likewise, `read_line()` does not return a line of input. It _might_ return a
line of input, though it might also fail to do so. This could happen if our program
isn't running in a terminal, but as part of a cron job, or some other context
where there's no standard input. Because of this, `read_line` returns a type
very similar to our `OptionalInt`: an `IoResult<T>`. We haven't talked about
`IoResult<T>` yet because it is the **generic** form of our `OptionalInt`.
Until then, you can think of it as being the same thing, just for any type –
not just `int`s.

Rust provides a method on these `IoResult<T>`s called `ok()`, which does the
same thing as our `match` statement but assumes that we have a valid value.
We then call `expect()` on the result, which will terminate our program if we
don't have a valid value. In this case, if we can't get input, our program
doesn't work, so we're okay with that. In most cases, we would want to handle
the error case explicitly. `expect()` allows us to give an error message if
this crash happens.

We will cover the exact details of how all of this works later in the Guide.
For now, this gives you enough of a basic understanding to work with.

Back to the code we were working on! Here's a refresher:

```{rust,ignore}
use std::io;

fn main() {
    println!("Type something!");

    let input = io::stdin().read_line().ok().expect("Failed to read line");

    println!("{}", input);
}
```

With long lines like this, Rust gives you some flexibility with the whitespace.
We _could_ write the example like this:

```{rust,ignore}
use std::io;

fn main() {
    println!("Type something!");

                                                  // here, we'll show the types at each step

    let input = io::stdin()                       // std::io::stdio::StdinReader
                  .read_line()                    // IoResult<String>
                  .ok()                           // Option<String>
                  .expect("Failed to read line"); // String

    println!("{}", input);
}
```

Sometimes, this makes things more readable – sometimes, less. Use your judgement
here.

That's all you need to get basic input from the standard input! It's not too
complicated, but there are a number of small parts.

# Guessing Game

Okay! We've got the basics of Rust down. Let's write a bigger program.

For our first project, we'll implement a classic beginner programming problem:
the guessing game. Here's how it works: Our program will generate a random
integer between one and a hundred. It will then prompt us to enter a guess.
Upon entering our guess, it will tell us if we're too low or too high. Once we
guess correctly, it will congratulate us. Sound good?

## Set up

Let's set up a new project. Go to your projects directory. Remember how we
had to create our directory structure and a `Cargo.toml` for `hello_world`? Cargo
has a command that does that for us. Let's give it a shot:

```{bash}
$ cd ~/projects
$ cargo new guessing_game --bin
$ cd guessing_game
```

We pass the name of our project to `cargo new`, and then the `--bin` flag,
since we're making a binary, rather than a library.

Check out the generated `Cargo.toml`:

```toml
[package]

name = "guessing_game"
version = "0.0.1"
authors = ["Your Name <you@example.com>"]
```

Cargo gets this information from your environment. If it's not correct, go ahead
and fix that.

Finally, Cargo generated a "Hello, world!" for us. Check out `src/main.rs`:

```{rust}
fn main() {
    println!("Hello, world!")
}
```

Let's try compiling what Cargo gave us:

```{bash}
$ cargo build
   Compiling guessing_game v0.0.1 (file:///home/you/projects/guessing_game)
```

Excellent! Open up your `src/main.rs` again. We'll be writing all of
our code in this file. We'll talk about multiple-file projects later on in the
guide.

Before we move on, let me show you one more Cargo command: `run`. `cargo run`
is kind of like `cargo build`, but it also then runs the produced executable.
Try it out:

```bash
$ cargo run
   Compiling guessing_game v0.0.1 (file:///home/you/projects/guessing_game)
     Running `target/guessing_game`
Hello, world!
```

Great! The `run` command comes in handy when you need to rapidly iterate on a project.
Our game is just such a project, we need to quickly test each iteration before moving on to the next one.

## Processing a Guess

Let's get to it! The first thing we need to do for our guessing game is
allow our player to input a guess. Put this in your `src/main.rs`:

```{rust,no_run}
use std::io;

fn main() {
    println!("Guess the number!");

    println!("Please input your guess.");

    let input = io::stdin().read_line()
                           .ok()
                           .expect("Failed to read line");

    println!("You guessed: {}", input);
}
```

You've seen this code before, when we talked about standard input. We
import the `std::io` module with `use`, and then our `main` function contains
our program's logic. We print a little message announcing the game, ask the
user to input a guess, get their input, and then print it out.

Because we talked about this in the section on standard I/O, I won't go into
more details here. If you need a refresher, go re-read that section.

## Generating a secret number

Next, we need to generate a secret number. To do that, we need to use Rust's
random number generation, which we haven't talked about yet. Rust includes a
bunch of interesting functions in its standard library. If you need a bit of
code, it's possible that it's already been written for you! In this case,
we do know that Rust has random number generation, but we don't know how to
use it.

Enter the docs. Rust has a page specifically to document the standard library.
You can find that page [here](std/index.html). There's a lot of information on
that page, but the best part is the search bar. Right up at the top, there's
a box that you can enter in a search term. The search is pretty primitive
right now, but is getting better all the time. If you type 'random' in that
box, the page will update to [this one](std/index.html?search=random). The very
first result is a link to [`std::rand::random`](std/rand/fn.random.html). If we
click on that result, we'll be taken to its documentation page.

This page shows us a few things: the type signature of the function, some
explanatory text, and then an example. Let's try to modify our code to add in the
`random` function and see what happens:

```{rust,ignore}
use std::io;
use std::rand;

fn main() {
    println!("Guess the number!");

    let secret_number = (rand::random() % 100i) + 1i; // secret_number: int

    println!("The secret number is: {}", secret_number);

    println!("Please input your guess.");

    let input = io::stdin().read_line()
                           .ok()
                           .expect("Failed to read line");


    println!("You guessed: {}", input);
}
```

The first thing we changed was to `use std::rand`, as the docs
explained.  We then added in a `let` expression to create a variable binding
named `secret_number`, and we printed out its result.

Also, you may wonder why we are using `%` on the result of `rand::random()`.
This operator is called 'modulo', and it returns the remainder of a division.
By taking the modulo of the result of `rand::random()`, we're limiting the
values to be between 0 and 99. Then, we add one to the result, making it from 1
to 100. Using modulo can give you a very, very small bias in the result, but
for this example, it is not important.

Let's try to compile this using `cargo build`:

```bash
$ cargo build
   Compiling guessing_game v0.0.1 (file:///home/you/projects/guessing_game)
src/main.rs:7:26: 7:34 error: the type of this value must be known in this context
src/main.rs:7     let secret_number = (rand::random() % 100i) + 1i;
                                       ^~~~~~~~
error: aborting due to previous error
```

It didn't work! Rust says "the type of this value must be known in this
context." What's up with that? Well, as it turns out, `rand::random()` can
generate many kinds of random values, not just integers. And in this case, Rust
isn't sure what kind of value `random()` should generate. So we have to help
it. With number literals, we just add an `i` onto the end to tell Rust they're
integers, but that does not work with functions. There's a different syntax,
and it looks like this:

```{rust,ignore}
rand::random::<int>();
```

This says "please give me a random `int` value." We can change our code to use
this hint:

```{rust,no_run}
use std::io;
use std::rand;

fn main() {
    println!("Guess the number!");

    let secret_number = (rand::random::<int>() % 100i) + 1i;

    println!("The secret number is: {}", secret_number);

    println!("Please input your guess.");

    let input = io::stdin().read_line()
                           .ok()
                           .expect("Failed to read line");


    println!("You guessed: {}", input);
}
```

Try running our new program a few times:

```bash
$ cargo run
   Compiling guessing_game v0.0.1 (file:///home/you/projects/guessing_game)
     Running `target/guessing_game`
Guess the number!
The secret number is: 7
Please input your guess.
4
You guessed: 4
$ ./target/guessing_game
Guess the number!
The secret number is: 83
Please input your guess.
5
You guessed: 5
$ ./target/guessing_game
Guess the number!
The secret number is: -29
Please input your guess.
42
You guessed: 42
```

Wait. Negative 29? We wanted a number between one and a hundred! We have two
options here: we can either ask `random()` to generate an unsigned integer, which
can only be positive, or we can use the `abs()` function. Let's go with the
unsigned integer approach. If we want a random positive number, we should ask for
a random positive number. Our code looks like this now:

```{rust,no_run}
use std::io;
use std::rand;

fn main() {
    println!("Guess the number!");

    let secret_number = (rand::random::<uint>() % 100u) + 1u;

    println!("The secret number is: {}", secret_number);

    println!("Please input your guess.");

    let input = io::stdin().read_line()
                           .ok()
                           .expect("Failed to read line");


    println!("You guessed: {}", input);
}
```

And trying it out:

```bash
$ cargo run
   Compiling guessing_game v0.0.1 (file:///home/you/projects/guessing_game)
     Running `target/guessing_game`
Guess the number!
The secret number is: 57
Please input your guess.
3
You guessed: 3
```

Great! Next up: let's compare our guess to the secret guess.

## Comparing guesses

If you remember, earlier in the guide, we made a `cmp` function that compared
two numbers. Let's add that in, along with a `match` statement to compare our
guess to the secret number:

```{rust,ignore}
use std::io;
use std::rand;
use std::cmp::Ordering;

fn main() {
    println!("Guess the number!");

    let secret_number = (rand::random::<uint>() % 100u) + 1u;

    println!("The secret number is: {}", secret_number);

    println!("Please input your guess.");

    let input = io::stdin().read_line()
                           .ok()
                           .expect("Failed to read line");


    println!("You guessed: {}", input);

    match cmp(input, secret_number) {
        Ordering::Less    => println!("Too small!"),
        Ordering::Greater => println!("Too big!"),
        Ordering::Equal   => println!("You win!"),
    }
}

fn cmp(a: int, b: int) -> Ordering {
    if a < b { Ordering::Less }
    else if a > b { Ordering::Greater }
    else { Ordering::Equal }
}
```

If we try to compile, we'll get some errors:

```bash
$ cargo build
   Compiling guessing_game v0.0.1 (file:///home/you/projects/guessing_game)
src/main.rs:20:15: 20:20 error: mismatched types: expected `int` but found `collections::string::String` (expected int but found struct collections::string::String)
src/main.rs:20     match cmp(input, secret_number) {
                             ^~~~~
src/main.rs:20:22: 20:35 error: mismatched types: expected `int` but found `uint` (expected int but found uint)
src/main.rs:20     match cmp(input, secret_number) {
                                    ^~~~~~~~~~~~~
error: aborting due to 2 previous errors
```

This often happens when writing Rust programs, and is one of Rust's greatest
strengths. You try out some code, see if it compiles, and Rust tells you that
you've done something wrong. In this case, our `cmp` function works on integers,
but we've given it unsigned integers. In this case, the fix is easy, because
we wrote the `cmp` function! Let's change it to take `uint`s:

```{rust,ignore}
use std::io;
use std::rand;
use std::cmp::Ordering;

fn main() {
    println!("Guess the number!");

    let secret_number = (rand::random::<uint>() % 100u) + 1u;

    println!("The secret number is: {}", secret_number);

    println!("Please input your guess.");

    let input = io::stdin().read_line()
                           .ok()
                           .expect("Failed to read line");


    println!("You guessed: {}", input);

    match cmp(input, secret_number) {
        Ordering::Less    => println!("Too small!"),
        Ordering::Greater => println!("Too big!"),
        Ordering::Equal   => println!("You win!"),
    }
}

fn cmp(a: uint, b: uint) -> Ordering {
    if a < b { Ordering::Less }
    else if a > b { Ordering::Greater }
    else { Ordering::Equal }
}
```

And try compiling again:

```bash
$ cargo build
   Compiling guessing_game v0.0.1 (file:///home/you/projects/guessing_game)
src/main.rs:20:15: 20:20 error: mismatched types: expected `uint` but found `collections::string::String` (expected uint but found struct collections::string::String)
src/main.rs:20     match cmp(input, secret_number) {
                             ^~~~~
error: aborting due to previous error
```

This error is similar to the last one: we expected to get a `uint`, but we got
a `String` instead! That's because our `input` variable is coming from the
standard input, and you can guess anything. Try it:

```bash
$ ./target/guessing_game
Guess the number!
The secret number is: 73
Please input your guess.
hello
You guessed: hello
```

Oops! Also, you'll note that we just ran our program even though it didn't compile.
This works because the older version we did successfully compile was still lying
around. Gotta be careful!

Anyway, we have a `String`, but we need a `uint`. What to do? Well, there's
a function for that:

```{rust,ignore}
let input = io::stdin().read_line()
                       .ok()
                       .expect("Failed to read line");
let input_num: Option<uint> = input.parse();
```

The `parse` function takes in a `&str` value and converts it into something.
We tell it what kind of something with a type hint. Remember our type hint with
`random()`? It looked like this:

```{rust,ignore}
rand::random::<uint>();
```

There's an alternate way of providing a hint too, and that's declaring the type
in a `let`:

```{rust,ignore}
let x: uint = rand::random();
```

In this case, we say `x` is a `uint` explicitly, so Rust is able to properly
tell `random()` what to generate. In a similar fashion, both of these work:

```{rust,ignore}
let input_num = "5".parse::<uint>();         // input_num: Option<uint>
let input_num: Option<uint> = "5".parse();   // input_num: Option<uint>
```

Anyway, with us now converting our input to a number, our code looks like this:

```{rust,ignore}
use std::io;
use std::rand;
use std::cmp::Ordering;

fn main() {
    println!("Guess the number!");

    let secret_number = (rand::random::<uint>() % 100u) + 1u;

    println!("The secret number is: {}", secret_number);

    println!("Please input your guess.");

    let input = io::stdin().read_line()
                           .ok()
                           .expect("Failed to read line");
    let input_num: Option<uint> = input.parse();

    println!("You guessed: {}", input_num);

    match cmp(input_num, secret_number) {
        Ordering::Less    => println!("Too small!"),
        Ordering::Greater => println!("Too big!"),
        Ordering::Equal   => println!("You win!"),
    }
}

fn cmp(a: uint, b: uint) -> Ordering {
    if a < b { Ordering::Less }
    else if a > b { Ordering::Greater }
    else { Ordering::Equal }
}
```

Let's try it out!

```bash
$ cargo build
   Compiling guessing_game v0.0.1 (file:///home/you/projects/guessing_game)
src/main.rs:22:15: 22:24 error: mismatched types: expected `uint` but found `core::option::Option<uint>` (expected uint but found enum core::option::Option)
src/main.rs:22     match cmp(input_num, secret_number) {
                             ^~~~~~~~~
error: aborting due to previous error
```

Oh yeah! Our `input_num` has the type `Option<uint>`, rather than `uint`. We
need to unwrap the Option. If you remember from before, `match` is a great way
to do that. Try this code:

```{rust,no_run}
use std::io;
use std::rand;
use std::cmp::Ordering;

fn main() {
    println!("Guess the number!");

    let secret_number = (rand::random::<uint>() % 100u) + 1u;

    println!("The secret number is: {}", secret_number);

    println!("Please input your guess.");

    let input = io::stdin().read_line()
                           .ok()
                           .expect("Failed to read line");
    let input_num: Option<uint> = input.parse();

    let num = match input_num {
        Some(num) => num,
        None      => {
            println!("Please input a number!");
            return;
        }
    };


    println!("You guessed: {}", num);

    match cmp(num, secret_number) {
        Ordering::Less    => println!("Too small!"),
        Ordering::Greater => println!("Too big!"),
        Ordering::Equal   => println!("You win!"),
    }
}

fn cmp(a: uint, b: uint) -> Ordering {
    if a < b { Ordering::Less }
    else if a > b { Ordering::Greater }
    else { Ordering::Equal }
}
```

We use a `match` to either give us the `uint` inside of the `Option`, or else
print an error message and return. Let's give this a shot:

```bash
$ cargo run
   Compiling guessing_game v0.0.1 (file:///home/you/projects/guessing_game)
     Running `target/guessing_game`
Guess the number!
The secret number is: 17
Please input your guess.
5
Please input a number!
```

Uh, what? But we did!

... actually, we didn't. See, when you get a line of input from `stdin()`,
you get all the input. Including the `\n` character from you pressing Enter.
Therefore, `parse()` sees the string `"5\n"` and says "nope, that's not a
number; there's non-number stuff in there!" Luckily for us, `&str`s have an easy
method we can use defined on them: `trim()`. One small modification, and our
code looks like this:

```{rust,no_run}
use std::io;
use std::rand;
use std::cmp::Ordering;

fn main() {
    println!("Guess the number!");

    let secret_number = (rand::random::<uint>() % 100u) + 1u;

    println!("The secret number is: {}", secret_number);

    println!("Please input your guess.");

    let input = io::stdin().read_line()
                           .ok()
                           .expect("Failed to read line");
    let input_num: Option<uint> = input.trim().parse();

    let num = match input_num {
        Some(num) => num,
        None      => {
            println!("Please input a number!");
            return;
        }
    };


    println!("You guessed: {}", num);

    match cmp(num, secret_number) {
        Ordering::Less    => println!("Too small!"),
        Ordering::Greater => println!("Too big!"),
        Ordering::Equal   => println!("You win!"),
    }
}

fn cmp(a: uint, b: uint) -> Ordering {
    if a < b { Ordering::Less }
    else if a > b { Ordering::Greater }
    else { Ordering::Equal }
}
```

Let's try it!

```bash
$ cargo run
   Compiling guessing_game v0.0.1 (file:///home/you/projects/guessing_game)
     Running `target/guessing_game`
Guess the number!
The secret number is: 58
Please input your guess.
  76
You guessed: 76
Too big!
```

Nice! You can see I even added spaces before my guess, and it still figured
out that I guessed 76. Run the program a few times, and verify that guessing
the number works, as well as guessing a number too small.

The Rust compiler helped us out quite a bit there! This technique is called
"lean on the compiler", and it's often useful when working on some code. Let
the error messages help guide you towards the correct types.

Now we've got most of the game working, but we can only make one guess. Let's
change that by adding loops!

## Looping

As we already discussed, the `loop` keyword gives us an infinite loop.
Let's add that in:

```{rust,no_run}
use std::io;
use std::rand;
use std::cmp::Ordering;

fn main() {
    println!("Guess the number!");

    let secret_number = (rand::random::<uint>() % 100u) + 1u;

    println!("The secret number is: {}", secret_number);

    loop {

        println!("Please input your guess.");

        let input = io::stdin().read_line()
                               .ok()
                               .expect("Failed to read line");
        let input_num: Option<uint> = input.trim().parse();

        let num = match input_num {
            Some(num) => num,
            None      => {
                println!("Please input a number!");
                return;
            }
        };


        println!("You guessed: {}", num);

        match cmp(num, secret_number) {
            Ordering::Less    => println!("Too small!"),
            Ordering::Greater => println!("Too big!"),
            Ordering::Equal   => println!("You win!"),
        }
    }
}

fn cmp(a: uint, b: uint) -> Ordering {
    if a < b { Ordering::Less }
    else if a > b { Ordering::Greater }
    else { Ordering::Equal }
}
```

And try it out. But wait, didn't we just add an infinite loop? Yup. Remember
that `return`? If we give a non-number answer, we'll `return` and quit. Observe:

```bash
$ cargo run
   Compiling guessing_game v0.0.1 (file:///home/you/projects/guessing_game)
     Running `target/guessing_game`
Guess the number!
The secret number is: 59
Please input your guess.
45
You guessed: 45
Too small!
Please input your guess.
60
You guessed: 60
Too big!
Please input your guess.
59
You guessed: 59
You win!
Please input your guess.
quit
Please input a number!
```

Ha! `quit` actually quits. As does any other non-number input. Well, this is
suboptimal to say the least. First, let's actually quit when you win the game:

```{rust,no_run}
use std::io;
use std::rand;
use std::cmp::Ordering;

fn main() {
    println!("Guess the number!");

    let secret_number = (rand::random::<uint>() % 100u) + 1u;

    println!("The secret number is: {}", secret_number);

    loop {

        println!("Please input your guess.");

        let input = io::stdin().read_line()
                               .ok()
                               .expect("Failed to read line");
        let input_num: Option<uint> = input.trim().parse();

        let num = match input_num {
            Some(num) => num,
            None      => {
                println!("Please input a number!");
                return;
            }
        };


        println!("You guessed: {}", num);

        match cmp(num, secret_number) {
            Ordering::Less    => println!("Too small!"),
            Ordering::Greater => println!("Too big!"),
            Ordering::Equal   => {
                println!("You win!");
                return;
            },
        }
    }
}

fn cmp(a: uint, b: uint) -> Ordering {
    if a < b { Ordering::Less }
    else if a > b { Ordering::Greater }
    else { Ordering::Equal }
}
```

By adding the `return` line after the `You win!`, we'll exit the program when
we win. We have just one more tweak to make: when someone inputs a non-number,
we don't want to quit, we just want to ignore it. Change that `return` to
`continue`:


```{rust,no_run}
use std::io;
use std::rand;
use std::cmp::Ordering;

fn main() {
    println!("Guess the number!");

    let secret_number = (rand::random::<uint>() % 100u) + 1u;

    println!("The secret number is: {}", secret_number);

    loop {

        println!("Please input your guess.");

        let input = io::stdin().read_line()
                               .ok()
                               .expect("Failed to read line");
        let input_num: Option<uint> = input.trim().parse();

        let num = match input_num {
            Some(num) => num,
            None      => {
                println!("Please input a number!");
                continue;
            }
        };


        println!("You guessed: {}", num);

        match cmp(num, secret_number) {
            Ordering::Less    => println!("Too small!"),
            Ordering::Greater => println!("Too big!"),
            Ordering::Equal   => {
                println!("You win!");
                return;
            },
        }
    }
}

fn cmp(a: uint, b: uint) -> Ordering {
    if a < b { Ordering::Less }
    else if a > b { Ordering::Greater }
    else { Ordering::Equal }
}
```

Now we should be good! Let's try:

```bash
$ cargo run
   Compiling guessing_game v0.0.1 (file:///home/you/projects/guessing_game)
     Running `target/guessing_game`
Guess the number!
The secret number is: 61
Please input your guess.
10
You guessed: 10
Too small!
Please input your guess.
99
You guessed: 99
Too big!
Please input your guess.
foo
Please input a number!
Please input your guess.
61
You guessed: 61
You win!
```

Awesome! With one tiny last tweak, we have finished the guessing game. Can you
think of what it is? That's right, we don't want to print out the secret number.
It was good for testing, but it kind of ruins the game. Here's our final source:

```{rust,no_run}
use std::io;
use std::rand;
use std::cmp::Ordering;

fn main() {
    println!("Guess the number!");

    let secret_number = (rand::random::<uint>() % 100u) + 1u;

    loop {

        println!("Please input your guess.");

        let input = io::stdin().read_line()
                               .ok()
                               .expect("Failed to read line");
        let input_num: Option<uint> = input.trim().parse();

        let num = match input_num {
            Some(num) => num,
            None      => {
                println!("Please input a number!");
                continue;
            }
        };


        println!("You guessed: {}", num);

        match cmp(num, secret_number) {
            Ordering::Less    => println!("Too small!"),
            Ordering::Greater => println!("Too big!"),
            Ordering::Equal   => {
                println!("You win!");
                return;
            },
        }
    }
}

fn cmp(a: uint, b: uint) -> Ordering {
    if a < b { Ordering::Less }
    else if a > b { Ordering::Greater }
    else { Ordering::Equal }
}
```

## Complete!

At this point, you have successfully built the Guessing Game! Congratulations!

You've now learned the basic syntax of Rust. All of this is relatively close to
various other programming languages you have used in the past. These
fundamental syntactical and semantic elements will form the foundation for the
rest of your Rust education.

Now that you're an expert at the basics, it's time to learn about some of
Rust's more unique features.

# Crates and Modules

Rust features a strong module system, but it works a bit differently than in
other programming languages. Rust's module system has two main components:
**crate**s and **module**s.

A crate is Rust's unit of independent compilation. Rust always compiles one
crate at a time, producing either a library or an executable. However, executables
usually depend on libraries, and many libraries depend on other libraries as well.
To support this, crates can depend on other crates.

Each crate contains a hierarchy of modules. This tree starts off with a single
module, called the **crate root**. Within the crate root, we can declare other
modules, which can contain other modules, as deeply as you'd like.

Note that we haven't mentioned anything about files yet. Rust does not impose a
particular relationship between your filesystem structure and your module
structure. That said, there is a conventional approach to how Rust looks for
modules on the file system, but it's also overridable.

Enough talk, let's build something! Let's make a new project called `modules`.

```{bash,ignore}
$ cd ~/projects
$ cargo new modules --bin
$ cd modules
```

Let's double check our work by compiling:

```{bash}
$ cargo run
   Compiling modules v0.0.1 (file:///home/you/projects/modules)
     Running `target/modules`
Hello, world!
```

Excellent! We already have a single crate here: our `src/main.rs` is a crate.
Everything in that file is in the crate root. A crate that generates an executable
defines a `main` function inside its root, as we've done here.

Let's define a new module inside our crate. Edit `src/main.rs` to look like this:

```
fn main() {
    println!("Hello, world!")
}

mod hello {
    fn print_hello() {
        println!("Hello, world!")
    }
}
```

We now have a module named `hello` inside of our crate root. Modules use
`snake_case` naming, like functions and variable bindings.

Inside the `hello` module, we've defined a `print_hello` function. This will
also print out our "hello world" message. Modules allow you to split up your
program into nice neat boxes of functionality, grouping common things together,
and keeping different things apart. It's kinda like having a set of shelves:
a place for everything and everything in its place.

To call our `print_hello` function, we use the double colon (`::`):

```{rust,ignore}
hello::print_hello();
```

You've seen this before, with `io::stdin()` and `rand::random()`. Now you know
how to make your own. However, crates and modules have rules about
**visibility**, which controls who exactly may use the functions defined in a
given module. By default, everything in a module is private, which means that
it can only be used by other functions in the same module. This will not
compile:

```{rust,ignore}
fn main() {
    hello::print_hello();
}

mod hello {
    fn print_hello() {
        println!("Hello, world!")
    }
}
```

It gives an error:

```bash
   Compiling modules v0.0.1 (file:///home/you/projects/modules)
src/main.rs:2:5: 2:23 error: function `print_hello` is private
src/main.rs:2     hello::print_hello();
                  ^~~~~~~~~~~~~~~~~~
```

To make it public, we use the `pub` keyword:

```{rust}
fn main() {
    hello::print_hello();
}

mod hello {
    pub fn print_hello() {
        println!("Hello, world!")
    }
}
```

Usage of the `pub` keyword is sometimes called 'exporting', because
we're making the function available for other modules. This will work:

```bash
$ cargo run
   Compiling modules v0.0.1 (file:///home/you/projects/modules)
     Running `target/modules`
Hello, world!
```

Nice! There are more things we can do with modules, including moving them into
their own files. This is enough detail for now.

# Testing

Traditionally, testing has not been a strong suit of most systems programming
languages. Rust, however, has very basic testing built into the language
itself.  While automated testing cannot prove that your code is bug-free, it is
useful for verifying that certain behaviors work as intended.

Here's a very basic test:

```{rust}
#[test]
fn is_one_equal_to_one() {
    assert_eq!(1i, 1i);
}
```

You may notice something new: that `#[test]`. Before we get into the mechanics
of testing, let's talk about attributes.

## Attributes

Rust's testing system uses **attribute**s to mark which functions are tests.
Attributes can be placed on any Rust **item**. Remember how most things in
Rust are an expression, but `let` is not? Item declarations are also not
expressions. Here's a list of things that qualify as an item:

* functions
* modules
* type definitions
* structures
* enumerations
* static items
* traits
* implementations

You haven't learned about all of these things yet, but that's the list. As
you can see, functions are at the top of it.

Attributes can appear in three ways:

1. A single identifier, the attribute name. `#[test]` is an example of this.
2. An identifier followed by an equals sign (`=`) and a literal. `#[cfg=test]`
   is an example of this.
3. An identifier followed by a parenthesized list of sub-attribute arguments.
   `#[cfg(unix, target_word_size = "32")]` is an example of this, where one of
    the sub-arguments is of the second kind.

There are a number of different kinds of attributes, enough that we won't go
over them all here. Before we talk about the testing-specific attributes, I
want to call out one of the most important kinds of attributes: stability
markers.

## Stability attributes

Rust provides six attributes to indicate the stability level of various
parts of your library. The six levels are:

* deprecated: This item should no longer be used. No guarantee of backwards
  compatibility.
* experimental: This item was only recently introduced or is otherwise in a
  state of flux. It may change significantly, or even be removed. No guarantee
  of backwards-compatibility.
* unstable: This item is still under development and requires more testing to
  be considered stable. No guarantee of backwards-compatibility.
* stable: This item is considered stable, and will not change significantly.
  Guarantee of backwards-compatibility.
* frozen: This item is very stable, and is unlikely to change. Guarantee of
  backwards-compatibility.
* locked: This item will never change unless a serious bug is found. Guarantee
  of backwards-compatibility.

All of Rust's standard library uses these attribute markers to communicate
their relative stability, and you should use them in your code, as well.
There's an associated attribute, `warn`, that allows you to warn when you
import an item marked with certain levels: deprecated, experimental and
unstable. For now, only deprecated warns by default, but this will change once
the standard library has been stabilized.

You can use the `warn` attribute like this:

```{rust,ignore}
#![warn(unstable)]
```

And later, when you import a crate:

```{rust,ignore}
extern crate some_crate;
```

You'll get a warning if you use something marked unstable.

You may have noticed an exclamation point in the `warn` attribute declaration.
The `!` in this attribute means that this attribute applies to the enclosing
item, rather than to the item that follows the attribute. This `warn`
attribute declaration applies to the enclosing crate itself, rather than
to whatever item statement follows it:

```{rust,ignore}
// applies to the crate we're in
#![warn(unstable)]

extern crate some_crate;

// applies to the following `fn`.
#[test]
fn a_test() {
  // ...
}
```

## Writing tests

Let's write a very simple crate in a test-driven manner. You know the drill by
now: make a new project:

```{bash,ignore}
$ cd ~/projects
$ cargo new testing --bin
$ cd testing
```

And try it out:

```bash
$ cargo run
   Compiling testing v0.0.1 (file:///home/you/projects/testing)
     Running `target/testing`
Hello, world!
```

Great. Rust's infrastructure supports tests in two sorts of places, and they're
for two kinds of tests: you include **unit test**s inside of the crate itself,
and you place **integration test**s inside a `tests` directory. "Unit tests"
are small tests that test one focused unit; "integration tests" test multiple
units in integration. That being said, this is a social convention – they're no
different in syntax. Let's make a `tests` directory:

```{bash,ignore}
$ mkdir tests
```

Next, let's create an integration test in `tests/lib.rs`:

```{rust,no_run}
#[test]
fn foo() {
    assert!(false);
}
```

It doesn't matter what you name your test functions, though it's nice if
you give them descriptive names. You'll see why in a moment. We then use a
macro, `assert!`, to assert that something is true. In this case, we're giving
it `false`, so this test should fail. Let's try it!

```bash
$ cargo test
   Compiling testing v0.0.1 (file:///home/you/projects/testing)
/home/you/projects/testing/src/main.rs:1:1: 3:2 warning: function is never used: `main`, #[warn(dead_code)] on by default
/home/you/projects/testing/src/main.rs:1 fn main() {
/home/you/projects/testing/src/main.rs:2     println!("Hello, world!")
/home/you/projects/testing/src/main.rs:3 }
     Running target/lib-654ce120f310a3a5

running 1 test
test foo ... FAILED

failures:

---- foo stdout ----
        thread 'foo' failed at 'assertion failed: false', /home/you/projects/testing/tests/lib.rs:3



failures:
    foo

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured

thread '<main>' failed at 'Some tests failed', /home/you/src/rust/src/libtest/lib.rs:243
```

Lots of output! Let's break this down:

```bash
$ cargo test
   Compiling testing v0.0.1 (file:///home/you/projects/testing)
```

You can run all of your tests with `cargo test`. This runs both your tests in
`tests`, as well as the tests you put inside of your crate.

```text
/home/you/projects/testing/src/main.rs:1:1: 3:2 warning: function is never used: `main`, #[warn(dead_code)] on by default
/home/you/projects/testing/src/main.rs:1 fn main() {
/home/you/projects/testing/src/main.rs:2     println!("Hello, world!")
/home/you/projects/testing/src/main.rs:3 }
```

Rust has a **lint** called 'warn on dead code' used by default. A lint is a
bit of code that checks your code, and can tell you things about it. In this
case, Rust is warning us that we've written some code that's never used: our
`main` function. Of course, since we're running tests, we don't use `main`.
We'll turn this lint off for just this function soon. For now, just ignore this
output.

```text
     Running target/lib-654ce120f310a3a5

running 1 test
test foo ... FAILED
```

Now we're getting somewhere. Remember when we talked about naming our tests
with good names? This is why. Here, it says 'test foo' because we called our
test 'foo'. If we had given it a good name, it'd be more clear which test
failed, especially as we accumulate more tests.

```text
failures:

---- foo stdout ----
        thread 'foo' failed at 'assertion failed: false', /home/you/projects/testing/tests/lib.rs:3



failures:
    foo

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured

thread '<main>' failed at 'Some tests failed', /home/you/src/rust/src/libtest/lib.rs:243
```

After all the tests run, Rust will show us any output from our failed tests.
In this instance, Rust tells us that our assertion failed, with false. This was
what we expected.

Whew! Let's fix our test:

```{rust}
#[test]
fn foo() {
    assert!(true);
}
```

And then try to run our tests again:

```bash
$ cargo test
   Compiling testing v0.0.1 (file:///home/you/projects/testing)
     Running target/lib-654ce120f310a3a5

running 1 test
test foo ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured

     Running target/testing-6d7518593c7c3ee5

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured
```

Nice! Our test passes, as we expected. Note how we didn't get the
`main` warning this time? This is because `src/main.rs` didn't
need recompiling, but we'll get that warning again if we
change (and recompile) that file. Let's get rid of that
warning; change your `src/main.rs` to look like this:

```{rust}
#[cfg(not(test))]
fn main() {
    println!("Hello, world!")
}
```

This attribute combines two things: `cfg` and `not`. The `cfg` attribute allows
you to conditionally compile code based on something. The following item will
only be compiled if the configuration says it's true. And when Cargo compiles
our tests, it sets things up so that `cfg(test)` is true. But we want to only
include `main` when it's _not_ true. So we use `not` to negate things:
`cfg(not(test))` will only compile our code when the `cfg(test)` is false.

With this attribute, we won't get the warning (even
though `src/main.rs` gets recompiled this time):

```bash
$ cargo test
   Compiling testing v0.0.1 (file:///home/you/projects/testing)
     Running target/lib-654ce120f310a3a5

running 1 test
test foo ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured

     Running target/testing-6d7518593c7c3ee5

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured
```

Nice. Okay, let's write a real test now. Change your `tests/lib.rs`
to look like this:

```{rust,ignore}
#[test]
fn math_checks_out() {
    let result = add_three_times_four(5i);

    assert_eq!(32i, result);
}
```

And try to run the test:

```bash
$ cargo test
   Compiling testing v0.0.1 (file:///home/you/projects/testing)
/home/you/projects/testing/tests/lib.rs:3:18: 3:38 error: unresolved name `add_three_times_four`.
/home/you/projects/testing/tests/lib.rs:3     let result = add_three_times_four(5i);
                                                           ^~~~~~~~~~~~~~~~~~~~
error: aborting due to previous error
Build failed, waiting for other jobs to finish...
Could not compile `testing`.

To learn more, run the command again with `--verbose`.
```

Rust can't find this function. That makes sense, as we didn't write it yet!

In order to share this code with our tests, we'll need to make a library crate.
This is also just good software design: as we mentioned before, it's a good idea
to put most of your functionality into a library crate, and have your executable
crate use that library. This allows for code reuse.

To do that, we'll need to make a new module. Make a new file, `src/lib.rs`,
and put this in it:

```{rust}
# fn main() {}
pub fn add_three_times_four(x: int) -> int {
    (x + 3) * 4
}
```

We're calling this file `lib.rs`, because Cargo uses that filename as the crate
root by convention.

We'll then need to use this crate in our `src/main.rs`:

```{rust,ignore}
extern crate testing;

#[cfg(not(test))]
fn main() {
    println!("Hello, world!")
}
```

Finally, let's import this function in our `tests/lib.rs`:

```{rust,ignore}
extern crate testing;
use testing::add_three_times_four;

#[test]
fn math_checks_out() {
    let result = add_three_times_four(5i);

    assert_eq!(32i, result);
}
```

Let's give it a run:

```bash
$ cargo test
   Compiling testing v0.0.1 (file:///home/you/projects/testing)
     Running target/lib-654ce120f310a3a5

running 1 test
test math_checks_out ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured

     Running target/testing-6d7518593c7c3ee5

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured

     Running target/testing-8a94b31f7fd2e8fe

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured

   Doc-tests testing

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured
```

Great! One test passed. We've got an integration test showing that our public
method works, but maybe we want to test some of the internal logic as well.
While this function is simple, if it were more complicated, you can imagine
we'd need more tests. Let's break it up into two helper functions and write
some unit tests to test those.

Change your `src/lib.rs` to look like this:

```{rust,ignore}
pub fn add_three_times_four(x: int) -> int {
    times_four(add_three(x))
}

fn add_three(x: int) -> int { x + 3 }

fn times_four(x: int) -> int { x * 4 }
```

If you run `cargo test`, you should get the same output:

```bash
$ cargo test
   Compiling testing v0.0.1 (file:///home/you/projects/testing)
     Running target/lib-654ce120f310a3a5

running 1 test
test math_checks_out ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured

     Running target/testing-6d7518593c7c3ee5

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured

     Running target/testing-8a94b31f7fd2e8fe

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured

   Doc-tests testing

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured
```

If we tried to write a test for these two new functions, it wouldn't
work. For example:

```{rust,ignore}
extern crate testing;
use testing::add_three_times_four;
use testing::add_three;

#[test]
fn math_checks_out() {
    let result = add_three_times_four(5i);

    assert_eq!(32i, result);
}

#[test]
fn test_add_three() {
    let result = add_three(5i);

    assert_eq!(8i, result);
}
```

We'd get this error:

```text
   Compiling testing v0.0.1 (file:///home/you/projects/testing)
/home/you/projects/testing/tests/lib.rs:3:5: 3:24 error: function `add_three` is private
/home/you/projects/testing/tests/lib.rs:3 use testing::add_three;
                                              ^~~~~~~~~~~~~~~~~~~
```

Right. It's private. So external, integration tests won't work. We need a
unit test. Open up your `src/lib.rs` and add this:

```{rust,ignore}
pub fn add_three_times_four(x: int) -> int {
    times_four(add_three(x))
}

fn add_three(x: int) -> int { x + 3 }

fn times_four(x: int) -> int { x * 4 }

#[cfg(test)]
mod test {
    use super::add_three;
    use super::times_four;

    #[test]
    fn test_add_three() {
        let result = add_three(5i);

        assert_eq!(8i, result);
    }

    #[test]
    fn test_times_four() {
        let result = times_four(5i);

        assert_eq!(20i, result);
    }
}
```

Let's give it a shot:

```bash
$ cargo test
   Compiling testing v0.0.1 (file:///home/you/projects/testing)
     Running target/lib-654ce120f310a3a5

running 1 test
test math_checks_out ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured

     Running target/testing-6d7518593c7c3ee5

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured

     Running target/testing-8a94b31f7fd2e8fe

running 2 tests
test test::test_times_four ... ok
test test::test_add_three ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured

   Doc-tests testing

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured
```

Cool! We now have two tests of our internal functions. You'll note that there
are three sets of output now: one for `src/main.rs`, one for `src/lib.rs`, and
one for `tests/lib.rs`. There's one interesting thing that we haven't talked
about yet, and that's these lines:

```{rust,ignore}
use super::add_three;
use super::times_four;
```

Because we've made a nested module, we can import functions from the parent
module by using `super`. Sub-modules are allowed to 'see' private functions in
the parent.

We've now covered the basics of testing. Rust's tools are primitive, but they
work well in the simple cases. There are some Rustaceans working on building
more complicated frameworks on top of all of this, but they're just starting
out.

# Pointers

In systems programming, pointers are an incredibly important topic. Rust has a
very rich set of pointers, and they operate differently than in many other
languages. They are important enough that we have a specific [Pointer
Guide](guide-pointers.html) that goes into pointers in much detail. In fact,
while you're currently reading this guide, which covers the language in broad
overview, there are a number of other guides that put a specific topic under a
microscope. You can find the list of guides on the [documentation index
page](index.html#guides).

In this section, we'll assume that you're familiar with pointers as a general
concept. If you aren't, please read the [introduction to
pointers](guide-pointers.html#an-introduction) section of the Pointer Guide,
and then come back here. We'll wait.

Got the gist? Great. Let's talk about pointers in Rust.

## References

The most primitive form of pointer in Rust is called a **reference**.
References are created using the ampersand (`&`). Here's a simple
reference:

```{rust}
let x = 5i;
let y = &x;
```

`y` is a reference to `x`. To dereference (get the value being referred to
rather than the reference itself) `y`, we use the asterisk (`*`):

```{rust}
let x = 5i;
let y = &x;

assert_eq!(5i, *y);
```

Like any `let` binding, references are immutable by default.

You can declare that functions take a reference:

```{rust}
fn add_one(x: &int) -> int { *x + 1 }

fn main() {
    assert_eq!(6, add_one(&5));
}
```

As you can see, we can make a reference from a literal by applying `&` as well.
Of course, in this simple function, there's not a lot of reason to take `x` by
reference. It's just an example of the syntax.

Because references are immutable, you can have multiple references that
**alias** (point to the same place):

```{rust}
let x = 5i;
let y = &x;
let z = &x;
```

We can make a mutable reference by using `&mut` instead of `&`:

```{rust}
let mut x = 5i;
let y = &mut x;
```

Note that `x` must also be mutable. If it isn't, like this:

```{rust,ignore}
let x = 5i;
let y = &mut x;
```

Rust will complain:

```text
error: cannot borrow immutable local variable `x` as mutable
 let y = &mut x;
              ^
```

We don't want a mutable reference to immutable data! This error message uses a
term we haven't talked about yet, 'borrow'. We'll get to that in just a moment.

This simple example actually illustrates a lot of Rust's power: Rust has
prevented us, at compile time, from breaking our own rules. Because Rust's
references check these kinds of rules entirely at compile time, there's no
runtime overhead for this safety.  At runtime, these are the same as a raw
machine pointer, like in C or C++.  We've just double-checked ahead of time
that we haven't done anything dangerous.

Rust will also prevent us from creating two mutable references that alias.
This won't work:

```{rust,ignore}
let mut x = 5i;
let y = &mut x;
let z = &mut x;
```

It gives us this error:

```text
error: cannot borrow `x` as mutable more than once at a time
     let z = &mut x;
                  ^
note: previous borrow of `x` occurs here; the mutable borrow prevents subsequent moves, borrows, or modification of `x` until the borrow ends
     let y = &mut x;
                  ^
note: previous borrow ends here
 fn main() {
     let mut x = 5i;
     let y = &mut x;
     let z = &mut x;
 }
 ^
```

This is a big error message. Let's dig into it for a moment. There are three
parts: the error and two notes. The error says what we expected, we cannot have
two mutable pointers that point to the same memory.

The two notes give some extra context. Rust's error messages often contain this
kind of extra information when the error is complex. Rust is telling us two
things: first, that the reason we cannot **borrow** `x` as `z` is that we
previously borrowed `x` as `y`. The second note shows where `y`'s borrowing
ends.

Wait, borrowing?

In order to truly understand this error, we have to learn a few new concepts:
**ownership**, **borrowing**, and **lifetimes**.

## Ownership, borrowing, and lifetimes

Whenever a resource of some kind is created, something must be responsible
for destroying that resource as well. Given that we're discussing pointers
right now, let's discuss this in the context of memory allocation, though
it applies to other resources as well.

When you allocate heap memory, you need a mechanism to free that memory. Many
languages use a garbage collector to handle deallocation. This is a valid,
time-tested strategy, but it's not without its drawbacks: it adds overhead, and
can lead to unpredictable pauses in execution. Because the programmer does not
have to think as much about deallocation, allocation becomes something
commonplace, leading to more memory usage. And if you need precise control
over when something is deallocated, leaving it up to your runtime can make this
difficult.

Rust chooses a different path, and that path is called **ownership**. Any
binding that creates a resource is the **owner** of that resource.

Being an owner affords you some privileges:

1. You control when that resource is deallocated.
2. You may lend that resource, immutably, to as many borrowers as you'd like.
3. You may lend that resource, mutably, to a single borrower.

But it also comes with some restrictions:

1. If someone is borrowing your resource (either mutably or immutably), you may
   not mutate the resource or mutably lend it to someone.
2. If someone is mutably borrowing your resource, you may not lend it out at
   all (mutably or immutably) or access it in any way.

What's up with all this 'lending' and 'borrowing'? When you allocate memory,
you get a pointer to that memory. This pointer allows you to manipulate said
memory. If you are the owner of a pointer, then you may allow another
binding to temporarily borrow that pointer, and then they can manipulate the
memory. The length of time that the borrower is borrowing the pointer
from you is called a **lifetime**.

If two distinct bindings share a pointer, and the memory that pointer points to
is immutable, then there are no problems. But if it's mutable, the result of
changing it can vary unpredictably depending on who happens to access it first,
which is called a **race condition**. To avoid this, if someone wants to mutate
something that they've borrowed from you, you must not have lent out that
pointer to anyone else.

Rust has a sophisticated system called the **borrow checker** to make sure that
everyone plays by these rules. At compile time, it verifies that none of these
rules are broken. If our program compiles successfully, Rust can guarantee it
is free of data races and other memory errors, and there is no runtime overhead
for any of this. The borrow checker works only at compile time. If the borrow
checker did find a problem, it will report an error and your program will
refuse to compile.

That's a lot to take in. It's also one of the _most_ important concepts in
all of Rust. Let's see this syntax in action:

```{rust}
{
    let x = 5i; // x is the owner of this integer, which is memory on the stack.

    // other code here...

} // privilege 1: when x goes out of scope, this memory is deallocated

/// this function borrows an integer. It's given back automatically when the
/// function returns.
fn foo(x: &int) -> &int { x }

{
    // x is the owner of the integer, which is memory on the stack.
    let x = 5i;

    // privilege 2: you may lend that resource to as many borrowers as you like
    let y = &x;
    let z = &x;

    foo(&x); // functions can borrow too!

    let a = &x; // we can do this alllllll day!
}

{
    // x is the owner of this integer, which is memory on the stack.
    let mut x = 5i;

    // privilege 3: you may lend that resource to a single borrower, mutably
    let y = &mut x;
}
```

If you are a borrower, you get a few privileges as well, but must also obey a
restriction:

1. If the borrow is immutable, you may read the data the pointer points to.
2. If the borrow is mutable, you may read and write the data the pointer points to.
3. You may lend the pointer to someone else, **BUT**
4. When you do so, they must return it before you can give your own borrow back.

This last requirement can seem odd, but it also makes sense. If you have to
return something, and you've lent it to someone, they need to give it back to
you for you to give it back! If we didn't, then the owner could deallocate
the memory, and the person we've loaned it out to would have a pointer to
invalid memory. This is called a 'dangling pointer'.

Let's re-examine the error that led us to talk about all of this, which was a
violation of the restrictions placed on owners who lend something out mutably.
The code:

```{rust,ignore}
let mut x = 5i;
let y = &mut x;
let z = &mut x;
```

The error:

```text
error: cannot borrow `x` as mutable more than once at a time
     let z = &mut x;
                  ^
note: previous borrow of `x` occurs here; the mutable borrow prevents subsequent moves, borrows, or modification of `x` until the borrow ends
     let y = &mut x;
                  ^
note: previous borrow ends here
 fn main() {
     let mut x = 5i;
     let y = &mut x;
     let z = &mut x;
 }
 ^
```

This error comes in three parts. Let's go over each in turn.

```text
error: cannot borrow `x` as mutable more than once at a time
     let z = &mut x;
                  ^
```

This error states the restriction: you cannot lend out something mutable more
than once at the same time. The borrow checker knows the rules!

```text
note: previous borrow of `x` occurs here; the mutable borrow prevents subsequent moves, borrows, or modification of `x` until the borrow ends
     let y = &mut x;
                  ^
```

Some compiler errors come with notes to help you fix the error. This error comes
with two notes, and this is the first. This note informs us of exactly where
the first mutable borrow occurred. The error showed us the second. So now we
see both parts of the problem. It also alludes to rule #3, by reminding us that
we can't change `x` until the borrow is over.

```text
note: previous borrow ends here
 fn main() {
     let mut x = 5i;
     let y = &mut x;
     let z = &mut x;
 }
 ^
```

Here's the second note, which lets us know where the first borrow would be over.
This is useful, because if we wait to try to borrow `x` after this borrow is
over, then everything will work.

For more advanced patterns, please consult the [Ownership
Guide](guide-ownership.html).  You'll also learn what this type signature with
the `'a` syntax is:

```{rust,ignore}
pub fn as_maybe_owned(&self) -> MaybeOwned<'a> { ... }
```

## Boxes

Most of the types we've seen so far have a fixed size or number of components.
The compiler needs this fact to lay out values in memory. However, some data
structures, such as a linked list, do not have a fixed size. You might think to
implement a linked list with an enum that's either a `Node` or the end of the
list (`Nil`), like this:

```{rust,ignore}
enum List {             // error: illegal recursive enum type
    Node(u32, List),
    Nil
}
```

But the compiler complains that the type is recursive, that is, it could be
arbitrarily large. To remedy this, Rust provides a fixed-size container called
a **box** that can hold any type. You can box up any value with the `box`
keyword. Our boxed List gets the type `Box<List>` (more on the notation when we
get to generics):

```{rust}
enum List {
    Node(u32, Box<List>),
    Nil
}

fn main() {
    let list = List::Node(0, box List::Node(1, box List::Nil));
}
```

A box dynamically allocates memory to hold its contents. The great thing about
Rust is that that memory is *automatically*, *efficiently*, and *predictably*
deallocated when you're done with the box.

A box is a pointer type, and you access what's inside using the `*` operator,
just like regular references. This (rather silly) example dynamically allocates
an integer `5` and makes `x` a pointer to it:

```{rust}
{
    let x = box 5i;
    println!("{}", *x);     // Prints 5
}
```

The great thing about boxes is that we don't have to manually free this
allocation! Instead, when `x` reaches the end of its lifetime – in this case,
when it goes out of scope at the end of the block – Rust `free`s `x`. This
isn't because Rust has a garbage collector (it doesn't). Instead, by tracking
the ownership and lifetime of a variable (with a little help from you, the
programmer), the compiler knows precisely when it is no longer used.

The Rust code above will do the same thing as the following C code:

```{c,ignore}
{
    int *x = (int *)malloc(sizeof(int));
    if (!x) abort();
    *x = 5;
    printf("%d\n", *x);
    free(x);
}
```

We get the benefits of manual memory management, while ensuring we don't
introduce any bugs. We can't forget to `free` our memory.

Boxes are the sole owner of their contents, so you cannot take a mutable
reference to them and then use the original box:

```{rust,ignore}
let mut x = box 5i;
let y = &mut x;

*x; // you might expect 5, but this is actually an error
```

This gives us this error:

```text
error: cannot use `*x` because it was mutably borrowed
 *x;
 ^~
note: borrow of `x` occurs here
 let y = &mut x;
              ^
```

As long as `y` is borrowing the contents, we cannot use `x`. After `y` is
done borrowing the value, we can use it again. This works fine:

```{rust}
let mut x = box 5i;

{
    let y = &mut x;
} // y goes out of scope at the end of the block

*x;
```

Boxes are simple and efficient pointers to dynamically allocated values with a
single owner. They are useful for tree-like structures where the lifetime of a
child depends solely on the lifetime of its (single) parent. If you need a
value that must persist as long as any of several referrers, read on.

## Rc and Arc

Sometimes you need a variable that is referenced from multiple places
(immutably!), lasting as long as any of those places, and disappearing when it
is no longer referenced. For instance, in a graph-like data structure, a node
might be referenced from all of its neighbors. In this case, it is not possible
for the compiler to determine ahead of time when the value can be freed – it
needs a little run-time support.

Rust's **Rc** type provides shared ownership of a dynamically allocated value
that is automatically freed at the end of its last owner's lifetime. (`Rc`
stands for 'reference counted', referring to the way these library types are
implemented.) This provides more flexibility than single-owner boxes, but has
some runtime overhead.

To create an `Rc` value, use `Rc::new()`. To create a second owner, use the
`.clone()` method:

```{rust}
use std::rc::Rc;

let x = Rc::new(5i);
let y = x.clone();

println!("{} {}", *x, *y);      // Prints 5 5
```

The `Rc` will live as long as any of its owners are alive. After that, the
memory will be `free`d.

**Arc** is an 'atomically reference counted' value, identical to `Rc` except
that ownership can be safely shared among multiple threads. Why two types?
`Arc` has more overhead, so if you're not in a multi-threaded scenario, you
don't have to pay the price.

If you use `Rc` or `Arc`, you have to be careful about introducing cycles. If
you have two `Rc`s that point to each other, they will happily keep each other
alive forever, creating a memory leak. To learn more, check out [the section on
`Rc` and `Arc` in the pointers guide](guide-pointers.html#rc-and-arc).

# Patterns

We've made use of patterns a few times in the guide: first with `let` bindings,
then with `match` statements. Let's go on a whirlwind tour of all of the things
patterns can do!

A quick refresher: you can match against literals directly, and `_` acts as an
'any' case:

```{rust}
let x = 1i;

match x {
    1 => println!("one"),
    2 => println!("two"),
    3 => println!("three"),
    _ => println!("anything"),
}
```

You can match multiple patterns with `|`:

```{rust}
let x = 1i;

match x {
    1 | 2 => println!("one or two"),
    3 => println!("three"),
    _ => println!("anything"),
}
```

You can match a range of values with `...`:

```{rust}
let x = 1i;

match x {
    1 ... 5 => println!("one through five"),
    _ => println!("anything"),
}
```

Ranges are mostly used with integers and single characters.

If you're matching multiple things, via a `|` or a `...`, you can bind
the value to a name with `@`:

```{rust}
let x = 1i;

match x {
    e @ 1 ... 5 => println!("got a range element {}", e),
    _ => println!("anything"),
}
```

If you're matching on an enum which has variants, you can use `..` to
ignore the value and type in the variant:

```{rust}
enum OptionalInt {
    Value(int),
    Missing,
}

let x = OptionalInt::Value(5i);

match x {
    OptionalInt::Value(..) => println!("Got an int!"),
    OptionalInt::Missing   => println!("No such luck."),
}
```

You can introduce **match guards** with `if`:

```{rust}
enum OptionalInt {
    Value(int),
    Missing,
}

let x = OptionalInt::Value(5i);

match x {
    OptionalInt::Value(i) if i > 5 => println!("Got an int bigger than five!"),
    OptionalInt::Value(..) => println!("Got an int!"),
    OptionalInt::Missing   => println!("No such luck."),
}
```

If you're matching on a pointer, you can use the same syntax as you declared it
with. First, `&`:

```{rust}
let x = &5i;

match x {
    &val => println!("Got a value: {}", val),
}
```

Here, the `val` inside the `match` has type `int`. In other words, the left-hand
side of the pattern destructures the value. If we have `&5i`, then in `&val`, `val`
would be `5i`.

If you want to get a reference, use the `ref` keyword:

```{rust}
let x = 5i;

match x {
    ref r => println!("Got a reference to {}", r),
}
```

Here, the `r` inside the `match` has the type `&int`. In other words, the `ref`
keyword _creates_ a reference, for use in the pattern. If you need a mutable
reference, `ref mut` will work in the same way:

```{rust}
let mut x = 5i;

match x {
    ref mut mr => println!("Got a mutable reference to {}", mr),
}
```

If you have a struct, you can destructure it inside of a pattern:

```{rust}
# #![allow(non_shorthand_field_patterns)]
struct Point {
    x: int,
    y: int,
}

let origin = Point { x: 0i, y: 0i };

match origin {
    Point { x: x, y: y } => println!("({},{})", x, y),
}
```

If we only care about some of the values, we don't have to give them all names:

```{rust}
# #![allow(non_shorthand_field_patterns)]
struct Point {
    x: int,
    y: int,
}

let origin = Point { x: 0i, y: 0i };

match origin {
    Point { x: x, .. } => println!("x is {}", x),
}
```

You can do this kind of match on any member, not just the first:

```{rust}
# #![allow(non_shorthand_field_patterns)]
struct Point {
    x: int,
    y: int,
}

let origin = Point { x: 0i, y: 0i };

match origin {
    Point { y: y, .. } => println!("y is {}", y),
}
```

If you want to match against a slice or array, you can use `[]`:

```{rust}
fn main() {
    let v = vec!["match_this", "1"];

    match v.as_slice() {
        ["match_this", second] => println!("The second element is {}", second),
        _ => {},
    }
}
```

Whew! That's a lot of different ways to match things, and they can all be
mixed and matched, depending on what you're doing:

```{rust,ignore}
match x {
    Foo { x: Some(ref name), y: None } => ...
}
```

Patterns are very powerful.  Make good use of them.

# Method Syntax

Functions are great, but if you want to call a bunch of them on some data, it
can be awkward. Consider this code:

```{rust,ignore}
baz(bar(foo(x)));
```

We would read this left-to right, and so we see 'baz bar foo.' But this isn't the
order that the functions would get called in, that's inside-out: 'foo bar baz.'
Wouldn't it be nice if we could do this instead?

```{rust,ignore}
x.foo().bar().baz();
```

Luckily, as you may have guessed with the leading question, you can! Rust provides
the ability to use this **method call syntax** via the `impl` keyword.

Here's how it works:

```{rust}
struct Circle {
    x: f64,
    y: f64,
    radius: f64,
}

impl Circle {
    fn area(&self) -> f64 {
        std::f64::consts::PI * (self.radius * self.radius)
    }
}

fn main() {
    let c = Circle { x: 0.0, y: 0.0, radius: 2.0 };
    println!("{}", c.area());
}
```

This will print `12.566371`.

We've made a struct that represents a circle. We then write an `impl` block,
and inside it, define a method, `area`. Methods take a  special first
parameter, `&self`. There are three variants: `self`, `&self`, and `&mut self`.
You can think of this first parameter as being the `x` in `x.foo()`. The three
variants correspond to the three kinds of things `x` could be: `self` if it's
just a value on the stack, `&self` if it's a reference, and `&mut self` if it's
a mutable reference. We should default to using `&self`, as it's the most
common.

Finally, as you may remember, the value of the area of a circle is `π*r²`.
Because we took the `&self` parameter to `area`, we can use it just like any
other parameter. Because we know it's a `Circle`, we can access the `radius`
just like we would with any other struct. An import of π and some
multiplications later, and we have our area.

You can also define methods that do not take a `self` parameter. Here's a
pattern that's very common in Rust code:

```{rust}
# #![allow(non_shorthand_field_patterns)]
struct Circle {
    x: f64,
    y: f64,
    radius: f64,
}

impl Circle {
    fn new(x: f64, y: f64, radius: f64) -> Circle {
        Circle {
            x: x,
            y: y,
            radius: radius,
        }
    }
}

fn main() {
    let c = Circle::new(0.0, 0.0, 2.0);
}
```

This **static method** builds a new `Circle` for us. Note that static methods
are called with the `Struct::method()` syntax, rather than the `ref.method()`
syntax.

# Closures

So far, we've made lots of functions in Rust, but we've given them all names.
Rust also allows us to create anonymous functions. Rust's anonymous
functions are called **closure**s. By themselves, closures aren't all that
interesting, but when you combine them with functions that take closures as
arguments, really powerful things are possible.

Let's make a closure:

```{rust}
let add_one = |x| { 1i + x };

println!("The sum of 5 plus 1 is {}.", add_one(5i));
```

We create a closure using the `|...| { ... }` syntax, and then we create a
binding so we can use it later. Note that we call the function using the
binding name and two parentheses, just like we would for a named function.

Let's compare syntax. The two are pretty close:

```{rust}
let add_one = |x: int| -> int { 1i + x };
fn  add_one   (x: int) -> int { 1i + x }
```

As you may have noticed, closures infer their argument and return types, so you
don't need to declare one. This is different from named functions, which
default to returning unit (`()`).

There's one big difference between a closure and named functions, and it's in
the name: a closure "closes over its environment." What does that mean? It means
this:

```{rust}
fn main() {
    let x = 5i;

    let printer = || { println!("x is: {}", x); };

    printer(); // prints "x is: 5"
}
```

The `||` syntax means this is an anonymous closure that takes no arguments.
Without it, we'd just have a block of code in `{}`s.

In other words, a closure has access to variables in the scope where it's
defined. The closure borrows any variables it uses, so this will error:

```{rust,ignore}
fn main() {
    let mut x = 5i;

    let printer = || { println!("x is: {}", x); };

    x = 6i; // error: cannot assign to `x` because it is borrowed
}
```

## Moving closures

Rust has a second type of closure, called a **moving closure**. Moving
closures are indicated using the `move` keyword (e.g., `move || x *
x`). The difference between a moving closure and an ordinary closure
is that a moving closure always takes ownership of all variables that
it uses. Ordinary closures, in contrast, just create a reference into
the enclosing stack frame. Moving closures are most useful with Rust's
concurrency features, and so we'll just leave it at this for
now. We'll talk about them more in the "Threads" section of the guide.

## Accepting closures as arguments

Closures are most useful as an argument to another function. Here's an example:

```{rust}
fn twice(x: int, f: |int| -> int) -> int {
    f(x) + f(x)
}

fn main() {
    let square = |x: int| { x * x };

    twice(5i, square); // evaluates to 50
}
```

Let's break the example down, starting with `main`:

```{rust}
let square = |x: int| { x * x };
```

We've seen this before. We make a closure that takes an integer, and returns
its square.

```{rust}
# fn twice(x: int, f: |int| -> int) -> int { f(x) + f(x) }
# let square = |x: int| { x * x };
twice(5i, square); // evaluates to 50
```

This line is more interesting. Here, we call our function, `twice`, and we pass
it two arguments: an integer, `5`, and our closure, `square`. This is just like
passing any other two variable bindings to a function, but if you've never
worked with closures before, it can seem a little complex. Just think: "I'm
passing two variables: one is an int, and one is a function."

Next, let's look at how `twice` is defined:

```{rust,ignore}
fn twice(x: int, f: |int| -> int) -> int {
```

`twice` takes two arguments, `x` and `f`. That's why we called it with two
arguments. `x` is an `int`, we've done that a ton of times. `f` is a function,
though, and that function takes an `int` and returns an `int`. Notice
how the `|int| -> int` syntax looks a lot like our definition of `square`
above, if we added the return type in:

```{rust}
let square = |x: int| -> int { x * x };
//           |int|    -> int
```

This function takes an `int` and returns an `int`.

This is the most complicated function signature we've seen yet! Give it a read
a few times until you can see how it works. It takes a teeny bit of practice, and
then it's easy.

Finally, `twice` returns an `int` as well.

Okay, let's look at the body of `twice`:

```{rust}
fn twice(x: int, f: |int| -> int) -> int {
  f(x) + f(x)
}
```

Since our closure is named `f`, we can call it just like we called our closures
before, and we pass in our `x` argument to each one, hence the name `twice`.

If you do the math, `(5 * 5) + (5 * 5) == 50`, so that's the output we get.

Play around with this concept until you're comfortable with it. Rust's standard
library uses lots of closures where appropriate, so you'll be using
this technique a lot.

If we didn't want to give `square` a name, we could just define it inline.
This example is the same as the previous one:

```{rust}
fn twice(x: int, f: |int| -> int) -> int {
    f(x) + f(x)
}

fn main() {
    twice(5i, |x: int| { x * x }); // evaluates to 50
}
```

A named function's name can be used wherever you'd use a closure. Another
way of writing the previous example:

```{rust}
fn twice(x: int, f: |int| -> int) -> int {
    f(x) + f(x)
}

fn square(x: int) -> int { x * x }

fn main() {
    twice(5i, square); // evaluates to 50
}
```

Doing this is not particularly common, but it's useful every once in a while.

That's all you need to get the hang of closures! Closures are a little bit
strange at first, but once you're used to them, you'll miss them
in other languages. Passing functions to other functions is
incredibly powerful, as you will see in the following chapter about iterators.

# Iterators

Let's talk about loops.

Remember Rust's `for` loop? Here's an example:

```{rust}
for x in range(0i, 10i) {
    println!("{}", x);
}
```

Now that you know more Rust, we can talk in detail about how this works. The
`range` function returns an **iterator**. An iterator is something that we can
call the `.next()` method on repeatedly, and it gives us a sequence of things.

Like this:

```{rust}
let mut range = range(0i, 10i);

loop {
    match range.next() {
        Some(x) => {
            println!("{}", x);
        },
        None => { break }
    }
}
```

We make a mutable binding to the return value of `range`, which is our iterator.
We then `loop`, with an inner `match`. This `match` is used on the result of
`range.next()`, which gives us a reference to the next value of the iterator.
`next` returns an `Option<int>`, in this case, which will be `Some(int)` when
we have a value and `None` once we run out. If we get `Some(int)`, we print it
out, and if we get `None`, we `break` out of the loop.

This code sample is basically the same as our `for` loop version. The `for`
loop is just a handy way to write this `loop`/`match`/`break` construct.

`for` loops aren't the only thing that uses iterators, however. Writing your
own iterator involves implementing the `Iterator` trait. While doing that is
outside of the scope of this guide, Rust provides a number of useful iterators
to accomplish various tasks. Before we talk about those, we should talk about a
Rust anti-pattern. And that's `range`.

Yes, we just talked about how `range` is cool. But `range` is also very
primitive. For example, if you needed to iterate over the contents of
a vector, you may be tempted to write this:

```{rust}
let nums = vec![1i, 2i, 3i];

for i in range(0u, nums.len()) {
    println!("{}", nums[i]);
}
```

This is strictly worse than using an actual iterator. The `.iter()` method on
vectors returns an iterator that iterates through a reference to each element
of the vector in turn. So write this:

```{rust}
let nums = vec![1i, 2i, 3i];

for num in nums.iter() {
    println!("{}", num);
}
```

There are two reasons for this. First, this more directly expresses what we
mean. We iterate through the entire vector, rather than iterating through
indexes, and then indexing the vector. Second, this version is more efficient:
the first version will have extra bounds checking because it used indexing,
`nums[i]`. But since we yield a reference to each element of the vector in turn
with the iterator, there's no bounds checking in the second example. This is
very common with iterators: we can ignore unnecessary bounds checks, but still
know that we're safe.

There's another detail here that's not 100% clear because of how `println!`
works. `num` is actually of type `&int`. That is, it's a reference to an `int`,
not an `int` itself. `println!` handles the dereferencing for us, so we don't
see it. This code works fine too:

```{rust}
let nums = vec![1i, 2i, 3i];

for num in nums.iter() {
    println!("{}", *num);
}
```

Now we're explicitly dereferencing `num`. Why does `iter()` give us references?
Well, if it gave us the data itself, we would have to be its owner, which would
involve making a copy of the data and giving us the copy. With references,
we're just borrowing a reference to the data, and so it's just passing
a reference, without needing to do the copy.

So, now that we've established that `range` is often not what you want, let's
talk about what you do want instead.

There are three broad classes of things that are relevant here: iterators,
**iterator adapters**, and **consumers**. Here's some definitions:

* 'iterators' give you a sequence of values.
* 'iterator adapters' operate on an iterator, producing a new iterator with a
  different output sequence.
* 'consumers' operate on an iterator, producing some final set of values.

Let's talk about consumers first, since you've already seen an iterator,
`range`.

## Consumers

A 'consumer' operates on an iterator, returning some kind of value or values.
The most common consumer is `collect()`. This code doesn't quite compile,
but it shows the intention:

```{rust,ignore}
let one_to_one_hundred = range(1i, 101i).collect();
```

As you can see, we call `collect()` on our iterator. `collect()` takes
as many values as the iterator will give it, and returns a collection
of the results. So why won't this compile? Rust can't determine what
type of things you want to collect, and so you need to let it know.
Here's the version that does compile:

```{rust}
let one_to_one_hundred = range(1i, 101i).collect::<Vec<int>>();
```

If you remember, the `::<>` syntax allows us to give a type hint,
and so we tell it that we want a vector of integers.

`collect()` is the most common consumer, but there are others too. `find()`
is one:

```{rust}
let greater_than_forty_two = range(0i, 100i)
                             .find(|x| *x > 42);

match greater_than_forty_two {
    Some(_) => println!("We got some numbers!"),
    None    => println!("No numbers found :("),
}
```

`find` takes a closure, and works on a reference to each element of an
iterator. This closure returns `true` if the element is the element we're
looking for, and `false` otherwise. Because we might not find a matching
element, `find` returns an `Option` rather than the element itself.

Another important consumer is `fold`. Here's what it looks like:

```{rust}
let sum = range(1i, 4i)
              .fold(0i, |sum, x| sum + x);
```

`fold()` is a consumer that looks like this:
`fold(base, |accumulator, element| ...)`. It takes two arguments: the first
is an element called the "base". The second is a closure that itself takes two
arguments: the first is called the "accumulator," and the second is an
"element." Upon each iteration, the closure is called, and the result is the
value of the accumulator on the next iteration. On the first iteration, the
base is the value of the accumulator.

Okay, that's a bit confusing. Let's examine the values of all of these things
in this iterator:

| base | accumulator | element | closure result |
|------|-------------|---------|----------------|
| 0i   | 0i          | 1i      | 1i             |
| 0i   | 1i          | 2i      | 3i             |
| 0i   | 3i          | 3i      | 6i             |

We called `fold()` with these arguments:

```{rust}
# range(1i, 4i)
.fold(0i, |sum, x| sum + x);
```

So, `0i` is our base, `sum` is our accumulator, and `x` is our element.  On the
first iteration, we set `sum` to `0i`, and `x` is the first element of `nums`,
`1i`. We then add `sum` and `x`, which gives us `0i + 1i = 1i`. On the second
iteration, that value becomes our accumulator, `sum`, and the element is
the second element of the array, `2i`. `1i + 2i = 3i`, and so that becomes
the value of the accumulator for the last iteration. On that iteration,
`x` is the last element, `3i`, and `3i + 3i = 6i`, which is our final
result for our sum. `1 + 2 + 3 = 6`, and that's the result we got.

Whew. `fold` can be a bit strange the first few times you see it, but once it
clicks, you can use it all over the place. Any time you have a list of things,
and you want a single result, `fold` is appropriate.

Consumers are important due to one additional property of iterators we haven't
talked about yet: laziness. Let's talk some more about iterators, and you'll
see why consumers matter.

## Iterators

As we've said before, an iterator is something that we can call the
`.next()` method on repeatedly, and it gives us a sequence of things.
Because you need to call the method, this means that iterators
are **lazy** and don't need to generate all of the values upfront.
This code, for example, does not actually generate the numbers
`1-100`, and just creates a value that represents the sequence:

```{rust}
let nums = range(1i, 100i);
```

Since we didn't do anything with the range, it didn't generate the sequence.
Let's add the consumer:

```{rust}
let nums = range(1i, 100i).collect::<Vec<int>>();
```

Now, `collect()` will require that `range()` give it some numbers, and so
it will do the work of generating the sequence.

`range` is one of two basic iterators that you'll see. The other is `iter()`,
which you've used before. `iter()` can turn a vector into a simple iterator
that gives you each element in turn:

```{rust}
let nums = [1i, 2i, 3i];

for num in nums.iter() {
   println!("{}", num);
}
```

These two basic iterators should serve you well. There are some more
advanced iterators, including ones that are infinite. Like `count`:

```{rust}
std::iter::count(1i, 5i);
```

This iterator counts up from one, adding five each time. It will give
you a new integer every time, forever (well, technically, until it reaches the
maximum number representable by an `int`). But since iterators are lazy,
that's okay! You probably don't want to use `collect()` on it, though...

That's enough about iterators. Iterator adapters are the last concept
we need to talk about with regards to iterators. Let's get to it!

## Iterator adapters

"Iterator adapters" take an iterator and modify it somehow, producing
a new iterator. The simplest one is called `map`:

```{rust,ignore}
range(1i, 100i).map(|x| x + 1i);
```

`map` is called upon another iterator, and produces a new iterator where each
element reference has the closure it's been given as an argument called on it.
So this would give us the numbers from `2-100`. Well, almost! If you
compile the example, you'll get a warning:

```text
warning: unused result which must be used: iterator adaptors are lazy and
         do nothing unless consumed, #[warn(unused_must_use)] on by default
 range(1i, 100i).map(|x| x + 1i);
 ^~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
```

Laziness strikes again! That closure will never execute. This example
doesn't print any numbers:

```{rust,ignore}
range(1i, 100i).map(|x| println!("{}", x));
```

If you are trying to execute a closure on an iterator for its side effects,
just use `for` instead.

There are tons of interesting iterator adapters. `take(n)` will return an
iterator over the next `n` elements of the original iterator, note that this
has no side effect on the original iterator. Let's try it out with our infinite
iterator from before, `count()`:

```{rust}
for i in std::iter::count(1i, 5i).take(5) {
    println!("{}", i);
}
```

This will print

```text
1
6
11
16
21
```

`filter()` is an adapter that takes a closure as an argument. This closure
returns `true` or `false`. The new iterator `filter()` produces
only the elements that that closure returns `true` for:

```{rust}
for i in range(1i, 100i).filter(|&x| x % 2 == 0) {
    println!("{}", i);
}
```

This will print all of the even numbers between one and a hundred.
(Note that because `filter` doesn't consume the elements that are
being iterated over, it is passed a reference to each element, and
thus the filter predicate uses the `&x` pattern to extract the integer
itself.)

You can chain all three things together: start with an iterator, adapt it
a few times, and then consume the result. Check it out:

```{rust}
range(1i, 1000i)
    .filter(|&x| x % 2 == 0)
    .filter(|&x| x % 3 == 0)
    .take(5)
    .collect::<Vec<int>>();
```

This will give you a vector containing `6`, `12`, `18`, `24`, and `30`.

This is just a small taste of what iterators, iterator adapters, and consumers
can help you with. There are a number of really useful iterators, and you can
write your own as well. Iterators provide a safe, efficient way to manipulate
all kinds of lists. They're a little unusual at first, but if you play with
them, you'll get hooked. For a full list of the different iterators and
consumers, check out the [iterator module documentation](std/iter/index.html).

# Generics

Sometimes, when writing a function or data type, we may want it to work for
multiple types of arguments. For example, remember our `OptionalInt` type?

```{rust}
enum OptionalInt {
    Value(int),
    Missing,
}
```

If we wanted to also have an `OptionalFloat64`, we would need a new enum:

```{rust}
enum OptionalFloat64 {
    Valuef64(f64),
    Missingf64,
}
```

Such repetition is unfortunate. Luckily, Rust has a feature that gives us a
better way: **generics**. Generics are called **parametric polymorphism** in
type theory, which means that they are types or functions that have multiple
forms over a given parameter ("parametric").

Let's see how generics help us escape `OptionalInt`. `Option` is already
provided in Rust's standard library and looks like this:

```rust
enum Option<T> {
    Some(T),
    None,
}
```

The `<T>` part, which you've seen a few times before, indicates that this is a
generic data type. `T` is called a **type parameter**. When we create instances
of `Option`, we need to provide a concrete type in place of the type
parameter. For example, if we wanted something like our `OptionalInt`, we would
need to instantiate an `Option<int>`. Inside the declaration of our enum,
wherever we see a `T`, we replace it with the type specified (or inferred by the
the compiler).

```{rust}
let x: Option<int> = Some(5i);
```

In this particular `Option`, `T` has the value of `int`. On the right-hand side
of the binding, we do make a `Some(T)`, where `T` is `5i`.  Since that's an
`int`, the two sides match, and Rust is happy. If they didn't match, we'd get an
error:

```{rust,ignore}
let x: Option<f64> = Some(5i);
// error: mismatched types: expected `core::option::Option<f64>`,
// found `core::option::Option<int>` (expected f64, found int)
```

That doesn't mean we can't make `Option<T>`s that hold an `f64`! They just have to
match up:

```{rust}
let x: Option<int> = Some(5i);
let y: Option<f64> = Some(5.0f64);
```

Generics don't have to only be generic over one type. Consider Rust's built-in
`Result<T, E>` type:

```{rust}
enum Result<T, E> {
    Ok(T),
    Err(E),
}
```

This type is generic over _two_ types: `T` and `E`. By the way, the capital letters
can be any letter you'd like. We could define `Result<T, E>` as:

```{rust}
enum Result<H, N> {
    Ok(H),
    Err(N),
}
```

Convention says that the first generic parameter should be `T`, for "type," and
that we use `E` for "error."

The `Result<T, E>` type is intended to be used to return the result of a
computation and to have the ability to return an error if it didn't work
out. Here's an example:

```{rust}
let x: Result<f64, String> = Ok(2.3f64);
let y: Result<f64, String> = Err("There was an error.".to_string());
```

This particular `Result` will return an `f64` upon success and a `String` if
there's a failure. Let's write a function that uses `Result<T, E>`:

```{rust}
fn inverse(x: f64) -> Result<f64, String> {
    if x == 0.0f64 { return Err("x cannot be zero!".to_string()); }

    Ok(1.0f64 / x)
}
```

We want to indicate that `inverse(0.0f64)` is undefined or is an erroneous usage
of the function, so we check to make sure that we weren't passed zero. If we
were, we return an `Err` with a message. If it's okay, we return an `Ok` with
the answer.

Why does this matter? Well, remember how `match` does exhaustive matches?
Here's how this function gets used:

```{rust}
# fn inverse(x: f64) -> Result<f64, String> {
# if x == 0.0f64 { return Err("x cannot be zero!".to_string()); }
# Ok(1.0f64 / x)
# }
let x = inverse(25.0f64);

match x {
    Ok(x) => println!("The inverse of 25 is {}", x),
    Err(msg) => println!("Error: {}", msg),
}
```

The `match` enforces that we handle the `Err` case. In addition, because the
answer is wrapped up in an `Ok`, we can't just use the result without doing
the match:

```{rust,ignore}
let x = inverse(25.0f64);
println!("{}", x + 2.0f64); // error: binary operation `+` cannot be applied
           // to type `core::result::Result<f64,collections::string::String>`
```

This function is great, but there's one other problem: it only works for 64 bit
floating point values. If we wanted to handle 32 bit floating point values we'd
have to write this:

```{rust}
fn inverse32(x: f32) -> Result<f32, String> {
    if x == 0.0f32 { return Err("x cannot be zero!".to_string()); }

    Ok(1.0f32 / x)
}
```

What we need is a **generic function**. We can do that with Rust! However, it
won't _quite_ work yet. We need to talk about syntax. A first attempt at a
generic version of `inverse` might look something like this:

```{rust,ignore}
fn inverse<T>(x: T) -> Result<T, String> {
    if x == 0.0 { return Err("x cannot be zero!".to_string()); }

    Ok(1.0 / x)
}
```

Just like how we had `Option<T>`, we use a similar syntax for `inverse<T>`.  We
can then use `T` inside the rest of the signature: `x` has type `T`, and half of
the `Result` has type `T`. However, if we try to compile that example, we'll get
some errors:

```text
error: binary operation `==` cannot be applied to type `T`
     if x == 0.0 { return Err("x cannot be zero!".to_string()); }
                ^~~~~~~~
error: mismatched types: expected `_`, found `T` (expected floating-point variable, found type parameter)
     Ok(1.0 / x)
              ^
error: mismatched types: expected `core::result::Result<T, collections::string::String>`, found `core::result::Result<_, _>` (expected type parameter, found floating-point variable)
     Ok(1.0 / x)
     ^~~~~~~~~~~
```

The problem is that `T` is unconstrained: it can be _any_ type. It could be a
`String`, and the expression `1.0 / x` has no meaning if `x` is a `String`. It
may be a type that doesn't implement `==`, and the first line would be
wrong. What do we do?

To fix this example, we need to learn about another Rust feature: **traits**.

# Traits

Our discussion of **traits** begins with the `impl` keyword. We used it before
to specify methods.

```{rust}
struct Circle {
    x: f64,
    y: f64,
    radius: f64,
}

impl Circle {
    fn area(&self) -> f64 {
        std::f64::consts::PI * (self.radius * self.radius)
    }
}
```

We define a trait in terms of its methods. We then `impl` a trait `for` a type
(or many types).

```{rust}
struct Circle {
    x: f64,
    y: f64,
    radius: f64,
}

trait HasArea {
    fn area(&self) -> f64;
}

impl HasArea for Circle {
    fn area(&self) -> f64 {
        std::f64::consts::PI * (self.radius * self.radius)
    }
}
```

The `trait` block defines only type signatures. When we `impl` a trait, we use
`impl Trait for Item`, rather than just `impl Item`.

The first of the three errors we got with our generic `inverse` function was
this:

```text
error: binary operation `==` cannot be applied to type `T`
```

We can use traits to constrain generic type parameters. Consider this function,
which does not compile, and gives us a similar error:

```{rust,ignore}
fn print_area<T>(shape: T) {
    println!("This shape has an area of {}", shape.area());
}
```

Rust complains:

```text
error: type `T` does not implement any method in scope named `area`
```

Because `T` can be any type, we can't be sure that it implements the `area`
method. But we can add a **trait constraint** to our generic `T`, ensuring that
we can only compile the function if it's called with types which `impl` the
`HasArea` trait:

```{rust}
# trait HasArea {
#     fn area(&self) -> f64;
# }
fn print_area<T: HasArea>(shape: T) {
    println!("This shape has an area of {}", shape.area());
}
```

The syntax `<T: HasArea>` means "any type that implements the HasArea trait."
Because traits define method signatures, we can be sure that any type which
implements `HasArea` will have an `area` method.

Here's an extended example of how this works:

```{rust}
trait HasArea {
    fn area(&self) -> f64;
}

struct Circle {
    x: f64,
    y: f64,
    radius: f64,
}

impl HasArea for Circle {
    fn area(&self) -> f64 {
        std::f64::consts::PI * (self.radius * self.radius)
    }
}

struct Square {
    x: f64,
    y: f64,
    side: f64,
}

impl HasArea for Square {
    fn area(&self) -> f64 {
        self.side * self.side
    }
}

fn print_area<T: HasArea>(shape: T) {
    println!("This shape has an area of {}", shape.area());
}

fn main() {
    let c = Circle {
        x: 0.0f64,
        y: 0.0f64,
        radius: 1.0f64,
    };

    let s = Square {
        x: 0.0f64,
        y: 0.0f64,
        side: 1.0f64,
    };

    print_area(c);
    print_area(s);
}
```

This program outputs:

```text
This shape has an area of 3.141593
This shape has an area of 1
```

As you can see, `print_area` is now generic, but also ensures that we
have passed in the correct types. If we pass in an incorrect type:

```{rust,ignore}
print_area(5i);
```

We get a compile-time error:

```text
error: failed to find an implementation of trait main::HasArea for int
```

So far, we've only added trait implementations to structs, but you can
implement a trait for any type. So technically, we _could_ implement
`HasArea` for `int`:

```{rust}
trait HasArea {
    fn area(&self) -> f64;
}

impl HasArea for int {
    fn area(&self) -> f64 {
        println!("this is silly");

        *self as f64
    }
}

5i.area();
```

It is considered poor style to implement methods on such primitive types, even
though it is possible.

## Scoped Method Resolution and Orphan `impl`s

There are two restrictions for implementing traits that prevent this from
getting out of hand.

1. **Scope-based Method Resolution**: Traits must be `use`d in any scope where
   you wish to use the trait's methods
2. **No Orphan `impl`s**: Either the trait or the type you're writing the `impl`
   for must be inside your crate.

If we organize our crate differently by using modules, we'll need to ensure both
of the conditions are satisfied. Don't worry, you can lean on the compiler since
it won't let you get away with violating them.

```{rust}
use shapes::HasArea; // satisfies #1

mod shapes {
    use std::f64::consts;

    pub trait HasArea {
        fn area(&self) -> f64;
    }

    pub struct Circle {
        pub x: f64,
        pub y: f64,
        pub radius: f64,
    }

    impl HasArea for Circle {
        fn area(&self) -> f64 {
            consts::PI * (self.radius * self.radius)
        }
    }
}

fn main() {
    // use shapes::HasArea; // This would satisfy #1, too
    let c = shapes::Circle {
        x: 0.0f64,
        y: 0.0f64,
        radius: 1.0f64,
    };

    println!("{}", c.area());
}
```

Requiring us to `use` traits whose methods we want means that even if someone
does something bad like add methods to `int`, it won't affect us, unless you
`use` that trait.

The second condition allows us to `impl` built-in `trait`s for types we define,
or allows us to `impl` our own `trait`s for built-in types, but restricts us
from mixing and matching third party or built-in `impl`s with third party or
built-in types.

We could `impl` the `HasArea` trait for `int`, because `HasArea` is in our
crate. But if we tried to implement `Float`, a standard library `trait`, for
`int`, we could not, because neither the `trait` nor the `type` are in our
crate.

## Monomorphization

One last thing about generics and traits: the compiler performs
**monomorphization** on generic functions so they are statically dispatched. To
see what that means, let's take a look at `print_area` again:

```{rust,ignore}
fn print_area<T: HasArea>(shape: T) {
    println!("This shape has an area of {}", shape.area());
}

fn main() {
    let c = Circle { ... };

    let s = Square { ... };

    print_area(c);
    print_area(s);
}
```

Because we have called `print_area` with two different types in place of its
type paramater `T`, Rust will generate two versions of the function with the
appropriate concrete types, replacing the call sites with calls to the concrete
implementations. In other words, the compiler will actually compile something
more like this:

```{rust,ignore}
fn __print_area_circle(shape: Circle) {
    println!("This shape has an area of {}", shape.area());
}

fn __print_area_square(shape: Square) {
    println!("This shape has an area of {}", shape.area());
}

fn main() {
    let c = Circle { ... };

    let s = Square { ... };

    __print_area_circle(c);
    __print_area_square(s);
}
```

These names are for illustration; the compiler will generate its own cryptic
names for internal uses. The point is that there is no runtime overhead of
deciding which version to call. The function to be called is determined
statically, at compile time. Thus, generic functions are **statically
dispatched**. The downside is that we have two similar functions, so our binary
is larger.

# Threads

Concurrency and parallelism are topics that are of increasing interest to a
broad subsection of software developers. Modern computers are often multi-core,
to the point that even embedded devices like cell phones have more than one
processor. Rust's semantics lend themselves very nicely to solving a number of
issues that programmers have with concurrency. Many concurrency errors that are
runtime errors in other languages are compile-time errors in Rust.

Rust's concurrency primitive is called a **thread**. It's worth noting that
threads are implemented as a library, and not part of the language. This means
that in the future, other concurrency libraries can be written for Rust to help
in specific scenarios. Here's an example of creating a thread:

```{rust,ignore}
spawn(move || {
    println!("Hello from a thread!");
});
```

The `spawn` function takes a closure as an argument, and runs that
closure in a new thread. Typically, you will want to use a moving
closure, so that the closure takes ownership of any variables that it
touches.  This implies that those variables are not usable from the
parent thread after the child thread is spawned:

```{rust,ignore}
let mut x = vec![1i, 2i, 3i];

spawn(move || {
    println!("The value of x[0] is: {}", x[0]);
});

println!("The value of x[0] is: {}", x[0]); // error: use of moved value: `x`
```

`x` is now owned by the closure, and so we can't use it anymore. Many
other languages would let us do this, but it's not safe to do
so. Rust's borrow checker catches the error.

If threads were only able to capture these values, they wouldn't be very useful.
Luckily, threads can communicate with each other through **channel**s. Channels
work like this:

```{rust,ignore}
let (tx, rx) = channel();

spawn(move || {
    tx.send("Hello from a thread!".to_string());
});

let message = rx.recv();
println!("{}", message);
```

The `channel()` function returns two endpoints: a `Receiver<T>` and a
`Sender<T>`. You can use the `.send()` method on the `Sender<T>` end, and
receive the message on the `Receiver<T>` side with the `recv()` method.  This
method blocks until it gets a message. There's a similar method, `.try_recv()`,
which returns an `Result<T, TryRecvError>` and does not block.

If you want to send messages to the thread as well, create two channels!

```{rust,ignore}
let (tx1, rx1) = channel();
let (tx2, rx2) = channel();

spawn(move || {
    tx1.send("Hello from a thread!".to_string());
    let message = rx2.recv();
    println!("{}", message);
});

let message = rx1.recv();
println!("{}", message);

tx2.send("Goodbye from main!".to_string());
```

The closure has one sending end and one receiving end, and the main thread has
one of each as well. Now they can talk back and forth in whatever way they
wish.

Notice as well that because `Sender` and `Receiver` are generic, while you can
pass any kind of information through the channel, the ends are strongly typed.
If you try to pass a string, and then an integer, Rust will complain.

## Futures

With these basic primitives, many different concurrency patterns can be
developed. Rust includes some of these types in its standard library. For
example, if you wish to compute some value in the background, `Future` is
a useful thing to use:

```{rust}
# #![allow(deprecated)]
use std::sync::Future;

let mut delayed_value = Future::spawn(move || {
    // just return anything for examples' sake

    12345i
});
println!("value = {}", delayed_value.get());
```

Calling `Future::spawn` works just like `spawn()`: it takes a
closure. In this case, though, you don't need to mess with the
channel: just have the closure return the value.

`Future::spawn` will return a value which we can bind with `let`. It needs
to be mutable, because once the value is computed, it saves a copy of the
value, and if it were immutable, it couldn't update itself.

The future will go on processing in the background, and when we need
the final value, we can call `get()` on it. This will block until the
result is done, but if it's finished computing in the background,
we'll just get the value immediately.

## Success and failure

Threads don't always succeed, they can also panic. A thread that wishes to panic
can call the `panic!` macro, passing a message:

```{rust,ignore}
spawn(move || {
    panic!("Nope.");
});
```

If a thread panics, it is not possible for it to recover. However, it can
notify other thread that it has panicked. We can do this with `thread::try`:

```{rust,ignore}
use std::thread;
use std::rand;

let result = thread::try(move || {
    if rand::random() {
        println!("OK");
    } else {
        panic!("oops!");
    }
});
```

This thread will randomly panic or succeed. `thread::try` returns a `Result`
type, so we can handle the response like any other computation that may
fail.

# Macros

One of Rust's most advanced features is its system of **macro**s. While
functions allow you to provide abstractions over values and operations, macros
allow you to provide abstractions over syntax. Do you wish Rust had the ability
to do something that it can't currently do? You may be able to write a macro
to extend Rust's capabilities.

You've already used one macro extensively: `println!`. When we invoke
a Rust macro, we need to use the exclamation mark (`!`). There are two reasons
why this is so: the first is that it makes it clear when you're using a
macro. The second is that macros allow for flexible syntax, and so Rust must
be able to tell where a macro starts and ends. The `!(...)` helps with this.

Let's talk some more about `println!`. We could have implemented `println!` as
a function, but it would be worse. Why? Well, what macros allow you to do
is write code that generates more code. So when we call `println!` like this:

```{rust}
let x = 5i;
println!("x is: {}", x);
```

The `println!` macro does a few things:

1. It parses the string to find any `{}`s.
2. It checks that the number of `{}`s matches the number of other arguments.
3. It generates a bunch of Rust code, taking this in mind.

What this means is that you get type checking at compile time, because
Rust will generate code that takes all of the types into account. If
`println!` was a function, it could still do this type checking, but it
would happen at run time rather than compile time.

We can check this out using a special flag to `rustc`. Put this code in a file
called `print.rs`:

```{rust}
fn main() {
    let x = 5i;
    println!("x is: {}", x);
}
```

You can have the macros expanded like this: `rustc --pretty=expanded print.rs`, which will
give us this huge result:

```{rust,ignore}
#![feature(phase)]
#![no_std]
#![feature(globs)]
#[phase(plugin, link)]
extern crate "std" as std;
extern crate "native" as rt;
#[prelude_import]
use std::prelude::*;
fn main() {
    let x = 5i;
    match (&x,) {
        (__arg0,) => {
            #[inline]
            #[allow(dead_code)]
            static __STATIC_FMTSTR: [&'static str, ..1u] = ["x is: "];
            let __args_vec =
                &[::std::fmt::argument(::std::fmt::secret_show, __arg0)];
            let __args =
                unsafe {
                    ::std::fmt::Arguments::new(__STATIC_FMTSTR, __args_vec)
                };
            ::std::io::stdio::println_args(&__args)
        }
    };
}
```

Whew! This isn't too terrible. You can see that we still `let x = 5i`,
but then things get a little bit hairy. Three more bindings get set: a
static format string, an argument vector, and the arguments. We then
invoke the `println_args` function with the generated arguments.

This is the code that Rust actually compiles. You can see all of the extra
information that's here. We get all of the type safety and options that it
provides, but at compile time, and without needing to type all of this out.
This is how macros are powerful: without them you would need to type all of
this by hand to get a type-checked `println`.

For more on macros, please consult [the Macros Guide](guide-macros.html).
Macros are a very advanced and still slightly experimental feature, but they don't
require a deep understanding to be called, since they look just like functions. The
Guide can help you if you want to write your own.

# Unsafe

Finally, there's one more Rust concept that you should be aware of: `unsafe`.
There are two circumstances where Rust's safety provisions don't work well.
The first is when interfacing with C code, and the second is when building
certain kinds of abstractions.

Rust has support for [FFI](http://en.wikipedia.org/wiki/Foreign_function_interface)
(which you can read about in the [FFI Guide](guide-ffi.html)), but can't guarantee
that the C code will be safe. Therefore, Rust marks such functions with the `unsafe`
keyword, which indicates that the function may not behave properly.

Second, if you'd like to create some sort of shared-memory data structure, Rust
won't allow it, because memory must be owned by a single owner. However, if
you're planning on making access to that shared memory safe – such as with a
mutex – _you_ know that it's safe, but Rust can't know. Writing an `unsafe`
block allows you to ask the compiler to trust you. In this case, the _internal_
implementation of the mutex is considered unsafe, but the _external_ interface
we present is safe. This allows it to be effectively used in normal Rust, while
being able to implement functionality that the compiler can't double check for
us.

Doesn't an escape hatch undermine the safety of the entire system? Well, if
Rust code segfaults, it _must_ be because of unsafe code somewhere. By
annotating exactly where that is, you have a significantly smaller area to
search.

We haven't even talked about any examples here, and that's because I want to
emphasize that you should not be writing unsafe code unless you know exactly
what you're doing. The vast majority of Rust developers will only interact with
it when doing FFI, and advanced library authors may use it to build certain
kinds of abstraction.

# Conclusion

We covered a lot of ground here. When you've mastered everything in this Guide,
you will have a firm grasp of basic Rust development. There's a whole lot more
out there, we've just covered the surface. There's tons of topics that you can
dig deeper into, and we've built specialized guides for many of them. To learn
more, dig into the [full documentation index](index.html).

Happy hacking!
