/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.common.ssl;

import javax.net.ssl.X509ExtendedTrustManager;
import java.nio.file.Path;
import java.util.Collection;

/**
 * An interface for building a trust manager at runtime.
 * The method for constructing the trust manager is implementation dependent.
 */
public interface SslTrustConfig {

    /**
     * @return A collection of files that are read by this config object.
     * The {@link #createTrustManager()} method will read these files dynamically, so the behaviour of this trust config may change if
     * any of these files are modified.
     */
    Collection<Path> getDependentFiles();

    /**
     * @return A new {@link X509ExtendedTrustManager}.
     * @throws SslConfigException if there is a problem configuring the trust manager.
     */
    X509ExtendedTrustManager createTrustManager();

}

