% The Rust Guide

<div style="border: 2px solid red; padding:5px;">
This guide is a work in progress. Until it is ready, we highly recommend that
you read the <a href="tutorial.html">Tutorial</a> instead. This work-in-progress Guide is being
displayed here in line with Rust's open development policy. Please open any
issues you find as usual.
</div>

## Welcome!

Hey there! Welcome to the Rust guide. This is the place to be if you'd like to
learn how to program in Rust. Rust is a systems programming language with a
focus on "high-level, bare-metal programming": the lowest level control a
programming language can give you, but with zero-cost, higher level
abstractions, because people aren't computers. We really think Rust is
something special, and we hope you do too.

To show you how to get going with Rust, we're going to write the traditional
"Hello, World!" program. Next, we'll introduce you to a tool that's useful for
writing real-world Rust programs and libraries: "Cargo." Then, we'll show off
Rust's features by writing a little program together.

Sound good? Let's go!

## Installing Rust

The first step to using Rust is to install it! There are a number of ways to
install Rust, but the easiest is to use the the `rustup` script. If you're on
Linux or a Mac, All you need to do is this:

```{ignore}
$ curl -s http://www.rust-lang.org/rustup.sh | sudo sh
```

(If you're concerned about `curl | sudo sh`, please keep reading. Disclaimer
below.)

If you're on Windows, please [download this .exe and run
it](http://static.rust-lang.org/dist/rust-nightly-install.exe).

If you decide you don't want Rust anymore, we'll be a bit sad, but that's okay.
Not every programming language is great for everyone. Just pass an argument to
the script:

```{ignore}
$ curl -s http://www.rust-lang.org/rustup.sh | sudo sh -s -- --uninstall
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

* Windows (7, 8, Server 2008 R2), x86 only
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

```{ignore}
$ rustc --version
```

You should see some output that looks something like this:

```{ignore}
rustc 0.11.0-pre (443a1cd 2014-06-08 14:56:52 -0700)
host: x86_64-unknown-linux-gnu
```

If you did, Rust has been installed successfully! Congrats!

If not, there are a number of places where you can get help. The easiest is
IRC, which you can access
[here](http://chat.mibbit.com/?server=irc.mozilla.org&channel=%23rust). Click
that link, and you'll be chatting with other Rustaceans (a silly nickname we
call ourselves), and we can help you out. Other great resources include our
[mailing list](https://mail.mozilla.org/listinfo/rust-dev),
[subreddit](http://www.reddit.com/r/rust), and
[StackOverflow](http://stackoverflow.com/questions/tagged/rust).

## Hello, world!

Now that you have Rust installed, let's write your first Rust program. It's
traditional to make your first program in any new language one that prints the
text "Hello, world!" to the screen. The nice thing about starting with such a
simple program is that you can verify that your compiler isn't just installed,
but also working properly. And printing information to the screen is a pretty
common thing to do.

The first thing that we need to do is make a file to put our code in. I like
to make a projects directory in my home directory, and keep all my projects
there. Rust does not care where your code lives.

This actually leads to one other concern we should address: this tutorial will
assume that you have basic familiarity with the command-line. Rust does not
require that you know a whole ton about the command line, but until the
language is in a more finished state, IDE support is spotty. Rust makes no
specific demands on your editing tooling, or where your code lives.

With that said, let's make a directory in our projects directory. Note that you
don't need to type in the `$`s, they just indicate the start of each command:

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
whatever method you want. We'll call our file `hello_world.rs`:

```{bash}
$ editor hello_world.rs
```

Rust files always end in a `.rs` extension. If you're using more than one word
in your file name, use an underscore. `hello_world.rs` versus `goodbye.rs`.

Now that you've got your file open, type this in:

```
fn main() {
    println!("Hello, world");
}
```

Save the file, and then type this into your terminal window:

```{bash}
$ rustc hello_world.rs
$ ./hello_world  # on Windows, this is ./hello_world.exe
Hello, world
```

Success! Let's go over what just happened in detail.

```
fn main() {
   
}
```

These two lines define a **function** in Rust. The `main` function is special:
it's the beginning of every Rust program. The first line says "I'm declaring a
function named `main`, which takes no arguments and returns nothing." If there
were arguments, they would go inside the parentheses (`(` and `)`), and because
we aren't returning anything from this function, we've dropped that notation
entirely.  We'll get to it later.

You'll also note that the function is wrapped in curly braces (`{` and `}`).
Rust requires these around all function bodies. It is also considered good
style to put the curly brace on the same line as the function declaration, with
one space in between.

Next up is this line:

```
    println!("Hello, world");
```

This line does all of the work in our little program. There are a number of
details that are important here. The first is that it's indented with four
spaces, not tabs. Please configure your editor of choice to insert four spaces
with the tab key. We provide some sample configurations for various editors
[here](https://github.com/rust-lang/rust/tree/master/src/etc).

The second point is the `println!()` part. This is calling a Rust **macro**,
which is how metaprogramming is done in Rust. If it were a function instead, it
would look like this: `println()`. For our purposes, we don't need to worry
about this difference. Just know that sometimes, you'll see a `!`, and that
means that you're calling a macro instead of a normal function.

Next, `"Hello, world"` is a **string**. Strings are a surprisingly
complicated topic in a systems programming language, and this is a **staticly
allocated** string. We will talk more about different kinds of allocation
later. We pass this string as an argument to `println!`, which prints the
string to the screen. Easy enough!

Finally, the line ends with a semicolon (`;`). Rust is an **expression
oriented** language, which means that most things are expressions. The `;` is
used to indicate that this expression is over, and the next one is ready to
begin. Most lines of Rust code end with a `;`. We will cover this in-depth
later in the tutorial.

Finally, actually **compiling** and **running** our program. We can compile
with our compiler, `rustc`, by passing it the name of our source file:

```{bash}
$ rustc hello_world.rs
```

This is similar to `gcc` or `clang`, if you come from a C or C++ background. Rust
will output a binary executable. You can see it with `ls`:

```{bash}
$ ls
hello_world  hello_world.rs
```

Or on Windows:

```{bash}
$ dir
hello_world.exe  hello_world.rs
```

There are now two files: our source code, with the `.rs`, and the executable.
We ran the executable like this:

```{bash}
$ ./hello_world  # or ./hello_world.exe on Windows
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

## Hello, Cargo!
























