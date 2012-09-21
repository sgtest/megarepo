//! Logging

// NB: transitionary, de-mode-ing.
#[forbid(deprecated_mode)];
#[forbid(deprecated_pattern)];

export console_on, console_off;

#[nolink]
extern mod rustrt {
    fn rust_log_console_on();
    fn rust_log_console_off();
}

/// Turns on logging to stdout globally
fn console_on() {
    rustrt::rust_log_console_on();
}

/**
 * Turns off logging to stdout globally
 *
 * Turns off the console unless the user has overridden the
 * runtime environment's logging spec, e.g. by setting
 * the RUST_LOG environment variable
 */
fn console_off() {
    rustrt::rust_log_console_off();
}