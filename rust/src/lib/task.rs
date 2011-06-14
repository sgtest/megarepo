native "rust" mod rustrt {
    fn task_sleep(uint time_in_us);
    fn task_yield();
}

/**
 * Hints the scheduler to yield this task for a specified ammount of time.
 *
 * arg: time_in_us maximum number of microseconds to yield control for
 */
fn sleep(uint time_in_us) {
    ret rustrt::task_sleep(time_in_us);
}

fn yield() {
    ret rustrt::task_yield();
}

fn join(task t) {
    // TODO: figure out how to pass tasks to the runtime and call the builtin
    // join.
}

// Local Variables:
// mode: rust;
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// compile-command: "make -k -C .. 2>&1 | sed -e 's/\\/x\\//x:\\//g'";
// End:
