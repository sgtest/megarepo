
#ifndef RUST_ENV_H
#define RUST_ENV_H

#include "rust_globals.h"

struct rust_env {
    size_t num_sched_threads;
    size_t min_stack_size;
    size_t max_stack_size;
    char* logspec;
    bool detailed_leaks;
    char* rust_seed;
    bool poison_on_free;
    int argc;
    char **argv;
};

rust_env* load_env(int argc, char **argv);
void free_env(rust_env *rust_env);

#endif
