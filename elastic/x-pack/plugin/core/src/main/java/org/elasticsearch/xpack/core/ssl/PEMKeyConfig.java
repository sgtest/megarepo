/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.ssl;

import org.elasticsearch.ElasticsearchException;
import org.elasticsearch.common.Nullable;
import org.elasticsearch.common.settings.SecureString;
import org.elasticsearch.env.Environment;
import org.elasticsearch.xpack.core.ssl.cert.CertificateInfo;

import javax.net.ssl.X509ExtendedKeyManager;
import javax.net.ssl.X509ExtendedTrustManager;

import java.io.IOException;
import java.io.Reader;
import java.io.UncheckedIOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.security.KeyStoreException;
import java.security.NoSuchAlgorithmException;
import java.security.PrivateKey;
import java.security.UnrecoverableKeyException;
import java.security.cert.Certificate;
import java.security.cert.CertificateException;
import java.security.cert.X509Certificate;
import java.util.ArrayList;
import java.util.Collection;
import java.util.Collections;
import java.util.List;
import java.util.Objects;

/**
 * Implementation of a key configuration that is backed by a PEM encoded key file and one or more certificates
 */
class PEMKeyConfig extends KeyConfig {

    private final String keyPath;
    private final SecureString keyPassword;
    private final String certPath;

    /**
     * Creates a new key configuration backed by the key and certificate chain provided
     *
     * @param keyPath       the path to the key file
     * @param keyPassword   the password for the key.
     * @param certChainPath the path to the file containing the certificate chain
     */
    PEMKeyConfig(String keyPath, SecureString keyPassword, String certChainPath) {
        this.keyPath = Objects.requireNonNull(keyPath, "key file must be specified");
        this.keyPassword = Objects.requireNonNull(keyPassword).clone();
        this.certPath = Objects.requireNonNull(certChainPath, "certificate must be specified");
    }

    @Override
    X509ExtendedKeyManager createKeyManager(@Nullable Environment environment) {
        try {
            PrivateKey privateKey = readPrivateKey(CertUtils.resolvePath(keyPath, environment), keyPassword);
            if (privateKey == null) {
                throw new IllegalArgumentException("private key [" + keyPath + "] could not be loaded");
            }
            Certificate[] certificateChain = getCertificateChain(environment);

            return CertUtils.keyManager(certificateChain, privateKey, keyPassword.getChars());
        } catch (IOException | UnrecoverableKeyException | NoSuchAlgorithmException | CertificateException | KeyStoreException e) {
            throw new ElasticsearchException("failed to initialize a KeyManagerFactory", e);
        }
    }

    private Certificate[] getCertificateChain(@Nullable Environment environment) throws CertificateException, IOException {
        return CertUtils.readCertificates(Collections.singletonList(certPath), environment);
    }

    @Override
    Collection<CertificateInfo> certificates(Environment environment) throws CertificateException, IOException {
        final Certificate[] chain = getCertificateChain(environment);
        final List<CertificateInfo> info = new ArrayList<>(chain.length);
        for (int i = 0; i < chain.length; i++) {
            final Certificate cert = chain[i];
            if (cert instanceof X509Certificate) {
                info.add(new CertificateInfo(certPath, "PEM", null, i == 0, (X509Certificate) cert));
            }
        }
        return info;
    }

    @Override
    List<PrivateKey> privateKeys(@Nullable Environment environment) {
        try {
            return Collections.singletonList(readPrivateKey(CertUtils.resolvePath(keyPath, environment), keyPassword));
        } catch (IOException e) {
            throw new UncheckedIOException("failed to read key", e);
        }
    }

    private static PrivateKey readPrivateKey(Path keyPath, SecureString keyPassword) throws IOException {
        try (Reader reader = Files.newBufferedReader(keyPath, StandardCharsets.UTF_8)) {
            return CertUtils.readPrivateKey(reader, keyPassword::getChars);
        }
    }

    @Override
    X509ExtendedTrustManager createTrustManager(@Nullable Environment environment) {
        try {
            Certificate[] certificates = getCertificateChain(environment);
            return CertUtils.trustManager(certificates);
        } catch (Exception e) {
            throw new ElasticsearchException("failed to initialize a TrustManagerFactory", e);
        }
    }

    @Override
    List<Path> filesToMonitor(@Nullable Environment environment) {
        List<Path> paths = new ArrayList<>(2);
        paths.add(CertUtils.resolvePath(keyPath, environment));
        paths.add(CertUtils.resolvePath(certPath, environment));
        return paths;
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (o == null || getClass() != o.getClass()) return false;

        PEMKeyConfig that = (PEMKeyConfig) o;

        if (keyPath != null ? !keyPath.equals(that.keyPath) : that.keyPath != null) return false;
        if (keyPassword != null ? !keyPassword.equals(that.keyPassword) : that.keyPassword != null) return false;
        return certPath != null ? certPath.equals(that.certPath) : that.certPath == null;

    }

    @Override
    public int hashCode() {
        int result = keyPath != null ? keyPath.hashCode() : 0;
        result = 31 * result + (keyPassword != null ? keyPassword.hashCode() : 0);
        result = 31 * result + (certPath != null ? certPath.hashCode() : 0);
        return result;
    }

    @Override
    public String toString() {
        return "keyPath=[" + keyPath +
                "], certPaths=[" + certPath +
                "]";
    }
}
