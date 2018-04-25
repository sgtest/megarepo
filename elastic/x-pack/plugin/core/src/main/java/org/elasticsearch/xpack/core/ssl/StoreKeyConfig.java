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
import java.io.InputStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.security.GeneralSecurityException;
import java.security.Key;
import java.security.KeyStore;
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
import java.util.Enumeration;
import java.util.List;
import java.util.Objects;

/**
 * A key configuration that is backed by a {@link KeyStore}
 */
class StoreKeyConfig extends KeyConfig {

    final String keyStorePath;
    final String keyStoreType;
    final SecureString keyStorePassword;
    final String keyStoreAlgorithm;
    final SecureString keyPassword;
    final String trustStoreAlgorithm;

    /**
     * Creates a new configuration that can be used to load key and trust material from a {@link KeyStore}
     * @param keyStorePath the path to the keystore file
     * @param keyStoreType the type of the keystore file
     * @param keyStorePassword the password for the keystore
     * @param keyPassword the password for the private key in the keystore
     * @param keyStoreAlgorithm the algorithm for the keystore
     * @param trustStoreAlgorithm the algorithm to use when loading as a truststore
     */
    StoreKeyConfig(String keyStorePath, String keyStoreType, SecureString keyStorePassword, SecureString keyPassword,
                   String keyStoreAlgorithm, String trustStoreAlgorithm) {
        this.keyStorePath = Objects.requireNonNull(keyStorePath, "keystore path must be specified");
        this.keyStoreType = Objects.requireNonNull(keyStoreType, "keystore type must be specified");
        // since we support reloading the keystore, we must store the passphrase in memory for the life of the node, so we
        // clone the password and never close it during our uses below
        this.keyStorePassword = Objects.requireNonNull(keyStorePassword, "keystore password must be specified").clone();
        this.keyPassword = Objects.requireNonNull(keyPassword).clone();
        this.keyStoreAlgorithm = keyStoreAlgorithm;
        this.trustStoreAlgorithm = trustStoreAlgorithm;
    }

    @Override
    X509ExtendedKeyManager createKeyManager(@Nullable Environment environment) {
        try {
            KeyStore ks = getKeyStore(environment);
            checkKeyStore(ks);
            return CertUtils.keyManager(ks, keyPassword.getChars(), keyStoreAlgorithm);
        } catch (IOException | CertificateException | NoSuchAlgorithmException | UnrecoverableKeyException | KeyStoreException e) {
            throw new ElasticsearchException("failed to initialize a KeyManagerFactory", e);
        }
    }

    @Override
    X509ExtendedTrustManager createTrustManager(@Nullable Environment environment) {
        try {
            return CertUtils.trustManager(keyStorePath, keyStoreType, keyStorePassword.getChars(), trustStoreAlgorithm, environment);
        } catch (Exception e) {
            throw new ElasticsearchException("failed to initialize a TrustManagerFactory", e);
        }
    }

    @Override
    Collection<CertificateInfo> certificates(Environment environment) throws GeneralSecurityException, IOException {
        final Path path = CertUtils.resolvePath(keyStorePath, environment);
        final KeyStore trustStore = CertUtils.readKeyStore(path, keyStoreType, keyStorePassword.getChars());
        final List<CertificateInfo> certificates = new ArrayList<>();
        final Enumeration<String> aliases = trustStore.aliases();
        while (aliases.hasMoreElements()) {
            String alias = aliases.nextElement();
            final Certificate[] chain = trustStore.getCertificateChain(alias);
            if (chain == null) {
                continue;
            }
            for (int i = 0; i < chain.length; i++) {
                final Certificate certificate = chain[i];
                if (certificate instanceof X509Certificate) {
                    certificates.add(new CertificateInfo(keyStorePath, keyStoreType, alias, i == 0, (X509Certificate) certificate));
                }
            }
        }
        return certificates;
    }

    @Override
    List<Path> filesToMonitor(@Nullable Environment environment) {
        return Collections.singletonList(CertUtils.resolvePath(keyStorePath, environment));
    }

    @Override
    List<PrivateKey> privateKeys(@Nullable Environment environment) {
        try {
            KeyStore keyStore = getKeyStore(environment);
            List<PrivateKey> privateKeys = new ArrayList<>();
            for (Enumeration<String> e = keyStore.aliases(); e.hasMoreElements(); ) {
                final String alias = e.nextElement();
                if (keyStore.isKeyEntry(alias)) {
                    Key key = keyStore.getKey(alias, keyPassword.getChars());
                    if (key instanceof PrivateKey) {
                        privateKeys.add((PrivateKey) key);
                    }
                }
            }
            return privateKeys;
        } catch (Exception e) {
            throw new ElasticsearchException("failed to list keys", e);
        }
    }

    private KeyStore getKeyStore(@Nullable Environment environment)
                                throws KeyStoreException, CertificateException, NoSuchAlgorithmException, IOException {
        try (InputStream in = Files.newInputStream(CertUtils.resolvePath(keyStorePath, environment))) {
            KeyStore ks = KeyStore.getInstance(keyStoreType);
            ks.load(in, keyStorePassword.getChars());
            return ks;
        }
    }

    private void checkKeyStore(KeyStore keyStore) throws KeyStoreException {
        Enumeration<String> aliases = keyStore.aliases();
        while (aliases.hasMoreElements()) {
            String alias = aliases.nextElement();
            if (keyStore.isKeyEntry(alias)) {
                return;
            }
        }
        throw new IllegalArgumentException("the keystore [" + keyStorePath + "] does not contain a private key entry");
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (o == null || getClass() != o.getClass()) return false;

        StoreKeyConfig that = (StoreKeyConfig) o;

        if (keyStorePath != null ? !keyStorePath.equals(that.keyStorePath) : that.keyStorePath != null) return false;
        if (keyStorePassword != null ? !keyStorePassword.equals(that.keyStorePassword) : that.keyStorePassword != null)
            return false;
        if (keyStoreAlgorithm != null ? !keyStoreAlgorithm.equals(that.keyStoreAlgorithm) : that.keyStoreAlgorithm != null)
            return false;
        if (keyPassword != null ? !keyPassword.equals(that.keyPassword) : that.keyPassword != null) return false;
        return trustStoreAlgorithm != null ? trustStoreAlgorithm.equals(that.trustStoreAlgorithm) : that.trustStoreAlgorithm == null;
    }

    @Override
    public int hashCode() {
        int result = keyStorePath != null ? keyStorePath.hashCode() : 0;
        result = 31 * result + (keyStorePassword != null ? keyStorePassword.hashCode() : 0);
        result = 31 * result + (keyStoreAlgorithm != null ? keyStoreAlgorithm.hashCode() : 0);
        result = 31 * result + (keyPassword != null ? keyPassword.hashCode() : 0);
        result = 31 * result + (trustStoreAlgorithm != null ? trustStoreAlgorithm.hashCode() : 0);
        return result;
    }

    @Override
    public String toString() {
        return "keyStorePath=[" + keyStorePath +
                "], keyStoreType=[" + keyStoreType +
                "], keyStoreAlgorithm=[" + keyStoreAlgorithm +
                "], trustStoreAlgorithm=[" + trustStoreAlgorithm +
                "]";
    }
}
