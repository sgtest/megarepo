#!/usr/bin/env python

# this combines all the working run-pass tests into a single large crate so we
# can run it "fast": spawning zillions of windows processes is our major build
# bottleneck (and it doesn't hurt to run faster on other platforms as well).

import sys, os, re

def scrub(b):
  if sys.version_info >= (3,) and type(b) == bytes:
    return b.decode('ascii')
  else:
    return b

src_dir = scrub(os.getenv("CFG_SRC_DIR"))
if not src_dir:
  raise Exception("missing env var CFG_SRC_DIR")

run_pass = os.path.join(src_dir, "src", "test", "run-pass")
stage2_tests = []
take_args = {}

for t in os.listdir(run_pass):
    if t.endswith(".rs"):
        f = open(os.path.join(run_pass, t))
        s = f.read()
        if not ("xfail-stage2" in s):
            stage2_tests.append(t)
            if "main(vec[str] args)" in s:
                take_args[t] = True
        f.close()

stage2_tests.sort()

# add a .. prefix because we're about to write down into test/..
parent_run_pass = os.path.join("..", run_pass);


c = open("test/run_pass_stage2.rc", "w")
i = 0
c.write("// AUTO-GENERATED FILE: DO NOT EDIT\n")
c.write("#[link(name=\"run_pass_stage2\", vers=\"0.1\")];\n")
for t in stage2_tests:
    c.write("mod t_%d = \"%s\";\n"
            % (i, os.path.join(parent_run_pass, t)))
    i += 1
c.close()


d = open("test/run_pass_stage2_driver.rs", "w")
d.write("// AUTO-GENERATED FILE: DO NOT EDIT\n")
d.write("use std;\n")
d.write("use run_pass_stage2;\n")
d.write("import run_pass_stage2::*;\n")
d.write("fn main() {\n");
d.write("    auto out = std::io::stdout();\n");
i = 0
for t in stage2_tests:
    d.write("    out.write_str(\"run-pass [stage2]: %s\\n\");\n"
            % os.path.join("test", "run-pass", t))
    if t in take_args:
        d.write("    t_%d::main([\"arg0\"]);\n" % i)
    else:
        d.write("    t_%d::main();\n" % i)
    i += 1
d.write("}\n")
