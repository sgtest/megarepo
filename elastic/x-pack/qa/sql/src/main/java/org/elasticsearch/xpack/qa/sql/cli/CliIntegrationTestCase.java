/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.qa.sql.cli;

import org.apache.http.HttpEntity;
import org.apache.http.entity.ContentType;
import org.apache.http.entity.StringEntity;
import org.elasticsearch.common.CheckedConsumer;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.json.JsonXContent;
import org.elasticsearch.test.rest.ESRestTestCase;
import org.elasticsearch.xpack.qa.sql.cli.EmbeddedCli.SecurityConfig;
import org.junit.After;
import org.junit.Before;

import java.io.IOException;

import static java.util.Collections.singletonMap;
import static org.elasticsearch.xpack.qa.sql.rest.RestSqlTestCase.assertNoSearchContexts;

public abstract class CliIntegrationTestCase extends ESRestTestCase {
    /**
     * Read an address for Elasticsearch suitable for the CLI from the system properties.
     */
    public static String elasticsearchAddress() {
        String cluster = System.getProperty("tests.rest.cluster");
        // CLI only supports a single node at a time so we just give it one.
        return cluster.split(",")[0];
    }

    private EmbeddedCli cli;

    /**
     * Asks the CLI Fixture to start a CLI instance.
     */
    @Before
    public void startCli() throws IOException {
        cli = new EmbeddedCli(CliIntegrationTestCase.elasticsearchAddress(), true, securityConfig());
    }

    @After
    public void orderlyShutdown() throws Exception {
        if (cli == null) {
            // failed to connect to the cli so there is nothing to do here
            return;
        }
        cli.close();
        assertNoSearchContexts();
    }

    /**
     * Override to add security configuration to the cli.
     */
    protected SecurityConfig securityConfig() {
        return null;
    }

    protected void index(String index, CheckedConsumer<XContentBuilder, IOException> body) throws IOException {
        XContentBuilder builder = JsonXContent.contentBuilder().startObject();
        body.accept(builder);
        builder.endObject();
        HttpEntity doc = new StringEntity(Strings.toString(builder), ContentType.APPLICATION_JSON);
        client().performRequest("PUT", "/" + index + "/doc/1", singletonMap("refresh", "true"), doc);
    }

    public String command(String command) throws IOException {
        return cli.command(command);
    }

    /**
     * Read a line produced by the CLI.
     * Note that these lines will contain {@code xterm-256color}
     * escape sequences.
     */
    public String readLine() throws IOException {
        return cli.readLine();
    }

}
