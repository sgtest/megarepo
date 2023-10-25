#![feature(diagnostic_namespace)]

#[diagnostic::on_unimplemented(
    if(Self = "()"),
    //~^WARN malformed `on_unimplemented` attribute
    //~|WARN malformed `on_unimplemented` attribute
    message = "custom message",
    note = "custom note"
)]
#[diagnostic::on_unimplemented(message = "fallback!!")]
#[diagnostic::on_unimplemented(label = "fallback label")]
#[diagnostic::on_unimplemented(note = "fallback note")]
trait Foo {}

fn takes_foo(_: impl Foo) {}

fn main() {
    takes_foo(());
    //~^ERROR custom message
}
