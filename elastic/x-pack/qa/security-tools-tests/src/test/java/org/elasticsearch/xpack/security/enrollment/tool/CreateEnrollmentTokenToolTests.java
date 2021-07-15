/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.security.enrollment.tool;

import com.google.common.jimfs.Configuration;
import com.google.common.jimfs.Jimfs;

import org.elasticsearch.cli.Command;
import org.elasticsearch.cli.CommandTestCase;
import org.elasticsearch.cli.ExitCodes;
import org.elasticsearch.cli.UserException;
import org.elasticsearch.common.CheckedSupplier;
import org.elasticsearch.common.settings.KeyStoreWrapper;
import org.elasticsearch.common.settings.SecureString;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.core.CheckedFunction;
import org.elasticsearch.core.PathUtilsForTesting;
import org.elasticsearch.core.internal.io.IOUtils;
import org.elasticsearch.env.Environment;
import org.elasticsearch.xpack.core.XPackSettings;
import org.elasticsearch.xpack.security.enrollment.CreateEnrollmentToken;
import org.elasticsearch.xpack.security.tool.CommandLineHttpClient;
import org.elasticsearch.xpack.security.tool.HttpResponse;
import org.junit.AfterClass;
import org.junit.Before;
import org.junit.BeforeClass;

import java.io.IOException;
import java.net.HttpURLConnection;
import java.net.MalformedURLException;
import java.net.URISyntaxException;
import java.net.URL;
import java.nio.charset.StandardCharsets;
import java.nio.file.FileSystem;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import java.util.Map;

import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.equalTo;
import static org.mockito.Matchers.any;
import static org.mockito.Matchers.anyString;
import static org.mockito.Matchers.eq;
import static org.mockito.Mockito.mock;
import static org.mockito.Mockito.when;

@SuppressWarnings("unchecked")
public class CreateEnrollmentTokenToolTests extends CommandTestCase {

    static FileSystem jimfs;
    String pathHomeParameter;
    Path confDir;
    Settings settings;

    private CommandLineHttpClient client;
    private KeyStoreWrapper keyStoreWrapper;
    private CreateEnrollmentToken createEnrollmentTokenService;

    @Override
    protected Command newCommand() {
        return new CreateEnrollmentTokenTool(environment -> client, environment -> keyStoreWrapper,
            environment -> createEnrollmentTokenService) {
            @Override
            protected Environment createEnv(Map<String, String> settings) throws UserException {
                return new Environment(CreateEnrollmentTokenToolTests.this.settings, confDir);
            }
        };
    }

    @BeforeClass
    public static void muteInFips(){
        assumeFalse("Enrollment mode is not supported in FIPS mode.", inFipsJvm());
    }

    @BeforeClass
    public static void setupJimfs() {
        String view = randomFrom("basic", "posix");
        Configuration conf = Configuration.unix().toBuilder().setAttributeViews(view).build();
        jimfs = Jimfs.newFileSystem(conf);
        PathUtilsForTesting.installMock(jimfs);
    }

    @Before
    public void setup() throws Exception {
        Path homeDir = jimfs.getPath("eshome");
        IOUtils.rm(homeDir);
        confDir = homeDir.resolve("config");
        Files.createDirectories(confDir);
        Files.write(confDir.resolve("users"), List.of(), StandardCharsets.UTF_8);
        Files.write(confDir.resolve("users_roles"),  List.of(), StandardCharsets.UTF_8);
        settings = Settings.builder()
            .put("path.home", homeDir)
            .put("xpack.security.enrollment.enabled", true)
            .build();
        pathHomeParameter = "-Epath.home=" + homeDir;

        this.keyStoreWrapper = mock(KeyStoreWrapper.class);
        when(keyStoreWrapper.isLoaded()).thenReturn(true);

        this.client = mock(CommandLineHttpClient.class);
        when(client.getDefaultURL()).thenReturn("https://localhost:9200");

        URL url = new URL(client.getDefaultURL());
        HttpResponse healthResponse =
            new HttpResponse(HttpURLConnection.HTTP_OK, Map.of("status", randomFrom("yellow", "green")));
        when(client.execute(anyString(), eq(clusterHealthUrl(url)), anyString(), any(SecureString.class), any(CheckedSupplier.class),
            any(CheckedFunction.class))).thenReturn(healthResponse);

        this.createEnrollmentTokenService = mock(CreateEnrollmentToken.class);
        when(createEnrollmentTokenService.createKibanaEnrollmentToken(anyString(), any(SecureString.class)))
            .thenReturn("eyJ2ZXIiOiI4LjAuMCIsImFkciI6WyJbOjoxXTo5MjAwIiwiMTI3LjAuMC4xOjkyMDAiXSwiZmdyIjoiOWQ4MTRmYzdiNDQ0MWE0MWJlMDA5ZmQ0" +
                "MzlkOWU5MzRiMDZiMjZjZjk4N2I1YzNjOGU0OWI1NmQ2MGYzMmMxMiIsImtleSI6Im5NMmVYbm9CbnBvT3ZncGFiaWU5OlRHaHF5UU9UVENhUEJpOVZQak1i" +
                "OWcifQ==");
        when(createEnrollmentTokenService.createNodeEnrollmentToken(anyString(), any(SecureString.class)))
            .thenReturn("eyJ2ZXIiOiI4LjAuMCIsImFkciI6WyJbOjoxXTo5MjAwIiwiMTI3LjAuMC4xOjkyMDAiXSwiZmdyIjoiOWQ4MTRmYzdiNDQ0MWE0MWJlMDA5ZmQ0" +
                "MzlkOWU5MzRiMDZiMjZjZjk4N2I1YzNjOGU0OWI1NmQ2MGYzMmMxMiIsImtleSI6IndLTmZYSG9CQTFPMHI4UXBOV25FOnRkdUgzTmNTVHNTOGN0c3AwaWNU" +
                "eEEifQ==");
    }

    @AfterClass
    public static void closeJimfs() throws IOException {
        if (jimfs != null) {
            jimfs.close();
            jimfs = null;
        }
    }

    public void testCreateToken() throws Exception {
        String scope = randomBoolean() ? "node" : "kibana";
        String output = execute("--scope", scope);
        if (scope.equals("kibana")) {
            assertThat(output, containsString("WU5OlRHaHF5UU9UVENhUEJpOVZQak1iOWcifQ=="));
        } else {
            assertThat(output, containsString("25FOnRkdUgzTmNTVHNTOGN0c3AwaWNUeEEifQ=="));
        }
    }

    public void testInvalidScope() throws Exception {
        String scope = randomAlphaOfLength(14);
        UserException e = expectThrows(UserException.class, () -> {
            execute(randomFrom("-s", "--s"), scope);
        });
        assertThat(e.exitCode, equalTo(ExitCodes.USAGE));
        assertThat(e.getMessage(), equalTo("Invalid scope"));
        assertThat(terminal.getErrorOutput(),
            containsString("The scope of this enrollment token, can only be one of "+ CreateEnrollmentTokenTool.ALLOWED_SCOPES));
    }

    public void testUnhealthyCluster() throws Exception {
        String scope = randomBoolean() ? "node" : "kibana";
        URL url = new URL(client.getDefaultURL());
        HttpResponse healthResponse =
            new HttpResponse(HttpURLConnection.HTTP_OK, Map.of("status", randomFrom("red")));
        when(client.execute(anyString(), eq(clusterHealthUrl(url)), anyString(), any(SecureString.class), any(CheckedSupplier.class),
            any(CheckedFunction.class))).thenReturn(healthResponse);
        UserException e = expectThrows(UserException.class, () -> {
            execute(randomFrom("-s", "--s"), scope);
        });
        assertThat(e.exitCode, equalTo(ExitCodes.UNAVAILABLE));
        assertThat(e.getMessage(), containsString("RED"));
    }

    public void testUnhealthyClusterWithForce() throws Exception {
        String scope = randomBoolean() ? "node" : "kibana";
        String output = execute("--scope", scope);
        if (scope.equals("kibana")) {
            assertThat(output, containsString("WU5OlRHaHF5UU9UVENhUEJpOVZQak1iOWcifQ=="));
        } else {
            assertThat(output, containsString("25FOnRkdUgzTmNTVHNTOGN0c3AwaWNUeEEifQ=="));
        }
    }

    public void testEnrollmentDisabled() throws Exception {
        settings = Settings.builder()
            .put(settings)
            .put(XPackSettings.ENROLLMENT_ENABLED.getKey(), false)
            .build();

        String scope = randomBoolean() ? "node" : "kibana";
        UserException e = expectThrows(UserException.class, () -> {
            execute(randomFrom("-s", "--s"), scope);
        });
        assertThat(e.exitCode, equalTo(ExitCodes.CONFIG));
        assertThat(e.getMessage(),
            equalTo("[xpack.security.enrollment.enabled] must be set to `true` to create an enrollment token"));
    }

    public void testUnableToCreateToken() throws Exception {
        this.createEnrollmentTokenService = mock(CreateEnrollmentToken.class);
        when(createEnrollmentTokenService.createKibanaEnrollmentToken(anyString(), any(SecureString.class)))
            .thenThrow(new IllegalStateException("example exception message"));
        when(createEnrollmentTokenService.createNodeEnrollmentToken(anyString(), any(SecureString.class)))
            .thenThrow(new IllegalStateException("example exception message"));
        String scope = randomBoolean() ? "node" : "kibana";
        UserException e = expectThrows(UserException.class, () -> {
            execute(randomFrom("-s", "--s"), scope);
        });
        assertThat(e.exitCode, equalTo(ExitCodes.CANT_CREATE));
        assertThat(e.getMessage(),
            equalTo("example exception message"));
    }

    private URL clusterHealthUrl(URL url) throws MalformedURLException, URISyntaxException {
        return new URL(url, (url.toURI().getPath() + "/_cluster/health").replaceAll("/+", "/") + "?pretty");
    }
}
