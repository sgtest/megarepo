/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */
package org.elasticsearch.xpack.security.transport.netty4;

import io.netty.channel.Channel;
import io.netty.channel.ChannelHandler;
import io.netty.handler.ssl.SslHandler;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.elasticsearch.common.network.NetworkService;
import org.elasticsearch.common.settings.ClusterSettings;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.ssl.SslConfiguration;
import org.elasticsearch.common.util.PageCacheRecycler;
import org.elasticsearch.http.HttpChannel;
import org.elasticsearch.http.netty4.Netty4HttpServerTransport;
import org.elasticsearch.threadpool.ThreadPool;
import org.elasticsearch.transport.netty4.SharedGroupFactory;
import org.elasticsearch.xcontent.NamedXContentRegistry;
import org.elasticsearch.xpack.core.ssl.SSLService;
import org.elasticsearch.xpack.security.transport.SecurityHttpExceptionHandler;
import org.elasticsearch.xpack.security.transport.filter.IPFilter;

import javax.net.ssl.SSLEngine;

import static org.elasticsearch.xpack.core.XPackSettings.HTTP_SSL_ENABLED;

public class SecurityNetty4HttpServerTransport extends Netty4HttpServerTransport {
    private static final Logger logger = LogManager.getLogger(SecurityNetty4HttpServerTransport.class);

    private final SecurityHttpExceptionHandler securityExceptionHandler;
    private final IPFilter ipFilter;
    private final SSLService sslService;
    private final SslConfiguration sslConfiguration;

    public SecurityNetty4HttpServerTransport(
        Settings settings,
        NetworkService networkService,
        PageCacheRecycler recycler,
        IPFilter ipFilter,
        SSLService sslService,
        ThreadPool threadPool,
        NamedXContentRegistry xContentRegistry,
        Dispatcher dispatcher,
        ClusterSettings clusterSettings,
        SharedGroupFactory sharedGroupFactory
    ) {
        super(settings, networkService, threadPool, xContentRegistry, dispatcher, clusterSettings, sharedGroupFactory);
        this.securityExceptionHandler = new SecurityHttpExceptionHandler(logger, lifecycle, (c, e) -> super.onException(c, e));
        this.ipFilter = ipFilter;
        final boolean ssl = HTTP_SSL_ENABLED.get(settings);
        this.sslService = sslService;
        if (ssl) {
            this.sslConfiguration = sslService.getHttpTransportSSLConfiguration();
            if (SSLService.isConfigurationValidForServerUsage(sslConfiguration) == false) {
                throw new IllegalArgumentException(
                    "a key must be provided to run as a server. the key should be configured using the "
                        + "[xpack.security.http.ssl.key] or [xpack.security.http.ssl.keystore.path] setting"
                );
            }
        } else {
            this.sslConfiguration = null;
        }
    }

    @Override
    public void onException(HttpChannel channel, Exception e) {
        securityExceptionHandler.accept(channel, e);
    }

    @Override
    protected void doStart() {
        super.doStart();
        ipFilter.setBoundHttpTransportAddress(this.boundAddress());
    }

    @Override
    public ChannelHandler configureServerChannelHandler() {
        return new HttpSslChannelHandler();
    }

    private final class HttpSslChannelHandler extends HttpChannelHandler {
        HttpSslChannelHandler() {
            super(SecurityNetty4HttpServerTransport.this, handlingSettings);
        }

        @Override
        protected void initChannel(Channel ch) throws Exception {
            super.initChannel(ch);
            if (sslConfiguration != null) {
                SSLEngine sslEngine = sslService.createSSLEngine(sslConfiguration, null, -1);
                sslEngine.setUseClientMode(false);
                ch.pipeline().addFirst("ssl", new SslHandler(sslEngine));
            }
            ch.pipeline().addFirst("ip_filter", new IpFilterRemoteAddressFilter(ipFilter, IPFilter.HTTP_PROFILE_NAME));
        }
    }
}
