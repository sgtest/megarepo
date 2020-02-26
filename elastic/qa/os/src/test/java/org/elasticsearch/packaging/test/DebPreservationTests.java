/*
 * Licensed to Elasticsearch under one or more contributor
 * license agreements. See the NOTICE file distributed with
 * this work for additional information regarding copyright
 * ownership. Elasticsearch licenses this file to you under
 * the Apache License, Version 2.0 (the "License"); you may
 * not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *    http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing,
 * software distributed under the License is distributed on an
 * "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
 * KIND, either express or implied.  See the License for the
 * specific language governing permissions and limitations
 * under the License.
 */

package org.elasticsearch.packaging.test;

import org.elasticsearch.packaging.util.Distribution;
import org.junit.BeforeClass;

import java.nio.file.Paths;

import static org.elasticsearch.packaging.util.FileExistenceMatchers.fileExists;
import static org.elasticsearch.packaging.util.FileUtils.append;
import static org.elasticsearch.packaging.util.FileUtils.assertPathsDoNotExist;
import static org.elasticsearch.packaging.util.FileUtils.assertPathsExist;
import static org.elasticsearch.packaging.util.Packages.SYSVINIT_SCRIPT;
import static org.elasticsearch.packaging.util.Packages.assertInstalled;
import static org.elasticsearch.packaging.util.Packages.assertRemoved;
import static org.elasticsearch.packaging.util.Packages.installPackage;
import static org.elasticsearch.packaging.util.Packages.packageStatus;
import static org.elasticsearch.packaging.util.Packages.remove;
import static org.elasticsearch.packaging.util.Packages.verifyPackageInstallation;
import static org.hamcrest.core.Is.is;
import static org.junit.Assume.assumeTrue;

public class DebPreservationTests extends PackagingTestCase {

    @BeforeClass
    public static void filterDistros() {
        assumeTrue("only deb", distribution.packaging == Distribution.Packaging.DEB);
        assumeTrue("only bundled jdk", distribution.hasJdk);
    }

    public void test10Install() throws Exception {
        assertRemoved(distribution());
        installation = installPackage(sh, distribution());
        assertInstalled(distribution());
        verifyPackageInstallation(installation, distribution(), sh);
    }

    public void test20Remove() throws Exception {
        append(installation.config(Paths.get("jvm.options.d", "heap.options")), "# foo");

        remove(distribution());

        // some config files were not removed
        assertPathsExist(
            installation.config,
            installation.config("elasticsearch.yml"),
            installation.config("jvm.options"),
            installation.config("log4j2.properties"),
            installation.config(Paths.get("jvm.options.d", "heap.options"))
        );

        if (distribution().isDefault()) {
            assertPathsExist(
                installation.config,
                installation.config("role_mapping.yml"),
                installation.config("roles.yml"),
                installation.config("users"),
                installation.config("users_roles")
            );
        }

        // keystore was removed

        assertPathsDoNotExist(installation.config("elasticsearch.keystore"), installation.config(".elasticsearch.keystore.initial_md5sum"));

        // doc files were removed

        assertPathsDoNotExist(
            Paths.get("/usr/share/doc/" + distribution().flavor.name),
            Paths.get("/usr/share/doc/" + distribution().flavor.name + "/copyright")
        );

        // sysvinit service file was not removed
        assertThat(SYSVINIT_SCRIPT, fileExists());

        // defaults file was not removed
        assertThat(installation.envFile, fileExists());
    }

    public void test30Purge() throws Exception {
        append(installation.config(Paths.get("jvm.options.d", "heap.options")), "# foo");

        sh.run("dpkg --purge " + distribution().flavor.name);

        assertRemoved(distribution());

        assertPathsDoNotExist(installation.config, installation.envFile, SYSVINIT_SCRIPT);

        assertThat(packageStatus(distribution()).exitCode, is(1));
    }
}
