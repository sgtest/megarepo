# ESQL's CSV-SPEC Integration Tests

ESQL has lots of different kinds of integration tests! Like the rest of
Elasticsearch it has YAML tests and Java Rest tests and ESIntegTestCase
subclasses, but it *also* has CSV-SPEC tests. You can think of them like
the YAML tests, but they can *only* call _query and assert on the response.
That simplicity let's us run them in lots of contexts and keeps them *fast*.
As such, most of ESQL's integration tests are CSV-SPEC tests.

## Running

CSV-SPEC tests run in lots of different ways. The simplest way to run a
CSV-SPEC test is to open ESQL's CsvTests.java and run it right in IntelliJ using
the unit runner. As of this writing that runs 1,350 tests in about 35 seconds.
It's fast because it doesn't stand up an Elasticsearch node at all. It runs
like a big unit test

The second-simplest way to run the CSV-SPEC tests is to run `EsqlSpecIT` in
`:x-pack:plugin:esql:qa:server:single-node` via the Gradle runner in IntelliJ
or on the command line. That will boot a real Elasticsearch node, create some
test data, and run the tests. The tests are reused in a few more scenarios,
include multi-node and mixed-cluster.

## Organization

The CSV-SPEC tests grew organically for a long time, but we've since grown
general organizing principles. But lots of tests don't follow those principles.
See organic growth. Anyway!

### Files named after types

Basic support for a type, like, say, `integer` or `geo_point` will live in a
file named after the type.

* `boolean`
* `date`
* `floats` (`double`)
* `ints` (`integer` and `long`)
* `ip`
* `null`
* `unsigned_long`
* `version`

Many functions can take lots of different types as input. Like `TO_STRING`
and `VALUES`. Those tests also live in these files.

### Themed functions

Some files are named after groups of functions and contain, unsurprisingly,
the tests for those functions:

* `comparison`
* `conditional`
* `math`

### Files named after operations

Lots of commands have files named after operations in the ESQL language and
contain the integration testing of the syntax and options in that operation.
Operations will appear in many of the other files, especially `FROM`, `WHERE`,
`LIMIT`, and `EVAL`, but to test particular functions.

* `dissect`
* `drop`
* `enrich`
* `eval`
* `grok`
* `order`
* `keep`
* `limit`
* `meta`
* `mv_expand`
* `rename`
* `row`
* `stats`
* `topN`
* `where`
* `where-like`

### Deprecated files

When we first implemented copying snippets into the documentation I dumped all
the snippets into `docs.csv-spec`. This was supposed to be a temporary holding
area until they were relocated, and we haven't had time to do that. Don't put
more tests in there.

## Embedding examples in the documentation

Snippets from these tests can be embedded into the asciidoc documentation of
ESQL using the following rather arcane snippet:

```asciidoc
[source.merge.styled,esql]
----
include::{esql-specs}/floats.csv-spec[tag=sin]
----
[%header.monospaced.styled,format=dsv,separator=|]
|===
include::{esql-specs}/floats.csv-spec[tag=sin-result]
|===
```
<details>
  <summary>What is this asciidoc syntax?</summary>

The first section is a source code block for the ES|QL query: 

- a [source](https://docs.asciidoctor.org/asciidoc/latest/verbatim/source-blocks/) code block (delimited by `----`)
	- `source.merge.styled,esql` indicates custom syntax highlighting for ES|QL
- an [include directive](https://docs.asciidoctor.org/asciidoc/latest/directives/include/) to import content from another file (i.e. test files here) into the current document
- a directory path defined as an [attribute](https://docs.asciidoctor.org/asciidoc/latest/attributes/document-attributes/) or variable, within curly braces: `{esql-specs}`
- a [tagged region](https://docs.asciidoctor.org/asciidoc/latest/directives/include-tagged-regions/#tagging-regions) `[tag=sin]` to only include a specific section of file

The second section is the response returned as a table:

- styled using `[%header.monospaced.styled,format=dsv,separator=|]`
- delimited by `|===`
- again using includes, attributes, and tagged regions
</details>

The example above extracts the `sin` test from the `floats` file. If you are
writing the tests for a function don't build this by hand, instead annotate
the `.java` file for the function with `@FunctionInfo` and add an `examples`
field like this:

```java
@FunctionInfo(
    returnType = "double",
    description = "Returns ths {wikipedia}/Sine_and_cosine[Sine] trigonometric function of an angle.",
    examples = @Example(file = "floats", tag = "sin")
)
```

Running the tests will generate the asciidoc files for you. See
`esql/functions/README.md` for all of the docs the tests generate.

Either way, CSV-SPEC files must be tagged using four special comments so snippets can be
included in the docs:

```csv-spec
sin
// tag::sin[]
ROW a=1.8
| EVAL sin=SIN(a)
// end::sin[]
;

// tag::sin-result[]
a:double | sin:double
     1.8 | 0.9738476308781951
// end::sin-result[]
;
```

The `// tag::` and `// end::` are standard asciidoc syntax for working with [tagged regions](https://docs.asciidoctor.org/asciidoc/latest/directives/include-tagged-regions/#tagging-regions). Weird looking but
you aren't going to type it by accident!

Finally, this'll appear in the docs as a table kind of like this:

| a:double |         sin:double |
|---------:|-------------------:|
|      1.8 | 0.9738476308781951 |

### Skipping tests in old versions

CSV-SPEC tests run against half-upgraded clusters in the
`x-pack:plugin:esql:qa:server:mixed-cluster` project and will fail if they test
new behavior against an old node. To stop them from running you should create
a `NodeFeature` in `EsqlFeatures` for your change. Then you can skip it by
adding a `required_feature` to your test like so:
```csv-spec
mvSlice
required_feature: esql.mv_sort

row a = [true, false, false, true]
| eval a1 = mv_slice(a, 1), a2 = mv_slice(a, 2, 3);
```

That skips nodes that don't have the `esql.mv_sort` feature.
