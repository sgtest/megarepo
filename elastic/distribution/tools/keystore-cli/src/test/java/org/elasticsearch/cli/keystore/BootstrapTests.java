/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */
package org.elasticsearch.cli.keystore;

import org.elasticsearch.bootstrap.BootstrapUtil;
import org.elasticsearch.common.settings.KeyStoreWrapper;
import org.elasticsearch.common.settings.SecureSettings;
import org.elasticsearch.common.settings.SecureString;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.core.IOUtils;
import org.elasticsearch.env.Environment;
import org.elasticsearch.test.ESTestCase;
import org.junit.After;
import org.junit.Before;

import java.io.ByteArrayInputStream;
import java.io.IOException;
import java.io.InputStream;
import java.nio.charset.StandardCharsets;
import java.nio.file.FileSystem;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;

import static org.hamcrest.Matchers.equalTo;

public class BootstrapTests extends ESTestCase {
    Environment env;
    List<FileSystem> fileSystems = new ArrayList<>();

    @After
    public void closeMockFileSystems() throws IOException {
        IOUtils.close(fileSystems);
    }

    @Before
    public void setupEnv() throws IOException {
        env = KeyStoreCommandTestCase.setupEnv(true, fileSystems);
    }

    public void testLoadSecureSettings() throws Exception {
        final char[] password = KeyStoreWrapperTests.getPossibleKeystorePassword();
        final Path configPath = env.configFile();
        final SecureString seed;
        try (KeyStoreWrapper keyStoreWrapper = KeyStoreWrapper.create()) {
            seed = KeyStoreWrapper.SEED_SETTING.get(Settings.builder().setSecureSettings(keyStoreWrapper).build());
            assertNotNull(seed);
            assertTrue(seed.length() > 0);
            keyStoreWrapper.save(configPath, password);
        }
        final InputStream in = password.length > 0
            ? new ByteArrayInputStream(new String(password).getBytes(StandardCharsets.UTF_8))
            : System.in;
        assertTrue(Files.exists(configPath.resolve("elasticsearch.keystore")));
        try (SecureSettings secureSettings = BootstrapUtil.loadSecureSettings(env, in)) {
            SecureString seedAfterLoad = KeyStoreWrapper.SEED_SETTING.get(Settings.builder().setSecureSettings(secureSettings).build());
            assertEquals(seedAfterLoad.toString(), seed.toString());
            assertTrue(Files.exists(configPath.resolve("elasticsearch.keystore")));
        }
    }

    public void testReadCharsFromStdin() throws Exception {
        assertPassphraseRead("hello", "hello");
        assertPassphraseRead("hello\n", "hello");
        assertPassphraseRead("hello\r\n", "hello");

        assertPassphraseRead("hellohello", "hellohello");
        assertPassphraseRead("hellohello\n", "hellohello");
        assertPassphraseRead("hellohello\r\n", "hellohello");

        assertPassphraseRead("hello\nhi\n", "hello");
        assertPassphraseRead("hello\r\nhi\r\n", "hello");
    }

    public void testNoPassPhraseProvided() throws Exception {
        byte[] source = "\r\n".getBytes(StandardCharsets.UTF_8);
        try (InputStream stream = new ByteArrayInputStream(source)) {
            expectThrows(
                RuntimeException.class,
                "Keystore passphrase required but none provided.",
                () -> BootstrapUtil.readPassphrase(stream)
            );
        }
    }

    private void assertPassphraseRead(String source, String expected) {
        try (InputStream stream = new ByteArrayInputStream(source.getBytes(StandardCharsets.UTF_8))) {
            SecureString result = BootstrapUtil.readPassphrase(stream);
            assertThat(result, equalTo(expected));
        } catch (IOException e) {
            throw new RuntimeException(e);
        }
    }

}
