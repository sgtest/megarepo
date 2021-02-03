/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.transform.log4j;

import junit.framework.TestCase;

import java.util.List;

import static org.hamcrest.Matchers.equalTo;
import static org.junit.Assert.assertThat;

public class TransformLog4jConfigTests extends TestCase {

    /**
     * Check that the transformer doesn't explode when given an empty file.
     */
    public void testTransformEmptyConfig() {
        runTest(List.of(), List.of());
    }

    /**
     * Check that the transformer leaves non-appender lines alone.
     */
    public void testTransformEchoesNonAppenderLines() {
        List<String> input = List.of(
            "status = error",
            "",
            "##############################",
            "rootLogger.level = info",
            "example = \"broken\\",
            "    line\""
        );

        runTest(input, input);
    }

    /**
     * Check that the root logger appenders are filtered to just the "rolling" appender
     */
    public void testTransformFiltersRootLogger() {
        List<String> input = List.of(
            "rootLogger.appenderRef.console.ref = console",
            "rootLogger.appenderRef.rolling.ref = rolling",
            "rootLogger.appenderRef.rolling_old.ref = rolling_old"
        );
        List<String> expected = List.of("rootLogger.appenderRef.rolling.ref = rolling");

        runTest(input, expected);
    }

    /**
     * Check that any explicit 'console' or 'rolling_old' appenders are removed.
     */
    public void testTransformRemoveExplicitConsoleAndRollingOldAppenders() {
        List<String> input = List.of(
            "appender.console.type = Console",
            "appender.console.name = console",
            "appender.console.layout.type = PatternLayout",
            "appender.console.layout.pattern = [%d{ISO8601}][%-5p][%-25c{1.}] [%node_name]%marker %m%n",
            "appender.rolling_old.type = RollingFile",
            "appender.rolling_old.name = rolling_old",
            "appender.rolling_old.layout.type = PatternLayout",
            "appender.rolling_old.layout.pattern = [%d{ISO8601}][%-5p][%-25c{1.}] [%node_name]%marker %m%n"
        );

        runTest(input, List.of());
    }

    /**
     * Check that rolling file appenders are converted to console appenders.
     */
    public void testTransformConvertsRollingToConsole() {
        List<String> input = List.of("appender.rolling.type = RollingFile", "appender.rolling.name = rolling");

        List<String> expected = List.of("appender.rolling.type = Console", "appender.rolling.name = rolling");

        runTest(input, expected);
    }

    /**
     * Check that rolling file appenders have redundant properties removed.
     */
    public void testTransformRemovedRedundantProperties() {
        List<String> input = List.of(
            "appender.rolling.fileName = ${sys:es.logs.base_path}/${sys:es.logs.cluster_name}_server.json",
            "appender.rolling.layout.type = ECSJsonLayout",
            "appender.rolling.layout.dataset = elasticsearch.server",
            "appender.rolling.filePattern = ${sys:es.logs.base_path}/${sys:es.logs.cluster_name}-%d{yyyy-MM-dd}-%i.json.gz",
            "appender.rolling.policies.type = Policies",
            "appender.rolling.strategy.type = DefaultRolloverStrategy"
        );

        List<String> expected = List.of(
            "appender.rolling.layout.type = ECSJsonLayout",
            "appender.rolling.layout.dataset = elasticsearch.server"
        );

        runTest(input, expected);
    }

    /**
     * Check that rolling file appenders have redundant properties removed.
     */
    public void testTransformSkipsPropertiesWithLineBreaks() {
        List<String> input = List.of(
            "appender.rolling.fileName = ${sys:es.logs.base_path}${sys:file.separator}\\",
            "    ${sys:es.logs.cluster_name}_server.json",
            "appender.rolling.layout.type = ECSJsonLayout"
        );

        List<String> expected = List.of("appender.rolling.layout.type = ECSJsonLayout");

        runTest(input, expected);
    }

    /**
     * Check that as well as skipping old appenders, logger references to them are also skipped.
     */
    public void testTransformSkipsOldAppenderRefs() {
        List<String> input = List.of(
            "logger.index_indexing_slowlog.appenderRef.index_indexing_slowlog_rolling_old.ref = index_indexing_slowlog_rolling_old"
        );

        runTest(input, List.of());
    }

    /**
     * Check that multiple blank lines are reduced to a single line.
     */
    public void testMultipleBlanksReducedToOne() {
        List<String> input = List.of("status = error", "", "", "rootLogger.level = info");

        List<String> expected = List.of("status = error", "", "rootLogger.level = info");

        final List<String> transformed = TransformLog4jConfig.skipBlanks(input);
        assertThat(transformed, equalTo(expected));
    }

    private void runTest(List<String> input, List<String> expected) {
        final List<String> transformed = TransformLog4jConfig.transformConfig(input);

        assertThat(transformed, equalTo(expected));
    }
}
