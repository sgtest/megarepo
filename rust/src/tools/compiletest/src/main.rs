use std::{env, sync::Arc};

use compiletest::{common::Mode, log_config, parse_config, run_tests};

fn main() {
    tracing_subscriber::fmt::init();

    let config = Arc::new(parse_config(env::args().collect()));

    if config.valgrind_path.is_none() && config.force_valgrind {
        panic!("Can't find Valgrind to run Valgrind tests");
    }

    if !config.has_tidy && config.mode == Mode::Rustdoc {
        eprintln!("warning: `tidy` is not installed; diffs will not be generated");
    }

    if !config.profiler_support && config.mode == Mode::CoverageRun {
        let actioned = if config.bless { "blessed" } else { "checked" };
        eprintln!(
            r#"
WARNING: profiler runtime is not available, so `.coverage` files won't be {actioned}
help: try setting `profiler = true` in the `[build]` section of `config.toml`"#
        );
    }

    log_config(&config);
    run_tests(config);
}
