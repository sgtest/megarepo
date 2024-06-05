## Changes to the language {#language}

<!-- go.dev/issue/61405, CL 557835, CL 584596 -->
Go 1.23 makes the (Go 1.22) ["range-over-func" experiment](https://tip.golang.org/wiki/RangefuncExperiment) a part of the language.
The "range" clause in a "for-range" loop now accepts iterator functions of the following types

        func(func() bool)
        func(func(K) bool)
        func(func(K, V) bool)

 as range expressions.
 Calls of the iterator argument function produce the iteration values for the "for-range" loop.
 For details see the [language spec](https://tip.golang.org/ref/spec#For_statements).

<!-- go.dev/issue/46477, CL 566856, CL 586955, CL 586956 -->
Go 1.23 includes preview support for [generic type aliases](/issue/46477).
Building the toolchain with `GOEXPERIMENT=aliastypeparams` enables this feature.
