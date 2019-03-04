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
package org.elasticsearch.gradle;

public enum Distribution {

    INTEG_TEST("integ-test"),
    DEFAULT("elasticsearch"),
    OSS("elasticsearch-oss");

    private final String fileName;

    Distribution(String name) {
        this.fileName = name;
    }

    public String getArtifactName() {
        return fileName;
    }

    public String getFileExtension() {
        if (this.equals(INTEG_TEST)) {
            return "zip";
        } else {
            return OS.conditionalString()
                .onUnix(() -> "tar.gz")
                .onWindows(() -> "zip")
                .supply();
        }
    }

    public String getClassifier() {
        return OS.<String>conditional()
            .onLinux(() -> "linux-x86_64")
            .onWindows(() -> "windows-x86_64")
            .onMac(() -> "darwin-x86_64")
            .supply();
    }
}
