
'use strict';

load('jstests/selinux/lib/selinux_base_test.js');

class TestDefinition extends SelinuxBaseTest {
    async run() {
        // On RHEL7 there is no python3, but check_has_tag.py will also work with python2
        const python = (0 == runNonMongoProgram("which", "python3")) ? "python3" : "python2";

        const dirs = ["jstests/core", "jstests/core_standalone"];

        for (let dir of dirs) {
            jsTest.log("Running tests in " + dir);

            const all_tests = ls(dir).filter(d => !d.endsWith("/")).sort();
            assert(all_tests);
            assert(all_tests.length);

            for (let t of all_tests) {
                // Tests in jstests/core weren't specifically made to pass in this very scenario, so
                // we will not be fixing what is not working, and instead exclude them from running
                // as "known" to not work. This is done by the means of "no_selinux" tag
                const HAS_TAG = 0;
                if (HAS_TAG ==
                    runNonMongoProgram(python,
                                       "buildscripts/resmokelib/utils/check_has_tag.py",
                                       t,
                                       "^no_selinux$")) {
                    jsTest.log("Skipping test due to no_selinux tag: " + t);
                    continue;
                }

                // Tests relying on featureFlagXXX will not work
                if (HAS_TAG ==
                    runNonMongoProgram(python,
                                       "buildscripts/resmokelib/utils/check_has_tag.py",
                                       t,
                                       "^featureFlag.+$")) {
                    jsTest.log("Skipping test due to feature flag tag: " + t);
                    continue;
                }

                jsTest.log("Running test: " + t);
                try {
                    let evalString = "import(" + tojson(t) + ")";
                    let handle = startParallelShell(evalString, db.getMongo().port);
                    let rc = handle();
                    assert.eq(rc, 0);
                } catch (e) {
                    print(tojson(e));
                    throw ("failed to load test " + t);
                }

                jsTest.log("Successful test: " + t);
            }
        }

        jsTest.log("code test suite ran successfully");
    }
}
