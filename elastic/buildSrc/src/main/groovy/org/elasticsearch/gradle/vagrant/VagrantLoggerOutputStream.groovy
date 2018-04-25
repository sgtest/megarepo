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
package org.elasticsearch.gradle.vagrant

import com.carrotsearch.gradle.junit4.LoggingOutputStream
import org.gradle.internal.logging.progress.ProgressLogger

/**
 * Adapts an OutputStream being written to by vagrant into a ProcessLogger. It
 * has three hacks to make the output nice:
 *
 * 1. Attempt to filter out the "unimportant" output from vagrant. Usually
 * vagrant prefixes its more important output with "==> $boxname: ". The stuff
 * that isn't prefixed that way can just be thrown out.
 *
 * 2. It also attempts to detect when vagrant does tricks assuming its writing
 * to a terminal emulator and renders the output more like gradle users expect.
 * This means that progress indicators for things like box downloading work and
 * box importing look pretty good.
 *
 * 3. It catches lines that look like "==> $boxName ==> Heading text" and stores
 * the text after the second arrow as a "heading" for use in annotating
 * provisioning. It does this because provisioning can spit out _lots_ of text
 * and its very easy to lose context when there isn't a scrollback. So we've
 * sprinkled `echo "==> Heading text"` into the provisioning scripts for this
 * to catch so it can render the output like
 * "Heading text > stdout from the provisioner".
 */
public class VagrantLoggerOutputStream extends LoggingOutputStream {
    private static final String HEADING_PREFIX = '==> '

    private final ProgressLogger progressLogger
    private boolean isStarted = false
    private String squashedPrefix
    private String lastLine = ''
    private boolean inProgressReport = false
    private String heading = ''

    VagrantLoggerOutputStream(Map args) {
        progressLogger = args.factory.newOperation(VagrantLoggerOutputStream)
        progressLogger.setDescription("Vagrant output for `$args.command`")
        squashedPrefix = args.squashedPrefix
    }

    @Override
    public void flush() {
        if (isStarted == false) {
            progressLogger.started()
            isStarted = true
        }
        if (end == start) return
        line(new String(buffer, start, end - start))
        start = end
    }

    void line(String line) {
        if (line.startsWith('\r\u001b')) {
            /* We don't want to try to be a full terminal emulator but we want to
              keep the escape sequences from leaking and catch _some_ of the
              meaning. */
            line = line.substring(2)
            if ('[K' == line) {
                inProgressReport = true
            }
            return
        }
        if (line.startsWith(squashedPrefix)) {
            line = line.substring(squashedPrefix.length())
            inProgressReport = false
            lastLine = line
            if (line.startsWith(HEADING_PREFIX)) {
                line = line.substring(HEADING_PREFIX.length())
                heading = line + ' > '
            } else {
                line = heading + line
            }
        } else if (inProgressReport) {
            inProgressReport = false
            line = lastLine + line
        } else {
            return
        }
        progressLogger.progress(line)
    }
}
