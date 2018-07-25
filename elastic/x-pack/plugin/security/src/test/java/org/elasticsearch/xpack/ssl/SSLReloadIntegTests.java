/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.ssl;


import org.elasticsearch.ElasticsearchException;
import org.elasticsearch.bootstrap.JavaVersion;
import org.elasticsearch.common.settings.MockSecureSettings;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.transport.TransportAddress;
import org.elasticsearch.env.TestEnvironment;
import org.elasticsearch.test.SecurityIntegTestCase;
import org.elasticsearch.transport.Transport;
import org.elasticsearch.xpack.core.ssl.SSLConfiguration;
import org.elasticsearch.xpack.core.ssl.SSLService;

import javax.net.ssl.SSLHandshakeException;
import javax.net.ssl.SSLSocket;
import javax.net.ssl.SSLSocketFactory;

import java.io.IOException;
import java.net.SocketException;
import java.nio.file.AtomicMoveNotSupportedException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.util.Arrays;
import java.util.concurrent.CountDownLatch;

import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.is;

/**
 * Integration tests for SSL reloading support
 */
public class SSLReloadIntegTests extends SecurityIntegTestCase {

    private Path nodeKeyPath;
    private Path nodeCertPath;
    private Path clientCertPath;
    private Path updateableCertPath;

    @Override
    public Settings nodeSettings(int nodeOrdinal) {
        // Nodes start trusting testnode.crt and testclient.crt
        Path origKeyPath = getDataPath("/org/elasticsearch/xpack/security/transport/ssl/certs/simple/testnode.pem");
        Path origCertPath = getDataPath("/org/elasticsearch/xpack/security/transport/ssl/certs/simple/testnode.crt");
        Path origClientCertPath = getDataPath("/org/elasticsearch/xpack/security/transport/ssl/certs/simple/testclient.crt");
        Path tempDir = createTempDir();
        try {
            if (nodeKeyPath == null) {
                nodeKeyPath = tempDir.resolve("testnode.pem");
                Files.copy(origKeyPath, nodeKeyPath);
            }
            if (nodeCertPath == null) {
                nodeCertPath = tempDir.resolve("testnode.crt");
                Files.copy(origCertPath, nodeCertPath);
            }
            if (clientCertPath == null) {
                clientCertPath = tempDir.resolve("testclient.crt");
                Files.copy(origClientCertPath, clientCertPath);
            }
            // Placeholder trusted certificate that will be updated later on
            if (updateableCertPath == null) {
                updateableCertPath = tempDir.resolve("updateable.crt");
                Files.copy(origCertPath, updateableCertPath);
            }
        } catch (IOException e) {
            throw new ElasticsearchException("failed to copy key or certificate", e);
        }

        Settings settings = super.nodeSettings(nodeOrdinal);
        Settings.Builder builder = Settings.builder()
                .put(settings.filter((s) -> s.startsWith("xpack.ssl.") == false));

        builder.put("path.home", createTempDir())
            .put("xpack.ssl.key", nodeKeyPath)
            .put("xpack.ssl.key_passphrase", "testnode")
            .put("xpack.ssl.certificate", nodeCertPath)
            .putList("xpack.ssl.certificate_authorities", Arrays.asList(nodeCertPath.toString(), clientCertPath.toString(),
                updateableCertPath.toString()))
            .put("resource.reload.interval.high", "1s");

        return builder.build();
    }

    @Override
    protected boolean transportSSLEnabled() {
        return true;
    }

    public void testThatSSLConfigurationReloadsOnModification() throws Exception {
        assumeTrue("test fails on JDK 11 currently", JavaVersion.current().compareTo(JavaVersion.parse("11")) < 0);
        Path keyPath = createTempDir().resolve("testnode_updated.pem");
        Path certPath = createTempDir().resolve("testnode_updated.crt");
        Files.copy(getDataPath("/org/elasticsearch/xpack/security/transport/ssl/certs/simple/testnode_updated.pem"), keyPath);
        Files.copy(getDataPath("/org/elasticsearch/xpack/security/transport/ssl/certs/simple/testnode_updated.crt"), certPath);
        MockSecureSettings secureSettings = new MockSecureSettings();
        secureSettings.setString("xpack.ssl.secure_key_passphrase", "testnode");
        Settings settings = Settings.builder()
            .put("path.home", createTempDir())
            .put("xpack.ssl.key", keyPath)
            .put("xpack.ssl.certificate", certPath)
            .putList("xpack.ssl.certificate_authorities", Arrays.asList(nodeCertPath.toString(), clientCertPath.toString(),
                updateableCertPath.toString()))
            .setSecureSettings(secureSettings)
            .build();
        String node = randomFrom(internalCluster().getNodeNames());
        SSLService sslService = new SSLService(settings, TestEnvironment.newEnvironment(settings));
        SSLConfiguration sslConfiguration = sslService.getSSLConfiguration("xpack.ssl");
        SSLSocketFactory sslSocketFactory = sslService.sslSocketFactory(sslConfiguration);
        TransportAddress address = internalCluster()
            .getInstance(Transport.class, node).boundAddress().publishAddress();
        // Fails as our nodes do not trust testnode_updated.crt
        try (SSLSocket socket = (SSLSocket) sslSocketFactory.createSocket(address.getAddress(), address.getPort())) {
            assertThat(socket.isConnected(), is(true));
            socket.startHandshake();
            fail("handshake should not have been successful!");
        } catch (SSLHandshakeException | SocketException expected) {
            logger.trace("expected exception", expected);
        }
        // Copy testnode_updated.crt to the placeholder updateable.crt so that the nodes will start trusting it now
        try {
            Files.move(certPath, updateableCertPath, StandardCopyOption.REPLACE_EXISTING, StandardCopyOption.ATOMIC_MOVE);
        } catch (AtomicMoveNotSupportedException e) {
            Files.move(certPath, updateableCertPath, StandardCopyOption.REPLACE_EXISTING);
        }
        CountDownLatch latch = new CountDownLatch(1);
        assertBusy(() -> {
            try (SSLSocket socket = (SSLSocket) sslSocketFactory.createSocket(address.getAddress(), address.getPort())) {
                logger.info("opened socket for reloading [{}]", socket);
                socket.addHandshakeCompletedListener(event -> {
                    try {
                        assertThat(event.getPeerPrincipal().getName(), containsString("Test Node"));
                        logger.info("ssl handshake completed on port [{}]", event.getSocket().getLocalPort());
                        latch.countDown();
                    } catch (Exception e) {
                        fail("caught exception in listener " + e.getMessage());
                    }
                });
                socket.startHandshake();

            } catch (Exception e) {
                fail("caught exception " + e.getMessage());
            }
        });
        latch.await();
    }
}
